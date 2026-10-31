//! Open-Meteo data and request scheduling. API/cache temperatures remain Celsius;
//! presentation units never change the request or invalidate a cached reading.
use crate::i18n::Language;
use crate::model::Preferences;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const REFRESH_SECONDS: i64 = 15 * 60;
pub const MAX_CACHE_AGE: i64 = 6 * 60 * 60;
const RETRY_SECONDS: i64 = 5 * 60;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Location {
    pub name: String,
    pub region: String,
    pub country: String,
    pub latitude: f64,
    pub longitude: f64,
}
impl Location {
    pub fn parse(value: &Value) -> Option<Self> {
        let latitude = value.get("latitude")?.as_f64()?;
        let longitude = value.get("longitude")?.as_f64()?;
        let name = label(value.get("name"));
        if name.is_empty()
            || !latitude.is_finite()
            || !longitude.is_finite()
            || latitude.abs() > 90.
            || longitude.abs() > 180.
        {
            return None;
        }
        Some(Self {
            name,
            region: label(value.get("region").or_else(|| value.get("admin1"))),
            country: label(value.get("country")),
            latitude,
            longitude,
        })
    }
    pub fn key(&self) -> String {
        format!(
            "{:.4},{:.4}",
            if self.latitude == 0. {
                0.
            } else {
                self.latitude
            },
            if self.longitude == 0. {
                0.
            } else {
                self.longitude
            }
        )
    }
    pub fn detail(&self) -> String {
        let mut parts = Vec::new();
        for part in [&self.region, &self.country] {
            if !part.is_empty() && part != &self.name && !parts.contains(&part.as_str()) {
                parts.push(part.as_str());
            }
        }
        parts.join(", ")
    }
}
fn label(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect()
}
pub fn deserialize_location<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Location>, D::Error> {
    Ok(Location::parse(&Value::deserialize(deserializer)?))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Celsius,
    Fahrenheit,
}
impl Unit {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Celsius => "°C",
            Self::Fahrenheit => "°F",
        }
    }
}
pub fn deserialize_unit<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Unit, D::Error> {
    Ok(
        if Value::deserialize(deserializer)?.as_str() == Some("fahrenheit") {
            Unit::Fahrenheit
        } else {
            Unit::Celsius
        },
    )
}
pub fn deserialize_enabled<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(Value::deserialize(deserializer)? != Value::Bool(false))
}
/// Opt-in switches: only `true` turns them on, and anything malformed leaves them off.
pub fn deserialize_flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(Value::deserialize(deserializer)? == Value::Bool(true))
}
pub fn format_temperature(celsius: f64, unit: Unit) -> String {
    format!("{}{}", degrees(celsius, unit), unit.symbol())
}
/// A temperature with just a degree sign, for compact places such as the hourly strip.
pub fn format_degrees(celsius: f64, unit: Unit) -> String {
    format!("{}°", degrees(celsius, unit))
}
fn degrees(celsius: f64, unit: Unit) -> i32 {
    let value = if unit == Unit::Fahrenheit {
        celsius * 9. / 5. + 32.
    } else {
        celsius
    };
    // Match JavaScript Math.round, including negative half-degree ties.
    (value + 0.5).floor() as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct Reading {
    pub temperature: f64,
    pub code: u8,
    pub is_day: bool,
}
impl Reading {
    pub fn valid(self) -> bool {
        self.temperature.is_finite()
            && (-100.0..=70.0).contains(&self.temperature)
            && self.code <= 99
    }
}
pub fn parse_reading(value: &Value) -> Result<Reading, String> {
    let current = &value["current"];
    let temperature = current["temperature_2m"].as_f64();
    let code = current["weather_code"]
        .as_f64()
        .filter(|n| n.fract() == 0. && (0.0..=99.0).contains(n));
    let day = current["is_day"].as_f64().filter(|n| *n == 0. || *n == 1.);
    if let (Some(temperature), Some(code), Some(day)) = (temperature, code, day) {
        let reading = Reading {
            temperature,
            code: code as u8,
            is_day: day == 1.,
        };
        if reading.valid() {
            return Ok(reading);
        }
    }
    Err("The weather service returned an unreadable reading.".into())
}
/// One hour of the forecast strip. Times are Unix seconds; `Details::offset` gives local time.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct Hour {
    pub time: i64,
    pub temperature: f64,
    pub code: u8,
    pub is_day: bool,
    /// Chance of precipitation in percent.
    pub rain: Option<u8>,
}

