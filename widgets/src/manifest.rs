//! A widget's manifest (docs/widgets.md §3): who it is, where it may connect, its settings and
//! sources. Parsing is strict: an unknown field or a broken rule is an error, so a mistake is
//! caught when the widget is checked rather than on a TV.
use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const MANIFEST_LIMIT: usize = 256 * 1024;
pub const PROGRAM_LIMIT: usize = 64 * 1024;
pub const REPLY_LIMIT: usize = 1024 * 1024;
const LANGUAGES: [&str; 7] = ["en", "de", "zh", "es", "pt", "it", "ru"];
/// Luma's bundled UI icons a widget may show in the store.
pub const ICONS: [&str; 14] = [
    "alert",
    "clock",
    "coffee",
    "gaming",
    "generic",
    "globe",
    "grid",
    "heart",
    "location",
    "sparkles",
    "thermometer",
    "tv",
    "weather-sun",
    "weather-cloud",
];
/// Headers a widget may not set.
const RESERVED_HEADERS: [&str; 7] = [
    "host",
    "cookie",
    "connection",
    "content-length",
    "transfer-encoding",
    "proxy-authorization",
    "upgrade",
];

/// Text in Luma's languages; English is required and stands in for a missing language.
#[derive(Clone, Debug, PartialEq)]
pub struct Localized(pub BTreeMap<String, String>);

impl Localized {
    pub fn get(&self, language: &str) -> &str {
        self.0
            .get(language)
            .or_else(|| self.0.get("en"))
            .map_or("", String::as_str)
    }

    fn from_value(value: Value, limit: usize, what: &str) -> Result<Self, Error> {
        let map: BTreeMap<String, String> =
            serde_json::from_value(value).map_err(|e| Error(format!("{what}: {e}")))?;
        if !map.contains_key("en") {
            return Err(Error(format!("{what}: English (\"en\") is required")));
        }
        for (language, text) in &map {
            if !LANGUAGES.contains(&language.as_str()) {
                return Err(Error(format!("{what}: unknown language \"{language}\"")));
            }
            if text.trim().is_empty() || text.chars().count() > limit {
                return Err(Error(format!("{what} ({language}): 1–{limit} characters")));
            }
        }
        Ok(Self(map))
    }
}

impl Serialize for Localized {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Localized {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(deserializer)?, 400, "text")
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.').map(|part| {
            (!part.is_empty() && part.len() <= 6 && part.bytes().all(|b| b.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        });
        let version = Self(parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("bad version {text}")))
    }
}

/// `https://host` or `https://host:port`: no path, wildcard, IP literal or localhost.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Origin(String);

impl Origin {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let bad = || Error(format!("origin \"{text}\" must be https://host[:port]"));
        let authority = text.strip_prefix("https://").ok_or_else(bad)?;
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        let label_ok = |label: &str| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        let labels: Vec<&str> = host.split('.').collect();
        let numeric = labels.iter().all(|l| l.bytes().all(|b| b.is_ascii_digit()));
        if labels.len() < 2 || !labels.iter().all(|l| label_ok(l)) || numeric || host.len() > 253 {
            return Err(bad());
        }
        if host == "localhost" || host.ends_with(".localhost") {
            return Err(bad());
        }
        match port {
            None => Ok(Self(format!("https://{host}"))),
            Some("443") => Ok(Self(format!("https://{host}"))),
            Some(port) if port.parse::<u16>().is_ok_and(|p| p > 0) && !port.starts_with('0') => {
                Ok(Self(format!("https://{host}:{port}")))
            }
            Some(_) => Err(bad()),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The origin of a full URL, if it has a valid one (no user info).
    pub fn of(url: &str) -> Option<Self> {
        let rest = url.strip_prefix("https://")?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..end];
        if authority.contains('@') {
            return None;
        }
        Self::parse(&format!("https://{authority}")).ok()
    }

    /// Whether `url` is on this origin.
    pub fn allows(&self, url: &str) -> bool {
        Self::of(url).as_ref() == Some(self)
    }
}

