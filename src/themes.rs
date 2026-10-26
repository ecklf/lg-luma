//! Home themes: a ready-made grid layout with its wallpaper and look. Choosing one replaces the
//! layout; the user can rearrange it afterwards.
use crate::home_grid::{Block, BlockKind, classic_layout};
use crate::model::Preferences;

pub struct Theme {
    pub id: &'static str,
    /// Message keys of the name and the one-line description.
    pub name_key: &'static str,
    pub hint_key: &'static str,
    pub wallpaper: &'static str,
    /// Focus rings on Home.
    pub accent: u32,
    /// Widget cards: colour, opacity and corner radius.
    pub surface: u32,
    pub surface_alpha: f32,
    pub radius: f32,
    /// Whether line widgets (weather) sit on a card; Classic keeps today's bare weather line.
    pub cards: bool,
    pub layout: fn() -> Vec<Block>,
}

const fn block(kind: BlockKind, x: u8, y: u8, w: u8, h: u8) -> Block {
    Block { kind, x, y, w, h }
}

pub static THEMES: [Theme; 6] = [
    Theme {
        id: "classic",
        name_key: "themeClassic",
        hint_key: "themeClassicHint",
        wallpaper: "dunes",
        accent: 0xf0f0f0,
        surface: 0x171c20,
        surface_alpha: 0.72,
        radius: 20.,
        cards: false,
        layout: classic_layout,
    },
    Theme {
        id: "cinema",
        name_key: "themeCinema",
        hint_key: "themeCinemaHint",
        wallpaper: "midnight",
        accent: 0xffc94d,
        surface: 0x0a0a0e,
        surface_alpha: 0.8,
        radius: 12.,
        cards: true,
        layout: || {
            vec![
                block(BlockKind::Greeting, 0, 0, 6, 1),
                block(BlockKind::Weather, 6, 0, 6, 1),
                block(BlockKind::Premieres, 0, 1, 12, 3),
                block(BlockKind::Favorites, 0, 4, 12, 2),
                block(BlockKind::Inputs, 0, 6, 12, 1),
            ]
        },
    },
    Theme {
        id: "minimal",
        name_key: "themeMinimal",
        hint_key: "themeMinimalHint",
        wallpaper: "slate",
        accent: 0xffffff,
        surface: 0xffffff,
        surface_alpha: 0.07,
        radius: 30.,
        cards: true,
        layout: || {
            vec![
                block(BlockKind::Greeting, 0, 0, 12, 3),
                block(BlockKind::Weather, 3, 3, 6, 1),
                block(BlockKind::Favorites, 0, 4, 12, 2),
            ]
        },
    },
    Theme {
        id: "dashboard",
        name_key: "themeDashboard",
        hint_key: "themeDashboardHint",
        wallpaper: "northern-lights",
        accent: 0x7dd3fc,
        surface: 0x081420,
        surface_alpha: 0.72,
        radius: 16.,
        cards: true,
        layout: || {
            vec![
                block(BlockKind::Greeting, 0, 0, 6, 1),
                block(BlockKind::Crypto, 6, 0, 6, 2),
                block(BlockKind::Weather, 0, 1, 6, 1),
                block(BlockKind::Favorites, 0, 2, 12, 2),
                block(BlockKind::Inputs, 0, 4, 12, 1),
                block(BlockKind::Premieres, 0, 5, 12, 2),
            ]
        },
    },
    Theme {
        id: "gaming",
        name_key: "themeGaming",
        hint_key: "themeGamingHint",
        wallpaper: "prism",
        accent: 0xc4b5fd,
        surface: 0x160a22,
        surface_alpha: 0.74,
        radius: 8.,
        cards: true,
        layout: || {
            vec![
                block(BlockKind::Greeting, 0, 0, 8, 1),
                block(BlockKind::Crypto, 8, 0, 4, 1),
                block(BlockKind::Inputs, 0, 1, 12, 1),
                block(BlockKind::Favorites, 0, 2, 12, 2),
                block(BlockKind::Premieres, 0, 4, 12, 2),
            ]
        },
    },
    Theme {
        id: "forest",
        name_key: "themeForest",
        hint_key: "themeForestHint",
        wallpaper: "forest",
        accent: 0xbbf7d0,
        surface: 0xf0f8f0,
        surface_alpha: 0.16,
        radius: 24.,
        cards: true,
        layout: || {
            vec![
                block(BlockKind::Greeting, 0, 0, 7, 2),
                block(BlockKind::Weather, 7, 0, 5, 2),
                block(BlockKind::Favorites, 0, 2, 12, 2),
                block(BlockKind::Premieres, 0, 4, 12, 2),
            ]
        },
    },
];

/// The theme with this id; unknown ids are Classic.
pub fn theme(id: &str) -> &'static Theme {
    THEMES.iter().find(|t| t.id == id).unwrap_or(&THEMES[0])
}

/// Replaces the Home layout and wallpaper with the theme's.
pub fn apply(preferences: &mut Preferences, id: &str) {
    let theme = theme(id);
    preferences.home_theme = theme.id.into();
    preferences.home_layout = (theme.layout)();
    preferences.background = theme.wallpaper.into();
}
