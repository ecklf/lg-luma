use crate::i18n::Language;
use crate::weather::{Location, Unit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const BACKGROUNDS: [(&str, &str); 1] = [("midnight", "Midnight")];
pub const DEFAULT_BACKGROUND: &str = "midnight";

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

impl Input {
    /// A storage identity, not an application ID or an HDMI source.
    pub fn is_usb_storage(&self) -> bool {
        self.id.starts_with("luma.usb.")
    }
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
pub enum AppDrawerStyle {
    #[default]
    #[serde(rename = "tvos")]
    Tvos,
    Minimal,
}

impl AppDrawerStyle {
    pub const ALL: [Self; 2] = [Self::Tvos, Self::Minimal];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Tvos => "appDrawerTvos",
            Self::Minimal => "appDrawerMinimal",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSorting {
    #[default]
    Alphabetical,
    Usage,
}

impl AppSorting {
    pub const ALL: [Self; 2] = [Self::Alphabetical, Self::Usage];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Alphabetical => "appSortingAlphabetical",
            Self::Usage => "appSortingUsage",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherStyle {
    Standard,
    NoIcon,
    Compact,
    #[default]
    OneLine,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherIconStyle {
    Breezy,
    Lucide,
    Kickstand,
    #[default]
    Kuankuan,
    None,
}

impl WeatherIconStyle {
    pub const ALL: [Self; 4] = [Self::Kuankuan, Self::Breezy, Self::Lucide, Self::Kickstand];

    pub fn label(self) -> &'static str {
        match self {
            Self::Breezy => "Breezy",
            Self::Lucide => "Lucide",
            Self::Kickstand => "Kickstand",
            Self::Kuankuan => "Kuankuan",
            Self::None => "No icon",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|style| *style == self).unwrap()
    }

    pub fn asset(self, condition: &str) -> String {
        match self {
            Self::Breezy => format!("weather/{condition}.png"),
            Self::Lucide => format!("ui-icons/weather-{condition}.lvec"),
            Self::Kickstand => format!(
                "ui-icons/weather-kickstand-{}.lvec",
                if condition == "drizzle" {
                    "rain"
                } else {
                    condition
                }
            ),
            Self::Kuankuan => format!("ui-icons/weather-kuankuan-{condition}.lvec"),
            Self::None => String::new(),
        }
    }

    pub fn monochrome(self) -> bool {
        matches!(self, Self::Lucide | Self::Kickstand)
    }
}

impl WeatherStyle {
    pub const ALL: [Self; 3] = [Self::Standard, Self::Compact, Self::OneLine];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Standard => "weatherStyleStandard",
            Self::NoIcon => "weatherStyleNoIcon",
            Self::Compact => "weatherStyleCompact",
            Self::OneLine => "weatherStyleOneLine",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|style| *style == self).unwrap()
    }
}

pub const HOME_GRID_COLUMNS: usize = 6;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecentAppsBehavior {
    ShortPress,
    #[default]
    LongPress,
    Off,
}

impl RecentAppsBehavior {
    pub const ALL: [Self; 3] = [Self::ShortPress, Self::LongPress, Self::Off];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::ShortPress => "shortPress",
            Self::LongPress => "longPress",
            Self::Off => "stateOff",
        }
    }
}

pub const HOME_GRID_ROWS: usize = 4;
pub const HOME_GRID_CELLS: usize = HOME_GRID_COLUMNS * HOME_GRID_ROWS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeWidget {
    Weather,
    Greeting,
    Crypto,
    Stocks,
    ExtraWeather(u8),
    ExtraCrypto(u8),
    ExtraStocks(u8),
}

impl HomeWidget {
    pub fn kind(self) -> Self {
        match self {
            Self::ExtraWeather(_) => Self::Weather,
            Self::ExtraCrypto(_) => Self::Crypto,
            Self::ExtraStocks(_) => Self::Stocks,
            kind => kind,
        }
    }

    pub const ALL: [Self; 4] = [Self::Greeting, Self::Weather, Self::Crypto, Self::Stocks];

