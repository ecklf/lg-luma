use crate::i18n::Language;
use crate::model::{
    App, BACKGROUNDS, CATEGORIES, INPUT_NAME_LIMIT, Input, InputMode, Inventory, Preferences,
    clean_input_name,
};
use crate::text_rendering::TextRendering;
use crate::weather::{self, Unit};
use serde::Serialize;
use std::time::{Duration, Instant};

pub const WEATHER_SECTION: usize = 2;
pub const INPUTS_SECTION: usize = 3;
pub const SYSTEM_SECTION: usize = 4;
pub const SECTION_COUNT: usize = 5;
pub const APPS_COLUMNS: usize = 6;
/// Automatic inventory retries before the error shows: with the 6.5 s Luna timeout about 50 s.
pub const INVENTORY_RETRIES: u32 = 5;
pub const INVENTORY_RETRY_DELAY: Duration = Duration::from_secs(2);
pub const HOME_STRIP_LEFT: f32 = 84.;
// Six favourites in full and the seventh cut off and fading out at the right (Apple TV rows):
// the strip ends after the sixth tile, so a focused tile never sits in the fade.
pub const HOME_STRIP_RIGHT: f32 = 1748.;
pub const HOME_TILE_WIDTH: f32 = 256.;
pub const HOME_TILE_PITCH: f32 = 281.6;
pub const HOME_FADE_LEFT: f32 = 8.;
pub const HOME_FADE_RIGHT: f32 = 1910.;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Page {
    Home,
    Apps,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Target {
    Nav(usize),
    TvSettings,
    Recent(String),
    RecentEmpty,
    RecentRetry,
    App(String),
    Input(String),
    Browse,
    Filter(usize),
    ClearFilter,
    Search,
    ClearSearch,
    Sidebar(usize),
    Setting(usize),
    GreetingName,
    GreetingNameClear,
    AppNamesHome,
    AppNamesApps,
    ClockFormat,
    ClockTimeFormat,
    Background(usize),
    /// A Home block without controls of its own (greeting, a widget), focusable to edit it.
    Block(crate::home_grid::BlockKind),
    /// "+ Add widget" below Home's last block.
    AddBlock,
    /// A row of the block menu, in `State::block_actions` order.
    BlockAction(usize),
    /// A row of the add-widget list, in `State::missing_blocks` order.
    AddKind(usize),
    /// A poster in the premieres widget, by its index in the feed.
    Poster(usize),
    /// An app in a poster's dialog, in `State::poster_apps` order; past the end is Close.
    PosterApp(usize),
    /// A card of a store widget: its slot and the item's index in its document.
    WidgetItem(u8, usize),
    /// A widget in the store's list, by its index in the catalog.
    StoreEntry(usize),
    /// A button on a store widget's page, in `State::store_actions` order.
    StoreAction(usize),
    /// A row of a store widget's settings page, in `State::widget_setting_rows` order.
    WidgetSetting(usize),
    InputMode(String),
    InputRename,
    InputNameReset,
    Choice(usize),
    CloseDialog,
    Retry,
    DismissNotice,
    Option(usize),
    OpenApp,
    ConfirmDelete,
    CancelDelete,
    Key(usize),
    WeatherConfigure,
    WeatherToggle,
    WeatherLocation,
    WeatherTemperature,
    WeatherQuery,
    WeatherSearch,
    WeatherClear,
    WeatherResult(usize),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn center(self) -> (f32, f32) {
        (self.x + self.w / 2., self.y + self.h / 2.)
    }
    pub fn inset(self, n: f32) -> Self {
        Self::new(self.x + n, self.y + n, self.w - 2. * n, self.h - 2. * n)
    }
}

/// Where Home's favourites strip lies: tiles start at `left`, whole tiles end at `right`, and
/// drawing is clipped to `clip_left`..`clip_right` (the screen edges when the block reaches them).
#[derive(Clone, Copy, Debug)]
pub struct Strip {
    pub left: f32,
    pub right: f32,
    pub clip_left: f32,
    pub clip_right: f32,
}

/// A Home block being moved or resized; `snapshot` is the layout when the edit began.
#[derive(Clone, Debug)]
pub struct HomeEdit {
    pub kind: crate::home_grid::BlockKind,
    pub resize: bool,
    pub snapshot: Vec<crate::home_grid::Block>,
}

#[derive(Clone, Debug)]
pub struct Control {
    pub target: Target,
    pub rect: Rect,
    pub scrollable: bool,
}

/// What the block menu offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockAction {
    /// The favourites' existing reorder mode, for the app the menu was opened on.
    Reorder,
    Move,
    Resize,
    Remove,
    /// The widget's own settings page.
    Settings,
}

