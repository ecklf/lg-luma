//! Automatic wallpaper changes: when the next one is due and which one it is.
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperCycle {
    #[default]
    Off,
    /// Once per Luma start; as LG's Home that is each cold boot of the TV.
    Launch,
    Quarter,
    Hourly,
    Daily,
}

impl WallpaperCycle {
    pub const ALL: [Self; 5] = [
        Self::Off,
        Self::Launch,
        Self::Quarter,
        Self::Hourly,
        Self::Daily,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Off => "stateOff",
            Self::Launch => "wallpaperCycleLaunch",
            Self::Quarter => "wallpaperCycleQuarter",
            Self::Hourly => "wallpaperCycleHourly",
            Self::Daily => "wallpaperCycleDaily",
        }
    }

    /// Seconds between changes for the timed modes.
    pub fn interval(self) -> Option<u64> {
        match self {
            Self::Quarter => Some(15 * 60),
            Self::Hourly => Some(60 * 60),
            Self::Daily => Some(24 * 60 * 60),
            Self::Off | Self::Launch => None,
        }
    }

    /// A timed change is due once the interval passed since the last change (Unix seconds).
    /// The TV boots with its clock in 2020 until it syncs: a clock behind the last change waits.
    pub fn due(self, changed: u64, now: u64) -> bool {
        self.interval()
            .is_some_and(|interval| now >= changed.saturating_add(interval))
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<WallpaperCycle, D::Error> {
    Ok(
        match serde_json::Value::deserialize(deserializer)?.as_str() {
            Some("launch") => WallpaperCycle::Launch,
            Some("quarter") => WallpaperCycle::Quarter,
            Some("hourly") => WallpaperCycle::Hourly,
            Some("daily") => WallpaperCycle::Daily,
            _ => WallpaperCycle::Off,
        },
    )
}

/// The wallpaper after `current` in gallery order that is not skipped, wrapping around; None
/// when no other wallpaper takes part.
pub fn next(ids: &[String], current: &str, skipped: &[String]) -> Option<String> {
    let start = ids
        .iter()
        .position(|id| id == current)
        .map_or(0, |index| index + 1);
    (0..ids.len())
        .map(|offset| &ids[(start + offset) % ids.len()])
        .find(|id| *id != current && !skipped.contains(id))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn the_next_wallpaper_follows_gallery_order_skips_and_wraps() {
        let all = ids(&["slate", "dunes", "forest", "custom:Lake.jpg"]);
        assert_eq!(next(&all, "dunes", &[]).as_deref(), Some("forest"));
        assert_eq!(
            next(&all, "forest", &ids(&["custom:Lake.jpg"])).as_deref(),
            Some("slate")
        );
        assert_eq!(next(&all, "custom:Lake.jpg", &[]).as_deref(), Some("slate"));
        // A current wallpaper outside the list (e.g. a removed custom image) starts at the front.
        assert_eq!(next(&all, "custom:Gone.jpg", &[]).as_deref(), Some("slate"));
        assert_eq!(
            next(&all, "dunes", &ids(&["slate", "forest", "custom:Lake.jpg"])),
            None
        );
        assert_eq!(next(&[], "dunes", &[]), None);
    }

    #[test]
    fn timed_modes_are_due_after_their_interval_and_wait_for_a_clock_behind() {
        let hour = 60 * 60;
        assert!(!WallpaperCycle::Hourly.due(10 * hour, 11 * hour - 1));
        assert!(WallpaperCycle::Hourly.due(10 * hour, 11 * hour));
        assert!(WallpaperCycle::Quarter.due(0, 15 * 60));
        assert!(!WallpaperCycle::Daily.due(10 * hour, 33 * hour));
        assert!(!WallpaperCycle::Hourly.due(1_800_000_000, 1_577_836_800));
        assert!(!WallpaperCycle::Off.due(0, u64::MAX));
        assert!(!WallpaperCycle::Launch.due(0, u64::MAX));
    }

    #[test]
    fn saved_modes_round_trip_and_unknown_values_turn_cycling_off() {
        for mode in WallpaperCycle::ALL {
            let value = serde_json::to_value(mode).unwrap();
            let preferences: crate::model::Preferences =
                serde_json::from_value(serde_json::json!({ "wallpaper_cycle": value })).unwrap();
            assert_eq!(preferences.wallpaper_cycle, mode);
        }
        for value in [serde_json::json!("weekly"), serde_json::json!(3)] {
            let preferences: crate::model::Preferences = serde_json::from_value(
                serde_json::json!({ "wallpaper_cycle": value, "greeting_name": "Ana" }),
            )
            .unwrap();
            assert_eq!(preferences.wallpaper_cycle, WallpaperCycle::Off);
            assert_eq!(preferences.greeting_name, "Ana");
        }
    }
}
