use super::*;
use serde_json::json;

fn place() -> Location {
    Location {
        name: "Vienna".into(),
        region: "Vienna".into(),
        country: "Austria".into(),
        latitude: 48.2085,
        longitude: 16.3721,
    }
}
fn reading() -> Reading {
    Reading {
        temperature: 14.2,
        code: 2,
        is_day: true,
    }
}
fn preferences() -> Preferences {
    Preferences {
        weather_location: Some(place()),
        ..Preferences::default()
    }
}
fn cache(updated_at: i64) -> Cache {
    Cache {
        key: place().key(),
        updated_at,
        reading: reading(),
        details: None,
    }
}
fn forecast() -> Forecast {
    Forecast {
        reading: reading(),
        details: Details::default(),
    }
}

#[test]
fn search_results_are_validated_normalized_and_deduplicated() {
    let result = parse_locations(&json!({"results":[
        {"name":"  Vienna\n", "admin1":"Vienna", "country":"Austria", "latitude":48.2085,"longitude":16.3721},
        {"name":"Duplicate", "latitude":48.2085,"longitude":16.3721},
        {"name":"Bad", "latitude":91,"longitude":16}, {"name":"", "latitude":0,"longitude":0}
    ]})).unwrap();
    assert_eq!(result, vec![place()]);
    assert_eq!(result[0].detail(), "Austria");
    assert!(parse_locations(&json!({})).unwrap().is_empty());
    assert!(parse_locations(&json!({"results":"wrong"})).is_err());
    assert!(parse_locations(&json!({"error":true})).is_err());
}

#[test]
fn temperatures_convert_the_unrounded_reading_and_use_web_rounding() {
    assert_eq!(format_temperature(14.2, Unit::Fahrenheit), "58°F");
    assert_eq!(format_temperature(14.2, Unit::Celsius), "14°C");
    assert_eq!(format_temperature(-0.5, Unit::Celsius), "0°C");
    assert_eq!(format_temperature(-5.5, Unit::Celsius), "-5°C");
}

#[test]
fn api_readings_and_conditions_cover_invalid_data_and_night() {
    let good = json!({"current":{"temperature_2m":14.2,"weather_code":2.0,"is_day":1.0}});
    assert_eq!(parse_reading(&good).unwrap(), reading());
    for value in [
        json!({}),
        json!({"current":{"temperature_2m":90,"weather_code":2,"is_day":1}}),
        json!({"current":{"temperature_2m":14,"weather_code":2.5,"is_day":1}}),
        json!({"current":{"temperature_2m":14,"weather_code":2,"is_day":true}}),
    ] {
        assert!(parse_reading(&value).is_err());
    }
    for (code, day, icon) in [
        (0, false, "moon"),
        (0, true, "sun"),
        (45, true, "fog"),
        (56, true, "drizzle"),
        (81, true, "rain"),
        (85, true, "snow"),
        (99, true, "storm"),
    ] {
        assert_eq!(
            condition(Reading {
                code,
                is_day: day,
                ..reading()
            })
            .icon,
            icon
        );
    }
}

#[test]
fn cache_reuse_and_units_do_not_trigger_another_request() {
    let mut weather = Weather {
        cache: Some(cache(1000)),
        ..Weather::default()
    };
    let mut preferences = preferences();
    assert!(weather.sync(&preferences, true, 1100).0.is_none());
    preferences.temperature_unit = Unit::Fahrenheit;
    assert!(weather.sync(&preferences, true, 1200).0.is_none());
    assert_eq!(weather.reading(&place()).unwrap().temperature, 14.2);
    assert!(matches!(
        weather.sync(&preferences, true, 1900).0,
        Some(Command::Forecast { .. })
    ));
}

#[test]
fn no_requests_without_a_location_enabled_widget_and_visible_home() {
    let mut weather = Weather::default();
    let mut preferences = preferences();
    preferences.show_weather = false;
    assert!(weather.sync(&preferences, true, 1000).0.is_none());
    assert!(preferences.weather_location.is_some());
    preferences.show_weather = true;
    assert!(weather.sync(&preferences, false, 1000).0.is_none());
    preferences.weather_location = None;
    assert!(weather.sync(&preferences, true, 1000).0.is_none());
}

