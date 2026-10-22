//! Bundled English, German, Spanish and Chinese message catalog.
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};
use time::OffsetDateTime;

type Messages = BTreeMap<String, BTreeMap<String, String>>;
fn messages() -> &'static Messages {
    static CATALOG: OnceLock<Messages> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/messages.json"))
            .expect("Bundled translations must be valid")
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    De,
    Zh,
    Es,
}
impl Language {
    pub const ALL: [Self; 4] = [Self::En, Self::De, Self::Zh, Self::Es];
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
            Self::Zh => "zh",
            Self::Es => "es",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::De => "Deutsch",
            Self::Zh => "简体中文",
            Self::Es => "Español",
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::En => 0,
            Self::De => 1,
            Self::Zh => 2,
            Self::Es => 3,
        }
    }
    pub fn t(self, key: &str) -> &str {
        messages()
            .get(key)
            .and_then(|entry| entry.get(self.code()).or_else(|| entry.get("en")))
            .map(String::as_str)
            .unwrap_or(key)
    }
    pub fn format(self, key: &str, values: &[(&str, &str)]) -> String {
        let mut value = self.t(key).to_owned();
        for (name, replacement) in values {
            value = value.replace(&format!("{{{name}}}"), replacement);
        }
        value
    }
    pub fn category(self, value: &str) -> &str {
        match value {
            "Streaming" => self.t("categoryStreaming"),
            "Music" => self.t("categoryMusic"),
            "Gaming" => self.t("categoryGaming"),
            "Utilities" => self.t("categoryUtilities"),
            _ => self.t("categoryUtilities"),
        }
    }
    pub fn wallpaper(self, id: &str) -> &str {
        self.t(match id {
            "dunes" => "monumentValley",
            "san-francisco" => "sanFrancisco",
            "northern-lights" => "northernLights",
            _ => id,
        })
    }
    pub fn greeting(self, hour: u8, name: &str) -> String {
        let key = match hour {
            0..=4 => "greetingNight",
            5..=11 => "morning",
            12..=17 => "afternoon",
            _ => "evening",
        };
        let name = name.trim();
        if name.is_empty() {
            self.t(key).to_owned()
        } else {
            self.format(&format!("{key}Named"), &[("name", name)])
        }
    }
    pub fn date(self, now: OffsetDateTime) -> String {
        let weekday = now.weekday().number_days_from_monday() as usize;
        let month = now.month() as usize - 1;
        match self {
            Self::En => format!(
                "{}, {} {}",
                [
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                    "Sunday"
                ][weekday],
                [
                    "January",
                    "February",
                    "March",
                    "April",
                    "May",
                    "June",
                    "July",
                    "August",
                    "September",
                    "October",
                    "November",
                    "December"
                ][month],
                now.day()
            )
            .to_uppercase(),
            Self::De => format!(
                "{}, {}. {}",
                [
                    "Montag",
                    "Dienstag",
                    "Mittwoch",
                    "Donnerstag",
                    "Freitag",
                    "Samstag",
                    "Sonntag"
                ][weekday],
                now.day(),
                [
                    "Januar",
                    "Februar",
                    "März",
                    "April",
                    "Mai",
                    "Juni",
                    "Juli",
                    "August",
                    "September",
                    "Oktober",
                    "November",
                    "Dezember"
                ][month]
            )
            .to_uppercase(),
            Self::Zh => format!(
                "{}月{}日星期{}",
                now.month() as u8,
                now.day(),
                ["一", "二", "三", "四", "五", "六", "日"][weekday]
            ),
            Self::Es => format!(
                "{}, {} de {}",
                [
                    "lunes",
                    "martes",
                    "miércoles",
                    "jueves",
                    "viernes",
                    "sábado",
                    "domingo"
                ][weekday],
                now.day(),
                [
                    "enero",
                    "febrero",
                    "marzo",
                    "abril",
                    "mayo",
                    "junio",
                    "julio",
                    "agosto",
                    "septiembre",
                    "octubre",
                    "noviembre",
                    "diciembre"
                ][month]
            )
            .to_uppercase(),
        }
    }
    pub fn clock(self, now: OffsetDateTime, hour24: bool) -> String {
        if hour24 {
            return if self == Self::En {
                format!("{:02}:{:02}", now.hour(), now.minute())
            } else {
                format!("{}:{:02}", now.hour(), now.minute())
            };
        }
        let hour = match now.hour() % 12 {
            0 => 12,
            value => value,
        };
        if self == Self::Zh {
            format!(
                "{}{hour}:{:02}",
                if now.hour() >= 12 { "下午" } else { "上午" },
                now.minute()
            )
        } else {
            format!(
                "{hour}:{:02} {}",
                now.minute(),
                if now.hour() >= 12 { "PM" } else { "AM" }
            )
        }
    }
}
pub fn deserialize_language<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Language, D::Error> {
    Ok(
        match serde_json::Value::deserialize(deserializer)?.as_str() {
            Some("de") => Language::De,
            Some("zh") => Language::Zh,
            Some("es") => Language::Es,
            _ => Language::En,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_message_has_all_three_languages_and_no_empty_values() {
        for (key, entry) in messages() {
            for language in Language::ALL {
                assert!(
                    !entry
                        .get(language.code())
                        .unwrap_or_else(|| panic!("Missing {key} / {}", language.code()))
                        .is_empty()
                );
            }
        }
    }
    #[test]
    fn localization_keeps_user_provided_names_intact() {
        assert_eq!(
            Language::De.format("favoriteAdded", &[("app", "Netflix")]),
            "Netflix zu Favoriten hinzugefügt."
        );
        assert_eq!(
            Language::Es.format("favoriteAdded", &[("app", "Netflix")]),
            "Netflix añadida a favoritos."
        );
        assert_eq!(
            Language::Zh.format("hidden", &[("count", "3")]),
            "已隐藏（3）"
        );
        assert_eq!(Language::De.category("Gaming"), "Spiele");
        assert_eq!(Language::En.greeting(15, ""), "Good afternoon.");
        assert_eq!(Language::En.greeting(15, "  Ana "), "Good afternoon, Ana.");
        assert_eq!(Language::Es.greeting(2, "Ana"), "¿Aún despiertos, Ana?");
        assert_eq!(Language::Zh.greeting(20, "小明"), "小明，晚上好。");
    }
    #[test]
    fn clock_and_date_follow_the_selected_locale() {
        let now = OffsetDateTime::parse(
            "2026-10-21T20:41:00+02:00",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        assert_eq!(Language::De.date(now), "MITTWOCH, 21. OKTOBER");
        assert_eq!(Language::Es.date(now), "MIÉRCOLES, 21 DE OCTUBRE");
        assert_eq!(Language::Zh.date(now), "10月21日星期三");
        assert_eq!(Language::Zh.clock(now, false), "下午8:41");
        assert_eq!(Language::En.clock(now, true), "20:41");
    }
}
