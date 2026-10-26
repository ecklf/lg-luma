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
    /// Open-Meteo's id for the place, which looks it up in another language. Places saved
    /// before it was kept have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
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
            id: value.get("id").and_then(Value::as_u64),
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
pub fn format_temperature(celsius: f64, unit: Unit) -> String {
    let value = if unit == Unit::Fahrenheit {
        celsius * 9. / 5. + 32.
    } else {
        celsius
    };
    // Match JavaScript Math.round, including negative half-degree ties.
    format!("{}{}", (value + 0.5).floor() as i32, unit.symbol())
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
    /// The saved place's names in `to`, found from its name in `from`.
    Rename {
        id: u64,
        location: Location,
        from: Language,
        to: Language,
    },
    Forecast {
        id: u64,
        location: Location,
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
    #[serde(skip)]
    rename_id: u64,
    /// The pending lookup of the saved place in a new interface language: its id, the
    /// place's key, and the language its saved name is in.
    #[serde(skip)]
    rename: Option<(u64, String, Language)>,
}
impl Weather {
    pub fn generations(&self) -> (u64, u64, u64) {
        (self.search_id, self.forecast_id, self.rename_id)
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
    /// Looks the saved place up again, so its name follows the interface language. Separate
    /// from the city search, so leaving Settings does not drop it. A newer rename replaces a
    /// pending one; the saved name is still in the language before both.
    pub fn rename(&mut self, location: &Location, from: Language, to: Language) -> Command {
        let from = self.rename.as_ref().map_or(from, |(_, _, from)| *from);
        self.cancel_rename();
        self.rename = Some((self.rename_id, location.key(), from));
        Command::Rename {
            id: self.rename_id,
            location: location.clone(),
            from,
            to,
        }
    }
    /// Drops a pending rename, whose reply is then ignored.
    pub fn cancel_rename(&mut self) {
        self.rename_id += 1;
        self.rename = None;
    }
    /// The result of the current `rename`: the place, if found at exactly the coordinates it
    /// was asked for. A failed lookup (offline) keeps the saved names.
    pub fn accept_rename(
        &mut self,
        id: u64,
        result: Result<Option<Location>, String>,
    ) -> Option<Location> {
        let current = self.rename_id;
        let (_, key, _) = self
            .rename
            .take_if(|(rename_id, ..)| *rename_id == id && id == current)?;
        result.ok().flatten().filter(|place| place.key() == key)
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
        self.cancel_rename();
        self.invalidate_forecast();
        self.cache = None;
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
            Some(&c.key) == key.as_ref() && c.age(now).is_some_and(|age| age < REFRESH_SECONDS)
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
        result: Result<Reading, String>,
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
            Ok(reading) if reading.valid() => {
                let cache = Cache {
                    key: key.into(),
                    updated_at: now,
                    reading,
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

#[cfg(test)]
#[path = "weather-tests.rs"]
mod tests;
