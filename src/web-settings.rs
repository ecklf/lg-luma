//! The launcher settings a paired phone or computer may change, and how the page labels them.
//! Labels come from the TV's own message catalog, so the page reads like Settings on the TV.
use crate::i18n::Language;
use crate::model::{
    AppSorting, DateFormat, INPUT_ICONS, InputMode, Preferences, WeatherStyle, clean_input_name,
};
use crate::text_rendering::TextRendering;
use crate::weather::Unit;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The preferences a companion page may change. Lists the launcher keeps itself (favorites,
/// hidden apps, launch counts, tiles, wallpapers) stay out, so a page can never replace them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub hour_24: bool,
    pub show_seconds: bool,
    pub language: Language,
    pub app_sorting: AppSorting,
    pub reduced_motion: bool,
    pub ui_sounds: bool,
    pub show_greeting: bool,
    pub greeting_name: String,
    pub show_date: bool,
    pub date_format: DateFormat,
    pub show_weather: bool,
    pub temperature_unit: Unit,
    pub weather_style: WeatherStyle,
    /// One of the backgrounds in the catalog; the launcher ignores any other id.
    pub background: String,
    pub wallpaper_cycle: bool,
    pub wallpaper_gradient: bool,
    pub text_rendering: TextRendering,
}

impl Settings {
    pub fn of(preferences: &Preferences) -> Self {
        Self {
            hour_24: preferences.hour_24,
            show_seconds: preferences.show_seconds,
            language: preferences.language,
            app_sorting: preferences.app_sorting,
            reduced_motion: preferences.reduced_motion,
            ui_sounds: preferences.ui_sounds,
            show_greeting: preferences.show_greeting,
            greeting_name: preferences.greeting_name.clone(),
            show_date: preferences.show_date,
            date_format: preferences.date_format,
            show_weather: preferences.show_weather,
            temperature_unit: preferences.temperature_unit,
            weather_style: preferences.weather_style,
            background: preferences.background.clone(),
            wallpaper_cycle: preferences.wallpaper_cycle,
            wallpaper_gradient: preferences.wallpaper_gradient,
            text_rendering: preferences.text_rendering,
        }
    }

    /// The same cleanup the TV keyboard applies to a typed greeting name.
    pub fn cleaned(mut self) -> Self {
        self.greeting_name = clean_input_name(&self.greeting_name);
        self
    }
}

/// A background the page can show and choose, with the TV's own thumbnail.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Background {
    pub id: String,
    /// Empty for the owner's own photos.
    pub name: String,
    #[serde(skip)]
    pub thumbnail: PathBuf,
}

/// The apps the page can arrange: every app and shortcut in the order the page shows them (and
/// the tile sheets are drawn in), the Home favorites in their order, and the hidden ones.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Library {
    pub apps: Vec<LibraryApp>,
    pub favorites: Vec<String>,
    pub hidden: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LibraryApp {
    pub id: String,
    pub title: String,
}

/// A TV input as Settings › Widgets › Inputs shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WebInput {
    pub id: String,
    /// The TV's port name ("HDMI 2"), shown when no custom name is saved.
    pub port: String,
    pub name: String,
    pub mode: InputMode,
    /// The chosen icon, or empty for automatic.
    pub icon: String,
    /// What automatic picks right now.
    pub auto_icon: String,
}

/// One input's name, visibility and icon from the page.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputChange {
    pub id: String,
    pub name: String,
    pub mode: InputMode,
    pub icon: String,
}

/// Everything besides plain settings the page lists, kept current by the launcher.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub backgrounds: Vec<Background>,
    pub library: Library,
    pub inputs: Vec<WebInput>,
    pub weather_location: Option<crate::weather::Location>,
    pub tiles: Vec<crate::model::CustomTile>,
}

/// A new arrangement from the page. Unknown ids are dropped by the launcher.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arrangement {
    pub favorites: Vec<String>,
    pub hidden: Vec<String>,
}

fn choice<T: Serialize>(value: T, label: &str) -> serde_json::Value {
    serde_json::json!({ "value": value, "label": label })
}

/// Page text that is not a setting, in the TV's language. Needed before pairing too.
pub fn strings(language: Language) -> serde_json::Value {
    let t = |key| language.t(key);
    serde_json::json!({
        "lang": serde_json::to_value(language).unwrap_or_default(),
        "preview": t("webPreview"),
        "previewUnavailable": t("webPreviewUnavailable"),
        "previewBackground": t("webPreviewBackground"),
        "saving": t("webSaving"),
        "saved": t("webSaved"),
        "offline": t("webOffline"),
        "timeout": t("webTimeout"),
        "pairTitle": t("webPairTitle"),
        "pairIntro": t("webPairIntro"),
        "pair": t("webPair"),
        "pairing": t("webPairing"),
        "pairedAs": t("webPairedAs"),
        "unpair": t("webUnpair"),
        "unpaired": t("webUnpaired"),
        "restart": t("textRenderingRestart"),
        "custom": t("webCustomPhoto"),
    })
}

