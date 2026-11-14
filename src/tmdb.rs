//! TMDB Top Shelf: popular shows for mapped streaming apps (or global trending for Stremio).
use crate::model::App;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const REVEAL_MS: u64 = 5_000;
pub const ROTATE_MS: u64 = 8_000;
pub const MAX_SHOWS: usize = 3;
pub const CACHE_TTL_SECS: i64 = 12 * 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Catalog {
    /// JustWatch / TMDB watch-provider id(s); first matching discover reply wins.
    Providers(&'static [u32]),
    /// No platform filter (Stremio / aggregators).
    Trending,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Show {
    pub id: u64,
    pub name: String,
    pub overview: String,
    pub backdrop: String,
    /// Local cache path once the backdrop has been downloaded.
    #[serde(skip)]
    pub backdrop_path: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub shows: HashMap<String, Vec<Show>>,
    pub focus_app: Option<String>,
    pub focus_since: Option<std::time::Instant>,
    pub revealed: bool,
    pub selected: usize,
    pub rotate_since: Option<std::time::Instant>,
    pub visibility: f32,
    pub pending: Option<String>,
    pub generation: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Refresh {
        app_id: String,
        catalog: Catalog,
        key: String,
        region: String,
        language: String,
    },
}

#[derive(Clone, Debug)]
pub struct Response {
    pub generation: u64,
    pub app_id: String,
    pub result: Result<Vec<Show>, String>,
}

/// Streaming region for `watch_region` / trending locale hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub code: &'static str,
    pub name: &'static str,
}

pub const REGIONS: &[Region] = &[
    Region {
        code: "auto",
        name: "Automatic",
    },
    Region {
        code: "ES",
        name: "Spain",
    },
    Region {
        code: "US",
        name: "United States",
    },
    Region {
        code: "GB",
        name: "United Kingdom",
    },
    Region {
        code: "FR",
        name: "France",
    },
    Region {
        code: "DE",
        name: "Germany",
    },
    Region {
        code: "IT",
        name: "Italy",
    },
    Region {
        code: "PT",
        name: "Portugal",
    },
    Region {
        code: "NL",
        name: "Netherlands",
    },
    Region {
        code: "BE",
        name: "Belgium",
    },
    Region {
        code: "MX",
        name: "Mexico",
    },
    Region {
        code: "AR",
        name: "Argentina",
    },
    Region {
        code: "BR",
        name: "Brazil",
    },
    Region {
        code: "CA",
        name: "Canada",
    },
    Region {
        code: "AU",
        name: "Australia",
    },
    Region {
        code: "IN",
        name: "India",
    },
    Region {
        code: "JP",
        name: "Japan",
    },
    Region {
        code: "KR",
        name: "South Korea",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuggestLanguage {
    pub code: &'static str,
    pub name: &'static str,
}

pub const SUGGEST_LANGUAGES: &[SuggestLanguage] = &[
    SuggestLanguage {
        code: "auto",
        name: "Automatic",
    },
    SuggestLanguage {
        code: "ca-ES",
        name: "Català",
    },
    SuggestLanguage {
        code: "es-ES",
        name: "Español",
    },
    SuggestLanguage {
        code: "en-US",
        name: "English",
    },
    SuggestLanguage {
        code: "fr-FR",
        name: "Français",
    },
    SuggestLanguage {
        code: "de-DE",
        name: "Deutsch",
    },
    SuggestLanguage {
        code: "it-IT",
        name: "Italiano",
    },
    SuggestLanguage {
        code: "pt-BR",
        name: "Português (Brasil)",
    },
    SuggestLanguage {
        code: "pt-PT",
        name: "Português",
    },
    SuggestLanguage {
        code: "nl-NL",
        name: "Nederlands",
    },
];

pub fn region_index(code: &str) -> usize {
    REGIONS
        .iter()
        .position(|region| region.code.eq_ignore_ascii_case(code))
        .unwrap_or(0)
}

pub fn language_index(code: &str) -> usize {
    SUGGEST_LANGUAGES
        .iter()
        .position(|language| language.code.eq_ignore_ascii_case(code))
        .unwrap_or(0)
}

pub fn sanitize_key(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(64)
        .collect()
}

pub fn resolve_region(pref: &str, ui: crate::i18n::Language) -> String {
    if pref != "auto" && REGIONS.iter().any(|region| region.code == pref) {
        return pref.into();
    }
    match ui {
        crate::i18n::Language::Es | crate::i18n::Language::Ca => "ES".into(),
        crate::i18n::Language::Pt => "BR".into(),
        crate::i18n::Language::Fr => "FR".into(),
        crate::i18n::Language::De => "DE".into(),
        crate::i18n::Language::It => "IT".into(),
        crate::i18n::Language::Nl => "NL".into(),
        crate::i18n::Language::Zh | crate::i18n::Language::ZhTw => "US".into(),
        _ => "US".into(),
    }
}

pub fn resolve_language(pref: &str, ui: crate::i18n::Language) -> String {
    if pref != "auto"
        && SUGGEST_LANGUAGES
            .iter()
            .any(|language| language.code == pref)
    {
        return pref.into();
    }
    match ui {
        crate::i18n::Language::Ca => "ca-ES".into(),
        crate::i18n::Language::Es => "es-ES".into(),
        crate::i18n::Language::Pt => "pt-BR".into(),
        crate::i18n::Language::Fr => "fr-FR".into(),
        crate::i18n::Language::De => "de-DE".into(),
        crate::i18n::Language::It => "it-IT".into(),
        crate::i18n::Language::Nl => "nl-NL".into(),
        crate::i18n::Language::Zh => "zh-CN".into(),
        crate::i18n::Language::ZhTw => "zh-TW".into(),
        _ => "en-US".into(),
    }
}

pub fn catalog_for(app: &App) -> Option<Catalog> {
    let id = app.id.to_lowercase();
    let title = app.title.to_lowercase();
    let brand = app.brand.to_lowercase();
    if id.contains("stremio") || title.contains("stremio") {
        return Some(Catalog::Trending);
    }
    if brand == "netflix" || id == "netflix" {
        return Some(Catalog::Providers(&[8]));
    }
    if brand == "prime-video" || id == "amazon" || id.contains("primevideo") {
        return Some(Catalog::Providers(&[9, 119]));
    }
    if brand == "disney-plus" || id.contains("disney") {
        return Some(Catalog::Providers(&[337]));
    }
    if brand == "youtube" {
        return None;
    }
    if id.contains("appletv") || id.contains("apple.tv") || brand.contains("apple-tv") {
        return Some(Catalog::Providers(&[350]));
    }
    if id.contains("wbd")
        || id.contains("hbomax")
        || (title.contains("max") && id.contains("stream"))
    {
        return Some(Catalog::Providers(&[1899, 384]));
    }
    if id.contains("rtve") || title.contains("rtve") || title.contains("play rtve") {
        return Some(Catalog::Providers(&[541]));
    }
    if id.contains("3cat") || id.contains("tv3") || title.contains("3cat") || title == "tv3" {
        return Some(Catalog::Providers(&[2237]));
    }
    if title.contains("atresplayer") || id.contains("atres") {
        return Some(Catalog::Providers(&[62]));
    }
    if title.contains("mitele") || id.contains("mitele") {
        return Some(Catalog::Providers(&[456]));
    }
    if title.contains("movistar") || id.contains("movistar") {
        return Some(Catalog::Providers(&[2241, 149]));
    }
    None
}

pub fn parse_shows(raw: &Value) -> Vec<Show> {
    raw.get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_u64()?;
            let name = entry
                .get("name")
                .or_else(|| entry.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if name.is_empty() {
                return None;
            }
            let backdrop = entry.get("backdrop_path").and_then(Value::as_str)?;
            if backdrop.is_empty() {
                return None;
            }
            let overview = entry
                .get("overview")
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control())
                .take(280)
                .collect();
            Some(Show {
                id,
                name: name.chars().take(120).collect(),
                overview,
                backdrop: backdrop.into(),
                backdrop_path: None,
            })
        })
        .take(MAX_SHOWS)
        .collect()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DiskCache {
    pub context: String,
    pub ts: i64,
    pub items: HashMap<String, Vec<Show>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_streaming_apps_and_stremio_trending() {
        assert_eq!(
            catalog_for(&App::new("netflix", "Netflix")),
            Some(Catalog::Providers(&[8]))
        );
        assert_eq!(
            catalog_for(&App::new("com.stremio.tv", "Stremio")),
            Some(Catalog::Trending)
        );
        assert_eq!(
            catalog_for(&App::new("com.example.rtve", "RTVE Play")),
            Some(Catalog::Providers(&[541]))
        );
        assert_eq!(
            catalog_for(&App::new("com.example.3cat", "3Cat")),
            Some(Catalog::Providers(&[2237]))
        );
        assert!(catalog_for(&App::new("youtube.leanback.v4", "YouTube")).is_none());
    }

    #[test]
    fn parses_discover_results() {
        let shows = parse_shows(&json!({
            "results": [
                {"id": 1, "name": "Silo", "backdrop_path": "/a.jpg", "overview": "Underground"},
                {"id": 2, "title": "No art"},
                {"id": 3, "name": "Slow Horses", "backdrop_path": "/b.jpg"}
            ]
        }));
        assert_eq!(shows.len(), 2);
        assert_eq!(shows[0].name, "Silo");
        assert_eq!(shows[1].name, "Slow Horses");
    }

    #[test]
    fn sanitizes_api_keys() {
        assert_eq!(sanitize_key(" ab-cd_12! "), "abcd12");
    }
}
