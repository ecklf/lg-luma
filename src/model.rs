use crate::i18n::Language;
use crate::weather::{Location, Unit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

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

// Generic fallbacks use delimited words, so "game" does not match "endgame".
// Non-ASCII bytes count as boundaries for localized titles. Curated vector
// branding is selected separately by exact ID in branding.rs.
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
    #[serde(skip)]
    pub icon: Option<crate::app_artwork::Source>,
    /// The interface icon a shortcut tile shows; empty for real apps.
    #[serde(skip)]
    pub mark: &'static str,
}

impl App {
    pub fn new(id: &str, title: &str) -> Self {
        if id.starts_with(CUSTOM_TILE_PREFIX) {
            return Self {
                id: id.into(),
                title: title.into(),
                brand: "custom".into(),
                category: "Utilities".into(),
                icon: None,
                mark: TILE_ICONS[0].0,
            };
        }
        let identity = format!("{id} {title}").to_lowercase();
        let generic = [
            ("kodi", "kodi", "Streaming"),
            ("moonlight", "moonlight", "Gaming"),
            ("limelight", "moonlight", "Gaming"),
            ("game", "gaming", "Gaming"),
            ("hbchannel", "homebrew", "Utilities"),
        ]
        .into_iter()
        .find_map(|(needle, brand, category)| {
            contains_word(&identity, needle).then_some((brand, category))
        })
        .unwrap_or(("generic", "Utilities"));
        let (brand, category) =
            crate::branding::for_app(id).map_or(generic, |brand| (brand.key, brand.category));
        Self {
            id: id.into(),
            title: title.into(),
            brand: brand.into(),
            category: category.into(),
            icon: None,
            mark: "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Input {
    pub id: String,
    pub title: String,
    pub port: String,
    pub connected: Option<bool>,
    /// Optional signal information; never overrides the cable state or Auto visibility.
    pub signal: Option<bool>,
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

/// POST is the default because Home Assistant webhooks accept only POST and PUT
/// unless GET is enabled on the trigger.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum TileMethod {
    Get,
    #[default]
    Post,
}

impl TileMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// A home-screen tile that fires a one-way HTTP request instead of launching
/// an app. Its id lives in `favorites`/`hidden` like any real app id, so
/// ordering, favoriting and hiding reuse the existing machinery unchanged.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct CustomTile {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub method: TileMethod,
    /// One of `TILE_ICONS`; empty shows the first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
}

impl CustomTile {
    pub fn icon(&self) -> &'static str {
        TILE_ICONS
            .iter()
            .find(|(icon, _)| *icon == self.icon)
            .map_or(TILE_ICONS[0].0, |(icon, _)| icon)
    }
}

/// Interface icons a shortcut tile can show, with their message keys.
pub const TILE_ICONS: [(&str, &str); 15] = [
    ("weather-moon", "iconMoon"),
    ("weather-sun", "iconSun"),
    ("weather-cloud", "iconCloud"),
    ("tv", "iconTv"),
    ("power", "iconPower"),
    ("globe", "iconGlobe"),
    ("clock", "iconClock"),
    ("coffee", "iconCoffee"),
    ("gaming", "iconGaming"),
    ("heart", "iconHeart"),
    ("location", "iconLocation"),
    ("thermometer", "iconThermometer"),
    ("sparkles", "iconSparkles"),
    ("alert", "iconAlert"),
    ("eye", "iconEye"),
];

pub const CUSTOM_TILE_PREFIX: &str = "custom.";
pub const TILE_LIMIT: usize = 64;
pub const TILE_URL_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    #[default]
    Localized,
    DayMonthYear,
    MonthDayYear,
    YearMonthDay,
}