/// Page labels and the message keys the TV uses for the same words.
const LABELS: &[(&str, &str)] = &[
    ("apps", "apps"),
    ("favorites", "favorites"),
    ("addFavorite", "addFavorite"),
    ("removeFavorite", "removeFavorite"),
    ("hideApp", "hideApp"),
    ("showApp", "showApp"),
    ("hidden", "hiddenLabel"),
    ("makeFavorites", "makeFavorites"),
    ("inputs", "inputs"),
    ("inputName", "inputName"),
    ("inputNameHint", "inputNameHint"),
    ("inputIcon", "inputIcon"),
    ("location", "location"),
    ("cityPostcode", "cityPostcode"),
    ("searchLocations", "searchLocations"),
    ("removeWeatherLocation", "removeWeatherLocation"),
    ("noLocations", "noLocations"),
    ("searching", "searching"),
    ("setWeatherLocation", "setWeatherLocation"),
    ("shortcuts", "shortcuts"),
    ("noTiles", "noTiles"),
    ("addTile", "addTile"),
    ("newTile", "newTile"),
    ("tileName", "tileName"),
    ("tileNameHint", "tileNameHint"),
    ("tileUrl", "tileUrl"),
    ("tileUrlHint", "tileUrlHint"),
    ("requestMethod", "requestMethod"),
    ("removeTile", "removeTile"),
    ("testShortcut", "testShortcut"),
    ("tileSent", "tileSent"),
    ("tileFailed", "tileFailed"),
    ("tileIncomplete", "tileIncomplete"),
    ("tileLimit", "tileLimit"),
    ("icon", "icon"),
    ("save", "webSave"),
    ("cancel", "cancel"),
    ("general", "interface"),
    ("background", "background"),
    ("widgets", "widgets"),
    ("system", "system"),
    ("clock", "clock"),
    ("clockFormat", "clockFormat"),
    ("timeFormat", "timeFormat"),
    ("language", "language"),
    ("appSorting", "appSorting"),
    ("reducedMotion", "reducedMotion"),
    ("uiSounds", "uiSounds"),
    ("greeting", "greeting"),
    ("greetingName", "greetingName"),
    ("showWidget", "showWidget"),
    ("date", "date"),
    ("dateFormat", "dateFormat"),
    ("weather", "weather"),
    ("temperature", "temperature"),
    ("weatherStyle", "weatherStyle"),
    ("wallpaperCycle", "wallpaperCycle"),
    ("wallpaperGradient", "wallpaperGradient"),
    ("textRendering", "textRendering"),
];

/// The page's whole view of the TV: current values plus every label in the TV's language, so the
/// page reads like the Settings the owner already knows.
pub fn listing(settings: &Settings, catalog: &Catalog) -> serde_json::Value {
    let language = settings.language;
    let t = |key| language.t(key);
    let labels: serde_json::Map<_, _> = LABELS
        .iter()
        .map(|(name, key)| ((*name).to_owned(), serde_json::json!(t(key))))
        .collect();
    let choices = serde_json::json!({
        "hour_24": [choice(false, t("hour12")), choice(true, t("hour24"))],
        "show_seconds": [choice(false, "HH:MM"), choice(true, "HH:MM:SS")],
        "language": Language::ALL
            .iter()
            .map(|item| choice(item, item.name()))
            .collect::<Vec<_>>(),
        "app_sorting": AppSorting::ALL
            .iter()
            .map(|item| choice(item, t(item.label_key())))
            .collect::<Vec<_>>(),
        "date_format": DateFormat::ALL
            .iter()
            .map(|item| choice(item, item.label(language)))
            .collect::<Vec<_>>(),
        "temperature_unit": [
            choice(Unit::Celsius, t("celsius")),
            choice(Unit::Fahrenheit, t("fahrenheit")),
        ],
        "weather_style": WeatherStyle::ALL
            .iter()
            .map(|item| choice(item, t(item.label_key())))
            .collect::<Vec<_>>(),
        "input_mode": [
            serde_json::json!({ "value": InputMode::On, "label": t("stateOn"), "hint": t("inputAlways") }),
            serde_json::json!({ "value": InputMode::Off, "label": t("stateOff"), "hint": t("inputNever") }),
            serde_json::json!({ "value": InputMode::Auto, "label": t("stateAuto"), "hint": t("inputWhenConnected") }),
        ],
        "input_icon": std::iter::once(choice("", t("inputIconAutomatic")))
            .chain(INPUT_ICONS.iter().map(|(icon, key)| choice(icon, t(key))))
            .collect::<Vec<_>>(),
        "text_rendering": TextRendering::ALL
            .iter()
            .map(|item| choice(item, t(item.label_key())))
            .collect::<Vec<_>>(),
    });
    serde_json::json!({
        "settings": settings,
        "strings": strings(language),
        "backgrounds": catalog.backgrounds,
        "library": catalog.library,
        "inputs": catalog.inputs,
        "weatherLocation": catalog.weather_location,
        "tiles": catalog.tiles,
        "tileIcons": crate::model::TILE_ICONS
            .iter()
            .map(|(icon, name)| serde_json::json!({ "id": icon, "name": t(name) }))
            .collect::<Vec<_>>(),
        "tileLimit": crate::model::TILE_LIMIT,
        "sheet": { "tiles": crate::view::SHEET_TILES, "columns": crate::view::SHEET_COLUMNS },
        "labels": labels,
        "choices": choices,
    })
}