    pub fn span(self) -> (u8, u8) {
        match self.kind() {
            Self::Greeting => (4, 1),
            Self::Weather => (2, 2),
            Self::Crypto | Self::Stocks => (1, 1),
            _ => unreachable!(),
        }
    }

    pub fn key(self) -> &'static str {
        match self.kind() {
            Self::Weather => "weather",
            Self::Greeting => "greeting",
            Self::Crypto => "crypto",
            Self::Stocks => "stocks",
            _ => unreachable!(),
        }
    }

    pub fn default_position(self) -> WidgetPosition {
        WidgetPosition {
            column: match self.kind() {
                Self::Greeting => 0,
                Self::Weather => 0,
                Self::Crypto => 0,
                Self::Stocks => 1,
                _ => unreachable!(),
            },
            row: match self.kind() {
                Self::Weather => 1,
                Self::Crypto | Self::Stocks => 2,
                _ => 0,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WeatherWidgetSettings {
    pub enabled: bool,
    pub position: WidgetPosition,
    #[serde(deserialize_with = "crate::weather::deserialize_location")]
    pub location: Option<Location>,
    #[serde(deserialize_with = "crate::weather::deserialize_unit")]
    pub unit: Unit,
    pub small: bool,
    pub style: WeatherStyle,
    pub show_location: bool,
    pub show_icon: bool,
    pub icons: WeatherIconStyle,
}

impl Default for WeatherWidgetSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            position: HomeWidget::Weather.default_position(),
            location: None,
            unit: Unit::default(),
            small: false,
            style: WeatherStyle::default(),
            show_location: true,
            show_icon: true,
            icons: WeatherIconStyle::default(),
        }
    }
}