impl DateFormat {
    pub const ALL: [Self; 4] = [
        Self::Localized,
        Self::DayMonthYear,
        Self::MonthDayYear,
        Self::YearMonthDay,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|format| *format == self).unwrap()
    }

    pub fn label(self, language: Language) -> &'static str {
        match self {
            Self::Localized => language.t("dateLocalized"),
            Self::DayMonthYear => "DD.MM.YYYY",
            Self::MonthDayYear => "MM/DD/YYYY",
            Self::YearMonthDay => "YYYY-MM-DD",
        }
    }

    pub fn format(self, language: Language, now: time::OffsetDateTime) -> String {
        let (year, month, day) = (now.year(), now.month() as u8, now.day());
        match self {
            Self::Localized => language.date(now),
            Self::DayMonthYear => format!("{day:02}.{month:02}.{year:04}"),
            Self::MonthDayYear => format!("{month:02}/{day:02}/{year:04}"),
            Self::YearMonthDay => format!("{year:04}-{month:02}-{day:02}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeLayout {
    Centered,
    Grid,
    #[default]
    #[serde(other)]
    Row,
}

impl HomeLayout {
    pub fn key(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Centered => "centered",
            Self::Grid => "grid",
        }
    }
    pub fn slots(self) -> usize {
        match self {
            Self::Row => 3,
            Self::Centered => 1,
            Self::Grid => 6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Row => "1 × 3",
            Self::Centered => "1 × 1",
            Self::Grid => "2 × 3",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Row => Self::Centered,
            Self::Centered => Self::Grid,
            Self::Grid => Self::Row,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeWidget {
    Weather,
    Clock,
    Date,
    Greeting,
    #[default]
    #[serde(other)]
    Empty,
}

impl HomeWidget {
    pub const ALL: [Self; 5] = [
        Self::Empty,
        Self::Weather,
        Self::Clock,
        Self::Date,
        Self::Greeting,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Empty => "emptySlot",
            Self::Weather => "weather",
            Self::Clock => "clock",
            Self::Date => "date",
            Self::Greeting => "greeting",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub home_layout: HomeLayout,
    pub home_widgets: [HomeWidget; 6],
    pub home_clock_depth: bool,
    pub favorites: Option<Vec<String>>,
    pub custom_tiles: Vec<CustomTile>,
    pub hidden: Vec<String>,
    pub background: String,
    /// Last manually chosen splash, independent of automatic rotation.
    pub splash_background: Option<String>,
    pub deleted_backgrounds: Vec<String>,
    pub hour_24: bool,
    pub reduced_motion: bool,
    pub show_seconds: bool,
    /// Optional name appended to the Home greeting ("Good evening, Ana."). Empty = plain greeting.
    pub greeting_name: String,
    pub show_greeting: bool,
    pub show_date: bool,
    pub date_format: DateFormat,
    pub input_modes: BTreeMap<String, InputMode>,
    /// Custom names by input app ID; the TV's port and label stay visible as detail.
    pub input_names: BTreeMap<String, String>,
    /// Bundled input icon overrides by input app ID; absent means automatic detection.
    pub input_icons: BTreeMap<String, String>,
    #[serde(deserialize_with = "crate::weather::deserialize_enabled")]
    pub show_weather: bool,
    #[serde(deserialize_with = "crate::weather::deserialize_location")]
    pub weather_location: Option<Location>,
    #[serde(deserialize_with = "crate::weather::deserialize_unit")]
    pub temperature_unit: Unit,
    #[serde(deserialize_with = "crate::i18n::deserialize_language")]
    pub language: Language,
    /// Applied at launch because MSAA is selected when SDL creates the GL window.
    #[serde(deserialize_with = "crate::text_rendering::deserialize")]
    pub text_rendering: crate::text_rendering::TextRendering,
    #[serde(deserialize_with = "crate::wallpaper_cycle::deserialize")]
    pub wallpaper_cycle: bool,
    pub wallpaper_gradient: bool,
    /// Unix seconds of the last automatic or chosen wallpaper change, for the timed modes.
    pub wallpaper_changed: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            home_layout: HomeLayout::default(),
            home_clock_depth: true,
            home_widgets: [
                HomeWidget::Weather,
                HomeWidget::Empty,
                HomeWidget::Empty,
                HomeWidget::Empty,
                HomeWidget::Empty,
                HomeWidget::Empty,
            ],
            favorites: None,
            custom_tiles: vec![],
            hidden: vec![],
            background: "dunes".into(),
            splash_background: None,
            deleted_backgrounds: vec![],
            hour_24: false,
            reduced_motion: false,
            show_seconds: false,
            greeting_name: String::new(),
            show_greeting: true,
            show_date: true,
            date_format: DateFormat::default(),
            input_modes: BTreeMap::new(),
            input_names: BTreeMap::new(),
            input_icons: BTreeMap::new(),
            show_weather: true,
            weather_location: None,
            temperature_unit: Unit::Celsius,
            language: Language::En,
            text_rendering: crate::text_rendering::TextRendering::default(),
            wallpaper_cycle: false,
            wallpaper_gradient: true,
            wallpaper_changed: 0,
        }
    }
}

impl Preferences {
    pub fn weather_visible(&self) -> bool {
        self.show_weather
            && self.home_widgets[..self.home_layout.slots()].contains(&HomeWidget::Weather)
    }

    pub fn cycle_home_widget(&mut self, slot: usize) {
        if slot >= self.home_layout.slots() {
            return;
        }
        let current = HomeWidget::ALL
            .iter()
            .position(|widget| *widget == self.home_widgets[slot])
            .unwrap_or(0);
        // Keep actionable widgets unique, including slots hidden by a smaller preset.
        for step in 1..=HomeWidget::ALL.len() {
            let next = HomeWidget::ALL[(current + step) % HomeWidget::ALL.len()];
            if next == HomeWidget::Empty || !self.home_widgets.contains(&next) {
                self.home_widgets[slot] = next;
                break;
            }
        }
    }

    pub fn splash_background(&self) -> &str {
        self.splash_background
            .as_deref()
            .unwrap_or(&self.background)
    }

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

    /// Synthetic `App` entries for custom tiles, merged into the real
    /// inventory so favoriting, hiding, search and the Apps grid treat them
    /// exactly like any installed app.
    pub fn custom_apps(&self) -> Vec<App> {
        self.custom_tiles
            .iter()
            .map(|tile| App {
                mark: tile.icon(),
                ..App::new(&tile.id, &tile.title)
            })
            .collect()
    }

    pub fn custom_tile(&self, id: &str) -> Option<&CustomTile> {
        self.custom_tiles.iter().find(|tile| tile.id == id)
    }

    pub fn remove_tile(&mut self, id: &str) -> Option<CustomTile> {
        let index = self.custom_tiles.iter().position(|tile| tile.id == id)?;
        if let Some(favorites) = &mut self.favorites {
            favorites.retain(|item| item != id);
        }
        self.hidden.retain(|item| item != id);
        Some(self.custom_tiles.remove(index))
    }

    pub fn input_visible(&self, input: &Input) -> bool {
        match self.input_modes.get(&input.id).copied().unwrap_or_default() {
            InputMode::On => true,
            InputMode::Off => false,
            InputMode::Auto => input.connected == Some(true),
        }
    }

    pub fn input_name(&self, input: &Input) -> Option<&str> {
        self.input_names.get(&input.id).map(String::as_str)
    }

    /// The custom name if one is saved, otherwise the port ("HDMI 2").
    pub fn input_title<'a>(&'a self, input: &'a Input) -> &'a str {
        self.input_name(input).unwrap_or(&input.port)
    }

    /// Saves a cleaned name; an empty name restores the port title.
    pub fn set_input_name(&mut self, id: &str, name: &str) -> bool {
        if !valid_app_id(id) {
            return false;
        }
        let name = clean_input_name(name);
        if name.is_empty() {
            self.input_names.remove(id).is_some()
        } else {
            self.input_names.insert(id.into(), name.clone()) != Some(name)
        }
    }

    pub fn input_icon(&self, id: &str) -> Option<&'static str> {
        self.input_icons.get(id).and_then(|saved| {
            INPUT_ICONS
                .iter()
                .find(|(icon, _)| *icon == saved)
                .map(|(icon, _)| *icon)
        })
    }

    /// An empty icon restores automatic detection. Unknown assets are never saved.
    pub fn set_input_icon(&mut self, id: &str, icon: &str) -> bool {
        if !valid_app_id(id) {
            return false;
        }
        if icon.is_empty() {
            return self.input_icons.remove(id).is_some();
        }
        if !INPUT_ICONS.iter().any(|(known, _)| *known == icon) {
            return false;
        }
        self.input_icons.insert(id.into(), icon.into()) != Some(icon.into())
    }

    pub fn sanitize(&mut self) {
        for slot in 0..self.home_widgets.len() {
            if self.home_widgets[slot] != HomeWidget::Empty
                && self.home_widgets[..slot].contains(&self.home_widgets[slot])
            {
                self.home_widgets[slot] = HomeWidget::Empty;
            }
        }
        if self.splash_background.as_ref().is_some_and(|id| {
            !BACKGROUNDS.iter().any(|(known, _)| *known == id) && !crate::wallpapers::valid_id(id)
        }) {
            self.splash_background = None;
        }
        if !BACKGROUNDS.iter().any(|(id, _)| *id == self.background)
            && !crate::wallpapers::valid_id(&self.background)
        {
            self.background = "dunes".into();
        }
        let mut seen = HashSet::new();
        self.deleted_backgrounds.retain(|id| {
            id != &self.background
                && BACKGROUNDS.iter().any(|(known, _)| *known == id)
                && seen.insert(id.clone())
        });
        fn clean(ids: &mut Vec<String>) {
            let mut seen = HashSet::new();
            ids.retain(|id| valid_app_id(id) && seen.insert(id.clone()));
            ids.truncate(1000);
        }
        if let Some(ids) = &mut self.favorites {
            clean(ids);
        }
        clean(&mut self.hidden);
        {
            let mut seen = HashSet::new();
            for tile in &mut self.custom_tiles {
                tile.title = clean_input_name(&tile.title);
                tile.url = tile.url.trim().into();
                if tile.icon() != tile.icon {
                    tile.icon.clear();
                }
            }
            self.custom_tiles.retain(|tile| {
                valid_app_id(&tile.id)
                    && tile.id.starts_with(CUSTOM_TILE_PREFIX)
                    && !tile.title.is_empty()
                    && valid_tile_url(&tile.url)
                    && seen.insert(tile.id.clone())
            });
            self.custom_tiles.truncate(TILE_LIMIT);
        }
        self.input_icons.retain(|id, icon| {
            valid_app_id(id) && INPUT_ICONS.iter().any(|(known, _)| *known == icon)
        });
        self.input_modes.retain(|id, _| valid_app_id(id));
        self.input_names = std::mem::take(&mut self.input_names)
            .into_iter()
            .filter(|(id, _)| valid_app_id(id))
            .map(|(id, name)| (id, clean_input_name(&name)))
            .filter(|(_, name)| !name.is_empty())
            .take(128)
            .collect();
    }
}

/// Every bundled device SVG offered by the native input icon picker.
pub const INPUT_ICONS: [(&str, &str); 15] = [
    ("hdmi", "inputIconHdmi"),
    ("usb", "inputIconUsb"),
    ("apple-tv", "inputIconAppleTv"),
    ("computer", "inputIconComputer"),
    ("console", "inputIconConsole"),
    ("playstation", "inputIconPlaystation"),
    ("shield", "inputIconShield"),
    ("streamer", "inputIconStreamer"),
    ("switch", "inputIconSwitch"),
    ("switch2", "inputIconSwitch2"),
    ("sony", "inputIconSony"),
    ("dvd", "inputIconDvd"),
    ("blu-ray", "inputIconBluRay"),
    ("disk", "inputIconDisk"),
    ("xbox", "inputIconXbox"),
];

pub const INPUT_NAME_LIMIT: usize = 40;

pub fn clean_input_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    let clean: String = clean.trim().chars().take(INPUT_NAME_LIMIT).collect();
    clean.trim_end().into()
}

/// The id for a new tile titled `title`, unique among `tiles`.
pub fn new_tile_id(tiles: &[CustomTile], title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(48)
        .collect();
    let slug = match slug.trim_end_matches('-') {
        "" => "tile",
        slug => slug,
    };
    let base = format!("{CUSTOM_TILE_PREFIX}{slug}");
    (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|id| !tiles.iter().any(|tile| &tile.id == id))
        .unwrap()
}

/// Whether the URL uses https, and the address after the scheme.
pub fn split_tile_url(url: &str) -> Option<(bool, &str)> {
    let start = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    if start.starts_with("https://") {
        Some((true, &url[8..]))
    } else if start.starts_with("http://") {
        Some((false, &url[7..]))
    } else {
        None
    }
}

/// Whether the URL can be sent as typed. It must parse with the request
/// client's URI rules and name a host, so a tile never saves with an address
/// that only fails once it is pressed.
pub fn valid_tile_url(url: &str) -> bool {
    split_tile_url(url).is_some()
        && url.chars().count() <= TILE_URL_LIMIT
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
        && url.parse::<ureq::http::Uri>().is_ok_and(|uri| {
            uri.authority()
                .is_some_and(|authority| valid_tile_authority(authority.as_str()))
        })
}

/// Host and optional port after any user info: a host name, an IPv4 address
/// or a bracketed IPv6 address, then a port from 1 to 65535.
fn valid_tile_authority(authority: &str) -> bool {
    let address = authority.rsplit('@').next().unwrap_or_default();
    let (host, port) = match address.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((ip, port)) if ip.parse::<std::net::Ipv6Addr>().is_ok() => (None, port),
            _ => return false,
        },
        None => match address.find(':') {
            Some(colon) => (Some(&address[..colon]), &address[colon..]),
            None => (Some(address), ""),
        },
    };
    let valid_port = port.is_empty()
        || port.strip_prefix(':').is_some_and(|digits| {
            !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && digits.parse::<u16>().is_ok_and(|port| port != 0)
        });
    valid_port && host.is_none_or(valid_host_name)
}