impl Serialize for Origin {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Origin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    Scoreboard,
    Media,
    Ticker,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub default: [u8; 2],
    pub min: [u8; 2],
    pub max: [u8; 2],
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub value: String,
    pub label: Localized,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingKind {
    Toggle {
        default: bool,
    },
    Choice {
        options: Vec<Choice>,
        default: String,
    },
    Choices {
        options: Vec<Choice>,
        default: Vec<String>,
        min: usize,
        max: usize,
    },
    Text {
        default: String,
        max_length: usize,
        pattern: Option<String>,
    },
    Secret {
        send_to: Vec<Origin>,
        max_length: usize,
        pattern: Option<String>,
        hint: Option<Localized>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub id: String,
    pub label: Localized,
    pub kind: SettingKind,
}

impl Setting {
    /// Whether `value` may be saved for this text or secret setting.
    pub fn accepts(&self, value: &str) -> bool {
        let (max_length, pattern) = match &self.kind {
            SettingKind::Text {
                max_length,
                pattern,
                ..
            }
            | SettingKind::Secret {
                max_length,
                pattern,
                ..
            } => (*max_length, pattern),
            _ => return false,
        };
        value.chars().count() <= max_length
            && !value.chars().any(char::is_control)
            && pattern.as_ref().is_none_or(|pattern| {
                regex_lite::Regex::new(pattern).is_ok_and(|re| re.is_match(value))
            })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum RawSetting {
    Toggle {
        id: String,
        label: Localized,
        default: bool,
    },
    Choice {
        id: String,
        label: Localized,
        options: Vec<Choice>,
        default: String,
    },
    Choices {
        id: String,
        label: Localized,
        options: Vec<Choice>,
        default: Vec<String>,
        min: Option<usize>,
        max: Option<usize>,
    },
    Text {
        id: String,
        label: Localized,
        #[serde(default)]
        default: String,
        max_length: Option<usize>,
        pattern: Option<String>,
    },
    Secret {
        id: String,
        label: Localized,
        send_to: Vec<Origin>,
        max_length: Option<usize>,
        pattern: Option<String>,
        hint: Option<Localized>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    Secret(String),
    NoSecret(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum RawWhen {
    Secret(String),
    NoSecret(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refresh {
    #[serde(default = "default_every")]
    pub every: u64,
    #[serde(default = "default_min")]
    pub min: u64,
}
fn default_every() -> u64 {
    300
}
fn default_min() -> u64 {
    30
}
impl Default for Refresh {
    fn default() -> Self {
        Self {
            every: default_every(),
            min: default_min(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub for_each: Option<String>,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
    pub max_age: Option<u64>,
    #[serde(default = "yes")]
    pub required: bool,
    #[serde(default = "reply_limit")]
    pub limit: usize,
}
fn yes() -> bool {
    true
}
fn reply_limit() -> usize {
    REPLY_LIMIT
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub id: String,
    pub when: When,
    pub refresh: Refresh,
    pub requests: Vec<Step>,
    pub transform: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSource {
    id: String,
    when: Option<RawWhen>,
    #[serde(default)]
    refresh: Refresh,
    #[serde(default)]
    requests: Vec<Step>,
    transform: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema: u32,
    id: String,
    version: Version,
    luma: Version,
    name: Value,
    description: Value,
    #[serde(default)]
    authors: Vec<String>,
    icon: String,
    origins: Vec<Origin>,
    size: Size,
    #[serde(default = "one")]
    instances: u8,
    card: CardKind,
    #[serde(default)]
    strings: BTreeMap<String, Localized>,
    #[serde(default)]
    settings: Vec<RawSetting>,
    #[serde(default)]
    library: Value,
    sources: Vec<RawSource>,
}
fn one() -> u8 {
    1
}

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub version: Version,
    pub luma: Version,
    pub name: Localized,
    pub description: Localized,
    pub authors: Vec<String>,
    pub icon: String,
    pub origins: Vec<Origin>,
    pub size: Size,
    pub instances: u8,
    pub card: CardKind,
    pub strings: BTreeMap<String, Localized>,
    pub settings: Vec<Setting>,
    pub library: String,
    pub sources: Vec<Source>,
}

/// What a widget may reach: its origins and where its secrets go. An update that widens it
/// needs the user's consent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    pub origins: BTreeSet<String>,
    pub secret_destinations: BTreeSet<String>,
}

impl Access {
    pub fn widens(&self, newer: &Access) -> bool {
        !newer.origins.is_subset(&self.origins)
            || !newer
                .secret_destinations
                .is_subset(&self.secret_destinations)
    }
}

fn identifier(text: &str, max: usize) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && text.len() <= max
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn program(value: Value, what: &str, allow_empty: bool) -> Result<String, Error> {
    match value {
        Value::Null if allow_empty => Ok(String::new()),
        Value::String(text) if text.len() <= PROGRAM_LIMIT => Ok(text),
        Value::String(_) => Err(Error(format!("{what} is over 64 KiB"))),
        Value::Object(map) if map.contains_key("file") => Err(Error(format!(
            "{what} names a file: pack the widget with luma-widget pack"
        ))),
        _ => Err(Error(format!("{what} must be a jq program"))),
    }
}

/// Option values of a choice setting: 2–16 distinct, 1–64 characters each.
fn check_options<'a>(id: &str, options: &'a [Choice]) -> Result<BTreeSet<&'a str>, Error> {
    let values: BTreeSet<_> = options.iter().map(|o| o.value.as_str()).collect();
    if !(2..=16).contains(&options.len())
        || values.len() != options.len()
        || options
            .iter()
            .any(|o| o.value.is_empty() || o.value.len() > 64)
    {
        return Err(Error(format!(
            "setting {id}: 2–16 distinct options, values of 1–64 characters"
        )));
    }
    Ok(values)
}

/// The placeholders of a template, without braces; an unclosed brace is an error.
pub fn placeholders(template: &str) -> Result<Vec<&str>, Error> {
    let mut found = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let end = after
            .find('}')
            .ok_or_else(|| Error(format!("unclosed {{ in \"{template}\"")))?;
        found.push(&after[..end]);
        rest = &after[end + 1..];
    }
    if rest.contains('}') {
        return Err(Error(format!("stray }} in \"{template}\"")));
    }
    Ok(found)
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MANIFEST_LIMIT {
            return Err(Error("the manifest is over 256 KiB".into()));
        }
        let raw: RawManifest =
            serde_json::from_slice(bytes).map_err(|e| Error(format!("manifest: {e}")))?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: RawManifest) -> Result<Self, Error> {
        let fail = |message: String| Err(Error(message));
        if raw.schema != 1 {
            return fail(format!("schema {} is not supported", raw.schema));
        }
        let id_ok = raw.id.len() >= 2
            && raw.id.len() <= 32
            && raw.id.starts_with(|c: char| c.is_ascii_lowercase())
            && raw
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            return fail(format!(
                "id \"{}\" must match [a-z][a-z0-9-]{{1,31}}",
                raw.id
            ));
        }
        let name = Localized::from_value(raw.name, 24, "name")?;
        let description = Localized::from_value(raw.description, 140, "description")?;
        if !ICONS.contains(&raw.icon.as_str()) {
            return fail(format!("icon \"{}\" is not one of Luma's", raw.icon));
        }
        let origins = raw.origins;
        let unique: BTreeSet<_> = origins.iter().collect();
        if origins.is_empty() || origins.len() > 8 || unique.len() != origins.len() {
            return fail("1–8 distinct origins".into());
        }
        let size = raw.size;
        for cells in [size.default, size.min, size.max] {
            if !(4..=12).contains(&cells[0]) || !(1..=3).contains(&cells[1]) {
                return fail("sizes are 4–12 cells wide and 1–3 tall".into());
            }
        }
        if (0..2).any(|i| size.min[i] > size.default[i] || size.default[i] > size.max[i]) {
            return fail("size: min ≤ default ≤ max".into());
        }
        if !(1..=4).contains(&raw.instances) {
            return fail("instances: 1–4".into());
        }
        if raw.strings.len() > 200
            || raw.strings.keys().any(|key| {
                !key.starts_with(|c: char| c.is_ascii_lowercase())
                    || key.len() > 64
                    || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            return fail("strings: at most 200, ids like \"mapNumber\"".into());
        }
        let settings = raw
            .settings
            .into_iter()
            .map(|raw| Self::parse_setting(raw, &origins))
            .collect::<Result<Vec<_>, _>>()?;
        if settings.len() > 12 {
            return fail("at most 12 settings".into());
        }
        let ids: BTreeSet<_> = settings.iter().map(|s| s.id.as_str()).collect();
        if ids.len() != settings.len() {
            return fail("setting ids must be unique".into());
        }
        let library = program(raw.library, "library", true)?;
        if raw.sources.is_empty() || raw.sources.len() > 4 {
            return fail("1–4 sources".into());
        }
        let mut sources = Vec::new();
        for raw in raw.sources {
            sources.push(Self::source(raw, &settings, &origins)?);
        }
        let ids: BTreeSet<_> = sources.iter().map(|s| s.id.as_str()).collect();
        if ids.len() != sources.len() {
            return fail("source ids must be unique".into());
        }
        if raw.authors.len() > 8 || raw.authors.iter().any(|a| a.is_empty() || a.len() > 39) {
            return fail("authors: up to 8 GitHub logins".into());
        }
        Ok(Self {
            schema: raw.schema,
            id: raw.id,
            version: raw.version,
            luma: raw.luma,
            name,
            description,
            authors: raw.authors,
            icon: raw.icon,
            origins,
            size,
            instances: raw.instances,
            card: raw.card,
            strings: raw.strings,
            settings,
            library,
            sources,
        })
    }

    fn parse_setting(raw: RawSetting, origins: &[Origin]) -> Result<Setting, Error> {
        let check_pattern = |id: &str, pattern: &Option<String>| match pattern {
            Some(pattern) if regex_lite::Regex::new(pattern).is_err() => {
                Err(Error(format!("setting {id}: bad pattern")))
            }
            _ => Ok(()),
        };
        let (id, label, kind) = match raw {
            RawSetting::Toggle { id, label, default } => {
                (id, label, SettingKind::Toggle { default })
            }
            RawSetting::Choice {
                id,
                label,
                options,
                default,
            } => {
                if !check_options(&id, &options)?.contains(default.as_str()) {
                    return Err(Error(format!("setting {id}: default is not an option")));
                }
                (id, label, SettingKind::Choice { options, default })
            }
            RawSetting::Choices {
                id,
                label,
                options,
                default,
                min,
                max,
            } => {
                let values = check_options(&id, &options)?;
                let (min, max) = (min.unwrap_or(1), max.unwrap_or(options.len()));
                let chosen: BTreeSet<_> = default.iter().map(String::as_str).collect();
                if !chosen.is_subset(&values)
                    || chosen.len() != default.len()
                    || min > max
                    || max > options.len()
                    || !(min..=max).contains(&default.len())
                {
                    return Err(Error(format!(
                        "setting {id}: defaults must be min–max distinct options"
                    )));
                }
                (
                    id,
                    label,
                    SettingKind::Choices {
                        options,
                        default,
                        min,
                        max,
                    },
                )
            }
            RawSetting::Text {
                id,
                label,
                default,
                max_length,
                pattern,
            } => {
                let max_length = max_length.unwrap_or(64);
                check_pattern(&id, &pattern)?;
                if max_length > 64 || default.chars().count() > max_length {
                    return Err(Error(format!("setting {id}: at most 64 characters")));
                }
                (
                    id,
                    label,
                    SettingKind::Text {
                        default,
                        max_length,
                        pattern,
                    },
                )
            }
            RawSetting::Secret {
                id,
                label,
                send_to,
                max_length,
                pattern,
                hint,
            } => {
                check_pattern(&id, &pattern)?;
                let max_length = max_length.unwrap_or(128);
                if send_to.is_empty() || send_to.iter().any(|o| !origins.contains(o)) {
                    return Err(Error(format!(
                        "setting {id}: send_to must list some of the widget's origins"
                    )));
                }
                if max_length == 0 || max_length > 256 {
                    return Err(Error(format!("setting {id}: max_length 1–256")));
                }
                (
                    id,
                    label,
                    SettingKind::Secret {
                        send_to,
                        max_length,
                        pattern,
                        hint,
                    },
                )
            }
        };
        if !identifier(&id, 32) {
            return Err(Error(format!(
                "setting id \"{id}\" must match [a-z][a-z0-9_]*"
            )));
        }
        Ok(Setting { id, label, kind })
    }

    fn source(raw: RawSource, settings: &[Setting], origins: &[Origin]) -> Result<Source, Error> {
        let id = raw.id;
        if !identifier(&id, 32) {
            return Err(Error(format!(
                "source id \"{id}\" must match [a-z][a-z0-9_]*"
            )));
        }
        let secret = |name: &str| {
            settings
                .iter()
                .find(|s| s.id == name && matches!(s.kind, SettingKind::Secret { .. }))
        };
        let when = match raw.when {
            None => When::Always,
            Some(RawWhen::Secret(name)) | Some(RawWhen::NoSecret(name))
                if secret(&name).is_none() =>
            {
                return Err(Error(format!("source {id}: \"{name}\" is not a secret")));
            }
            Some(RawWhen::Secret(name)) => When::Secret(name),
            Some(RawWhen::NoSecret(name)) => When::NoSecret(name),
        };
        let refresh = raw.refresh;
        if refresh.min < 30 || refresh.min > refresh.every || refresh.every > 3600 {
            return Err(Error(format!(
                "source {id}: 30 ≤ refresh.min ≤ refresh.every ≤ 3600"
            )));
        }
        let transform = program(raw.transform, &format!("source {id}'s transform"), false)?;
        if raw.requests.len() > 8 {
            return Err(Error(format!("source {id}: at most 8 requests")));
        }
        let mut seen = BTreeSet::new();
        for step in &raw.requests {
            Self::step(step, settings, origins, &id)?;
            if !seen.insert(step.id.as_str()) {
                return Err(Error(format!("source {id}: step ids must be unique")));
            }
        }
        Ok(Source {
            id,
            when,
            refresh,
            requests: raw.requests,
            transform,
        })
    }

    fn step(
        step: &Step,
        settings: &[Setting],
        origins: &[Origin],
        source: &str,
    ) -> Result<(), Error> {
        let fail = |message: &str| Err(Error(format!("{source}.{}: {message}", step.id)));
        if !identifier(&step.id, 32) {
            return fail("step ids must match [a-z][a-z0-9_]*");
        }
        if let Some(name) = &step.for_each
            && !settings
                .iter()
                .any(|s| &s.id == name && matches!(s.kind, SettingKind::Choices { .. }))
        {
            return fail("for_each must name a choices setting");
        }
        // The origin is literal text, and one of the widget's.
        let Some(rest) = step.url.strip_prefix("https://") else {
            return fail("the URL must start with https://");
        };
        let host_end = rest.find(['/', '?']).unwrap_or(rest.len());
        if rest[..host_end].contains(['{', '}']) {
            return fail("the URL's origin must be literal");
        }
        let origin = match Origin::of(&step.url) {
            Some(origin) if origins.contains(&origin) => origin,
            _ => return fail("the URL is not on one of the widget's origins"),
        };
        if step.vars.len() > 8
            || step
                .vars
                .iter()
                .any(|(name, program)| !identifier(name, 32) || program.trim().is_empty())
        {
            return fail("vars: up to 8 named jq expressions");
        }
        let known = |name: &str, header: bool| -> bool {
            if name == "item" {
                return step.for_each.is_some();
            }
            if let Some(var) = name.strip_prefix("var.") {
                return step.vars.contains_key(var);
            }
            if let Some(setting) = name.strip_prefix("settings.") {
                return settings
                    .iter()
                    .any(|s| s.id == setting && !matches!(s.kind, SettingKind::Secret { .. }));
            }
            if let Some(secret) = name.strip_prefix("secret.") {
                return header
                    && settings.iter().any(|s| {
                        s.id == secret
                            && matches!(&s.kind, SettingKind::Secret { send_to, .. } if send_to.contains(&origin))
                    });
            }
            false
        };
        for name in placeholders(&step.url)? {
            if !known(name, false) {
                return fail(&format!(
                    "unknown or forbidden placeholder {{{name}}} in the URL"
                ));
            }
        }
        if step.headers.len() > 8 {
            return fail("at most 8 headers");
        }
        for (header, value) in &step.headers {
            let token = !header.is_empty()
                && header
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-');
            if !token || RESERVED_HEADERS.contains(&header.to_ascii_lowercase().as_str()) {
                return fail(&format!("header {header} is not allowed"));
            }
            for name in placeholders(value)? {
                if !known(name, true) {
                    return fail(&format!(
                        "unknown placeholder {{{name}}} in {header}, or a secret sent outside its send_to"
                    ));
                }
            }
        }
        if step.max_age.is_some_and(|age| age > 86_400) {
            return fail("max_age is at most a day");
        }
        if step.limit == 0 || step.limit > REPLY_LIMIT {
            return fail("limit: 1 B – 1 MiB");
        }
        Ok(())
    }

    /// The instance's saved values checked against the settings: anything unknown or no longer
    /// valid is the default. Secrets are not settings values.
    pub fn settings_values(&self, saved: &Map<String, Value>) -> Map<String, Value> {
        let mut values = Map::new();
        for setting in &self.settings {
            let saved = saved.get(&setting.id);
            let value = match &setting.kind {
                SettingKind::Secret { .. } => continue,
                SettingKind::Toggle { default } => {
                    Value::Bool(saved.and_then(Value::as_bool).unwrap_or(*default))
                }
                SettingKind::Choice { options, default } => {
                    let saved = saved.and_then(Value::as_str);
                    Value::from(
                        saved
                            .filter(|v| options.iter().any(|o| o.value == *v))
                            .unwrap_or(default),
                    )
                }
                SettingKind::Choices {
                    options,
                    default,
                    min,
                    max,
                } => {
                    let present = saved.is_some_and(Value::is_array);
                    let saved: Vec<&str> = saved
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    let chosen: Vec<&str> = options
                        .iter()
                        .map(|o| o.value.as_str())
                        .filter(|v| saved.contains(v))
                        .collect();
                    // An option the widget dropped is dropped; what is left must still do.
                    let explicit_none = present && saved.is_empty() && *min == 0;
                    if (*min..=*max).contains(&chosen.len())
                        && (!chosen.is_empty() || explicit_none)
                    {
                        Value::from(chosen)
                    } else {
                        Value::from(default.clone())
                    }
                }
                SettingKind::Text { default, .. } => Value::from(
                    saved
                        .and_then(Value::as_str)
                        .filter(|v| setting.accepts(v))
                        .unwrap_or(default),
                ),
            };
            values.insert(setting.id.clone(), value);
        }
        values
    }

    pub fn access(&self) -> Access {
        Access {
            origins: self.origins.iter().map(|o| o.0.clone()).collect(),
            secret_destinations: self
                .settings
                .iter()
                .filter_map(|s| match &s.kind {
                    SettingKind::Secret { send_to, .. } => Some(send_to),
                    _ => None,
                })
                .flatten()
                .map(|o| o.0.clone())
                .collect(),
        }
    }

    pub fn setting(&self, id: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.id == id)
    }
}