impl WeatherWidgetSettings {
    pub fn visible_icon_style(&self) -> WeatherIconStyle {
        if self.show_icon {
            self.icons
        } else {
            WeatherIconStyle::None
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct WidgetPosition {
    pub column: u8,
    pub row: u8,
}

impl WidgetPosition {
    pub fn from_cell(cell: usize) -> Option<Self> {
        (cell < HOME_GRID_CELLS).then_some(Self {
            column: (cell % HOME_GRID_COLUMNS) as u8,
            row: (cell / HOME_GRID_COLUMNS) as u8,
        })
    }

    pub fn cell(self) -> usize {
        usize::from(self.row) * HOME_GRID_COLUMNS + usize::from(self.column)
    }

    pub fn fits(self, widget: HomeWidget) -> bool {
        let (width, height) = widget.span();
        usize::from(self.column) + usize::from(width) <= HOME_GRID_COLUMNS
            && usize::from(self.row) + usize::from(height) <= HOME_GRID_ROWS
    }

    fn overlaps(self, (w, h): (u8, u8), other: Self, (ow, oh): (u8, u8)) -> bool {
        u16::from(self.column) < u16::from(other.column) + u16::from(ow)
            && u16::from(other.column) < u16::from(self.column) + u16::from(w)
            && u16::from(self.row) < u16::from(other.row) + u16::from(oh)
            && u16::from(other.row) < u16::from(self.row) + u16::from(h)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub extra_crypto: Vec<crate::markets::Settings>,
    pub extra_stocks: Vec<crate::markets::Settings>,
    pub extra_weather: Vec<WeatherWidgetSettings>,
    #[serde(deserialize_with = "crate::markets::deserialize_settings")]
    pub crypto: crate::markets::Settings,
    #[serde(deserialize_with = "crate::markets::deserialize_settings")]
    pub stocks: crate::markets::Settings,
    pub weather_small: bool,
    pub weather_show_location: bool,
    pub weather_show_icon: bool,
    pub weather_icons: WeatherIconStyle,
    pub recent_apps_behavior: RecentAppsBehavior,
    pub weather_style: WeatherStyle,
    pub app_sorting: AppSorting,
    pub app_drawer_style: AppDrawerStyle,
    pub show_drawer_app_names: bool,
    /// The drawer grid turns opaque at 70% of its motion, which keeps the animation at 60 fps
    /// on webOS 5 TVs; off, it fades in over the whole motion.
    pub smooth_drawer_fade: bool,
    pub app_launch_counts: BTreeMap<String, u64>,
    pub greeting_position: WidgetPosition,
    pub weather_position: WidgetPosition,
    /// Read-only marker for migrating the short-lived separate-Date draft.
    #[serde(rename = "date_position", skip_serializing)]
    pub legacy_date_position: Option<WidgetPosition>,
    pub favorites: Option<Vec<String>>,
    pub custom_tiles: Vec<CustomTile>,
    pub hidden: Vec<String>,
    pub background: String,
    pub deleted_backgrounds: Vec<String>,
    pub hour_24: bool,
    pub reduced_motion: bool,
    pub ui_sounds: bool,
    pub show_seconds: bool,
    pub show_greeting: bool,
    pub greeting_name: String,
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
    /// With the gradient on: uploaded photos get the subtle, photo-aware one instead.
    pub subtle_photo_gradient: bool,
    /// Unix seconds of the last automatic or chosen wallpaper change, for the timed modes.
    pub wallpaper_changed: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            extra_crypto: vec![],
            extra_stocks: vec![],
            extra_weather: vec![],
            weather_style: WeatherStyle::default(),
            crypto: crate::markets::Settings::default(),
            stocks: crate::markets::Settings {
                position: HomeWidget::Stocks.default_position(),
                ..Default::default()
            },
            weather_small: false,
            weather_show_location: true,
            weather_show_icon: true,
            weather_icons: WeatherIconStyle::default(),
            recent_apps_behavior: RecentAppsBehavior::default(),
            app_sorting: AppSorting::default(),
            app_drawer_style: AppDrawerStyle::default(),
            show_drawer_app_names: false,
            smooth_drawer_fade: true,
            app_launch_counts: BTreeMap::new(),
            greeting_position: HomeWidget::Greeting.default_position(),
            weather_position: HomeWidget::Weather.default_position(),
            legacy_date_position: None,
            favorites: None,
            custom_tiles: vec![],
            hidden: vec![],
            background: DEFAULT_BACKGROUND.into(),
            deleted_backgrounds: vec![],
            hour_24: false,
            reduced_motion: false,
            ui_sounds: false,
            show_seconds: false,
            show_greeting: true,
            greeting_name: String::new(),
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
            subtle_photo_gradient: false,
            wallpaper_changed: 0,
        }
    }
}

impl Preferences {
    pub fn widgets(&self) -> Vec<HomeWidget> {
        HomeWidget::ALL
            .into_iter()
            .chain((0..self.extra_crypto.len()).map(|i| HomeWidget::ExtraCrypto(i as u8)))
            .chain((0..self.extra_stocks.len()).map(|i| HomeWidget::ExtraStocks(i as u8)))
            .chain((0..self.extra_weather.len()).map(|i| HomeWidget::ExtraWeather(i as u8)))
            .collect()
    }

    pub fn weather_settings(&self, widget: HomeWidget) -> WeatherWidgetSettings {
        if let HomeWidget::ExtraWeather(i) = widget {
            return self.extra_weather[usize::from(i)].clone();
        }
        WeatherWidgetSettings {
            enabled: self.show_weather,
            position: self.weather_position,
            location: self.weather_location.clone(),
            unit: self.temperature_unit,
            small: self.weather_small,
            style: self.weather_style,
            show_location: self.weather_show_location,
            show_icon: self.weather_show_icon,
            icons: self.weather_icons,
        }
    }

    pub fn edit_weather(
        &mut self,
        widget: HomeWidget,
        edit: impl FnOnce(&mut WeatherWidgetSettings),
    ) {
        let mut config = self.weather_settings(widget);
        edit(&mut config);
        if let HomeWidget::ExtraWeather(i) = widget {
            self.extra_weather[usize::from(i)] = config;
        } else {
            self.show_weather = config.enabled;
            self.weather_position = config.position;
            self.weather_location = config.location;
            self.temperature_unit = config.unit;
            self.weather_small = config.small;
            self.weather_style = config.style;
            self.weather_show_location = config.show_location;
            self.weather_show_icon = config.show_icon;
            self.weather_icons = config.icons;
        }
    }

    pub fn available_widget(&self, kind: HomeWidget) -> Option<HomeWidget> {
        self.widgets()
            .into_iter()
            .find(|widget| widget.kind() == kind && !self.has_widget(*widget))
            .or(match kind {
                HomeWidget::Weather if self.extra_weather.len() < HOME_GRID_CELLS => {
                    Some(HomeWidget::ExtraWeather(self.extra_weather.len() as u8))
                }
                HomeWidget::Crypto if self.extra_crypto.len() < HOME_GRID_CELLS => {
                    Some(HomeWidget::ExtraCrypto(self.extra_crypto.len() as u8))
                }
                HomeWidget::Stocks if self.extra_stocks.len() < HOME_GRID_CELLS => {
                    Some(HomeWidget::ExtraStocks(self.extra_stocks.len() as u8))
                }
                _ => None,
            })
    }

    pub fn prepare_widget(&mut self, widget: HomeWidget) {
        match widget {
            HomeWidget::ExtraWeather(i) if usize::from(i) == self.extra_weather.len() => {
                self.extra_weather.push(WeatherWidgetSettings::default())
            }
            HomeWidget::ExtraCrypto(i) if usize::from(i) == self.extra_crypto.len() => {
                self.extra_crypto.push(Default::default())
            }
            HomeWidget::ExtraStocks(i) if usize::from(i) == self.extra_stocks.len() => {
                self.extra_stocks.push(Default::default())
            }
            _ => {}
        }
    }

    pub fn widget_span(&self, widget: HomeWidget) -> (u8, u8) {
        if widget.kind() == HomeWidget::Weather {
            let config = self.weather_settings(widget);
            if config.small {
                (1, 1)
            } else if config.style == WeatherStyle::OneLine {
                (2, 1)
            } else {
                widget.span()
            }
        } else {
            widget.span()
        }
    }

    pub fn widget_fits(&self, widget: HomeWidget, position: WidgetPosition) -> bool {
        if widget.kind() != HomeWidget::Weather {
            return position.fits(widget);
        }
        let (w, h) = self.widget_span(widget);
        usize::from(position.column) + usize::from(w) <= HOME_GRID_COLUMNS
            && usize::from(position.row) + usize::from(h) <= HOME_GRID_ROWS
    }

    #[cfg(test)]
    pub fn toggle_weather_size(&mut self) {
        self.toggle_weather_widget_size(HomeWidget::Weather);
    }

    pub fn toggle_weather_widget_size(&mut self, widget: HomeWidget) {
        self.edit_weather(widget, |config| config.small = !config.small);
        if !self.relocate_weather_widget(widget) {
            self.edit_weather(widget, |config| config.small = !config.small);
        }
    }

    pub fn set_weather_widget_style(&mut self, widget: HomeWidget, style: WeatherStyle) {
        let previous = self.weather_settings(widget).style;
        self.edit_weather(widget, |config| config.style = style);
        if !self.relocate_weather_widget(widget) {
            self.edit_weather(widget, |config| config.style = previous);
        }
    }

    fn relocate_weather_widget(&mut self, widget: HomeWidget) -> bool {
        // Expanding at an edge or beside Greeting needs a new valid anchor.
        if !self.can_place_widget(widget, self.widget_position(widget)) {
            if let Some(position) = (0..HOME_GRID_CELLS)
                .filter_map(WidgetPosition::from_cell)
                .find(|position| self.can_place_widget(widget, *position))
            {
                self.set_widget_position(widget, position);
            } else {
                return false;
            }
        }
        true
    }

    pub fn app_sort_key<'a>(&self, app: &'a App) -> (std::cmp::Reverse<u64>, String, &'a str) {
        let count = match self.app_sorting {
            AppSorting::Alphabetical => 0,
            AppSorting::Usage => self.app_launch_counts.get(&app.id).copied().unwrap_or(0),
        };
        (std::cmp::Reverse(count), app.title.to_lowercase(), &app.id)
    }

    pub fn weather_visible(&self) -> bool {
        self.show_weather
    }

    pub fn widget_position(&self, widget: HomeWidget) -> WidgetPosition {
        match widget {
            HomeWidget::Greeting => self.greeting_position,
            HomeWidget::Weather => self.weather_position,
            HomeWidget::Crypto => self.crypto.position,
            HomeWidget::Stocks => self.stocks.position,
            HomeWidget::ExtraWeather(i) => self.extra_weather[usize::from(i)].position,
            HomeWidget::ExtraCrypto(i) => self.extra_crypto[usize::from(i)].position,
            HomeWidget::ExtraStocks(i) => self.extra_stocks[usize::from(i)].position,
        }
    }

    pub fn has_widget(&self, widget: HomeWidget) -> bool {
        match widget {
            HomeWidget::Greeting => self.show_greeting,
            HomeWidget::Weather => self.show_weather,
            HomeWidget::Crypto => self.crypto.enabled,
            HomeWidget::Stocks => self.stocks.enabled,
            HomeWidget::ExtraWeather(i) => self
                .extra_weather
                .get(usize::from(i))
                .is_some_and(|s| s.enabled),
            HomeWidget::ExtraCrypto(i) => self
                .extra_crypto
                .get(usize::from(i))
                .is_some_and(|s| s.enabled),
            HomeWidget::ExtraStocks(i) => self
                .extra_stocks
                .get(usize::from(i))
                .is_some_and(|s| s.enabled),
        }
    }

    /// Restore widget membership and placement without discarding configuration.
    pub fn reset_widgets(&mut self) -> bool {
        let changed = self.widgets().into_iter().any(|widget| {
            self.has_widget(widget) != matches!(widget, HomeWidget::Greeting | HomeWidget::Weather)
                || self.widget_position(widget) != widget.default_position()
        });
        for widget in self.widgets() {
            self.set_widget_present(
                widget,
                matches!(widget, HomeWidget::Greeting | HomeWidget::Weather),
            );
            self.set_widget_position(widget, widget.default_position());
        }
        self.legacy_date_position = None;
        changed
    }

    pub fn set_widget_present(&mut self, widget: HomeWidget, present: bool) {
        match widget {
            HomeWidget::Greeting => self.show_greeting = present,
            HomeWidget::Weather => self.show_weather = present,
            HomeWidget::Crypto => self.crypto.enabled = present,
            HomeWidget::Stocks => self.stocks.enabled = present,
            HomeWidget::ExtraWeather(i) => self.extra_weather[usize::from(i)].enabled = present,
            HomeWidget::ExtraCrypto(i) => self.extra_crypto[usize::from(i)].enabled = present,
            HomeWidget::ExtraStocks(i) => self.extra_stocks[usize::from(i)].enabled = present,
        }
    }

    pub fn widget_at(&self, position: WidgetPosition) -> Option<HomeWidget> {
        self.widgets().into_iter().find(|widget| {
            let anchor = self.widget_position(*widget);
            let (w, h) = self.widget_span(*widget);
            self.has_widget(*widget)
                && position.column >= anchor.column
                && position.row >= anchor.row
                && u16::from(position.column) < u16::from(anchor.column) + u16::from(w)
                && u16::from(position.row) < u16::from(anchor.row) + u16::from(h)
        })
    }

    pub fn can_place_widget(&self, widget: HomeWidget, position: WidgetPosition) -> bool {
        self.widget_fits(widget, position)
            && self
                .widgets()
                .into_iter()
                .filter(|other| *other != widget && self.has_widget(*other))
                .all(|other| {
                    !position.overlaps(
                        self.widget_span(widget),
                        self.widget_position(other),
                        self.widget_span(other),
                    )
                })
    }

    pub fn place_widget(&mut self, widget: HomeWidget, position: WidgetPosition) -> bool {
        if !self.can_place_widget(widget, position) || self.widget_position(widget) == position {
            return false;
        }
        self.set_widget_position(widget, position);
        true
    }

    fn set_widget_position(&mut self, widget: HomeWidget, position: WidgetPosition) {
        match widget {
            HomeWidget::Greeting => self.greeting_position = position,
            HomeWidget::Weather => self.weather_position = position,
            HomeWidget::Crypto => self.crypto.position = position,
            HomeWidget::Stocks => self.stocks.position = position,
            HomeWidget::ExtraWeather(i) => self.extra_weather[usize::from(i)].position = position,
            HomeWidget::ExtraCrypto(i) => self.extra_crypto[usize::from(i)].position = position,
            HomeWidget::ExtraStocks(i) => self.extra_stocks[usize::from(i)].position = position,
        }
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

    /// The custom name, otherwise the HDMI port or the storage device's name.
    pub fn input_title<'a>(&'a self, input: &'a Input) -> &'a str {
        self.input_name(input).unwrap_or(if input.is_usb_storage() {
            &input.title
        } else {
            &input.port
        })
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
        if self.weather_style == WeatherStyle::NoIcon {
            self.weather_style = WeatherStyle::Standard;
            self.weather_icons = WeatherIconStyle::None;
        }
        if self.weather_icons == WeatherIconStyle::None {
            self.weather_icons = WeatherIconStyle::default();
            self.weather_show_icon = false;
        }
        for config in &mut self.extra_weather {
            if config.style == WeatherStyle::NoIcon {
                config.style = WeatherStyle::Standard;
                config.icons = WeatherIconStyle::None;
            }
            if config.icons == WeatherIconStyle::None {
                config.icons = WeatherIconStyle::default();
                config.show_icon = false;
            }
        }
        self.greeting_name = clean_input_name(&self.greeting_name);
        self.extra_crypto.truncate(HOME_GRID_CELLS);
        self.extra_stocks.truncate(HOME_GRID_CELLS);
        self.extra_weather.truncate(HOME_GRID_CELLS);
        for config in &mut self.extra_crypto {
            config.sanitize(HomeWidget::Crypto);
        }
        for config in &mut self.extra_stocks {
            config.sanitize(HomeWidget::Stocks);
        }
        self.crypto.sanitize(HomeWidget::Crypto);
        self.stocks.sanitize(HomeWidget::Stocks);
        // Collapse the previous draft's untouched separate-Date arrangement.
        // Custom Greeting/Weather anchors remain unchanged.
        if self.legacy_date_position.take().is_some()
            && self.show_greeting
            && self.show_weather
            && self.greeting_position == (WidgetPosition { column: 0, row: 1 })
            && self.weather_position == (WidgetPosition { column: 0, row: 2 })
        {
            self.greeting_position = HomeWidget::Greeting.default_position();
            self.weather_position = HomeWidget::Weather.default_position();
        }
        let mut placed = Vec::new();
        for widget in self.widgets() {
            if !self.has_widget(widget) {
                if !self.widget_fits(widget, self.widget_position(widget)) {
                    self.set_widget_position(widget, widget.default_position());
                }
                continue;
            }
            let valid = |position: WidgetPosition| {
                self.widget_fits(widget, position)
                    && placed.iter().all(|other| {
                        !position.overlaps(
                            self.widget_span(widget),
                            self.widget_position(*other),
                            self.widget_span(*other),
                        )
                    })
            };
            if !valid(self.widget_position(widget)) {
                let position = std::iter::once(widget.default_position())
                    .chain((0..HOME_GRID_CELLS).filter_map(WidgetPosition::from_cell))
                    .find(|position| valid(*position));
                let Some(position) = position else {
                    self.set_widget_present(widget, false);
                    continue;
                };
                self.set_widget_position(widget, position);
            }
            placed.push(widget);
        }
        if !BACKGROUNDS.iter().any(|(id, _)| *id == self.background)
            && !crate::wallpapers::valid_id(&self.background)
        {
            self.background = DEFAULT_BACKGROUND.into();
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
pub const INPUT_ICONS: [(&str, &str); 16] = [
    ("hdmi", "inputIconHdmi"),
    ("usb", "inputIconUsb"),
    ("apple-tv", "inputIconAppleTv"),
    ("computer", "inputIconComputer"),
    ("console", "inputIconConsole"),
    ("playstation", "inputIconPlaystation"),
    ("shield", "inputIconShield"),
    ("steam", "inputIconSteam"),
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
