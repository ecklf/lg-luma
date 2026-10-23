use crate::i18n::Language;
use crate::weather::{Location, Unit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const CATEGORIES: [&str; 4] = ["Streaming", "Music", "Gaming", "Utilities"];
pub const BACKGROUNDS: [(&str, &str); 9] = [
    ("slate", "Slate"),
    ("midnight", "Midnight"),
    ("prism", "Prism"),
    ("dunes", "Monument Valley"),
    ("san-francisco", "San Francisco"),
    ("summit", "Summit"),
    ("forest", "Forest"),
    ("yosemite", "Yosemite"),
    ("northern-lights", "Northern lights"),
];

// Keyword matching for brand icons: a needle only counts where the bytes
// immediately around it aren't also ASCII alphanumeric, so "ard" doesn't
// collide with unrelated apps like "dashboard". Any multi-byte UTF-8 byte
// (title text in another script) is never ASCII-alphanumeric either, so it
// reads as a boundary too; matching stays on raw bytes, not chars, which is
// safe here since only the two single bytes right at the needle's edges are
// ever inspected.
//
// A regex crate (`\bard\b` per needle) was the obvious alternative and isn't
// ruled out for being slow - reused compiled patterns would match at a
// comparable cost. It's skipped because: there's no regex dependency in this
// crate yet, so adding one and the once-compiled-pattern machinery needed to
// avoid recompiling per App::new() call is more than this fix needs; and
// `\b` is Unicode-aware by default, treating CJK letters as word characters,
// which would give different (not obviously better) results right next to
// titles in another script than the plain "any non-ASCII byte is a
// boundary" rule used here.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(needle).any(|(start, matched)| {
        let end = start + matched.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        before_ok && after_ok
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct App {
    pub id: String,
    pub title: String,
    pub brand: String,
    pub category: String,
}

impl App {
    pub fn new(id: &str, title: &str) -> Self {
        let identity = format!("{id} {title}").to_lowercase();
        let (brand, category) = [
            ("livetv", "live-tv", "Streaming"),
            ("netflix", "netflix", "Streaming"),
            ("youtube", "youtube", "Streaming"),
            ("appletv", "apple-tv", "Streaming"),
            ("disney", "disney-plus", "Streaming"),
            ("amazon", "prime-video", "Streaming"),
            ("jellyfin", "jellyfin", "Streaming"),
            ("arte", "arte", "Streaming"),
            ("zdf", "zdf", "Streaming"),
            ("ard", "das-erste", "Streaming"),
            ("plex", "plex", "Streaming"),
            ("kodi", "kodi", "Streaming"),
            ("spotify", "spotify", "Music"),
            ("moonlight", "moonlight", "Gaming"),
            ("limelight", "moonlight", "Gaming"),
            ("game", "gaming", "Gaming"),
            ("hbchannel", "homebrew", "Utilities"),
            ("browser", "browser", "Utilities"),
        ]
        .into_iter()
        .find_map(|(needle, brand, category)| {
            contains_word(&identity, needle).then_some((brand, category))
        })
        .unwrap_or(("generic", "Utilities"));
        Self {
            id: id.into(),
            title: title.into(),
            brand: brand.into(),
            category: category.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Input {
    pub id: String,
    pub title: String,
    pub port: String,
    pub connected: Option<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub apps: Vec<App>,
    pub inputs: Vec<Input>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
pub enum InputMode {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub favorites: Option<Vec<String>>,
    pub hidden: Vec<String>,
    pub background: String,
    pub show_names: bool,
    pub hour_24: bool,
    pub reduced_motion: bool,
    pub input_modes: BTreeMap<String, InputMode>,
    #[serde(deserialize_with = "crate::weather::deserialize_enabled")]
    pub show_weather: bool,
    #[serde(deserialize_with = "crate::weather::deserialize_location")]
    pub weather_location: Option<Location>,
    #[serde(deserialize_with = "crate::weather::deserialize_unit")]
    pub temperature_unit: Unit,
    #[serde(deserialize_with = "crate::i18n::deserialize_language")]
    pub language: Language,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            favorites: None,
            hidden: vec![],
            background: "dunes".into(),
            show_names: false,
            hour_24: false,
            reduced_motion: false,
            input_modes: BTreeMap::new(),
            show_weather: true,
            weather_location: None,
            temperature_unit: Unit::Celsius,
            language: Language::En,
        }
    }
}

impl Preferences {
    pub fn favorites(&self, apps: &[App]) -> Vec<String> {
        self.favorites.clone().unwrap_or_else(|| {
            let mut ids = Vec::new();
            for brand in [
                "netflix",
                "youtube",
                "apple-tv",
                "disney-plus",
                "prime-video",
                "live-tv",
            ] {
                if let Some(app) = apps.iter().find(|app| app.brand == brand) {
                    ids.push(app.id.clone());
                }
            }
            for app in apps {
                if ids.len() >= 6 {
                    break;
                }
                if !ids.contains(&app.id) {
                    ids.push(app.id.clone());
                }
            }
            ids
        })
    }

    pub fn input_visible(&self, input: &Input) -> bool {
        match self.input_modes.get(&input.id).copied().unwrap_or_default() {
            InputMode::On => true,
            InputMode::Off => false,
            InputMode::Auto => input.connected == Some(true),
        }
    }

    pub fn sanitize(&mut self) {
        if !BACKGROUNDS.iter().any(|(id, _)| *id == self.background) {
            self.background = "dunes".into();
        }
        fn clean(ids: &mut Vec<String>) {
            let mut seen = HashSet::new();
            ids.retain(|id| valid_app_id(id) && seen.insert(id.clone()));
            ids.truncate(1000);
        }
        if let Some(ids) = &mut self.favorites {
            clean(ids);
        }
        clean(&mut self.hidden);
        self.input_modes.retain(|id, _| valid_app_id(id));
    }
}

pub fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .split(['.', '_', '-'])
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}

pub fn demo_inventory() -> Inventory {
    Inventory {
        apps: [
            ("com.webos.app.livetv", "Live TV"),
            ("netflix", "Netflix"),
            ("youtube.leanback.v4", "YouTube"),
            ("com.apple.appletv", "Apple TV"),
            ("com.disney.disneyplus-prod", "Disney+"),
            ("amazon", "Prime Video"),
            ("plex", "Plex"),
            ("spotify-beehive", "Spotify"),
            ("org.jellyfin.webos", "Jellyfin"),
            ("org.xbmc.kodi", "Kodi"),
            ("com.limelight.webos", "Moonlight"),
            ("org.webosbrew.hbchannel", "Homebrew Channel"),
            ("com.webos.app.browser", "Web Browser"),
            ("com.arte.tv", "ARTE"),
            ("de.zdf.app.zdfm3", "ZDF"),
            ("ard.mediathek", "Das Erste"),
        ]
        .into_iter()
        .map(|(id, title)| App::new(id, title))
        .collect(),
        inputs: vec![
            Input {
                id: "com.webos.app.hdmi1".into(),
                title: "PlayStation 5".into(),
                port: "HDMI 1".into(),
                connected: Some(true),
            },
            Input {
                id: "com.webos.app.hdmi2".into(),
                title: "Apple TV".into(),
                port: "HDMI 2".into(),
                connected: Some(true),
            },
            Input {
                id: "com.webos.app.hdmi3".into(),
                title: "HDMI 3".into(),
                port: "HDMI 3".into(),
                connected: Some(false),
            },
        ],
    }
}