/// DNS-style labels, or a dotted IPv4 address when every label is a number.
fn valid_host_name(host: &str) -> bool {
    let labels: Vec<_> = host.split('.').collect();
    if labels
        .iter()
        .all(|label| !label.is_empty() && label.bytes().all(|b| b.is_ascii_digit()))
    {
        return host.parse::<std::net::Ipv4Addr>().is_ok();
    }
    host.len() <= 253
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

pub fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .split(['.', '_', '-'])
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}

pub fn demo_inventory() -> Inventory {
    let mut inventory = Inventory {
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
            ("coupangplay", "Coupang Play"),
            ("com.frograms.watchaplay.webos", "Watcha Play"),
            ("pooq", "Wavve"),
            ("com.cmb.rainbowtv", "Rainbow TV"),
            ("com.lgtvod.kr.app", "LG TV VOD"),
            ("com.webos.app.igallery", "올레드 갤러리"),
            ("com.webos.app.roomconnect", "룸투룸 셰어"),
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
                signal: None,
            },
            Input {
                id: "com.webos.app.hdmi2".into(),
                title: "Apple TV".into(),
                port: "HDMI 2".into(),
                connected: Some(true),
                signal: None,
            },
            Input {
                id: "com.webos.app.hdmi3".into(),
                title: "HDMI 3".into(),
                port: "HDMI 3".into(),
                connected: Some(false),
                signal: None,
            },
        ],
    };
    // Desktop-only artwork gallery. TV inventory is always read from services.
    for brand in crate::branding::BRANDS {
        if !inventory.apps.iter().any(|app| app.brand == brand.key) {
            inventory.apps.push(App::new(brand.ids[0], brand.title));
        }
    }
    inventory
}