impl BlockAction {
    pub fn key(self) -> &'static str {
        match self {
            Self::Settings => "blockSettings",
            Self::Reorder => "blockReorder",
            Self::Move => "blockMove",
            Self::Resize => "blockResize",
            Self::Remove => "blockRemove",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Overlay {
    Options {
        id: String,
        restore: Target,
    },
    Delete {
        id: String,
        restore: Target,
    },
    /// Hold OK on a Home block: move, resize or remove it.
    Block {
        kind: crate::home_grid::BlockKind,
        restore: Target,
    },
    /// "+ Add widget": the kinds not on Home.
    AddBlock {
        restore: Target,
    },
    /// A premiere's dialog: which app to watch it in.
    Poster {
        index: usize,
        restore: Target,
    },
    /// The QR code and steps for sending a store widget's secret from a phone.
    PhoneKey {
        restore: Target,
    },
    /// A store widget's item: its dialog, by the item's id.
    WidgetDialog {
        slot: u8,
        id: String,
        restore: Target,
    },
    /// The widget store's list.
    Store {
        restore: Target,
    },
    /// One widget's page in the store, by its id.
    StoreEntry {
        id: String,
        restore: Target,
    },
    Recents {
        restore: Target,
    },
    Keyboard,
    SystemKeyboard,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Subpage {
    AppNames,
    Greeting,
    Clock,
    ClockFormat,
    TimeFormat,
    Input(String),
    WeatherLocation,
    Temperature,
    Language,
    TextRendering,
    /// General › Home theme: a layout with its wallpaper and look.
    Theme,
    /// A store widget's settings, by slot, and the options of one of its settings.
    Widget(u8),
    WidgetOptions(u8, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum TextField {
    Apps,
    Location,
    Name,
    InputName,
    /// A store widget's text or secret setting, as `State::widget_editing` names it.
    Widget,
}

#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    Save,
    /// Read the app inventory again, keeping decoded icons of unchanged apps.
    Refresh,
    /// Refresh apps: read the inventory and every app icon again.
    Reload,
    Launch(String),
    Uninstall(String),
    Recent {
        generation: u64,
        command: crate::recents::Command,
    },
    Quit,
    Weather(weather::Command),
    /// A notice after the effects before it, e.g. a title to look for once an app opens.
    Notify(String),
    /// Start the one-time phone page for a store widget's secret.
    PhoneKeyStart,
}

pub const KEYS: [&str; 40] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "Space",
    "Delete", "Clear", "Done",
];

pub struct State {
    pub page: Page,
    pub focus: Target,
    pub inventory: Inventory,
    pub preferences: Preferences,
    pub query: String,
    pub filter: usize,
    pub section: usize,
    pub subpage: Option<Subpage>,
    pub scroll: f32,
    pub home_scroll: f32,
    /// The first card each row of cards shows (premieres, inputs); see design::Track.
    pub row_scroll: std::collections::HashMap<crate::home_grid::BlockKind, usize>,
    pub overlay: Option<Overlay>,
    pub moving: Option<String>,
    pub loading: bool,
    /// Set by Refresh apps: the next inventory also reloads every app icon.
    pub reload_artwork: bool,
    pub launching: bool,
    pub uninstalling: Option<String>,
    pub notice: Option<(String, Instant)>,
    pub error: Option<String>,
    /// Inventory loads that failed in a row while no apps were shown yet.
    pub inventory_failures: u32,
    pub dirty: bool,
    pub pointer: bool,
    pub pressed: Option<Target>,
    pub previous_background: Option<(String, Instant)>,
    pub panel_started: Option<Instant>,
    pub weather: weather::Weather,
    pub keyboard_field: TextField,
    pub greeting_name_draft: String,
    pub input_name: String,
    pub recents: crate::recents::Recents,
    pub home_edit: Option<HomeEdit>,
    pub feeds: crate::feeds::FeedData,
    /// The phone page's address and QR code while its dialog is open.
    pub phone_key: Option<(String, std::sync::Arc<crate::app_artwork::Artwork>)>,
    /// Set by `Effect::PhoneKeyStart`; the main loop starts the page.
    pub phone_key_start: bool,
    /// Widgets from the store.
    pub widgets: crate::widget_store::WidgetState,
    /// The store widget setting the keyboard or the phone page is filling: slot and setting id.
    pub widget_editing: Option<(u8, String)>,
    pub widget_text_draft: String,
}

/// A row of "Add widget".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddChoice {
    Kind(crate::home_grid::BlockKind),
    /// An installed store widget, by id.
    Widget(String),
    /// "Widget store…".
    Store,
}

impl State {
    pub fn new(preferences: Preferences) -> Self {
        Self {
            page: Page::Home,
            focus: Target::Nav(0),
            inventory: Inventory::default(),
            preferences,
            query: String::new(),
            filter: 0,
            section: 0,
            subpage: None,
            scroll: 0.,
            home_scroll: 0.,
            row_scroll: Default::default(),
            overlay: None,
            moving: None,
            loading: false,
            reload_artwork: false,
            launching: false,
            uninstalling: None,
            notice: None,
            error: None,
            inventory_failures: 0,
            dirty: true,
            pointer: false,
            pressed: None,
            previous_background: None,
            panel_started: None,
            weather: weather::Weather::default(),
            keyboard_field: TextField::Apps,
            greeting_name_draft: String::new(),
            input_name: String::new(),
            recents: crate::recents::Recents::default(),
            home_edit: None,
            feeds: crate::feeds::FeedData::default(),
            phone_key: None,
            phone_key_start: false,
            widgets: Default::default(),
            widget_editing: None,
            widget_text_draft: String::new(),
        }
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
        self.dirty = true;
    }

    pub fn tick(&mut self) {
        let recents_open = self.recents_open();
        if recents_open || self.recents.visibility > 0. {
            self.dirty |= self.recents.animate(
                recents_open,
                self.preferences.reduced_motion,
                self.pointer,
                Instant::now(),
            );
        }
        if let Some(start) = self.panel_started {
            self.dirty = true;
            let duration = if self.overlay.is_some() { 120 } else { 150 };
            if self.preferences.reduced_motion || start.elapsed() >= Duration::from_millis(duration)
            {
                self.panel_started = None;
            }
        }
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, start)| start.elapsed() < Duration::from_millis(120))
            && !self.preferences.reduced_motion
        {
            self.dirty = true;
        }
        if let Some((_, start)) = &self.previous_background {
            self.dirty = true;
            if self.preferences.reduced_motion || start.elapsed() >= Duration::from_millis(400) {
                self.previous_background = None;
            }
        }
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, start)| start.elapsed() > Duration::from_secs(5))
        {
            self.notice = None;
            self.dirty = true;
        }
    }

    pub fn page_index(&self) -> usize {
        match self.page {
            Page::Home => 0,
            Page::Apps => 1,
            Page::Settings => 2,
        }
    }

    pub fn animating(&self) -> bool {
        ((self.recents_open() || self.recents.visibility > 0.)
            && (self.recents.visibility != if self.recents_open() { 1. } else { 0. }
                || self.recents.cards_animating(self.pointer)))
            || self.previous_background.is_some()
            || self.panel_started.is_some()
            || (!self.preferences.reduced_motion
                && self
                    .notice
                    .as_ref()
                    .is_some_and(|(_, start)| start.elapsed() < Duration::from_millis(120)))
    }

    pub fn open(&mut self, index: usize) {
        self.weather.cancel_search();
        self.page = match index {
            1 => Page::Apps,
            2 => Page::Settings,
            _ => Page::Home,
        };
        self.focus = Target::Nav(self.page_index());
        self.scroll = 0.;
        if self.page == Page::Home {
            self.scroll_favorites(0.);
        }
        self.query.clear();
        self.filter = 0;
        self.overlay = None;
        self.moving = None;
        self.subpage = None;
        self.panel_started = None;
        self.dirty = true;
    }

    pub fn apps(&self) -> Vec<&App> {
        let ids = self.preferences.favorites(&self.inventory.apps);
        if self.page == Page::Home {
            return ids
                .iter()
                .filter(|id| !self.preferences.hidden.contains(id))
                .filter_map(|id| self.inventory.apps.iter().find(|app| &app.id == id))
                .collect();
        }
        let query = self.query.to_lowercase();
        let mut apps: Vec<_> = self
            .inventory
            .apps
            .iter()
            .filter(|app| {
                self.preferences.hidden.contains(&app.id) == (self.filter == 5)
                    && (self.filter == 0
                        || self.filter == 5
                        || app.category == CATEGORIES[self.filter - 1])
                    && app.title.to_lowercase().contains(&query)
            })
            .collect();
        apps.sort_by(|a, b| {
            let rank = |app: &App| {
                ids.iter()
                    .position(|id| id == &app.id)
                    .unwrap_or(usize::MAX)
            };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        apps
    }

    pub fn content_viewport(&self) -> Rect {
        let bottom = if self.page == Page::Apps {
            1080.
        } else if self.page == Page::Home && self.moving.is_none() {
            1053.6
        } else {
            994.4
        };
        Rect::new(64., 108.8, 1792., bottom - 108.8)
    }

    /// The visible inputs, in Home's order.
    pub fn home_inputs(&self) -> Vec<&Input> {
        self.inventory
            .inputs
            .iter()
            .filter(|input| self.preferences.input_visible(input))
            .collect()
    }

    /// A card in a row of cards: its block, its place in the row and how many cards the row
    /// has. The favourites scroll on their own.
    pub fn row_of(&self, target: &Target) -> Option<(crate::home_grid::Block, usize, usize)> {
        use crate::home_grid::BlockKind;
        let (kind, index, count) = match target {
            Target::Poster(index) => (BlockKind::Premieres, *index, self.feeds.premieres.len()),
            Target::WidgetItem(slot, index) => (
                BlockKind::Widget(*slot),
                *index,
                self.widgets
                    .slot(*slot)
                    .and_then(|s| s.document.as_ref())
                    .map_or(0, |d| d.items.len()),
            ),
            Target::Input(id) => {
                let inputs = self.home_inputs();
                let index = inputs.iter().position(|input| &input.id == id)?;
                (BlockKind::Inputs, index, inputs.len())
            }
            _ => return None,
        };
        let block = *self.home_blocks().iter().find(|block| block.kind == kind)?;
        Some((block, index, count))
    }

    /// The card at `index` of the row `target` is in.
    fn row_target(&self, target: &Target, index: usize) -> Option<Target> {
        Some(match target {
            Target::Poster(_) => Target::Poster(index),
            Target::WidgetItem(slot, _) => Target::WidgetItem(*slot, index),
            Target::Input(_) => Target::Input(self.home_inputs().get(index)?.id.clone()),
            _ => return None,
        })
    }

    /// A row's track, for the cards of `block`.
    pub fn row_track(block: &crate::home_grid::Block) -> crate::design::Track {
        crate::design::track(block, crate::home_grid::rect(block), crate::design::WIDE_W)
    }

    /// Home's blocks as drawn: the saved layout repaired for the current content.
    pub fn home_blocks(&self) -> Vec<crate::home_grid::Block> {
        let mut blocks = self.preferences.home_layout.clone();
        let inputs = self
            .inventory
            .inputs
            .iter()
            .filter(|input| self.preferences.input_visible(input))
            .count();
        crate::home_grid::normalize(&mut blocks, self.preferences.show_home_names, inputs);
        blocks
    }

    pub fn favorites_strip(&self) -> Strip {
        let blocks = self.home_blocks();
        let Some(block) = blocks
            .iter()
            .find(|b| b.kind == crate::home_grid::BlockKind::Favorites)
        else {
            return Strip {
                left: HOME_STRIP_LEFT,
                right: HOME_STRIP_RIGHT,
                clip_left: HOME_FADE_LEFT,
                clip_right: HOME_FADE_RIGHT,
            };
        };
        let rect = crate::home_grid::rect(block);
        let whole = ((rect.w + HOME_TILE_PITCH - HOME_TILE_WIDTH) / HOME_TILE_PITCH)
            .floor()
            .max(1.);
        Strip {
            left: rect.x,
            right: rect.x + (whole - 1.) * HOME_TILE_PITCH + HOME_TILE_WIDTH,
            clip_left: if block.x == 0 { HOME_FADE_LEFT } else { rect.x },
            clip_right: if block.x + block.w == crate::home_grid::COLUMNS {
                HOME_FADE_RIGHT
            } else {
                rect.x + rect.w
            },
        }
    }

    pub fn favorites_viewport(&self) -> Rect {
        let content = self.content_viewport();
        let strip = self.favorites_strip();
        let left = if self.home_scroll > 0. {
            strip.clip_left
        } else if strip.clip_left == HOME_FADE_LEFT {
            0.
        } else {
            strip.clip_left
        };
        Rect::new(left, content.y, strip.clip_right - left, content.h)
    }

    pub fn block_actions(
        &self,
        kind: crate::home_grid::BlockKind,
        from_app: bool,
    ) -> Vec<BlockAction> {
        let mut actions = Vec::new();
        if from_app && kind == crate::home_grid::BlockKind::Favorites {
            actions.push(BlockAction::Reorder);
        }
        if matches!(kind, crate::home_grid::BlockKind::Widget(_)) {
            actions.push(BlockAction::Settings);
        }
        actions.extend([BlockAction::Move, BlockAction::Resize]);
        if kind.removable() {
            actions.push(BlockAction::Remove);
        }
        actions
    }

    /// Kinds that can still be added to Home, in their usual order.
    pub fn missing_blocks(&self) -> Vec<crate::home_grid::BlockKind> {
        let blocks = self.home_blocks();
        crate::home_grid::BlockKind::ALL
            .into_iter()
            .filter(|kind| !blocks.iter().any(|b| b.kind == *kind))
            .collect()
    }

    /// The Home block a focus target belongs to.
    fn home_block_of(&self, target: &Target) -> Option<crate::home_grid::BlockKind> {
        use crate::home_grid::BlockKind;
        match target {
            Target::App(_) | Target::Browse => Some(BlockKind::Favorites),
            Target::Input(_) => Some(BlockKind::Inputs),
            Target::WeatherConfigure => Some(BlockKind::Weather),
            Target::Block(kind) => Some(*kind),
            Target::Poster(_) => Some(BlockKind::Premieres),
            Target::WidgetItem(slot, _) => Some(BlockKind::Widget(*slot)),
            _ => None,
        }
    }

    /// Streaming apps to watch a premiere in, the one chosen last first.
    pub fn poster_apps(&self) -> Vec<String> {
        let favorites = self.preferences.favorites(&self.inventory.apps);
        let mut apps: Vec<&App> = self
            .inventory
            .apps
            .iter()
            .filter(|app| app.category == "Streaming" && !self.preferences.hidden.contains(&app.id))
            .collect();
        apps.sort_by_key(|app| {
            (
                app.id != self.preferences.poster_app,
                favorites
                    .iter()
                    .position(|id| id == &app.id)
                    .unwrap_or(usize::MAX),
                app.title.to_lowercase(),
            )
        });
        apps.into_iter().take(6).map(|app| app.id.clone()).collect()
    }

    /// Moves or resizes the edited block by one cell and reflows the others from the layout
    /// at the start of the edit.
    fn edit_step(&mut self, edit: &HomeEdit, direction: Direction) {
        let Some(mut block) = self
            .preferences
            .home_layout
            .iter()
            .find(|b| b.kind == edit.kind)
            .copied()
        else {
            self.home_edit = None;
            return;
        };
        let (dx, dy): (i16, i16) = match direction {
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
            Direction::Up => (0, -1),
            Direction::Down => (0, 1),
        };
        let columns = i16::from(crate::home_grid::COLUMNS);
        let ((min_w, min_h), (max_w, max_h)) = self.block_limits(edit.kind);
        if edit.resize {
            let max_w = i16::from(max_w).min(columns - i16::from(block.x));
            block.w = (i16::from(block.w) + dx).clamp(i16::from(min_w), max_w) as u8;
            block.h = (i16::from(block.h) + dy).clamp(i16::from(min_h), i16::from(max_h)) as u8;
        } else {
            block.x = (i16::from(block.x) + dx).clamp(0, columns - i16::from(block.w)) as u8;
            block.y = (i16::from(block.y) + dy).clamp(0, 60) as u8;
        }
        self.preferences.home_layout = crate::home_grid::reflow(&edit.snapshot, block);
        let rect = crate::home_grid::rect(&block);
        if rect.y - self.scroll < 120.8 {
            self.scroll = (rect.y - 120.8).max(0.);
        } else if rect.y + rect.h - self.scroll > 1041.6 {
            self.scroll = rect.y + rect.h - 1041.6;
        }
        self.dirty = true;
    }

    pub fn hit_test(&self, controls: &[Control], x: f32, y: f32) -> Option<Target> {
        controls
            .iter()
            .rev()
            .find(|control| {
                let viewport =
                    if self.page == Page::Home && matches!(control.target, Target::App(_)) {
                        self.favorites_viewport()
                    } else {
                        self.content_viewport()
                    };
                control.rect.contains(x, y) && (!control.scrollable || viewport.contains(x, y))
            })
            .map(|control| control.target.clone())
    }

    /// Returns whether this wheel event scrolled favorites, so a pending card press is cancelled.
    pub fn wheel_scroll(
        &mut self,
        controls: &[Control],
        position: Option<(f32, f32)>,
        x: i32,
        y: i32,
    ) -> bool {
        let over_favorites = self.page == Page::Home
            && position.is_some_and(|(x, y)| {
                self.favorites_viewport().contains(x, y)
                    && controls.iter().any(|control| {
                        matches!(control.target, Target::App(_)) && control.rect.contains(x, y)
                    })
            });
        let favorites = self.page == Page::Home && (x != 0 || over_favorites);
        if favorites {
            self.scroll_favorites((if x != 0 { x } else { -y }) as f32 * HOME_TILE_PITCH);
        } else {
            let bottom = self.content_viewport().y + self.content_viewport().h - 12.;
            let max = controls
                .iter()
                .filter(|c| c.scrollable)
                .map(|c| c.rect.y + c.rect.h + self.scroll - bottom)
                .fold(0., f32::max);
            self.scroll = (self.scroll - y as f32 * 90.).clamp(0., max.max(0.));
        }
        self.pointer = true;
        self.dirty = true;
        favorites
    }

    pub fn scroll_favorites(&mut self, delta: f32) {
        let count = self
            .preferences
            .favorites(&self.inventory.apps)
            .iter()
            .filter(|id| {
                !self.preferences.hidden.contains(id)
                    && self.inventory.apps.iter().any(|app| &app.id == *id)
            })
            .count();
        let content = if count == 0 {
            0.
        } else {
            (count - 1) as f32 * HOME_TILE_PITCH + HOME_TILE_WIDTH
        };
        let strip = self.favorites_strip();
        let max = (content - (strip.right - strip.left)).max(0.);
        let next = (self.home_scroll + delta).clamp(0., max);
        if next != self.home_scroll {
            self.home_scroll = next;
            self.dirty = true;
        }
    }

    pub fn set_inventory(&mut self, next: Inventory, warning: Option<String>) {
        self.inventory.apps = next.apps;
        if let Some(warning) = warning {
            self.notify(warning);
        } else {
            self.inventory.inputs = next.inputs;
        }
        self.loading = false;
        self.error = None;
        self.inventory_failures = 0;
        self.dirty = true;
        self.reconcile_focus();
    }

    /// A failed inventory load. Right after a cold boot, or when the TV is short of memory, the
    /// Homebrew service that reads the inventory can take longer than one request allows. While
    /// no apps are shown yet, the screen keeps loading and the caller retries after the returned
    /// delay; the error and its Retry button only appear once those retries are used up.
    pub fn inventory_failed(&mut self, error: String) -> Option<Duration> {
        self.dirty = true;
        if self.inventory.apps.is_empty() && self.inventory_failures < INVENTORY_RETRIES {
            self.inventory_failures += 1;
            return Some(INVENTORY_RETRY_DELAY);
        }
        self.inventory_failures = 0;
        self.loading = false;
        self.error = Some(error);
        None
    }

    pub fn reconcile_focus(&mut self) {
        if self.page == Page::Home {
            self.scroll_favorites(0.);
        }
        if let Target::App(id) = &self.focus
            && !self.apps().iter().any(|app| &app.id == id)
        {
            self.focus = self.first_content();
        }
        if let Target::Input(id) = &self.focus
            && !self
                .inventory
                .inputs
                .iter()
                .any(|input| &input.id == id && self.preferences.input_visible(input))
        {
            self.focus = self.first_content();
        }
    }

    pub fn first_content(&self) -> Target {
        match self.page {
            Page::Home => self
                .apps()
                .get(((self.home_scroll - 0.01).max(0.) / HOME_TILE_PITCH).ceil() as usize)
                .map(|app| Target::App(app.id.clone()))
                .unwrap_or(if self.error.is_some() {
                    Target::Retry
                } else {
                    Target::Browse
                }),
            Page::Apps => self
                .apps()
                .first()
                .map(|app| Target::App(app.id.clone()))
                .unwrap_or(Target::Filter(1)),
            Page::Settings => Target::Sidebar(0),
        }
    }

    pub fn focus(&mut self, target: Target, controls: &[Control]) {
        self.pointer = false;
        self.focus = target;
        self.reveal_focus(controls);
        if let Target::Sidebar(section) = self.focus {
            if self.subpage == Some(Subpage::WeatherLocation) {
                self.weather.cancel_search();
            }
            self.section = section;
            self.subpage = None;
        }
        self.dirty = true;
    }

    /// Remote focus is revealed after layout changes, including hide/reorder.
    /// Pointer paths deliberately do not call this method.
    pub fn reveal_focus(&mut self, controls: &[Control]) -> bool {
        let previous = self.scroll;
        let previous_home = self.home_scroll;
        if let Some(control) = controls.iter().find(|c| c.target == self.focus) {
            if self.page == Page::Home && matches!(self.focus, Target::App(_)) {
                let strip = self.favorites_strip();
                if control.rect.x < strip.left {
                    self.home_scroll += control.rect.x - strip.left;
                } else if control.rect.x + control.rect.w > strip.right {
                    self.home_scroll += control.rect.x + control.rect.w - strip.right;
                }
            }
            if matches!(
                self.focus,
                Target::Nav(_)
                    | Target::Filter(_)
                    | Target::ClearFilter
                    | Target::Search
                    | Target::ClearSearch
                    | Target::Sidebar(_)
            ) {
                self.scroll = 0.;
            } else if control.scrollable {
                let top = if self.page == Page::Apps {
                    164.8
                } else {
                    120.8
                };
                if control.rect.y < top {
                    self.scroll = (self.scroll + control.rect.y - top).max(0.);
                }
                let bottom = if self.page == Page::Home && self.moving.is_none() {
                    1041.6
                } else {
                    982.4
                };
                if control.rect.y + control.rect.h > bottom {
                    self.scroll += control.rect.y + control.rect.h - bottom;
                }
            }
        }
        if self.page == Page::Home {
            self.scroll_favorites(0.);
        }
        let carousel = self.reveal_row();
        if self.scroll != previous || self.home_scroll != previous_home || carousel {
            self.dirty = true;
        }
        self.scroll != previous || self.home_scroll != previous_home || carousel
    }

    /// Scrolls a focused card of a row fully into view, however focus got there (the card cut
    /// off at the edge is a control too); true when the row moved.
    fn reveal_row(&mut self) -> bool {
        let Some((block, index, _)) = self.row_of(&self.focus) else {
            return false;
        };
        let track = Self::row_track(&block);
        let first = self.row_scroll.get(&block.kind).copied().unwrap_or(0);
        let wanted = track.reveal(first, index);
        self.row_scroll.insert(block.kind, wanted);
        wanted != first
    }

    /// Hover and pointer-down must not move the control before pointer-up.
    pub fn pointer_focus(&mut self, target: Target) {
        if !self.pointer {
            self.dirty = true;
        }
        self.pointer = true;
        if self.focus == target {
            return;
        }
        if let Target::Sidebar(section) = target {
            if self.subpage == Some(Subpage::WeatherLocation) {
                self.weather.cancel_search();
            }
            self.section = section;
            self.subpage = None;
        }
        self.focus = target;
        self.dirty = true;
    }

    pub fn hidden_count(&self) -> usize {
        self.inventory
            .apps
            .iter()
            .filter(|app| self.preferences.hidden.contains(&app.id))
            .count()
    }

    fn section_entry(&self) -> Target {
        match self.section {
            1 => Target::Background(
                BACKGROUNDS
                    .iter()
                    .position(|(id, _)| *id == self.preferences.background)
                    .unwrap_or(3),
            ),
            WEATHER_SECTION => Target::WeatherToggle,
            SYSTEM_SECTION => Target::Setting(3),
            INPUTS_SECTION => self
                .inventory
                .inputs
                .first()
                .map(|input| Target::InputMode(input.id.clone()))
                .unwrap_or(Target::Setting(3)),
            _ => Target::Setting(0),
        }
    }

    pub(crate) fn close_subpage(&mut self) {
        if self.subpage == Some(Subpage::WeatherLocation) {
            self.weather.cancel_search();
        }
        self.panel_started = None;
        self.pointer = false;
        self.focus = match self.subpage.take() {
            Some(Subpage::AppNames) => Target::Setting(0),
            Some(Subpage::Greeting) => Target::Setting(6),
            Some(Subpage::Clock) => Target::Setting(1),
            Some(Subpage::ClockFormat) => {
                self.subpage = Some(Subpage::Clock);
                Target::ClockFormat
            }
            Some(Subpage::TimeFormat) => {
                self.subpage = Some(Subpage::Clock);
                Target::ClockTimeFormat
            }
            Some(Subpage::Input(id)) => Target::InputMode(id),
            Some(Subpage::WeatherLocation) => Target::WeatherLocation,
            Some(Subpage::Temperature) => Target::WeatherTemperature,
            Some(Subpage::Language) => Target::Setting(4),
            Some(Subpage::TextRendering) => Target::Setting(7),
            Some(Subpage::Theme) => Target::Setting(9),
            Some(Subpage::Widget(_)) => Target::Sidebar(0),
            Some(Subpage::WidgetOptions(slot, setting)) => {
                self.subpage = Some(Subpage::Widget(slot));
                Target::WidgetSetting(self.widget_row_of(slot, setting))
            }
            None => Target::Sidebar(self.section),
        };
        self.scroll = 0.;
        self.dirty = true;
    }

    pub fn direction(&mut self, direction: Direction, controls: &[Control]) -> Vec<Effect> {
        self.pointer = false;
        self.dirty = true;
        if self.recents_open() {
            return self.recent_direction(direction);
        }
        if matches!(self.overlay, Some(Overlay::SystemKeyboard)) {
            // The TV keyboard owns its arrows until Done/Back dismisses it.
            return vec![];
        }
        if let Some(edit) = self.home_edit.clone() {
            self.edit_step(&edit, direction);
            return vec![];
        }
        if let Some(id) = self.moving.clone() {
            let mut ids = self.preferences.favorites(&self.inventory.apps);
            let slots: Vec<_> = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    (!self.preferences.hidden.contains(id)
                        && self.inventory.apps.iter().any(|app| &app.id == id))
                    .then_some(index)
                })
                .collect();
            if let Some(index) = slots.iter().position(|slot| ids[*slot] == id) {
                let next = match direction {
                    Direction::Left => index.saturating_sub(1),
                    Direction::Right => (index + 1).min(slots.len() - 1),
                    _ => index,
                };
                if next != index {
                    ids.swap(slots[index], slots[next]);
                    self.preferences.favorites = Some(ids);
                    self.dirty = true;
                    return vec![Effect::Save];
                }
            }
            return vec![];
        }
        if self.overlay.is_none() {
            if self.page == Page::Home
                && let Target::App(id) = &self.focus
            {
                let apps = self.apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    let next = match direction {
                        Direction::Left => index.checked_sub(1),
                        Direction::Right => (index + 1 < apps.len()).then_some(index + 1),
                        _ => None,
                    };
                    // Past either end, and Up/Down, the grid's spatial routes reach the
                    // neighbouring blocks.
                    if let Some(next) = next {
                        let id = apps[next].id.clone();
                        self.focus(Target::App(id), controls);
                        return vec![];
                    }
                }
            }
            let setting_rows = match self.subpage {
                Some(Subpage::Greeting) => Some([Target::GreetingName, Target::GreetingNameClear]),
                Some(Subpage::AppNames) => Some([Target::AppNamesHome, Target::AppNamesApps]),
                Some(Subpage::Clock) => Some([Target::ClockFormat, Target::ClockTimeFormat]),
                _ => None,
            };
            if let Some(rows) = setting_rows
                && let Some(index) = rows.iter().position(|target| *target == self.focus)
            {
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::Nav(2)
                        } else {
                            rows[0].clone()
                        },
                        controls,
                    ),
                    Direction::Down if index == 0 => self.focus(rows[1].clone(), controls),
                    Direction::Right
                        if matches!(
                            self.focus,
                            Target::GreetingName | Target::ClockFormat | Target::ClockTimeFormat
                        ) =>
                    {
                        return self.activate();
                    }
                    _ => {}
                }
                return vec![];
            }
            if let Target::Choice(index) = self.focus {
                let count = match self.subpage {
                    Some(
                        Subpage::ClockFormat
                        | Subpage::TimeFormat
                        | Subpage::Temperature
                        | Subpage::TextRendering,
                    ) => 2,
                    Some(Subpage::Language) => Language::ALL.len(),
                    Some(Subpage::Theme) => crate::themes::THEMES.len(),
                    Some(Subpage::WidgetOptions(slot, setting)) => {
                        self.widget_options(slot, setting).len().max(1)
                    }
                    _ => 3,
                };
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::Nav(2)
                        } else {
                            Target::Choice(index - 1)
                        },
                        controls,
                    ),
                    Direction::Down
                        if index + 1 == count
                            && matches!(self.subpage, Some(Subpage::Input(_))) =>
                    {
                        self.focus(Target::InputRename, controls)
                    }
                    Direction::Down => self.focus(Target::Choice((index + 1) % count), controls),
                    Direction::Right => {}
                }
                return vec![];
            }
            if matches!(self.focus, Target::InputRename | Target::InputNameReset) {
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if self.focus == Target::InputNameReset {
                            Target::InputRename
                        } else {
                            Target::Choice(2)
                        },
                        controls,
                    ),
                    Direction::Down
                        if self.focus == Target::InputRename
                            && controls.iter().any(|c| c.target == Target::InputNameReset) =>
                    {
                        self.focus(Target::InputNameReset, controls)
                    }
                    _ => {}
                }
                return vec![];
            }
            if let Target::Sidebar(index) = self.focus {
                match direction {
                    Direction::Right => {
                        let entry = self.section_entry();
                        self.focus(entry, controls);
                    }
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::Nav(2)
                        } else {
                            Target::Sidebar(index - 1)
                        },
                        controls,
                    ),
                    Direction::Down => {
                        self.focus(Target::Sidebar((index + 1) % SECTION_COUNT), controls)
                    }
                    Direction::Left => {}
                }
                return vec![];
            }
            if matches!(
                self.focus,
                Target::Setting(0 | 1 | 4 | 6 | 7 | 9)
                    | Target::WidgetSetting(_)
                    | Target::InputMode(_)
                    | Target::WeatherLocation
                    | Target::WeatherTemperature
            ) && matches!(direction, Direction::Right)
            {
                return self.activate();
            }
            if matches!(self.focus, Target::Setting(_) | Target::WeatherToggle)
                && matches!(direction, Direction::Right)
            {
                return vec![];
            }
            if let Target::InputMode(id) = &self.focus
                && matches!(direction, Direction::Up)
                && self
                    .inventory
                    .inputs
                    .first()
                    .is_some_and(|input| &input.id == id)
            {
                self.focus(Target::Sidebar(INPUTS_SECTION), controls);
                return vec![];
            }
            if self.focus == Target::Setting(0) && matches!(direction, Direction::Up) {
                self.focus(Target::Nav(2), controls);
                return vec![];
            }
            if self.focus == Target::WeatherToggle && matches!(direction, Direction::Up) {
                self.focus(Target::Nav(2), controls);
                return vec![];
            }
            if matches!(
                self.focus,
                Target::WeatherQuery | Target::WeatherSearch | Target::WeatherClear
            ) {
                match direction {
                    Direction::Left => {
                        self.close_subpage();
                        return vec![];
                    }
                    Direction::Up => {
                        self.focus(
                            if self.preferences.weather_location.is_some()
                                && self.focus != Target::WeatherClear
                            {
                                Target::WeatherClear
                            } else {
                                Target::Nav(2)
                            },
                            controls,
                        );
                        return vec![];
                    }
                    Direction::Down => {
                        let target = if self.focus == Target::WeatherClear {
                            Some(Target::WeatherQuery)
                        } else if !self.weather.results.is_empty() {
                            Some(Target::WeatherResult(0))
                        } else if self.focus == Target::WeatherQuery {
                            Some(Target::WeatherSearch)
                        } else {
                            None
                        };
                        if let Some(target) = target {
                            self.focus(target, controls);
                        }
                        return vec![];
                    }
                    _ => {}
                }
            }
            if let Target::WeatherResult(index) = self.focus {
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::WeatherQuery
                        } else {
                            Target::WeatherResult(index - 1)
                        },
                        controls,
                    ),
                    Direction::Down if index + 1 < self.weather.results.len() => {
                        self.focus(Target::WeatherResult(index + 1), controls)
                    }
                    _ => {}
                }
                return vec![];
            }
            if self.focus == Target::Search {
                match direction {
                    Direction::Left => {
                        self.focus(
                            Target::Filter(if self.hidden_count() > 0 { 5 } else { 4 }),
                            controls,
                        );
                        return vec![];
                    }
                    Direction::Up => {
                        self.focus(Target::Nav(1), controls);
                        return vec![];
                    }
                    _ => {}
                }
            }
            if let Target::Nav(index) = self.focus {
                let next = match direction {
                    Direction::Left if index > 0 => Some(index - 1),
                    Direction::Right if index < 2 => Some(index + 1),
                    _ => None,
                };
                if let Some(next) = next {
                    self.open(next);
                    return vec![];
                }
            }
            if self.page == Page::Apps
                && let Target::App(id) = &self.focus
                && matches!(direction, Direction::Up | Direction::Down)
            {
                let apps = self.apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    // A preceding row may be above the visible scroll region.
                    // Its grid index still defines the route; reveal_focus then
                    // scrolls it into view, without letting the fixed nav steal Up.
                    let next = match direction {
                        Direction::Up if index < APPS_COLUMNS => Some(Target::Filter(1)),
                        Direction::Up => Some(Target::App(apps[index - APPS_COLUMNS].id.clone())),
                        Direction::Down
                            if (index / APPS_COLUMNS + 1) * APPS_COLUMNS < apps.len() =>
                        {
                            Some(Target::App(
                                apps[(index + APPS_COLUMNS).min(apps.len() - 1)].id.clone(),
                            ))
                        }
                        _ => None,
                    };
                    if let Some(next) = next {
                        self.focus(next, controls);
                    }
                    return vec![];
                }
            }
            if let Target::Background(index) = self.focus {
                match direction {
                    Direction::Right if index + 1 < BACKGROUNDS.len() => {
                        self.focus(Target::Background(index + 1), controls);
                        return vec![];
                    }
                    Direction::Left if index % 3 != 0 => {
                        self.focus(Target::Background(index - 1), controls);
                        return vec![];
                    }
                    _ => {}
                }
            }
            // Rows of cards scroll: Left and Right step a card, past the row's edge too.
            if matches!(direction, Direction::Left | Direction::Right)
                && let Some((block, index, count)) = self.row_of(&self.focus)
            {
                let next = match direction {
                    Direction::Left => index.checked_sub(1),
                    _ => Some(index + 1).filter(|next| *next < count),
                };
                if let Some(next) = next.and_then(|next| self.row_target(&self.focus, next)) {
                    self.focus(next, controls);
                    return vec![];
                }
                // The row ends at its last card: Right goes on only to a block beside it,
                // never up to the TV settings button.
                let here = controls.iter().find(|c| c.target == self.focus);
                let beside = here.is_some_and(|here| {
                    let a = here.rect;
                    controls.iter().any(|c| {
                        self.row_of(&c.target)
                            .is_none_or(|(other, ..)| other.kind != block.kind)
                            && c.rect.y >= 150.
                            && c.rect.x >= a.x + a.w
                            && c.rect.y < a.y + a.h
                            && c.rect.y + c.rect.h > a.y
                    })
                });
                if matches!(direction, Direction::Right) && !beside {
                    return vec![];
                }
            }
            if matches!(self.focus, Target::Nav(_)) && matches!(direction, Direction::Down) {
                if self.page == Page::Settings {
                    self.subpage = None;
                    self.section = 0;
                }
                self.focus(self.first_content(), controls);
                return vec![];
            }
            if self.page == Page::Settings
                && matches!(direction, Direction::Left)
                && !matches!(
                    self.focus,
                    Target::Nav(_) | Target::Sidebar(_) | Target::TvSettings
                )
            {
                if let Target::Background(index) = self.focus {
                    if index % 3 != 0 { /* left follows gallery order */
                    } else {
                        self.focus(Target::Sidebar(self.section), controls);
                        return vec![];
                    }
                } else {
                    self.focus(Target::Sidebar(self.section), controls);
                    return vec![];
                }
            }
        }
        let Some(current) = controls.iter().find(|c| c.target == self.focus) else {
            self.focus(self.first_content(), controls);
            return vec![];
        };
        let (x, y) = current.rect.center();
        let strip = self.favorites_strip();
        let next = controls
            .iter()
            .filter(|candidate| candidate.target != self.focus)
            .filter(|candidate| {
                self.page != Page::Home
                    || !matches!(candidate.target, Target::App(_))
                    || (candidate.rect.x >= strip.left - 0.1
                        && candidate.rect.x + candidate.rect.w <= strip.right + 0.1)
            })
            .filter_map(|candidate| {
                // Header controls never steal an Up route from the active tab.
                if self.overlay.is_none()
                    && matches!(direction, Direction::Up)
                    && candidate.rect.y < 150.
                    && candidate.target != Target::Nav(self.page_index())
                {
                    return None;
                }
                let (cx, cy) = candidate.rect.center();
                let (primary, cross, overlap) = match direction {
                    Direction::Up => (
                        y - cy,
                        (x - cx).abs(),
                        (current.rect.w + candidate.rect.w) / 2.,
                    ),
                    Direction::Down => (
                        cy - y,
                        (x - cx).abs(),
                        (current.rect.w + candidate.rect.w) / 2.,
                    ),
                    Direction::Left => (
                        x - cx,
                        (y - cy).abs(),
                        (current.rect.h + candidate.rect.h) / 2.,
                    ),
                    Direction::Right => (
                        cx - x,
                        (y - cy).abs(),
                        (current.rect.h + candidate.rect.h) / 2.,
                    ),
                };
                (primary > 1.).then_some((
                    candidate.target.clone(),
                    primary + cross * 2. + if cross >= overlap { 10000. } else { 0. },
                ))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(target, _)| target);
        if let Some(next) = next {
            self.focus(next, controls);
        }
        vec![]
    }

    pub fn options(&mut self) {
        if self.overlay.is_some() || self.moving.is_some() {
            return;
        }
        if let Target::App(id) = &self.focus {
            self.overlay = Some(Overlay::Options {
                id: id.clone(),
                restore: self.focus.clone(),
            });
            self.focus = Target::OpenApp;
            if !self.preferences.reduced_motion {
                self.panel_started = Some(Instant::now());
            }
            self.dirty = true;
        }
    }

    pub fn uninstall_response(&mut self, id: String, result: Result<(), String>) -> Vec<Effect> {
        if self.uninstalling.as_ref() != Some(&id) {
            return vec![];
        }
        self.uninstalling = None;
        self.dirty = true;
        if let Err(error) = result {
            eprintln!("Luma uninstall: {error}");
            self.notify(self.preferences.language.t("deleteAppError"));
            if matches!(&self.overlay, Some(Overlay::Delete { id: current, .. } | Overlay::Options { id: current, .. }) if current == &id)
            {
                if let Some(Overlay::Delete { restore, .. } | Overlay::Options { restore, .. }) =
                    self.overlay.take()
                {
                    self.focus = restore;
                }
                self.panel_started = None;
                self.pressed = None;
                self.reconcile_focus();
            }
            // Even an uncertain/failed mutation may have changed TV inventory.
            // Refresh is read-only, and also clears a stale card after late removal.
            return vec![Effect::Refresh];
        }
        let old_ids: Vec<_> = self.apps().iter().map(|app| app.id.clone()).collect();
        let index = old_ids.iter().position(|item| item == &id).unwrap_or(0);
        let mut favorites = self.preferences.favorites(&self.inventory.apps);
        favorites.retain(|item| item != &id);
        self.preferences.favorites = Some(favorites);
        self.preferences.hidden.retain(|item| item != &id);
        self.inventory.apps.retain(|app| app.id != id);
        if self.filter == 5 && self.hidden_count() == 0 {
            self.filter = 0;
        }
        if matches!(&self.overlay, Some(Overlay::Delete { id: current, .. } | Overlay::Options { id: current, .. }) if current == &id)
        {
            self.overlay = None;
            self.panel_started = None;
            self.focus = Target::App(id.clone());
        }
        if self.focus == Target::App(id) {
            self.focus = self
                .apps()
                .get(index.min(self.apps().len().saturating_sub(1)))
                .map(|app| Target::App(app.id.clone()))
                .unwrap_or_else(|| self.first_content());
        }
        self.reconcile_focus();
        self.notify(self.preferences.language.t("appDeleted"));
        vec![Effect::Save, Effect::Refresh]
    }

    pub fn hold(&mut self) {
        if self.page == Page::Home && self.overlay.is_none() {
            if self.home_edit.is_some() || self.moving.is_some() {
                return;
            }
            if let Some(kind) = self.home_block_of(&self.focus) {
                self.overlay = Some(Overlay::Block {
                    kind,
                    restore: self.focus.clone(),
                });
                self.focus = Target::BlockAction(0);
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
                self.notice = None;
                self.dirty = true;
            }
        } else {
            self.options();
        }
    }

    pub fn text(&mut self, text: &str) {
        if self.text_input_active() {
            if self.keyboard_field == TextField::Location {
                let mut query = self.weather.query.clone();
                query.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(120usize.saturating_sub(query.chars().count())),
                );
                self.weather.edit(query);
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::Name {
                let name = &mut self.greeting_name_draft;
                name.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(40usize.saturating_sub(name.chars().count())),
                );
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::Widget {
                let limit = self.widget_text_limit();
                let draft = &mut self.widget_text_draft;
                let length = draft.chars().count();
                draft.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(limit.saturating_sub(length)),
                );
                self.dirty = true;
                return;
            }
            if self.keyboard_field == TextField::InputName {
                let length = self.input_name.chars().count();
                self.input_name.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(INPUT_NAME_LIMIT.saturating_sub(length)),
                );
                self.dirty = true;
                return;
            }
            self.query.extend(
                text.chars()
                    .filter(|c| !c.is_control())
                    .take(120usize.saturating_sub(self.query.chars().count())),
            );
            self.scroll = 0.;
            self.dirty = true;
        }
    }

    pub fn delete(&mut self) {
        if self.text_input_active() {
            if self.keyboard_field == TextField::Location {
                let mut query = self.weather.query.clone();
                query.pop();
                self.weather.edit(query);
            } else if self.keyboard_field == TextField::Name {
                self.greeting_name_draft.pop();
            } else if self.keyboard_field == TextField::InputName {
                self.input_name.pop();
            } else if self.keyboard_field == TextField::Widget {
                self.widget_text_draft.pop();
            } else {
                self.query.pop();
            }
            self.dirty = true;
        }
    }

    pub fn text_input_active(&self) -> bool {
        matches!(
            self.overlay,
            Some(Overlay::Keyboard | Overlay::SystemKeyboard)
        )
    }

    pub fn finish_text_input(&mut self) -> Vec<Effect> {
        if self.text_input_active() {
            self.overlay = None;
            self.panel_started = None;
            self.focus = self.keyboard_return();
            self.pointer = false;
            self.dirty = true;
            return self.commit_text_input();
        }
        vec![]
    }

    fn commit_text_input(&mut self) -> Vec<Effect> {
        if self.keyboard_field == TextField::Widget {
            return self.commit_widget_text();
        }
        if self.keyboard_field == TextField::Name {
            let name = std::mem::take(&mut self.greeting_name_draft);
            let name = name.trim();
            // Empty dismissal preserves the saved value; Clear name is explicit.
            if !name.is_empty() && self.preferences.greeting_name != name {
                self.preferences.greeting_name = name.into();
                return vec![Effect::Save];
            }
            return vec![];
        }
        self.commit_input_name()
    }

    /// Closing the keyboard by any route keeps a typed input name. An empty
    /// field leaves the saved name unchanged; Clear name removes it explicitly.
    fn commit_input_name(&mut self) -> Vec<Effect> {
        let name = std::mem::take(&mut self.input_name);
        if self.keyboard_field != TextField::InputName || clean_input_name(&name).is_empty() {
            return vec![];
        }
        let Some(Subpage::Input(id)) = self.subpage.clone() else {
            return vec![];
        };
        if self.preferences.set_input_name(&id, &name) {
            vec![Effect::Save]
        } else {
            vec![]
        }
    }

    pub fn delete_surrounding_text(&mut self, offset: i32, length: i32) {
        if !self.text_input_active() || length <= 0 {
            return;
        }
        // SDL-webOS forwards the native text model's UTF-8 byte offsets,
        // relative to the insertion cursor (at the end of our query).
        let mut query = self.editing_text().to_owned();
        let mut start =
            (query.len() as i64 + i64::from(offset)).clamp(0, query.len() as i64) as usize;
        let mut end = start.saturating_add(length as usize).min(query.len());
        while !query.is_char_boundary(start) {
            start -= 1;
        }
        while !query.is_char_boundary(end) {
            end += 1;
        }
        query.replace_range(start..end, "");
        if self.keyboard_field == TextField::Location {
            self.weather.edit(query);
        } else if self.keyboard_field == TextField::Name {
            self.greeting_name_draft = query;
        } else if self.keyboard_field == TextField::InputName {
            self.input_name = query;
        } else if self.keyboard_field == TextField::Widget {
            self.widget_text_draft = query;
        } else {
            self.query = query;
            self.scroll = 0.;
        }
        self.dirty = true;
    }

    pub fn editing_text(&self) -> &str {
        match self.keyboard_field {
            TextField::Location => &self.weather.query,
            TextField::Name => &self.greeting_name_draft,
            TextField::InputName => &self.input_name,
            TextField::Widget => &self.widget_text_draft,
            TextField::Apps => &self.query,
        }
    }
    pub fn open_keyboard(&mut self, field: TextField) {
        self.keyboard_field = field;
        if field == TextField::Name {
            // SDL-webOS cannot pre-fill the system keyboard's text model.
            self.greeting_name_draft.clear();
        }
        if field == TextField::InputName {
            // The TV keyboard starts empty, so a new name replaces the old one.
            self.input_name.clear();
        }
        if field == TextField::Widget {
            self.widget_text_draft.clear();
        }
        if cfg!(all(
            feature = "webos",
            target_os = "linux",
            target_arch = "arm"
        )) {
            self.overlay = Some(Overlay::SystemKeyboard);
            self.focus = match field {
                TextField::Location => Target::WeatherQuery,
                TextField::Name => Target::GreetingName,
                TextField::InputName => Target::InputRename,
                TextField::Widget => self.keyboard_return(),
                TextField::Apps => Target::Search,
            };
            self.panel_started = None;
            self.dirty = true;
            return;
        }
        self.overlay = Some(Overlay::Keyboard);
        self.focus = Target::Key(0);
        if !self.preferences.reduced_motion {
            self.panel_started = Some(Instant::now());
        }
        self.dirty = true;
    }
    fn open_location(&mut self) {
        self.weather.edit(String::new());
        self.subpage = Some(Subpage::WeatherLocation);
        self.focus = Target::WeatherQuery;
        self.scroll = 0.;
        if !self.preferences.reduced_motion {
            self.panel_started = Some(Instant::now());
        }
        self.dirty = true;
    }
    fn keyboard_return(&self) -> Target {
        match self.keyboard_field {
            TextField::Location => Target::WeatherSearch,
            TextField::Name => Target::GreetingName,
            TextField::InputName => Target::InputRename,
            TextField::Widget => self.widget_editing_target(),
            TextField::Apps => Target::Search,
        }
    }

    pub fn activate(&mut self) -> Vec<Effect> {
        if self.recents_open() {
            use crate::recents::Command;
            return match self.focus.clone() {
                Target::Recent(id) => self.recent_request(Command::Launch(id)),
                Target::RecentRetry => self.recent_request(Command::List),
                _ => vec![],
            };
        }
        if self.home_edit.take().is_some() {
            self.dirty = true;
            return vec![Effect::Save];
        }
        if self.moving.take().is_some() {
            // Order was saved at every move. A fresh OK release finishes the
            // mode and keeps the selected favorite, rather than launching it.
            self.dirty = true;
            return vec![];
        }
        self.dirty = true;
        match self.focus.clone() {
            Target::Nav(index) => self.open(index),
            Target::Browse => self.open(1),
            Target::Retry => return vec![Effect::Reload],
            Target::DismissNotice => self.notice = None,
            Target::CloseDialog => return self.back(),
            Target::TvSettings => return vec![Effect::Launch("com.palm.app.settings".into())],
            Target::App(_) if self.page == Page::Apps && !self.launching => self.options(),
            Target::App(id) | Target::Input(id) if !self.launching => {
                return vec![Effect::Launch(id)];
            }
            Target::OpenApp if !self.launching => {
                if let Some(Overlay::Options { id, .. }) = &self.overlay
                    && self.inventory.apps.iter().any(|app| &app.id == id)
                {
                    let id = id.clone();
                    self.back();
                    return vec![Effect::Launch(id)];
                }
            }
            Target::Filter(index) => {
                self.filter = if self.filter == index { 0 } else { index };
                self.scroll = 0.;
            }
            Target::ClearFilter => {
                self.filter = 0;
                self.focus = Target::Filter(1);
                self.scroll = 0.;
            }
            Target::ClearSearch => {
                self.query.clear();
                self.focus = Target::Search;
                self.scroll = 0.;
            }
            Target::Search => {
                self.open_keyboard(TextField::Apps);
            }
            Target::WeatherConfigure => {
                self.open(2);
                self.section = WEATHER_SECTION;
                self.open_location();
                self.open_keyboard(TextField::Location);
            }
            Target::WeatherToggle => {
                self.preferences.show_weather = !self.preferences.show_weather;
                self.weather.invalidate_forecast();
                return vec![Effect::Save];
            }
            Target::WeatherLocation => self.open_location(),
            Target::WeatherTemperature => {
                self.subpage = Some(Subpage::Temperature);
                self.focus = Target::Choice(usize::from(
                    self.preferences.temperature_unit == Unit::Fahrenheit,
                ));
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
            }
            Target::WeatherQuery => self.open_keyboard(TextField::Location),
            Target::GreetingName => self.open_keyboard(TextField::Name),
            Target::GreetingNameClear => {
                self.preferences.greeting_name.clear();
                self.focus = Target::GreetingName;
                return vec![Effect::Save];
            }
            Target::AppNamesHome => {
                self.preferences.show_home_names = !self.preferences.show_home_names;
                return vec![Effect::Save];
            }
            Target::AppNamesApps => {
                self.preferences.show_app_names = !self.preferences.show_app_names;
                return vec![Effect::Save];
            }
            Target::ClockFormat | Target::ClockTimeFormat => {
                let format = self.focus == Target::ClockFormat;
                self.subpage = Some(if format {
                    Subpage::ClockFormat
                } else {
                    Subpage::TimeFormat
                });
                self.focus = Target::Choice(usize::from(if format {
                    self.preferences.hour_24
                } else {
                    self.preferences.show_seconds
                }));
                self.scroll = 0.;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
            }
            Target::InputRename => self.open_keyboard(TextField::InputName),
            Target::InputNameReset => {
                if let Some(Subpage::Input(id)) = &self.subpage
                    && self.preferences.input_names.remove(id).is_some()
                {
                    self.focus = Target::InputRename;
                    return vec![Effect::Save];
                }
            }
            Target::WeatherSearch => {
                return self
                    .weather
                    .search(self.preferences.language)
                    .map(|command| vec![Effect::Weather(command)])
                    .unwrap_or_default();
            }
            Target::WeatherClear => {
                self.preferences.weather_location = None;
                self.weather.remove_location();
                self.focus = Target::WeatherSearch;
                return vec![Effect::Save, Effect::Weather(weather::Command::ClearCache)];
            }
            Target::WeatherResult(index) => {
                if let Some(place) = self.weather.results.get(index).cloned() {
                    self.preferences.weather_location = Some(place);
                    self.weather.invalidate_forecast();
                    self.weather.cancel_search();
                    self.focus = Target::WeatherSearch;
                    return vec![Effect::Save];
                }
            }
            Target::BlockAction(index) => {
                let Some(Overlay::Block { kind, restore }) = self.overlay.take() else {
                    return vec![];
                };
                self.panel_started = None;
                let actions = self.block_actions(kind, matches!(restore, Target::App(_)));
                self.focus = restore.clone();
                match actions.get(index) {
                    Some(BlockAction::Reorder) => {
                        if let Target::App(id) = restore {
                            self.moving = Some(id);
                        }
                    }
                    Some(action @ (BlockAction::Move | BlockAction::Resize)) => {
                        let snapshot = self.home_blocks();
                        self.preferences.home_layout = snapshot.clone();
                        self.home_edit = Some(HomeEdit {
                            kind,
                            resize: *action == BlockAction::Resize,
                            snapshot,
                        });
                    }
                    Some(BlockAction::Settings) => {
                        if let crate::home_grid::BlockKind::Widget(slot) = kind {
                            self.open(2);
                            self.open_widget_settings(slot);
                        }
                        return vec![];
                    }
                    Some(BlockAction::Remove) => {
                        self.preferences.home_layout.retain(|b| b.kind != kind);
                        if let crate::home_grid::BlockKind::Widget(slot) = kind {
                            self.remove_widget_block(slot);
                        }
                        self.focus = self.first_content();
                        return vec![Effect::Save];
                    }
                    None => {}
                }
                return vec![];
            }
            Target::WidgetItem(slot, index) => return self.open_widget_dialog(slot, index),
            Target::WidgetSetting(row) => return self.activate_widget_setting(row),
            Target::StoreEntry(index) => return self.open_store_entry(index),
            Target::StoreAction(index) => return self.store_action(index),
            Target::Poster(index) => {
                if index < self.feeds.premieres.len() {
                    self.overlay = Some(Overlay::Poster {
                        index,
                        restore: Target::Poster(index),
                    });
                    self.focus = Target::PosterApp(0);
                    if !self.preferences.reduced_motion {
                        self.panel_started = Some(Instant::now());
                    }
                }
                return vec![];
            }
            Target::PosterApp(choice) => {
                let Some(Overlay::Poster { index, restore }) = self.overlay.take() else {
                    return vec![];
                };
                self.panel_started = None;
                self.focus = restore;
                let apps = self.poster_apps();
                let (Some(id), Some(premiere)) =
                    (apps.get(choice), self.feeds.premieres.get(index))
                else {
                    return vec![];
                };
                let title = self
                    .inventory
                    .apps
                    .iter()
                    .find(|app| &app.id == id)
                    .map_or(id.as_str(), |app| app.title.as_str())
                    .to_owned();
                let notice = self.preferences.language.format(
                    "posterOpening",
                    &[("app", &title), ("title", &premiere.title)],
                );
                self.preferences.poster_app = id.clone();
                return vec![
                    Effect::Save,
                    Effect::Launch(id.clone()),
                    Effect::Notify(notice),
                ];
            }
            Target::AddBlock => {
                self.overlay = Some(Overlay::AddBlock {
                    restore: Target::AddBlock,
                });
                self.focus = Target::AddKind(0);
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
                return vec![];
            }
            Target::AddKind(index) => {
                let Some(Overlay::AddBlock { restore }) = self.overlay.take() else {
                    return vec![];
                };
                self.panel_started = None;
                self.focus = restore;
                let kind = match self.add_choices().get(index).cloned() {
                    Some(AddChoice::Kind(kind)) => kind,
                    Some(AddChoice::Widget(id)) => {
                        return if self.add_widget(&id).is_ok() {
                            vec![Effect::Save]
                        } else {
                            vec![]
                        };
                    }
                    Some(AddChoice::Store) => {
                        self.open_store(self.focus.clone());
                        return vec![];
                    }
                    None => return vec![],
                };
                let mut blocks = self.home_blocks();
                let (w, h) = kind.default_size();
                let (x, y) = crate::home_grid::first_free(&blocks, w, h);
                blocks.push(crate::home_grid::Block { kind, x, y, w, h });
                self.preferences.home_layout = blocks;
                self.focus = Target::Block(kind);
                return vec![Effect::Save];
            }
            Target::Sidebar(index) => {
                self.section = index;
                self.scroll = 0.;
                self.subpage = None;
                self.focus = self.section_entry();
            }
            Target::Setting(index) => {
                match index {
                    0 => {
                        self.subpage = Some(Subpage::AppNames);
                        self.focus = Target::AppNamesHome;
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    1 => {
                        self.subpage = Some(Subpage::Clock);
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        self.focus = Target::ClockFormat;
                        return vec![];
                    }
                    2 => self.preferences.reduced_motion = !self.preferences.reduced_motion,
                    4 => {
                        self.subpage = Some(Subpage::Language);
                        self.focus = Target::Choice(self.preferences.language.index());
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    5 => return vec![Effect::Quit],
                    7 => {
                        self.subpage = Some(Subpage::TextRendering);
                        self.focus = Target::Choice(self.preferences.text_rendering.index());
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    9 => {
                        self.subpage = Some(Subpage::Theme);
                        self.focus = Target::Choice(
                            crate::themes::THEMES
                                .iter()
                                .position(|theme| theme.id == self.preferences.home_theme)
                                .unwrap_or(0),
                        );
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    6 => {
                        self.subpage = Some(Subpage::Greeting);
                        self.focus = Target::GreetingName;
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    _ => return vec![Effect::Reload],
                }
                return vec![Effect::Save];
            }
            Target::Background(index) => {
                if !self.preferences.reduced_motion
                    && self.preferences.background != BACKGROUNDS[index].0
                {
                    self.previous_background =
                        Some((self.preferences.background.clone(), Instant::now()));
                }
                self.preferences.background = BACKGROUNDS[index].0.into();
                return vec![Effect::Save];
            }
            Target::InputMode(id) => {
                let mode = self
                    .preferences
                    .input_modes
                    .get(&id)
                    .copied()
                    .unwrap_or_default();
                self.subpage = Some(Subpage::Input(id));
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
                self.focus = Target::Choice(match mode {
                    InputMode::On => 0,
                    InputMode::Off => 1,
                    InputMode::Auto => 2,
                });
            }
            Target::Choice(index) => {
                match self.subpage.clone() {
                    Some(Subpage::ClockFormat) => self.preferences.hour_24 = index == 1,
                    Some(Subpage::TimeFormat) => self.preferences.show_seconds = index == 1,
                    Some(Subpage::Input(id)) => {
                        self.preferences.input_modes.insert(
                            id,
                            [InputMode::On, InputMode::Off, InputMode::Auto][index.min(2)],
                        );
                    }
                    Some(Subpage::Temperature) => {
                        self.preferences.temperature_unit = if index == 1 {
                            Unit::Fahrenheit
                        } else {
                            Unit::Celsius
                        }
                    }
                    // A store widget's options: a checklist stays open, a choice closes.
                    Some(Subpage::WidgetOptions(slot, setting)) => {
                        return self.choose_widget_option(slot, setting, index);
                    }
                    // A theme stays open too: its wallpaper shows behind the list at once.
                    Some(Subpage::Theme) => {
                        let theme =
                            &crate::themes::THEMES[index.min(crate::themes::THEMES.len() - 1)];
                        if !self.preferences.reduced_motion
                            && self.preferences.background != theme.wallpaper
                        {
                            self.previous_background =
                                Some((self.preferences.background.clone(), Instant::now()));
                        }
                        crate::themes::apply(&mut self.preferences, theme.id);
                        self.home_scroll = 0.;
                        return vec![Effect::Save];
                    }
                    Some(Subpage::Language) => {
                        self.preferences.language =
                            Language::ALL[index.min(Language::ALL.len() - 1)];
                        self.weather.cancel_search();
                    }
                    Some(Subpage::TextRendering) => {
                        let mode = TextRendering::ALL[index.min(TextRendering::ALL.len() - 1)];
                        if mode != self.preferences.text_rendering {
                            self.preferences.text_rendering = mode;
                            self.notify(self.preferences.language.t("textRenderingRestart"));
                        }
                    }
                    Some(
                        Subpage::WeatherLocation
                        | Subpage::Greeting
                        | Subpage::AppNames
                        | Subpage::Clock
                        | Subpage::Widget(_),
                    ) => {
                        return vec![];
                    }
                    None => return vec![],
                }
                self.close_subpage();
                return vec![Effect::Save];
            }
            Target::Option(index) => {
                if index == 2 {
                    if let Some(Overlay::Options { id, restore }) = &self.overlay
                        && crate::platform::can_uninstall(id)
                        && self.uninstalling.is_none()
                        && self.inventory.apps.iter().any(|app| &app.id == id)
                    {
                        self.overlay = Some(Overlay::Delete {
                            id: id.clone(),
                            restore: restore.clone(),
                        });
                        self.focus = Target::CancelDelete;
                        self.pointer = false;
                        self.pressed = None;
                        self.panel_started = None;
                    }
                    return vec![];
                }
                let Some(Overlay::Options { id, restore }) = self.overlay.take() else {
                    return vec![];
                };
                self.panel_started = None;
                let old_ids: Vec<_> = self.apps().iter().map(|a| a.id.clone()).collect();
                let old_index = old_ids.iter().position(|item| item == &id).unwrap_or(0);
                let mut favorites = self.preferences.favorites(&self.inventory.apps);
                let title = self
                    .inventory
                    .apps
                    .iter()
                    .find(|app| app.id == id)
                    .map_or(id.clone(), |app| app.title.clone());
                if index == 0 {
                    if favorites.contains(&id) {
                        favorites.retain(|item| item != &id);
                        self.notify(
                            self.preferences
                                .language
                                .format("favoriteRemoved", &[("app", &title)]),
                        );
                    } else {
                        favorites.push(id.clone());
                        self.notify(
                            self.preferences
                                .language
                                .format("favoriteAdded", &[("app", &title)]),
                        );
                    }
                    self.preferences.hidden.retain(|item| item != &id);
                    self.preferences.favorites = Some(favorites);
                } else if index == 1 {
                    if self.preferences.hidden.contains(&id) {
                        self.preferences.hidden.retain(|item| item != &id);
                        self.notify(
                            self.preferences
                                .language
                                .format("appRestored", &[("app", &title)]),
                        );
                    } else {
                        self.preferences.hidden.push(id.clone());
                        self.notify(
                            self.preferences
                                .language
                                .format("appHidden", &[("app", &title)]),
                        );
                    }
                }
                if self.filter == 5 && self.hidden_count() == 0 {
                    self.filter = 0;
                }
                self.focus = restore;
                self.pointer = false;
                if let Target::App(id) = &self.focus {
                    let apps = self.apps();
                    if !apps.iter().any(|app| &app.id == id) {
                        self.focus = apps
                            .get(old_index.min(apps.len().saturating_sub(1)))
                            .map(|app| Target::App(app.id.clone()))
                            .unwrap_or_else(|| self.first_content());
                    }
                }
                return if index < 2 {
                    vec![Effect::Save]
                } else {
                    vec![]
                };
            }
            Target::CancelDelete => return self.back(),
            Target::ConfirmDelete => {
                if let Some(Overlay::Delete { id, .. }) = &self.overlay
                    && self.uninstalling.is_none()
                    && crate::platform::can_uninstall(id)
                    && self.inventory.apps.iter().any(|app| &app.id == id)
                {
                    self.uninstalling = Some(id.clone());
                    self.focus = Target::CancelDelete;
                    return vec![Effect::Uninstall(id.clone())];
                }
            }
            Target::Key(index) => match KEYS[index] {
                "Space" => self.text(" "),
                "Delete" => self.delete(),
                "Clear" => match self.keyboard_field {
                    TextField::Location => self.weather.edit(String::new()),
                    TextField::Name => self.greeting_name_draft.clear(),
                    TextField::InputName => self.input_name.clear(),
                    TextField::Widget => self.widget_text_draft.clear(),
                    TextField::Apps => self.query.clear(),
                },
                "Done" => {
                    self.overlay = None;
                    self.panel_started = None;
                    self.focus = self.keyboard_return();
                    return self.commit_text_input();
                }
                key => self.text(key),
            },
            _ => {}
        }
        vec![]
    }

    pub fn back(&mut self) -> Vec<Effect> {
        self.pointer = false;
        self.dirty = true;
        if let Some(edit) = self.home_edit.take() {
            self.preferences.home_layout = edit.snapshot;
            return vec![];
        }
        if self.moving.take().is_some() {
            return vec![];
        }
        if let Some(overlay) = self.overlay.take() {
            self.panel_started = None;
            if let Overlay::Delete { id, restore } = overlay {
                self.overlay = Some(Overlay::Options { id, restore });
                self.focus = Target::Option(2);
                self.pressed = None;
                return vec![];
            }
            if matches!(overlay, Overlay::Recents { .. }) {
                self.recents.closing_focus = Some(self.focus.clone());
                self.recents.updated = Instant::now();
                self.pressed = None;
                if self.preferences.reduced_motion {
                    self.recents.visibility = 0.;
                    self.recents.closing_focus = None;
                }
            }
            if let Overlay::StoreEntry { id, restore } = overlay {
                self.back_to_store(&id, restore);
                return vec![];
            }
            if let Overlay::Options { restore, .. }
            | Overlay::Recents { restore }
            | Overlay::Block { restore, .. }
            | Overlay::AddBlock { restore }
            | Overlay::Poster { restore, .. }
            | Overlay::PhoneKey { restore }
            | Overlay::WidgetDialog { restore, .. }
            | Overlay::Store { restore } = overlay
            {
                self.focus = restore;
                self.recents.pending = None;
                self.reconcile_focus();
                return vec![];
            }
            self.focus = self.keyboard_return();
            return self.commit_text_input();
        }
        if self.page == Page::Settings && self.subpage.is_some() {
            self.close_subpage();
            return vec![];
        }
        if self.page == Page::Settings && !matches!(self.focus, Target::Nav(_)) {
            if matches!(self.focus, Target::Sidebar(_)) {
                self.focus = Target::Nav(2);
            } else {
                self.focus = Target::Sidebar(self.section);
            }
            self.scroll = 0.;
            return vec![];
        }
        if self.page == Page::Apps && matches!(self.focus, Target::Search | Target::ClearSearch) {
            self.query.clear();
            self.focus = Target::Nav(1);
            self.scroll = 0.;
            return vec![];
        }
        if self.page != Page::Home {
            self.open(0);
            vec![]
        } else {
            self.open_recents()
        }
    }
}