/// What Home can show beside the current reading. Every field is optional, so a partial reply
/// still shows what it has.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Details {
    /// The location's UTC offset in seconds, for its sunrise, sunset and hours.
    pub offset: i32,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
    pub uv: Option<f64>,
    pub hours: Vec<Hour>,
    /// European air quality index, when it was asked for and answered.
    pub air: Option<f64>,
    /// Whether air quality was asked for, so turning it on fetches once instead of waiting.
    pub air_requested: bool,
}
impl Details {
    /// The highest chance of rain in the three hours from `now`.
    pub fn rain_chance(&self, now: i64) -> Option<u8> {
        self.upcoming(now)
            .take_while(|hour| hour.time < now + 3 * 60 * 60)
            .filter_map(|hour| hour.rain)
            .max()
    }
    /// Hours from the current one on.
    pub fn upcoming(&self, now: i64) -> impl Iterator<Item = &Hour> {
        self.hours
            .iter()
            .filter(move |hour| hour.time + 60 * 60 > now)
    }
}

const MAX_HOURS: usize = 12;

fn temperature(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|t| t.is_finite() && (-100.0..=70.0).contains(t))
}

fn first(value: &Value, key: &str) -> Option<Value> {
    value[key]
        .as_array()
        .and_then(|values| values.first())
        .cloned()
}

/// Reads the daily, hourly and UV parts of a forecast reply. Anything missing or out of range is
/// left out rather than failing the reading.
pub fn parse_details(value: &Value) -> Details {
    let daily = &value["daily"];
    let hourly = &value["hourly"];
    let column = |key: &str| hourly[key].as_array().cloned().unwrap_or_default();
    let (times, temperatures, codes, days, rain) = (
        column("time"),
        column("temperature_2m"),
        column("weather_code"),
        column("is_day"),
        column("precipitation_probability"),
    );
    let hours = times
        .iter()
        .enumerate()
        .take(MAX_HOURS)
        .filter_map(|(index, time)| {
            Some(Hour {
                time: time.as_i64()?,
                temperature: temperature(temperatures.get(index)?)?,
                code: codes.get(index)?.as_u64().filter(|code| *code <= 99)? as u8,
                is_day: days.get(index)?.as_u64()? == 1,
                rain: rain
                    .get(index)
                    .and_then(Value::as_u64)
                    .filter(|chance| *chance <= 100)
                    .map(|chance| chance as u8),
            })
        })
        .collect();
    let time = |key: &str| first(daily, key).and_then(|value| value.as_i64());
    Details {
        offset: value["utc_offset_seconds"]
            .as_i64()
            .filter(|offset| offset.abs() <= 18 * 60 * 60)
            .unwrap_or(0) as i32,
        high: first(daily, "temperature_2m_max").and_then(|value| temperature(&value)),
        low: first(daily, "temperature_2m_min").and_then(|value| temperature(&value)),
        sunrise: time("sunrise"),
        sunset: time("sunset"),
        uv: value["current"]["uv_index"]
            .as_f64()
            .filter(|uv| uv.is_finite() && (0.0..=20.0).contains(uv)),
        hours,
        air: None,
        air_requested: false,
    }
}

/// The European air quality index from an air-quality reply.
pub fn parse_air(value: &Value) -> Option<f64> {
    value["current"]["european_aqi"]
        .as_f64()
        .filter(|aqi| aqi.is_finite() && (0.0..=500.0).contains(aqi))
}

/// The message key for a European air quality index.
pub fn air_key(aqi: f64) -> &'static str {
    match aqi {
        aqi if aqi < 20. => "airGood",
        aqi if aqi < 40. => "airFair",
        aqi if aqi < 60. => "airModerate",
        aqi if aqi < 80. => "airPoor",
        aqi if aqi < 100. => "airVeryPoor",
        _ => "airExtremelyPoor",
    }
}

/// A forecast reply: the current reading and what Home can show beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Forecast {
    pub reading: Reading,
    pub details: Details,
}

