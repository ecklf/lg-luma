//! The launcher settings a paired phone or computer may change, and how the page labels them.
//! Labels come from the TV's own message catalog, so the page reads like Settings on the TV.
use crate::i18n::Language;
use crate::model::{AppSorting, DateFormat, Preferences, WeatherStyle, clean_input_name};
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

/// The page's whole view of the TV: current values plus every label in the TV's language, so the
/// page reads like the Settings the owner already knows.
pub fn listing(settings: &Settings, backgrounds: &[Background]) -> serde_json::Value {
    let language = settings.language;
    let t = |key| language.t(key);
    serde_json::json!({
        "settings": settings,
        "strings": strings(language),
        "backgrounds": backgrounds,
        "labels": {
            "general": t("interface"),
            "background": t("background"),
            "widgets": t("widgets"),
            "system": t("system"),
            "clock": t("clock"),
            "clockFormat": t("clockFormat"),
            "timeFormat": t("timeFormat"),
            "language": t("language"),
            "appSorting": t("appSorting"),
            "reducedMotion": t("reducedMotion"),
            "uiSounds": t("uiSounds"),
            "greeting": t("greeting"),
            "greetingName": t("greetingName"),
            "showWidget": t("showWidget"),
            "date": t("date"),
            "dateFormat": t("dateFormat"),
            "weather": t("weather"),
            "temperature": t("temperature"),
            "weatherStyle": t("weatherStyle"),
            "wallpaperCycle": t("wallpaperCycle"),
            "wallpaperGradient": t("wallpaperGradient"),
            "textRendering": t("textRendering"),
        },
        "choices": {
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
            "text_rendering": TextRendering::ALL
                .iter()
                .map(|item| choice(item, t(item.label_key())))
                .collect::<Vec<_>>(),
        },
    })
}
