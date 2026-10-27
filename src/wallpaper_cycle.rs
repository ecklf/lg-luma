//! Automatic background cycling with one fixed interval and no per-image exclusions.
use serde::{Deserialize, Deserializer};
use std::time::Duration;

pub const INTERVAL: u64 = 15 * 60;

pub fn due(enabled: bool, changed: u64, now: u64) -> bool {
    enabled && now.saturating_sub(changed) >= INTERVAL
}

pub fn startup_due(enabled: bool, pending: bool, catalog_ready: bool, elapsed: Duration) -> bool {
    enabled && pending && (catalog_ready || elapsed >= Duration::from_secs(3))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Bool(enabled) => enabled,
        // Preserve enabled preferences from the former duration-based UI.
        serde_json::Value::String(mode) => matches!(
            mode.as_str(),
            "on" | "launch" | "quarter" | "hourly" | "daily"
        ),
        _ => false,
    })
}

/// Advance in gallery order and wrap; unavailable images are absent from `ids`.
pub fn next(ids: &[String], current: &str) -> Option<String> {
    let start = ids
        .iter()
        .position(|id| id == current)
        .map_or(0, |index| index + 1);
    (0..ids.len())
        .map(|offset| &ids[(start + offset) % ids.len()])
        .find(|id| *id != current)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_available_background_participates_in_order_and_wraps() {
        let ids = vec!["slate".into(), "dunes".into(), "custom-id".into()];
        assert_eq!(next(&ids, "slate").as_deref(), Some("dunes"));
        assert_eq!(next(&ids, "dunes").as_deref(), Some("custom-id"));
        assert_eq!(next(&ids, "custom-id").as_deref(), Some("slate"));
        assert_eq!(next(&ids, "missing").as_deref(), Some("slate"));
        assert_eq!(next(&["slate".into()], "slate"), None);
        assert_eq!(next(&[], "slate"), None);
    }

    #[test]
    fn cycling_is_due_only_when_enabled_after_fifteen_minutes() {
        assert!(!due(false, 0, u64::MAX));
        assert!(!due(true, 100, 100 + INTERVAL - 1));
        assert!(due(true, 100, 100 + INTERVAL));
        assert!(!due(true, 1_800_000_000, 1_577_836_800));
        assert!(!due(true, u64::MAX, u64::MAX));
    }

    #[test]
    fn enabled_rotation_advances_once_after_startup_catalog_or_timeout() {
        assert!(!startup_due(false, true, true, Duration::ZERO));
        assert!(!startup_due(true, false, true, Duration::ZERO));
        assert!(!startup_due(true, true, false, Duration::from_secs(2)));
        assert!(startup_due(true, true, true, Duration::ZERO));
        assert!(startup_due(true, true, false, Duration::from_secs(3)));
    }

    #[test]
    fn on_off_preferences_default_off_round_trip_and_load_legacy_modes() {
        assert!(!crate::model::Preferences::default().wallpaper_cycle);
        for enabled in [false, true] {
            let preferences: crate::model::Preferences =
                serde_json::from_value(serde_json::json!({"wallpaper_cycle": enabled})).unwrap();
            assert_eq!(preferences.wallpaper_cycle, enabled);
            assert_eq!(
                serde_json::to_value(preferences).unwrap()["wallpaper_cycle"],
                enabled
            );
        }
        for mode in ["on", "launch", "quarter", "hourly", "daily"] {
            let preferences: crate::model::Preferences =
                serde_json::from_value(serde_json::json!({"wallpaper_cycle": mode})).unwrap();
            assert!(preferences.wallpaper_cycle);
        }
        for value in [
            serde_json::json!("off"),
            serde_json::json!("weekly"),
            serde_json::json!(3),
            serde_json::Value::Null,
        ] {
            let preferences: crate::model::Preferences = serde_json::from_value(
                serde_json::json!({"wallpaper_cycle": value, "greeting_name": "Ana"}),
            )
            .unwrap();
            assert!(!preferences.wallpaper_cycle);
            assert_eq!(preferences.greeting_name, "Ana");
        }
    }
}