pub fn parse_locations(value: &Value) -> Result<Vec<Location>, String> {
    if !value.is_object()
        || value["error"] == true
        || value.get("results").is_some_and(|v| !v.is_array())
    {
        return Err("The location service returned an unreadable response.".into());
    }
    let mut seen = HashSet::new();
    Ok(value
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(20)
        .filter_map(Location::parse)
        .filter(|place| seen.insert(place.key()))
        .take(6)
        .collect())
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Cache {
    pub key: String,
    pub updated_at: i64,
    pub reading: Reading,
    /// Absent in caches written before details existed.
    #[serde(default)]
    pub details: Option<Details>,
}
impl Cache {
    pub fn age(&self, now: i64) -> Option<i64> {
        now.checked_sub(self.updated_at)
            .filter(|age| (0..=MAX_CACHE_AGE).contains(age) && self.reading.valid())
    }
}

pub struct Condition {
    pub key: &'static str,
    pub icon: &'static str,
}
pub fn condition(reading: Reading) -> Condition {
    let (key, icon) = match reading.code {
        0 if reading.is_day => ("clearSkies", "sun"),
        0 => ("clearNight", "moon"),
        1 => ("mostlyClear", "partly-cloudy"),
        2 => ("partlyCloudy", "partly-cloudy"),
        3 => ("overcast", "cloud"),
        45 | 48 => ("fog", "fog"),
        51 | 53 | 55 | 56 | 57 => ("drizzle", "drizzle"),
        61 | 63 | 65 | 66 | 67 | 80 | 81 | 82 => ("rain", "rain"),
        71 | 73 | 75 | 77 | 85 | 86 => ("snow", "snow"),
        95 | 96 | 99 => ("thunderstorms", "storm"),
        _ => ("currentWeather", "cloud"),
    };
    Condition { key, icon }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Search {
        id: u64,
        query: String,
        language: Language,
    },
    Forecast {
        id: u64,
        location: Location,
        /// Also ask for air quality.
        air: bool,
    },
    Persist(Cache),
    ClearCache,
}

#[derive(Default, Serialize)]
pub struct Weather {
    pub query: String,
    pub results: Vec<Location>,
    pub searching: bool,
    pub message: Option<String>,
    pub cache: Option<Cache>,
    pub failed: bool,
    pub loading: bool,
    #[serde(skip)]
    search_id: u64,
    #[serde(skip)]
    forecast_id: u64,
    #[serde(skip)]
    active_key: Option<String>,
    #[serde(skip)]
    pending: Option<(u64, String)>,
    #[serde(skip)]
    next_fetch: i64,
    #[serde(skip)]
    active_home: bool,
}
impl Weather {
    pub fn generations(&self) -> (u64, u64) {
        (self.search_id, self.forecast_id)
    }
    pub fn fetching(&self) -> bool {
        self.pending.is_some()
    }
    pub fn edit(&mut self, query: String) {
        self.query = query
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect();
        self.cancel_search();
    }
    pub fn cancel_search(&mut self) {
        self.search_id += 1;
        self.results.clear();
        self.searching = false;
        self.message = None;
    }
    pub fn search(&mut self, language: Language) -> Option<Command> {
        if self.searching {
            return None;
        }
        self.cancel_search();
        let query = self.query.trim().to_owned();
        if query.chars().count() < 2 {
            self.message = Some("enterCity".into());
            return None;
        }
        self.searching = true;
        Some(Command::Search {
            id: self.search_id,
            query,
            language,
        })
    }
    pub fn accept_search(&mut self, id: u64, result: Result<Vec<Location>, String>) -> bool {
        if id != self.search_id || !self.searching {
            return false;
        }
        self.searching = false;
        match result {
            Ok(places) => {
                self.results = places;
                if self.results.is_empty() {
                    self.message = Some("noLocations".into());
                }
            }
            Err(_) => self.message = Some("searchUnavailable".into()),
        }
        true
    }
    pub fn invalidate_forecast(&mut self) {
        self.forecast_id += 1;
        self.pending = None;
        self.active_key = None;
        self.next_fetch = 0;
        self.loading = false;
        self.failed = false;
    }
    pub fn remove_location(&mut self) {
        self.cancel_search();
        self.invalidate_forecast();
        self.cache = None;
    }
    pub fn details(&self, location: &Location) -> Option<&Details> {
        self.cache
            .as_ref()
            .filter(|cache| cache.key == location.key())
            .and_then(|cache| cache.details.as_ref())
    }
    pub fn reading(&self, location: &Location) -> Option<Reading> {
        self.cache
            .as_ref()
            .filter(|cache| cache.key == location.key())
            .map(|cache| cache.reading)
    }
    pub fn sync(
        &mut self,
        preferences: &Preferences,
        home_visible: bool,
        now: i64,
    ) -> (Option<Command>, bool) {
        let before = (
            self.loading,
            self.failed,
            self.cache.as_ref().map(|c| (c.updated_at, c.reading)),
        );
        if self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.age(now).is_none())
        {
            self.cache = None;
        }
        let key = preferences.weather_location.as_ref().map(Location::key);
        if self.active_key != key {
            self.invalidate_forecast();
            self.active_key = key.clone();
        }
        let active = preferences.show_weather && home_visible && key.is_some();
        if active && !self.active_home {
            self.next_fetch = 0;
        }
        self.active_home = active;
        let mut command = None;
        if !preferences.show_weather || !home_visible || key.is_none() {
            if self.pending.take().is_some() {
                self.forecast_id += 1;
            }
            self.loading = false;
        } else if let Some(cache) = self.cache.as_ref().filter(|c| {
            Some(&c.key) == key.as_ref()
                && c.age(now).is_some_and(|age| age < REFRESH_SECONDS)
                && has_details(preferences, c)
        }) {
            self.loading = false;
            self.failed = false;
            self.next_fetch = cache.updated_at + REFRESH_SECONDS;
        } else if self.pending.is_none() && now >= self.next_fetch {
            self.forecast_id += 1;
            self.pending = key.clone().map(|key| (self.forecast_id, key));
            self.loading = self
                .cache
                .as_ref()
                .is_none_or(|cache| Some(&cache.key) != key.as_ref());
            command = preferences
                .weather_location
                .clone()
                .map(|location| Command::Forecast {
                    id: self.forecast_id,
                    location,
                    air: preferences.weather_air,
                });
        }
        if active && self.pending.is_some() {
            self.loading = self
                .cache
                .as_ref()
                .is_none_or(|cache| Some(&cache.key) != key.as_ref());
        }
        let changed = before
            != (
                self.loading,
                self.failed,
                self.cache.as_ref().map(|c| (c.updated_at, c.reading)),
            );
        (command, changed)
    }
    pub fn accept_forecast(
        &mut self,
        id: u64,
        key: &str,
        result: Result<Forecast, String>,
        now: i64,
    ) -> (bool, Option<Command>) {
        if self
            .pending
            .as_ref()
            .is_none_or(|(pending, location)| *pending != id || location != key)
        {
            return (false, None);
        }
        self.pending = None;
        self.loading = false;
        match result {
            Ok(Forecast { reading, details }) if reading.valid() => {
                let cache = Cache {
                    key: key.into(),
                    updated_at: now,
                    reading,
                    details: Some(details),
                };
                self.cache = Some(cache.clone());
                self.failed = false;
                self.next_fetch = now + REFRESH_SECONDS;
                (true, Some(Command::Persist(cache)))
            }
            _ => {
                self.failed = true;
                self.next_fetch = now + RETRY_SECONDS;
                if self
                    .cache
                    .as_ref()
                    .is_some_and(|cache| cache.age(now).is_none())
                {
                    self.cache = None;
                }
                (true, None)
            }
        }
    }
}

/// Whether a cache answers everything Home is set to show, so turning a detail on fetches once
/// instead of waiting for the next refresh.
fn has_details(preferences: &Preferences, cache: &Cache) -> bool {
    let wanted = preferences.weather_high_low
        || preferences.weather_air
        || preferences.weather_rain
        || preferences.weather_sun
        || preferences.weather_hourly;
    !wanted
        || cache
            .details
            .as_ref()
            .is_some_and(|details| !preferences.weather_air || details.air_requested)
}

#[cfg(test)]
#[path = "weather-tests.rs"]
mod tests;
