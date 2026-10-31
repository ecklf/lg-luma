//! Bundled translations, including Simplified and Traditional Chinese.
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
    #[serde(rename = "zh-tw", alias = "zh-TW", alias = "zh-Hant")]
    ZhTw,
    Es,
    Pt,
    It,
    Ru,
    Pl,
    Fr,
    Vi,
    Nl,
    Sv,
}
impl Language {
    pub const ALL: [Self; 13] = [
        Self::De,
        Self::En,
        Self::Es,
        Self::Fr,
        Self::It,
        Self::Nl,
        Self::Pl,
        Self::Pt,
        Self::Sv,
        Self::Vi,
        Self::Ru,
        Self::Zh,
        Self::ZhTw,
    ];
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
            Self::Zh => "zh",
            Self::ZhTw => "zh-tw",
            Self::Es => "es",
            Self::Pt => "pt",
            Self::It => "it",
            Self::Ru => "ru",
            Self::Pl => "pl",
            Self::Fr => "fr",
            Self::Vi => "vi",
            Self::Nl => "nl",
            Self::Sv => "sv",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::De => "Deutsch",
            Self::Zh => "简体中文",
            Self::ZhTw => "繁體中文",
            Self::Es => "Español",
            Self::Pt => "Português (Brasil)",
            Self::It => "Italiano",
            Self::Ru => "Русский",
            Self::Pl => "Polski",
            Self::Fr => "Français",
            Self::Vi => "Tiếng Việt",
            Self::Nl => "Nederlands",
            Self::Sv => "Svenska",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|language| *language == self)
            .unwrap()
    }
    /// Open-Meteo geocoding exposes one Chinese locale, not a script-specific variant.
    pub fn geocoding_code(self) -> &'static str {
        if self == Self::ZhTw {
            "zh"
        } else {
            self.code()
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
            Self::Zh | Self::ZhTw => format!(
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
            Self::Pt => format!(
                "{}, {} de {}",
                [
                    "segunda-feira",
                    "terça-feira",
                    "quarta-feira",
                    "quinta-feira",
                    "sexta-feira",
                    "sábado",
                    "domingo"
                ][weekday],
                now.day(),
                [
                    "janeiro",
                    "fevereiro",
                    "março",
                    "abril",
                    "maio",
                    "junho",
                    "julho",
                    "agosto",
                    "setembro",
                    "outubro",
                    "novembro",
                    "dezembro"
                ][month]
            )
            .to_uppercase(),
            Self::It => format!(
                "{} {} {}",
                [
                    "lunedì",
                    "martedì",
                    "mercoledì",
                    "giovedì",
                    "venerdì",
                    "sabato",
                    "domenica"
                ][weekday],
                now.day(),
                [
                    "gennaio",
                    "febbraio",
                    "marzo",
                    "aprile",
                    "maggio",
                    "giugno",
                    "luglio",
                    "agosto",
                    "settembre",
                    "ottobre",
                    "novembre",
                    "dicembre"
                ][month]
            )
            .to_uppercase(),
            Self::Ru => format!(
                "{}, {} {}",
                [
                    "понедельник",
                    "вторник",
                    "среда",
                    "четверг",
                    "пятница",
                    "суббота",
                    "воскресенье"
                ][weekday],
                now.day(),
                [
                    "января",
                    "февраля",
                    "марта",
                    "апреля",
                    "мая",
                    "июня",
                    "июля",
                    "августа",
                    "сентября",
                    "октября",
                    "ноября",
                    "декабря"
                ][month]
            )
            .to_uppercase(),
            Self::Pl => format!(
                "{}, {} {}",
                [
                    "poniedziałek",
                    "wtorek",
                    "środa",
                    "czwartek",
                    "piątek",
                    "sobota",
                    "niedziela"
                ][weekday],
                now.day(),
                [
                    "stycznia",
                    "lutego",
                    "marca",
                    "kwietnia",
                    "maja",
                    "czerwca",
                    "lipca",
                    "sierpnia",
                    "września",
                    "października",
                    "listopada",
                    "grudnia"
                ][month]
            )
            .to_uppercase(),
            Self::Fr => format!(
                "{} {} {}",
                [
                    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"
                ][weekday],
                match now.day() {
                    1 => "1er".to_owned(),
                    day => day.to_string(),
                },
                [
                    "janvier",
                    "février",
                    "mars",
                    "avril",
                    "mai",
                    "juin",
                    "juillet",
                    "août",
                    "septembre",
                    "octobre",
                    "novembre",
                    "décembre"
                ][month]
            )
            .to_uppercase(),
            Self::Vi => format!(
                "{}, {} {}",
                [
                    "thứ hai",
                    "thứ ba",
                    "thứ tư",
                    "thứ năm",
                    "thứ sáu",
                    "thứ bảy",
                    "chủ nhật"
                ][weekday],
                now.day(),
                [
                    "tháng 1",
                    "tháng 2",
                    "tháng 3",
                    "tháng 4",
                    "tháng 5",
                    "tháng 6",
                    "tháng 7",
                    "tháng 8",
                    "tháng 9",
                    "tháng 10",
                    "tháng 11",
                    "tháng 12"
                ][month]
            )
            .to_uppercase(),
            Self::Nl => format!(
                "{} {} {}",
                [
                    "maandag",
                    "dinsdag",
                    "woensdag",
                    "donderdag",
                    "vrijdag",
                    "zaterdag",
                    "zondag"
                ][weekday],
                now.day(),
                [
                    "januari",
                    "februari",
                    "maart",
                    "april",
                    "mei",
                    "juni",
                    "juli",
                    "augustus",
                    "september",
                    "oktober",
                    "november",
                    "december"
                ][month]
            )
            .to_uppercase(),
            Self::Sv => format!(
                "{} {} {}",
                [
                    "måndag", "tisdag", "onsdag", "torsdag", "fredag", "lördag", "söndag"
                ][weekday],
                now.day(),
                [
                    "januari",
                    "februari",
                    "mars",
                    "april",
                    "maj",
                    "juni",
                    "juli",
                    "augusti",
                    "september",
                    "oktober",
                    "november",
                    "december"
                ][month]
            )
            .to_uppercase(),
        }
    }
    pub fn clock(self, now: OffsetDateTime, hour24: bool, seconds: bool) -> String {
        let base = self.clock_base(now, hour24);
        if !seconds {
            return base;
        }
        let tail = format!(":{:02}", now.second());
        match base.find(' ') {
            Some(space) => format!("{}{tail}{}", &base[..space], &base[space..]),
            None => format!("{base}{tail}"),
        }
    }
    fn clock_base(self, now: OffsetDateTime, hour24: bool) -> String {
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
        if matches!(self, Self::Zh | Self::ZhTw) {
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
            Some("zh-tw" | "zh-TW" | "zh-Hant") => Language::ZhTw,
            Some("es") => Language::Es,
            Some("pt") => Language::Pt,
            Some("it") => Language::It,
            Some("ru") => Language::Ru,
            Some("pl") => Language::Pl,
            Some("fr") => Language::Fr,
            Some("vi") => Language::Vi,
            Some("nl") => Language::Nl,
            Some("sv") => Language::Sv,
            _ => Language::En,
        },
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn traditional_chinese_follows_simplified_and_preserves_locale_preferences() {
        use super::Language;
        assert_eq!(Language::ALL[Language::Zh.index() + 1], Language::ZhTw);
        assert_eq!(Language::ZhTw.name(), "繁體中文");
        assert_eq!(Language::ZhTw.code(), "zh-tw");
        assert_eq!(Language::ZhTw.geocoding_code(), "zh");
        for code in ["zh-tw", "zh-TW", "zh-Hant"] {
            let preferences: crate::model::Preferences =
                serde_json::from_value(serde_json::json!({"language": code})).unwrap();
            assert_eq!(preferences.language, Language::ZhTw);
            assert_eq!(
                serde_json::to_value(&preferences).unwrap()["language"],
                "zh-tw"
            );
        }
        let now = time::OffsetDateTime::parse(
            "2026-10-21T20:41:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        assert_eq!(Language::ZhTw.date(now), "10月21日星期三");
        assert_eq!(Language::ZhTw.clock(now, false, false), "下午8:41");
        assert_eq!(Language::ZhTw.clock(now, false, true), "下午8:41:00");
        assert_eq!(Language::ZhTw.greeting(20, "小明"), "小明，晚安。");
    }

    #[test]
    fn bundled_cjk_font_covers_every_traditional_chinese_message() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/noto-sans-sc.ttf"
        ))
        .unwrap();
        let font = crate::font::Font::from_bytes(bytes).unwrap();
        let extra = "繁體中文一二三四五六日月星期上午下午";
        for character in super::messages()
            .values()
            .flat_map(|entry| entry["zh-tw"].chars())
            .chain(extra.chars())
        {
            if ('\u{3000}'..='\u{9fff}').contains(&character) {
                assert_ne!(
                    font.lookup_glyph_index(character),
                    0,
                    "Missing Traditional Chinese glyph {character}"
                );
            }
        }
    }

    #[test]
    fn homebrew_description_lists_every_supported_language() {
        let description = include_str!("../assets/homebrew-description.html");
        for language in super::Language::ALL {
            let name = match language {
                super::Language::En => "English",
                super::Language::De => "German",
                super::Language::Zh | super::Language::ZhTw => "Simplified / Traditional Chinese",
                super::Language::Es => "Spanish",
                super::Language::Pt => "Portuguese (Brazil)",
                super::Language::It => "Italian",
                super::Language::Ru => "Russian",
                super::Language::Pl => "Polish",
                super::Language::Fr => "French",
                super::Language::Vi => "Vietnamese",
                super::Language::Nl => "Dutch",
                super::Language::Sv => "Swedish",
            };
            assert!(
                description.contains(name),
                "Homebrew description is missing {name}"
            );
        }
    }
    use super::*;
    #[test]
    fn every_message_has_all_languages_and_no_empty_values() {
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
    fn every_translation_preserves_the_english_message_placeholders() {
        fn placeholders(message: &str) -> std::collections::BTreeSet<&str> {
            message
                .split('{')
                .skip(1)
                .map(|part| {
                    part.split_once('}')
                        .expect("Unclosed message placeholder")
                        .0
                })
                .collect()
        }

        for (key, entry) in messages() {
            let expected = placeholders(&entry["en"]);
            for language in Language::ALL {
                assert_eq!(
                    placeholders(&entry[language.code()]),
                    expected,
                    "Mismatched placeholders in {key} / {}",
                    language.code()
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
            Language::Pt.format("favoriteAdded", &[("app", "Netflix")]),
            "Netflix adicionado aos favoritos."
        );
        assert_eq!(
            Language::Zh.format("hidden", &[("count", "3")]),
            "已隐藏（3）"
        );
        assert_eq!(
            Language::Ru.format("favoriteAdded", &[("app", "Netflix")]),
            "Netflix: добавлено в избранное."
        );
        assert_eq!(
            Language::Pl.format("favoriteAdded", &[("app", "Netflix")]),
            "Dodano Netflix do ulubionych."
        );
        assert_eq!(
            Language::Fr.format("favoriteAdded", &[("app", "Netflix")]),
            "Netflix fait maintenant partie de vos favoris."
        );
        assert_eq!(Language::Fr.greeting(2, "Ana"), "Toujours debout, Ana ?");
        assert_eq!(
            Language::Vi.format("favoriteAdded", &[("app", "Netflix")]),
            "Đã thêm Netflix vào mục ưa thích."
        );
        assert_eq!(Language::Vi.greeting(2, "Ana"), "Vẫn chưa ngủ sao, Ana?");
        assert_eq!(
            Language::Nl.format("favoriteAdded", &[("app", "Netflix")]),
            "Netflix toegevoegd aan favorieten."
        );
        assert_eq!(Language::Nl.greeting(2, "Ana"), "Nog wakker, Ana?");
        assert_eq!(Language::Nl.category("Gaming"), "Games");
        assert_eq!(
            Language::Sv.format("favoriteAdded", &[("app", "Netflix")]),
            "Netflix har lagts till bland favoriterna."
        );
        assert_eq!(Language::Sv.greeting(2, "Ana"), "Fortfarande uppe, Ana?");
        assert_eq!(Language::Sv.category("Gaming"), "Spel");
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
        assert_eq!(Language::Pt.date(now), "QUARTA-FEIRA, 21 DE OUTUBRO");
        assert_eq!(Language::Zh.date(now), "10月21日星期三");
        assert_eq!(Language::Ru.date(now), "СРЕДА, 21 ОКТЯБРЯ");
        assert_eq!(Language::Ru.clock(now, true, false), "20:41");
        assert_eq!(Language::Pl.date(now), "ŚRODA, 21 PAŹDZIERNIKA");
        assert_eq!(Language::Pl.clock(now, true, false), "20:41");
        assert_eq!(Language::Fr.date(now), "MERCREDI 21 OCTOBRE");
        assert_eq!(Language::Fr.clock(now, true, false), "20:41");
        assert_eq!(Language::Vi.date(now), "THỨ TƯ, 21 THÁNG 10");
        assert_eq!(Language::Vi.clock(now, true, false), "20:41");
        assert_eq!(Language::Nl.date(now), "WOENSDAG 21 OKTOBER");
        assert_eq!(Language::Nl.clock(now, true, false), "20:41");
        assert_eq!(Language::Sv.date(now), "ONSDAG 21 OKTOBER");
        assert_eq!(Language::Sv.clock(now, true, false), "20:41");
        assert_eq!(Language::Zh.clock(now, false, false), "下午8:41");
        assert_eq!(Language::Zh.clock(now, false, true), "下午8:41:00");
        assert_eq!(Language::En.clock(now, true, false), "20:41");
        assert_eq!(Language::En.clock(now, true, true), "20:41:00");
        assert_eq!(Language::En.clock(now, false, true), "8:41:00 PM");
    }
    #[test]
    fn portuguese_dates_include_accented_names_and_handle_leap_days() {
        for (date, expected) in [
            ("2024-02-29T00:05:00-03:00", "QUINTA-FEIRA, 29 DE FEVEREIRO"),
            ("2026-03-03T12:00:00-03:00", "TERÇA-FEIRA, 3 DE MARÇO"),
            ("2026-10-24T20:41:00-03:00", "SÁBADO, 24 DE OUTUBRO"),
        ] {
            let now = OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                .unwrap();
            assert_eq!(Language::Pt.date(now), expected);
        }
    }
    #[test]
    fn french_dates_use_the_ordinal_first_day_and_accented_months() {
        for (date, expected) in [
            ("2026-08-01T09:00:00+02:00", "SAMEDI 1ER AOÛT"),
            ("2026-02-14T12:00:00+01:00", "SAMEDI 14 FÉVRIER"),
            ("2026-12-25T20:41:00+01:00", "VENDREDI 25 DÉCEMBRE"),
        ] {
            let now = OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                .unwrap();
            assert_eq!(Language::Fr.date(now), expected);
        }
    }

    #[test]
    fn vietnamese_dates_cover_leap_days_and_weekend_names() {
        for (date, expected) in [
            ("2024-02-29T00:05:00+07:00", "THỨ NĂM, 29 THÁNG 2"),
            ("2026-10-24T12:00:00+07:00", "THỨ BẢY, 24 THÁNG 10"),
            ("2026-11-01T20:41:00+07:00", "CHỦ NHẬT, 1 THÁNG 11"),
        ] {
            let now = OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                .unwrap();
            assert_eq!(Language::Vi.date(now), expected);
        }
    }

    #[test]
    fn dutch_dates_cover_leap_days_and_month_boundaries() {
        for (date, expected) in [
            ("2024-02-29T00:05:00+01:00", "DONDERDAG 29 FEBRUARI"),
            ("2026-05-01T12:00:00+02:00", "VRIJDAG 1 MEI"),
            ("2026-12-27T20:41:00+01:00", "ZONDAG 27 DECEMBER"),
        ] {
            let now = OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                .unwrap();
            assert_eq!(Language::Nl.date(now), expected);
        }
    }
}
