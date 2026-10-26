use super::*;
use serde_json::json;

fn place() -> Location {
    Location {
        name: "Vienna".into(),
        region: "Vienna".into(),
        country: "Austria".into(),
        latitude: 48.2085,
        longitude: 16.3721,
        id: None,
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
    let Some(Command::Forecast { id, location }) = weather.sync(&preferences, true, 2000).0 else {
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
    let Some(Command::Forecast { id, location }) = weather.sync(&preferences, true, 1000).0 else {
        panic!();
    };
    weather.remove_location();
    preferences.weather_location = None;
    assert_eq!(
        weather.accept_forecast(id, &location.key(), Ok(reading()), 1001),
        (false, None)
    );
    assert!(weather.cache.is_none());
    preferences.weather_location = Some(place());
    let Some(Command::Forecast { id, location }) = weather.sync(&preferences, true, 1002).0 else {
        panic!();
    };
    preferences.show_weather = false;
    weather.sync(&preferences, true, 1003);
    assert_eq!(
        weather.accept_forecast(id, &location.key(), Ok(reading()), 1004),
        (false, None)
    );
    preferences.show_weather = true;
    let Some(Command::Forecast {
        id: old,
        location: old_location,
    }) = weather.sync(&preferences, true, 1005).0
    else {
        panic!();
    };
    preferences.weather_location.as_mut().unwrap().latitude += 1.;
    let Some(Command::Forecast {
        id: new,
        location: new_location,
    }) = weather.sync(&preferences, true, 1006).0
    else {
        panic!();
    };
    assert_eq!(
        weather.accept_forecast(old, &old_location.key(), Ok(reading()), 1007),
        (false, None)
    );
    assert!(
        weather
            .accept_forecast(new, &new_location.key(), Ok(reading()), 1007)
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
    let Some(Command::Forecast { id, location }) = weather.sync(&preferences, true, 2000).0 else {
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
