//! Persisted whole-image edge contrast, independent of the font rendering mode.
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageClarity {
    #[default]
    Off,
    Mild,
    Strong,
}

impl ImageClarity {
    pub const ALL: [Self; 3] = [Self::Off, Self::Mild, Self::Strong];

    pub fn index(self) -> usize {
        match self {
            Self::Off => 0,
            Self::Mild => 1,
            Self::Strong => 2,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Off => "imageClarityOff",
            Self::Mild => "imageClarityMild",
            Self::Strong => "imageClarityStrong",
        }
    }

    /// Edge-contrast gain of the settled-frame filter.
    pub fn strength(self) -> f32 {
        match self {
            Self::Off => 0.,
            Self::Mild => 0.35,
            Self::Strong => 0.6,
        }
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ImageClarity, D::Error> {
    Ok(
        match serde_json::Value::deserialize(deserializer)?.as_str() {
            Some("mild") => ImageClarity::Mild,
            Some("strong") => ImageClarity::Strong,
            _ => ImageClarity::Off,
        },
    )
}

/// `LUMA_SHARPEN` (0 to 0.6) overrides the saved choice for diagnostics; invalid values keep it.
pub fn configured(preference: ImageClarity, value: Option<&str>) -> f32 {
    value
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0., 0.6))
        .unwrap_or(preference.strength())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_invalid_preferences_preserve_other_settings_and_env_is_bounded() {
        for value in [
            serde_json::json!({}),
            serde_json::json!({"image_clarity":false}),
            serde_json::json!({"image_clarity":"unknown"}),
        ] {
            let mut value = value;
            value["greeting_name"] = serde_json::json!("Ana");
            let prefs: crate::model::Preferences = serde_json::from_value(value).unwrap();
            assert_eq!(prefs.image_clarity, ImageClarity::Off);
            assert_eq!(prefs.greeting_name, "Ana");
        }
        for value in [None, Some("bad"), Some("NaN"), Some("inf")] {
            assert_eq!(configured(ImageClarity::Mild, value), 0.35);
        }
        assert_eq!(configured(ImageClarity::Mild, Some("0")), 0.);
        assert_eq!(configured(ImageClarity::Off, Some("10")), 0.6);
        assert_eq!(configured(ImageClarity::Strong, Some("-1")), 0.);
    }
}