#[test]
fn weather_failure_keeps_valid_cache_labels_it_saved_and_retries_later() {
    let mut weather = Weather {
        cache: Some(cache(1000)),
        ..Weather::default()
    };
    let preferences = preferences();
    let Some(Command::Forecast { id, location, .. }) = weather.sync(&preferences, true, 2000).0
    else {
        panic!("Expected stale-cache refresh");
    };
    weather.accept_forecast(id, &location.key(), Err("offline".into()), 2000);
    assert!(weather.failed);
    assert!(weather.reading(&place()).is_some());
    assert!(weather.sync(&preferences, true, 2299).0.is_none());
    assert!(weather.sync(&preferences, true, 2300).0.is_some());
    assert!(cache(1000).age(999).is_none());
    assert!(cache(1000).age(1001 + MAX_CACHE_AGE).is_none());
    weather.sync(&preferences, true, 1001 + MAX_CACHE_AGE);
    assert!(weather.cache.is_none());
}

#[test]
fn removed_disabled_or_changed_locations_reject_in_flight_results() {
    let mut weather = Weather::default();
    let mut preferences = preferences();
    let Some(Command::Forecast { id, location, .. }) = weather.sync(&preferences, true, 1000).0
    else {
        panic!();
    };
    weather.remove_location();
    preferences.weather_location = None;
    assert_eq!(
        weather.accept_forecast(id, &location.key(), Ok(forecast()), 1001),
        (false, None)
    );
    assert!(weather.cache.is_none());
    preferences.weather_location = Some(place());
    let Some(Command::Forecast { id, location, .. }) = weather.sync(&preferences, true, 1002).0
    else {
        panic!();
    };
    preferences.show_weather = false;
    weather.sync(&preferences, true, 1003);
    assert_eq!(
        weather.accept_forecast(id, &location.key(), Ok(forecast()), 1004),
        (false, None)
    );
    preferences.show_weather = true;
    let Some(Command::Forecast {
        id: old,
        location: old_location,
        ..
    }) = weather.sync(&preferences, true, 1005).0
    else {
        panic!();
    };
    preferences.weather_location.as_mut().unwrap().latitude += 1.;
    let Some(Command::Forecast {
        id: new,
        location: new_location,
        ..
    }) = weather.sync(&preferences, true, 1006).0
    else {
        panic!();
    };
    assert_eq!(
        weather.accept_forecast(old, &old_location.key(), Ok(forecast()), 1007),
        (false, None)
    );
    assert!(
        weather
            .accept_forecast(new, &new_location.key(), Ok(forecast()), 1007)
            .0
    );
    assert_eq!(weather.cache.as_ref().unwrap().key, new_location.key());
}

#[test]
fn returning_home_retries_stale_weather_without_waiting_for_the_old_retry_timer() {
    let mut weather = Weather {
        cache: Some(cache(1000)),
        ..Weather::default()
    };
    let preferences = preferences();
    let Some(Command::Forecast { id, location, .. }) = weather.sync(&preferences, true, 2000).0
    else {
        panic!();
    };
    weather.accept_forecast(id, &location.key(), Err("offline".into()), 2000);
    assert!(weather.sync(&preferences, false, 2010).0.is_none());
    assert!(weather.sync(&preferences, true, 2020).0.is_some());
}

#[test]
fn editing_query_invalidates_old_results_and_short_queries_do_not_search() {
    let mut weather = Weather::default();
    weather.edit("V".into());
    assert!(weather.search(Language::En).is_none());
    assert!(weather.message.is_some());
    weather.edit("Vienna".into());
    let Some(Command::Search { id, .. }) = weather.search(Language::En) else {
        panic!();
    };
    assert!(weather.search(Language::En).is_none());
    weather.edit("Paris".into());
    assert!(!weather.accept_search(id, Ok(vec![place()])));
    assert!(weather.results.is_empty());
}

#[test]
fn malformed_weather_preferences_do_not_lose_existing_favorites() {
    let preferences: Preferences = serde_json::from_value(json!({
        "favorites":["netflix"], "weather_location":{"name":"Invalid","latitude":"oops","longitude":0},
        "temperature_unit":"kelvin", "show_weather":"off"
    })).unwrap();
    assert_eq!(preferences.favorites, Some(vec!["netflix".into()]));
    assert!(preferences.weather_location.is_none());
    assert_eq!(preferences.temperature_unit, Unit::Celsius);
    assert!(preferences.show_weather);
    let old: Preferences = serde_json::from_value(json!({"background":"forest"})).unwrap();
    assert!(old.show_weather);
    assert!(old.weather_location.is_none());
}

fn reply() -> serde_json::Value {
    json!({
        "utc_offset_seconds": 3600,
        "current": {"temperature_2m": 12.4, "weather_code": 2, "is_day": 1, "uv_index": 3.2},
        "daily": {
            "temperature_2m_max": [15.1, 16.0], "temperature_2m_min": [6.6, 7.0],
            "sunrise": [1000, 2000], "sunset": [5000, 6000]
        },
        "hourly": {
            "time": [3600, 7200, 10800, 14400, 18000],
            "temperature_2m": [12.0, 13.0, "warm", 11.0, 10.0],
            "weather_code": [2, 3, 3, 61, 120],
            "is_day": [1, 1, 1, 0, 0],
            "precipitation_probability": [10, 60, 20, 101, 0]
        }
    })
}

