//! Versioned, bounded Home widget placement. No renderer or input-specific geometry lives here.
use crate::{model::Preferences, state::Rect};
use serde::{Deserialize, Deserializer, Serialize};

pub const COLUMNS: u8 = 12;
pub const ROWS: u8 = 22;
pub const GAP: f32 = 24.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Clock,
    Navigation,
    Settings,
    Date,
    Greeting,
    Weather,
    Favorites,
    Inputs,
}

impl Kind {
    pub const ALL: [Self; 8] = [
        Self::Clock,
        Self::Navigation,
        Self::Settings,
        Self::Date,
        Self::Greeting,
        Self::Weather,
        Self::Favorites,
        Self::Inputs,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Clock => "clock",
            Self::Navigation => "layoutNavigation",
            Self::Settings => "tvSettings",
            Self::Date => "date",
            Self::Greeting => "greeting",
            Self::Weather => "weather",
            Self::Favorites => "favorites",
            Self::Inputs => "inputs",
        }
    }

    pub fn minimum(self) -> (u8, u8) {
        match self {
            Self::Favorites => (3, 7),
            Self::Inputs => (3, 4),
            Self::Greeting => (3, 3),
            Self::Navigation => (3, 2),
            Self::Weather => (3, 2),
            Self::Clock => (2, 2),
            Self::Date => (2, 1),
            Self::Settings => (1, 2),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Widget {
    pub id: u32,
    pub kind: Kind,
    pub column: u8,
    pub row: u8,
    pub width: u8,
    pub height: u8,
    /// 0 = small, 1 = normal, 2 = large. Independent of the viewport width.
    pub size: u8,
    pub alignment: Alignment,
    pub heading: bool,
    pub labels: bool,
    pub large_clock: bool,
    pub compact_navigation: bool,
}

impl Default for Widget {
    fn default() -> Self {
        Self {
            id: 1,
            kind: Kind::Clock,
            column: 0,
            row: 0,
            width: 3,
            height: 2,
            size: 1,
            alignment: Alignment::Left,
            heading: true,
            labels: false,
            large_clock: false,
            compact_navigation: false,
        }
    }
}

impl Widget {
    pub fn rect(&self) -> Rect {
        Rect::new(
            84. + f32::from(self.column) * 148.,
            48. + f32::from(self.row) * 40.,
            f32::from(self.width) * 148. - GAP,
            f32::from(self.height) * 40. - 8.,
        )
    }

    pub fn card_width(&self) -> f32 {
        let width: f32 = match self.kind {
            Kind::Inputs => [320., 420., 480.][usize::from(self.size.min(2))],
            _ => [192., 256., 300.][usize::from(self.size.min(2))],
        };
        let width = width.min(self.rect().w);
        if self.kind == Kind::Favorites {
            let available = self.rect().h
                - if self.heading { 42. } else { 8. }
                - if self.labels { 76.64 } else { 0. };
            width.min(available * 256. / 154.4)
        } else {
            width
        }
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        u16::from(self.column) < u16::from(other.column) + u16::from(other.width)
            && u16::from(other.column) < u16::from(self.column) + u16::from(self.width)
            && u16::from(self.row) < u16::from(other.row) + u16::from(other.height)
            && u16::from(other.row) < u16::from(self.row) + u16::from(self.height)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Layout {
    pub version: u8,
    pub widgets: Vec<Widget>,
}

impl Layout {
    pub fn preset(index: usize, preferences: &Preferences) -> Self {
        let mut layout = Self {
            version: 1,
            widgets: Vec::new(),
        };
        let mut add = |kind, column, row, width, height| {
            layout.widgets.push(Widget {
                id: layout.widgets.len() as u32 + 1,
                kind,
                column,
                row,
                width,
                height,
                labels: preferences.show_home_names,
                ..Widget::default()
            });
        };
        add(Kind::Clock, 0, 0, 3, 2);
        add(Kind::Navigation, 8, 0, 3, 2);
        add(Kind::Settings, 11, 0, 1, 2);
        match index {
            1 => {
                if preferences.show_date {
                    add(Kind::Date, 0, 4, 7, 1);
                }
                if preferences.show_greeting {
                    add(Kind::Greeting, 0, 5, 7, 3);
                }
                if preferences.show_weather {
                    add(Kind::Weather, 8, 4, 4, 4);
                }
                add(Kind::Favorites, 0, 11, 7, 7);
                add(Kind::Inputs, 8, 11, 4, 4);
            }
            2 => {
                if preferences.show_greeting {
                    add(Kind::Greeting, 0, 5, 9, 3);
                }
                add(Kind::Favorites, 0, 11, 12, 7);
                add(Kind::Inputs, 0, 18, 12, 4);
            }
            _ => {
                if preferences.show_date {
                    add(Kind::Date, 0, 4, 12, 1);
                }
                if preferences.show_greeting {
                    add(Kind::Greeting, 0, 5, 12, 3);
                }
                if preferences.show_weather {
                    add(Kind::Weather, 0, 8, 12, 2);
                }
                add(Kind::Favorites, 0, 11, 12, 7);
                add(Kind::Inputs, 0, 18, 12, 4);
            }
        }
        layout
    }

    pub fn valid(&self) -> bool {
        self.version == 1
            && self.widgets.len() <= 9
            && self.widgets.iter().enumerate().all(|(i, w)| {
                let (min_w, min_h) = w.kind.minimum();
                w.id != 0
                    && w.size <= 2
                    && w.width >= min_w
                    && w.height >= min_h
                    && u16::from(w.column) + u16::from(w.width) <= u16::from(COLUMNS)
                    && u16::from(w.row) + u16::from(w.height) <= u16::from(ROWS)
                    && self.widgets[..i]
                        .iter()
                        .all(|other| other.id != w.id && !w.overlaps(other))
                    && self
                        .widgets
                        .iter()
                        .filter(|other| other.kind == w.kind)
                        .count()
                        <= if w.kind == Kind::Clock { 2 } else { 1 }
            })
    }

    pub fn widget(&self, kind: Kind) -> Option<&Widget> {
        self.widgets.iter().find(|w| w.kind == kind)
    }

    pub fn replace(&mut self, candidate: Widget) -> bool {
        let Some(index) = self.widgets.iter().position(|w| w.id == candidate.id) else {
            return false;
        };
        let old = std::mem::replace(&mut self.widgets[index], candidate);
        if self.valid() {
            true
        } else {
            self.widgets[index] = old;
            false
        }
    }

    pub fn add(&mut self, kind: Kind) -> Option<u32> {
        let (width, height) = kind.minimum();
        let id = self
            .widgets
            .iter()
            .map(|w| w.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)?;
        for row in 0..=ROWS - height {
            for column in 0..=COLUMNS - width {
                let widget = Widget {
                    id,
                    kind,
                    column,
                    row,
                    width,
                    height,
                    ..Widget::default()
                };
                self.widgets.push(widget);
                if self.valid() {
                    return Some(id);
                }
                self.widgets.pop();
            }
        }
        None
    }
}

/// Bad, unknown-version, or overlapping layouts cannot make Home inaccessible.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Layout>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value::<Layout>(value)
        .ok()
        .filter(Layout::valid))
}

#[derive(Clone, Debug)]
pub enum Mode {
    Select,
    Menu(u32),
    Configure(u32),
    Add,
    Presets,
    Exit,
    Move(Widget),
    Resize(Widget),
}

#[derive(Clone, Debug)]
pub struct Editor {
    pub draft: Layout,
    pub mode: Mode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_and_respect_legacy_visibility() {
        for index in 0..3 {
            let mut p = Preferences::default();
            assert!(Layout::preset(index, &p).valid());
            p.show_date = false;
            p.show_greeting = false;
            p.show_weather = false;
            let layout = Layout::preset(index, &p);
            assert!(layout.valid());
            assert!(layout.widget(Kind::Date).is_none());
            assert!(layout.widget(Kind::Greeting).is_none());
            assert!(layout.widget(Kind::Weather).is_none());
        }
    }

    #[test]
    fn reject_overlap_overflow_duplicates_and_unknown_versions() {
        let original = Layout::preset(0, &Preferences::default());
        for mutation in 0..5 {
            let mut layout = original.clone();
            match mutation {
                0 => layout.widgets[1].column = 0,
                1 => layout.widgets[0].column = 255,
                2 => layout.widgets[1].id = layout.widgets[0].id,
                3 => layout.version = 2,
                _ => layout.widgets[0].width = 0,
            }
            assert!(!layout.valid());
            let json = serde_json::json!({"home_layout": layout});
            let p: Preferences = serde_json::from_value(json).unwrap();
            assert!(p.home_layout.is_none());
        }
    }

    #[test]
    fn replacement_is_transactional_and_add_is_bounded() {
        let mut layout = Layout::preset(0, &Preferences::default());
        let original = layout.clone();
        let mut candidate = layout.widgets[0].clone();
        candidate.column = 9;
        assert!(!layout.replace(candidate));
        assert_eq!(layout, original);
        assert!(layout.add(Kind::Favorites).is_none());
        assert!(layout.add(Kind::Clock).is_some());
        assert!(layout.add(Kind::Clock).is_none());
        let p: Preferences =
            serde_json::from_value(serde_json::json!({"home_layout": layout})).unwrap();
        assert_eq!(p.home_layout, Some(layout));
    }
}