#[test]
fn details_keep_valid_values_and_drop_malformed_hours() {
    let details = parse_details(&reply());
    assert_eq!(details.offset, 3600);
    assert_eq!((details.high, details.low), (Some(15.1), Some(6.6)));
    assert_eq!((details.sunrise, details.sunset), (Some(1000), Some(5000)));
    assert_eq!(details.uv, Some(3.2));
    // A non-numeric temperature and an unknown weather code drop only their hour.
    let times: Vec<_> = details.hours.iter().map(|hour| hour.time).collect();
    assert_eq!(times, [3600, 7200, 14400]);
    // An impossible chance of rain keeps the hour without one.
    assert_eq!(details.hours[2].rain, None);
    assert!(!details.air_requested);

    let partial = parse_details(&json!({
        "utc_offset_seconds": 999_999,
        "current": {"uv_index": -1},
        "daily": {"temperature_2m_max": [200]}
    }));
    assert_eq!(partial, Details::default());
}

#[test]
fn rain_chance_looks_three_hours_ahead_from_the_current_hour() {
    let details = parse_details(&reply());
    // 7200 is still the current hour at 9000; 14400 is beyond three hours from 7200.
    assert_eq!(details.rain_chance(7200), Some(60));
    assert_eq!(
        details.upcoming(9000).next().map(|hour| hour.time),
        Some(7200)
    );
    assert_eq!(details.rain_chance(20000), None);
}

#[test]
fn air_quality_levels_follow_the_european_index() {
    for (aqi, key) in [
        (0., "airGood"),
        (19.9, "airGood"),
        (20., "airFair"),
        (45., "airModerate"),
        (65., "airPoor"),
        (85., "airVeryPoor"),
        (140., "airExtremelyPoor"),
    ] {
        assert_eq!(air_key(aqi), key, "{aqi}");
    }
    assert_eq!(
        parse_air(&json!({"current": {"european_aqi": 31}})),
        Some(31.)
    );
    assert_eq!(
        parse_air(&json!({"current": {"european_aqi": "bad"}})),
        None
    );
}

#[test]
fn turning_on_details_refetches_a_cache_without_them_once() {
    let mut preferences = preferences();
    let mut weather = Weather {
        cache: Some(cache(1000)),
        ..Weather::default()
    };
    // Nothing extra is shown, so a fresh cache without details is enough.
    assert!(weather.sync(&preferences, true, 1100).0.is_none());

    preferences.weather_hourly = true;
    weather.invalidate_forecast();
    let Some(Command::Forecast { id, location, air }) = weather.sync(&preferences, true, 1100).0
    else {
        panic!("a cache without details should be refetched");
    };
    assert!(!air);
    assert!(
        !weather.loading,
        "the old reading stays on screen meanwhile"
    );
    weather.accept_forecast(id, &location.key(), Ok(forecast()), 1101);
    assert!(weather.sync(&preferences, true, 1200).0.is_none());

    // Air quality is only asked for once it is on.
    preferences.weather_air = true;
    weather.invalidate_forecast();
    let Some(Command::Forecast { id, location, air }) = weather.sync(&preferences, true, 1200).0
    else {
        panic!("turning on air quality should fetch it");
    };
    assert!(air);
    let mut with_air = forecast();
    with_air.details.air_requested = true;
    weather.accept_forecast(id, &location.key(), Ok(with_air), 1201);
    assert!(weather.sync(&preferences, true, 1300).0.is_none());
}

#[test]
fn caches_from_before_details_still_load() {
    let old: Cache = serde_json::from_value(json!({
        "key": place().key(), "updated_at": 1000,
        "reading": {"temperature": 14.2, "code": 2, "is_day": true}
    }))
    .unwrap();
    assert_eq!(old, cache(1000));
}

#[test]
fn weather_details_are_off_by_default_and_malformed_switches_stay_off() {
    let preferences: Preferences = serde_json::from_value(json!({
        "favorites": ["netflix"],
        "weather_high_low": true,
        "weather_air": "yes",
        "weather_hourly": 1
    }))
    .unwrap();
    assert_eq!(preferences.favorites, Some(vec!["netflix".into()]));
    assert!(preferences.weather_high_low);
    assert!(!preferences.weather_air && !preferences.weather_hourly);
    let defaults = Preferences::default();
    assert!((0..5).all(|index| !defaults.weather_detail(index)));
}
