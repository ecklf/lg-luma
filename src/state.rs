use crate::i18n::Language;
use crate::model::{
    App, BACKGROUNDS, CUSTOM_TILE_PREFIX, CustomTile, HOME_GRID_CELLS, HOME_GRID_COLUMNS,
    HomeWidget, INPUT_NAME_LIMIT, InputMode, Inventory, Preferences, WidgetPosition,
    clean_input_name,
};
use crate::text_rendering::TextRendering;
use crate::weather::{self, Unit};
use serde::Serialize;
use std::time::{Duration, Instant};

pub const WIDGETS_SECTION: usize = 2;
pub const SYSTEM_SECTION: usize = 3;
/// Settings › System › Phone & computer.
pub const WEB_ACCESS_SETTING: usize = 17;
pub const APPS_SECTION: usize = 4;
pub const INPUTS_SECTION: usize = 5;
pub const SECTION_COUNT: usize = 6;
pub const SETTINGS_ORDER: [usize; SECTION_COUNT] = [
    0,
    APPS_SECTION,
    INPUTS_SECTION,
    1,
    WIDGETS_SECTION,
    SYSTEM_SECTION,
];
pub const SETTINGS_BOTTOM_PADDING: f32 = 64.;
pub const SETTINGS_LIST_BOTTOM_PADDING: f32 = 48.;
pub const APPS_COLUMNS: usize = 6;
pub const DRAWER_PADDING: f32 = 24.;
pub const DRAWER_NAME_TOP: f32 = 12.;
pub const DRAWER_NAME_HEIGHT: f32 = 28.;
/// Automatic inventory retries before the error shows: with the 6.5 s Luna timeout about 50 s.
pub const INVENTORY_RETRIES: u32 = 5;
pub const INVENTORY_RETRY_DELAY: Duration = Duration::from_secs(2);
pub const HOME_STRIP_LEFT: f32 = 84.;
// Six favourites in full inside the glass card, which keeps 24 px around them.
pub const HOME_STRIP_RIGHT: f32 = 1836.;
pub const HOME_TILE_WIDTH: f32 = 270.;
pub const HOME_TILE_PITCH: f32 = 296.4;
pub const HOME_DOCK_SLOTS: usize = 6;
pub const HOME_DOCK_PADDING: f32 = 24.;
pub const BACKGROUND_FADE: f32 = 0.4;
pub const CYCLE_FADE: f32 = 1.6;

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Page {
    Home,
    Apps,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Target {
    MarketSymbol(HomeWidget),
    MarketCurrency,
    Nav(usize),
    InputsMenu,
    SettingsMenu,
    LumaSettings,
    TvSettings,
    InstallUpdate,
    CheckUpdates,
    Recent(String),
    RecentEmpty,
    RecentRetry,
    App(String),
    DockApp(String),
    Input(String),
    Filter(usize),
    Search,
    Sidebar(usize),
    Setting(usize),
    Widget(usize),
    WidgetCell(usize),
    WidgetAdd,
    WidgetReset,
    AddWidget(HomeWidget),
    ConfigureWidget(HomeWidget),
    RemoveWidget(HomeWidget),
    MoveWidget(HomeWidget),
    DateToggle,
    GreetingName,
    GreetingNameReset,
    DateFormat,
    ClockFormat,
    ClockTimeFormat,
    Background(usize),
    UploadWallpaper,
    WallpaperCycle,
    WallpaperGradient,
    UseBackground,
    DeleteBackground,
    InputMode(String),
    InputRename,
    InputIcon,
    InputNameReset,
    Choice(usize),
    CloseDialog,
    Retry,
    DismissNotice,
    Option(usize),
    OpenApp,
    MoveApp,
    ConfirmDelete,
    CancelDelete,
    Key(usize),
    WeatherConfigure,
    WeatherLocation,
    WeatherTemperature,
    WeatherStyle,
    WeatherSize,
    WeatherShowLocation,
    WeatherShowIcon,
    WeatherIcons,
    Attributions,
    AttributionText,
    WeatherQuery,
    WeatherSearch,
    WeatherClear,
    WeatherResult(usize),
    Tile(String),
    TileTest(String),
    TileRemove(String),
    TileEditor,
    WebAccessToggle,
    TvRemoteToggle,
    WebAccessPair,
    WebAccessDevice(String),
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

#[derive(Clone, Debug)]
pub struct Control {
    pub target: Target,
    pub rect: Rect,
    pub scrollable: bool,
}

#[derive(Clone, Debug)]
pub enum Overlay {
    Inputs,
    Settings,
    Options { id: String, restore: Target },
    Delete { id: String, restore: Target },
    Recents { restore: Target },
    Keyboard,
    SystemKeyboard,
    WallpaperUpload,
    TileEditor,
    WebAccessPair,
    BackgroundOptions { id: String, restore: Target },
    BackgroundDelete { id: String, restore: Target },
    WidgetOptions { cell: usize },
    WidgetCatalog { cell: usize, restore: Target },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Subpage {
    Market(HomeWidget),
    WidgetPosition(HomeWidget),
    Greeting,
    DateFormat,
    Weather,
    Shortcuts,
    Clock,
    ClockFormat,
    TimeFormat,
    Input(String),
    InputIcons(String),
    WeatherLocation,
    Temperature,
    WeatherStyle,
    Language,
    AppSorting,
    AppDrawerStyle,
    AppDrawer,
    RecentApps,
    TextRendering,
    WebAccess,
    WeatherIcons,
    Attributions,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum TextField {
    GreetingName,
    Market(HomeWidget),
    Apps,
    Location,
    InputName,
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
    DeleteWallpaper(String),
    Recent {
        generation: u64,
        command: crate::recents::Command,
    },
    /// Fire-and-forget HTTP request for a custom tile, off the SDL thread.
    FireWebhook(CustomTile),
    Quit,
    Weather(weather::Command),
    UpdateDownload(String),
}

pub const KEYS: [&str; 40] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "Space",
    "Delete", "Clear", "Done",
];

pub struct State {
    pub widget_position_cell: usize,
    pub active_weather: HomeWidget,
    pub extra_weather: Vec<weather::Weather>,
    pub markets: crate::markets::Markets,
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
    pub drawer_visibility: f32,
    drawer_updated: Instant,
    input_status_visibility: std::collections::BTreeMap<String, f32>,
    pub overlay: Option<Overlay>,
    pub moving: Option<String>,
    pub loading: bool,
    /// Set by Refresh apps: the next inventory also reloads every app icon.
    pub reload_artwork: bool,
    /// System action consumed by the SDL loop, with disk work delegated to artwork's worker.
    pub reset_icon_cache: bool,
    pub launching: bool,
    pub uninstalling: Option<String>,
    pub notice: Option<(String, Instant)>,
    pub error: Option<String>,
    pub available_update: Option<String>,
    pub update_status: crate::update::Status,
    pub update_check_requested: bool,
    /// Inventory loads that failed in a row while no apps were shown yet.
    pub inventory_failures: u32,
    pub dirty: bool,
    pub pointer: bool,
    /// The control under the pointer right now; unlike focus, cleared when it leaves.
    pub hover: Option<Target>,
    pub pressed: Option<Target>,
    pub previous_background: Option<(String, Instant)>,
    pub background_fade: f32,
    pub wallpapers: Vec<crate::wallpapers::Wallpaper>,
    pub deleting_wallpaper: Option<String>,
    pub wallpaper_upload: Option<Result<crate::wallpapers::Session, String>>,
    pub tile_editor: Option<Result<crate::companion::Session, String>>,
    pub web_access_pairing: Option<Result<crate::web_access::Pairing, String>>,
    /// Paired phones and computers, as the web access worker last reported them.
    pub web_devices: Vec<crate::web_access::Device>,
    /// The whole-TV remote's connection, as the worker last reported it.
    pub tv_remote: crate::tv_remote::Status,
    /// Devices removed in Settings, for the main loop to revoke.
    pub forget_web_devices: Vec<String>,
    /// Luma update (Settings → System), driven by the update worker.
    pub update: crate::update::InstallStatus,
    pub panel_started: Option<Instant>,
    pub weather: weather::Weather,
    pub keyboard_field: TextField,
    pub input_name: String,
    pub recents: crate::recents::Recents,
}

impl State {
    pub fn widget_weather(&self, widget: HomeWidget) -> &weather::Weather {
        match widget {
            HomeWidget::ExtraWeather(i) => &self.extra_weather[usize::from(i)],
            _ => &self.weather,
        }
    }
    pub fn widget_weather_mut(&mut self, widget: HomeWidget) -> &mut weather::Weather {
        match widget {
            HomeWidget::ExtraWeather(i) => &mut self.extra_weather[usize::from(i)],
            _ => &mut self.weather,
        }
    }
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
            drawer_visibility: 0.,
            drawer_updated: Instant::now(),
            input_status_visibility: std::collections::BTreeMap::new(),
            overlay: None,
            moving: None,
            loading: false,
            reload_artwork: false,
            reset_icon_cache: false,
            launching: false,
            uninstalling: None,
            notice: None,
            error: None,
            available_update: None,
            update_status: crate::update::Status::Unchecked,
            update_check_requested: false,
            inventory_failures: 0,
            dirty: true,
            pointer: false,
            hover: None,
            pressed: None,
            previous_background: None,
            background_fade: BACKGROUND_FADE,
            wallpapers: vec![],
            deleting_wallpaper: None,
            wallpaper_upload: None,
            tile_editor: None,
            web_access_pairing: None,
            web_devices: vec![],
            forget_web_devices: vec![],
            tv_remote: crate::tv_remote::Status::default(),
            update: crate::update::InstallStatus::default(),
            panel_started: None,
            weather: weather::Weather::default(),
            markets: crate::markets::Markets::default(),
            active_weather: HomeWidget::Weather,
            widget_position_cell: 0,
            extra_weather: (0..HOME_GRID_CELLS)
                .map(|_| weather::Weather::default())
                .collect(),
            keyboard_field: TextField::Apps,
            input_name: String::new(),
            recents: crate::recents::Recents::default(),
        }
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
        self.dirty = true;
    }

    pub fn update_response(&mut self, result: Result<Option<String>, String>) {
        match result {
            Ok(latest) => {
                let announce = latest.is_some() && latest != self.available_update;
                self.available_update = latest;
                self.update_status = crate::update::Status::Checked;
                if announce {
                    self.notify(self.preferences.language.t("updateAvailable"));
                }
            }
            Err(error) => {
                eprintln!("Luma update check: {error}");
                self.update_status = crate::update::Status::Failed;
            }
        }
        if matches!(self.focus, Target::CheckUpdates | Target::InstallUpdate) {
            let target = if self.available_update.is_some() {
                Target::InstallUpdate
            } else {
                Target::CheckUpdates
            };
            if self.focus != target {
                self.focus = target;
                self.pressed = None;
            }
        }
        self.dirty = true;
    }

    pub fn update_busy(&self) -> bool {
        self.update_status == crate::update::Status::Checking
            || matches!(
                self.update,
                crate::update::InstallStatus::Downloading(_)
                    | crate::update::InstallStatus::Installing(_)
            )
    }

    pub fn install_response(&mut self, response: crate::update::Response) {
        use crate::update::{InstallStatus, Response};
        match response {
            Response::Progress(percent) => self.update = InstallStatus::Downloading(percent),
            Response::Installing(version) => {
                self.update = InstallStatus::Installing(version);
                self.notify(self.preferences.language.t("updateRestart"));
            }
            Response::Finished(result) => {
                self.available_update = None;
                match result {
                    Ok(()) => {
                        self.update = InstallStatus::Idle;
                        // This process may still run the old executable. Recheck rather
                        // than claim that its in-memory version was replaced.
                        self.update_status = crate::update::Status::Unchecked;
                    }
                    Err(error) => {
                        eprintln!("Luma update: {error}");
                        self.update = InstallStatus::Failed(error);
                        self.update_status = crate::update::Status::Failed;
                        self.notify(self.preferences.language.t("updateError"));
                    }
                }
                if self.focus == Target::InstallUpdate {
                    self.focus = Target::CheckUpdates;
                    self.pressed = None;
                }
            }
        }
        self.dirty = true;
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let goal = if self.page == Page::Apps { 1. } else { 0. };
        let dt = now
            .duration_since(self.drawer_updated)
            .as_secs_f32()
            .min(0.05);
        self.drawer_updated = now;
        self.animate_input_status(dt);
        if self.drawer_visibility != goal {
            self.drawer_visibility = if self.preferences.reduced_motion {
                goal
            } else {
                let step = dt / 0.3;
                if goal > self.drawer_visibility {
                    (self.drawer_visibility + step).min(goal)
                } else {
                    (self.drawer_visibility - step).max(goal)
                }
            };
            self.dirty = true;
        }
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
            if self.preferences.reduced_motion
                || start.elapsed().as_secs_f32() >= self.background_fade
            {
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

    pub fn input_status_active(&self, id: &str) -> bool {
        let target = Target::Input(id.into());
        (!self.pointer && self.focus == target)
            || (self.pointer && self.hover.as_ref() == Some(&target))
            || self.pressed.as_ref() == Some(&target)
    }

    pub fn input_status_progress(&self, id: &str) -> f32 {
        let goal = if self.input_status_active(id) { 1. } else { 0. };
        let t = if self.preferences.reduced_motion {
            goal
        } else {
            self.input_status_visibility
                .get(id)
                .copied()
                .unwrap_or(goal)
        };
        t * t * (3. - 2. * t)
    }

    pub(crate) fn animate_input_status(&mut self, dt: f32) {
        self.input_status_visibility
            .retain(|id, _| self.inventory.inputs.iter().any(|input| &input.id == id));
        for input in &self.inventory.inputs {
            let goal = if self.input_status_active(&input.id) {
                1.
            } else {
                0.
            };
            let value = self
                .input_status_visibility
                .entry(input.id.clone())
                .or_insert(goal);
            if *value != goal {
                *value = if self.preferences.reduced_motion {
                    goal
                } else if goal > *value {
                    (*value + dt / 0.16).min(goal)
                } else {
                    (*value - dt / 0.16).max(goal)
                };
                self.dirty = true;
            }
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
        (self.drawer_visibility != if self.page == Page::Apps { 1. } else { 0. })
            || (!self.preferences.reduced_motion
                && self.input_status_visibility.iter().any(|(id, value)| {
                    *value != if self.input_status_active(id) { 1. } else { 0. }
                }))
            || ((self.recents_open() || self.recents.visibility > 0.)
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

    /// Home's lifecycle relaunch returns directly to Home, even from an overlay.
    pub fn return_home(&mut self) -> Vec<Effect> {
        // Commit keyboard drafts before open() clears their settings subpage.
        let pending = self.finish_text_input();
        self.recents.pending = None;
        self.recents.visibility = 0.;
        self.recents.closing_focus = None;
        self.wallpaper_upload = None;
        self.pointer = false;
        self.pressed = None;
        self.open(0);
        // A system relaunch reveals Home directly, not the previous app's drawer
        // closing over it. Ordinary navigation keeps its open/close animation.
        self.drawer_visibility = 0.;
        pending
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
            self.home_scroll = 0.;
        }
        self.query.clear();
        self.filter = 0;
        self.overlay = None;
        self.moving = None;
        self.subpage = None;
        self.panel_started = None;
        if self.page == Page::Home {
            self.focus = self.first_content();
        } else if self.page == Page::Settings {
            self.section = 0;
            self.focus = Target::Sidebar(0);
        }
        self.drawer_updated = Instant::now();
        if self.preferences.reduced_motion {
            self.drawer_visibility = if self.page == Page::Apps { 1. } else { 0. };
        }
        self.dirty = true;
    }

    pub fn managing_apps(&self) -> bool {
        self.page == Page::Settings && self.section == APPS_SECTION && self.subpage.is_none()
    }

    pub fn drawer_progress(&self) -> f32 {
        let t = self.drawer_visibility;
        t * t * (3. - 2. * t)
    }

    pub fn dock_y(&self) -> f32 {
        let home_y = match self.preferences.app_drawer_style {
            // Leave 57.6px below the glass, with room for the 24px app-row peek.
            crate::model::AppDrawerStyle::Tvos => 844.,
            // Leave 48 logical pixels below the glass, including its 24px card padding.
            crate::model::AppDrawerStyle::Minimal => 853.6,
        };
        home_y - (home_y - 164.8) * self.drawer_progress()
    }

    pub fn apps(&self) -> Vec<&App> {
        if self.page == Page::Home {
            return self.dock_apps();
        }
        self.app_list(self.page == Page::Apps)
    }

    pub fn drawer_apps(&self) -> Vec<&App> {
        self.app_list(true)
    }

    /// Count successful app launches only, never inputs, system actions or preview cards.
    pub fn record_app_launch(&mut self, id: &str) -> Vec<Effect> {
        if !self.inventory.apps.iter().any(|app| app.id == id)
            || crate::scroll_preview::is_placeholder(id)
            || self.preferences.custom_tile(id).is_some()
        {
            return vec![];
        }
        let count = self
            .preferences
            .app_launch_counts
            .entry(id.into())
            .or_default();
        *count = count.saturating_add(1);
        self.dirty = true;
        vec![Effect::Save]
    }

    fn app_list(&self, drawer: bool) -> Vec<&App> {
        let ids = self.preferences.favorites(&self.inventory.apps);
        let dock = if drawer { self.dock_apps() } else { vec![] };
        let mut apps: Vec<_> = self
            .inventory
            .apps
            .iter()
            .filter(|app| {
                (self.managing_apps() || !self.preferences.hidden.contains(&app.id))
                    && (!drawer || !dock.iter().any(|pinned| pinned.id == app.id))
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
                .then_with(|| self.preferences.compare_apps(a, b))
        });
        apps
    }

    pub fn favorite_apps(&self) -> Vec<&App> {
        self.preferences
            .favorites(&self.inventory.apps)
            .iter()
            .filter(|id| !self.preferences.hidden.contains(id))
            .filter_map(|id| self.inventory.apps.iter().find(|app| &app.id == id))
            .collect()
    }

    /// Backfill the six visible dock slots without changing saved favorites.
    pub fn dock_apps(&self) -> Vec<&App> {
        let mut apps = self.favorite_apps();
        if apps.len() < HOME_DOCK_SLOTS {
            let mut remaining: Vec<_> = self
                .inventory
                .apps
                .iter()
                .filter(|app| {
                    !self.preferences.hidden.contains(&app.id)
                        && !apps.iter().any(|pinned| pinned.id == app.id)
                })
                .collect();
            remaining.sort_by(|a, b| self.preferences.compare_apps(a, b));
            apps.extend(remaining.into_iter().take(HOME_DOCK_SLOTS - apps.len()));
        }
        apps
    }

    pub fn content_viewport(&self) -> Rect {
        Rect::new(64., 108.8, 1792., 1080. - 108.8)
    }

    fn scroll_bottom(&self) -> f32 {
        let viewport = self.content_viewport();
        viewport.y + viewport.h
            - if self.page == Page::Settings {
                if self.subpage == Some(Subpage::Language)
                    || ((self.section == 1 || self.section == APPS_SECTION)
                        && self.subpage.is_none())
                {
                    SETTINGS_LIST_BOTTOM_PADDING
                } else {
                    SETTINGS_BOTTOM_PADDING
                }
            } else if self.page == Page::Apps {
                // Scroll card-only hit regions far enough to reveal their names too.
                DRAWER_PADDING
                    + if self.preferences.show_drawer_app_names {
                        DRAWER_NAME_TOP + DRAWER_NAME_HEIGHT
                    } else {
                        0.
                    }
            } else {
                12.
            }
    }

    /// The favourites' glass card: their tiles scroll, draw and take pointer input only inside it.
    pub fn favorites_viewport(&self) -> Rect {
        // dock_apps backfills from all visible apps. For pointer/wheel events we
        // only need the number of occupied slots, not their sorted order.
        let slots = self
            .inventory
            .apps
            .iter()
            .filter(|app| !self.preferences.hidden.contains(&app.id))
            .take(HOME_DOCK_SLOTS)
            .count();
        self.dock_viewport_for(slots)
    }

    pub fn dock_viewport_for(&self, count: usize) -> Rect {
        let content = self.content_viewport();
        let tiles = if (1..HOME_DOCK_SLOTS).contains(&count) {
            HOME_TILE_WIDTH + (count - 1) as f32 * HOME_TILE_PITCH
        } else {
            // Retain the six-slot viewport for overflow and the loading/empty UI.
            HOME_STRIP_RIGHT - HOME_STRIP_LEFT
        };
        let width = tiles + 2. * HOME_DOCK_PADDING;
        Rect::new((1920. - width) / 2., content.y, width, content.h)
    }

    pub fn apps_viewport(&self) -> Rect {
        let content = self.content_viewport();
        // Let scrolling cards continue behind the floating dock, up to its card edge.
        let top = 164.8;
        Rect::new(content.x, top, content.w, content.y + content.h - top)
    }

    pub fn drawer_grid_top(&self) -> f32 {
        // Pinned artwork height, glass inset, then the matching outer list padding.
        164.8 + 154.4 + 24. + DRAWER_PADDING
    }

    pub fn app_settings_viewport(&self) -> Rect {
        Rect::new(620., 148.8, 1160., 1080. - 148.8)
    }

    pub fn backgrounds_viewport(&self) -> Rect {
        let content = self.content_viewport();
        Rect::new(620., 244.8, 1160., content.y + content.h - 244.8)
    }

    pub fn attributions_viewport(&self) -> Rect {
        Rect::new(640., 164.8, 1120., 1080. - 164.8 - SETTINGS_BOTTOM_PADDING)
    }

    fn scroll_attributions(&mut self, controls: &[Control], delta: f32) {
        let viewport = self.attributions_viewport();
        let height = controls
            .iter()
            .find(|control| control.target == Target::AttributionText)
            .map_or(viewport.h, |control| control.rect.h);
        self.scroll = (self.scroll + delta).clamp(0., (height - viewport.h).max(0.));
        self.dirty = true;
    }

    pub fn hit_test(&self, controls: &[Control], x: f32, y: f32) -> Option<Target> {
        controls
            .iter()
            .rev()
            .find(|control| {
                // Moving presentations are not pointer targets until they reach their layout.
                if matches!(control.target, Target::App(_) | Target::DockApp(_))
                    && ((self.page == Page::Apps && self.drawer_visibility < 1.)
                        || (self.page == Page::Home && self.drawer_visibility > 0.))
                {
                    return false;
                }
                let viewport = if control.target == Target::AttributionText {
                    self.attributions_viewport()
                } else if (self.page == Page::Home && matches!(control.target, Target::App(_)))
                    || matches!(control.target, Target::DockApp(_))
                {
                    self.favorites_viewport()
                } else if matches!(control.target, Target::Background(_)) {
                    self.backgrounds_viewport()
                } else if self.managing_apps() && matches!(control.target, Target::App(_)) {
                    self.app_settings_viewport()
                } else if self.page == Page::Apps {
                    self.apps_viewport()
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
        if self.page == Page::Settings
            && self.subpage == Some(Subpage::Attributions)
            && self.overlay.is_none()
        {
            if position.is_none_or(|(x, y)| self.attributions_viewport().contains(x, y)) {
                self.scroll_attributions(controls, -y as f32 * 90.);
                self.pointer = true;
            }
            return false;
        }
        let backgrounds =
            self.page == Page::Settings && self.section == 1 && self.subpage.is_none();
        if self.managing_apps()
            && position.is_some_and(|(x, y)| !self.app_settings_viewport().contains(x, y))
        {
            return false;
        }
        if backgrounds && position.is_some_and(|(x, y)| !self.backgrounds_viewport().contains(x, y))
        {
            return false;
        }
        let over_favorites = (self.page == Page::Home || self.page == Page::Apps)
            && position.is_some_and(|(x, y)| {
                self.favorites_viewport().contains(x, y)
                    && controls.iter().any(|control| {
                        (self.page == Page::Home && matches!(control.target, Target::App(_))
                            || matches!(control.target, Target::DockApp(_)))
                            && control.rect.contains(x, y)
                    })
            });
        let favorites =
            (self.page == Page::Home || self.page == Page::Apps) && (x != 0 || over_favorites);
        if favorites {
            self.scroll_favorites((if x != 0 { x } else { -y }) as f32 * HOME_TILE_PITCH);
        } else {
            let bottom = self.scroll_bottom();
            let max = controls
                .iter()
                .filter(|c| {
                    c.scrollable && (!backgrounds || matches!(c.target, Target::Background(_)))
                })
                .map(|c| c.rect.y + c.rect.h + self.scroll - bottom)
                .fold(0., f32::max);
            self.scroll = (self.scroll - y as f32 * 90.).clamp(0., max.max(0.));
        }
        self.pointer = true;
        self.dirty = true;
        favorites
    }

    pub fn scroll_favorites(&mut self, delta: f32) {
        let count = self.dock_apps().len();
        let content = if count == 0 {
            0.
        } else {
            (count - 1) as f32 * HOME_TILE_PITCH + HOME_TILE_WIDTH
        };
        let max = (content - (HOME_STRIP_RIGHT - HOME_STRIP_LEFT)).max(0.);
        let next = (self.home_scroll + delta).clamp(0., max);
        if next != self.home_scroll {
            self.home_scroll = next;
            self.dirty = true;
        }
    }

    pub fn set_inventory(&mut self, next: Inventory, warning: Option<String>) {
        self.inventory.apps = next.apps;
        self.sync_custom_tiles();
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

    /// Replace only input status; leave installed apps, artwork and saved preferences alone.
    pub fn set_inputs(&mut self, inputs: Vec<crate::model::Input>) {
        if self.inventory.inputs == inputs {
            return;
        }
        self.inventory.inputs = inputs;
        self.dirty = true;
        self.reconcile_focus();
        let missing = |id: &str| !self.inventory.inputs.iter().any(|input| input.id == id);
        if self.page == Page::Settings
            && (matches!(&self.subpage, Some(Subpage::Input(id) | Subpage::InputIcons(id)) if missing(id))
                || matches!(&self.focus, Target::InputMode(id) if missing(id)))
        {
            self.section = INPUTS_SECTION;
            self.subpage = None;
            self.scroll = 0.;
            self.focus = self
                .inventory
                .inputs
                .first()
                .map_or(Target::Sidebar(INPUTS_SECTION), |input| {
                    Target::InputMode(input.id.clone())
                });
        }
    }

    /// A failed inventory load. Right after a cold boot, or when the TV is short of memory, the
    /// Homebrew service that reads the inventory can take longer than one request allows. While
    /// no apps are shown yet, the screen keeps loading and the caller retries after the returned
    /// delay; the error and its Retry button only appear once those retries are used up.
    /// Custom tiles come from preferences, not the TV, so they don't count as loaded apps.
    pub fn inventory_failed(&mut self, error: String) -> Option<Duration> {
        self.dirty = true;
        let no_tv_apps = self
            .inventory
            .apps
            .iter()
            .all(|app| app.id.starts_with(CUSTOM_TILE_PREFIX));
        if no_tv_apps && self.inventory_failures < INVENTORY_RETRIES {
            self.inventory_failures += 1;
            return Some(INVENTORY_RETRY_DELAY);
        }
        self.inventory_failures = 0;
        self.loading = false;
        self.error = Some(error);
        None
    }

    /// Custom webhook tiles are never read from the TV. They are merged into
    /// the inventory on every refresh and edit, so favoriting, hiding and
    /// search treat them like any other app.
    pub fn sync_custom_tiles(&mut self) {
        self.inventory
            .apps
            .retain(|app| !app.id.starts_with(CUSTOM_TILE_PREFIX));
        self.inventory.apps.extend(self.preferences.custom_apps());
        self.dirty = true;
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
        if let Target::DockApp(id) = &self.focus
            && !self.dock_apps().iter().any(|app| &app.id == id)
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
            self.focus = self.first_input_target();
        }
        if matches!(self.overlay, Some(Overlay::Inputs))
            && self.focus == Target::InputsMenu
            && let Some(input) = self
                .inventory
                .inputs
                .iter()
                .find(|input| self.preferences.input_visible(input))
        {
            self.focus = Target::Input(input.id.clone());
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
                } else if self.preferences.weather_visible()
                    && self.preferences.weather_location.is_none()
                {
                    Target::WeatherConfigure
                } else {
                    Target::SettingsMenu
                }),
            Page::Apps => self
                .apps()
                .first()
                .map(|app| Target::App(app.id.clone()))
                .or_else(|| {
                    self.dock_apps()
                        .first()
                        .map(|app| Target::DockApp(app.id.clone()))
                })
                .unwrap_or(Target::SettingsMenu),
            Page::Settings => Target::Sidebar(0),
        }
    }

    pub fn first_input_target(&self) -> Target {
        self.inventory
            .inputs
            .iter()
            .find(|input| self.preferences.input_visible(input))
            .map_or(Target::SettingsMenu, |input| {
                Target::Input(input.id.clone())
            })
    }

    pub fn focus(&mut self, target: Target, controls: &[Control]) {
        if matches!(self.subpage, Some(Subpage::WidgetPosition(_)))
            && let Target::Choice(cell) = target
        {
            self.widget_position_cell = cell;
        }
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
            if (self.page == Page::Home && matches!(self.focus, Target::App(_)))
                || matches!(self.focus, Target::DockApp(_))
            {
                if control.rect.x < HOME_STRIP_LEFT {
                    self.home_scroll += control.rect.x - HOME_STRIP_LEFT;
                } else if control.rect.x + control.rect.w > HOME_STRIP_RIGHT {
                    self.home_scroll += control.rect.x + control.rect.w - HOME_STRIP_RIGHT;
                }
            }
            if matches!(
                self.focus,
                Target::Nav(_)
                    | Target::InputsMenu
                    | Target::Input(_)
                    | Target::SettingsMenu
                    | Target::LumaSettings
                    | Target::Filter(_)
                    | Target::Search
                    | Target::Sidebar(_)
            ) {
                self.scroll = 0.;
            } else if control.scrollable
                && !matches!(self.focus, Target::DockApp(_) | Target::AttributionText)
                && self.page != Page::Home
            {
                let top = if self.managing_apps() {
                    self.app_settings_viewport().y + 16.
                } else if matches!(self.focus, Target::Background(_)) {
                    self.backgrounds_viewport().y + 8.
                } else if self.page == Page::Apps {
                    // Remote focus stays below the dock even though cards can draw behind it.
                    self.drawer_grid_top() - 8.
                } else {
                    120.8
                };
                if control.rect.y < top {
                    self.scroll = (self.scroll + control.rect.y - top).max(0.);
                }
                let bottom = self.scroll_bottom();
                if control.rect.y + control.rect.h > bottom {
                    self.scroll += control.rect.y + control.rect.h - bottom;
                }
            }
        }
        if self.page == Page::Home || self.page == Page::Apps {
            self.scroll_favorites(0.);
        }
        if self.scroll != previous || self.home_scroll != previous_home {
            self.dirty = true;
        }
        self.scroll != previous || self.home_scroll != previous_home
    }

    /// Hover and pointer-down must not move the control before pointer-up.
    pub fn pointer_focus(&mut self, target: Target) {
        if matches!(self.subpage, Some(Subpage::WidgetPosition(_)))
            && let Target::Choice(cell) = target
        {
            self.widget_position_cell = cell;
        }
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
            if self.section != section || self.subpage.is_some() {
                self.scroll = 0.;
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

    pub fn background_count(&self) -> usize {
        self.background_ids().count()
    }

    pub fn cycle_wallpaper(&mut self, now: u64) -> bool {
        let ids: Vec<_> = self
            .background_ids()
            .filter(|id| self.deleting_wallpaper.as_deref() != Some(*id))
            .map(str::to_owned)
            .collect();
        let next = crate::wallpaper_cycle::next(&ids, &self.preferences.background);
        self.preferences.wallpaper_changed = now;
        let Some(id) = next else {
            return false;
        };
        if self.preferences.reduced_motion {
            self.previous_background = None;
        } else {
            self.previous_background = Some((self.preferences.background.clone(), Instant::now()));
            self.background_fade = CYCLE_FADE;
        }
        self.preferences.background = id;
        self.dirty = true;
        true
    }

    pub fn background_id(&self, index: usize) -> Option<&str> {
        self.background_ids().nth(index)
    }

    fn background_ids(&self) -> impl Iterator<Item = &str> {
        BACKGROUNDS
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| {
                *id == self.preferences.background
                    || !self
                        .preferences
                        .deleted_backgrounds
                        .iter()
                        .any(|deleted| deleted == id)
            })
            .chain(self.wallpapers.iter().map(|item| item.id.as_str()))
    }

    pub fn can_delete_background(&self, id: &str) -> bool {
        id != self.preferences.background && self.background_ids().any(|known| known == id)
    }

    fn background_target(&self, id: &str) -> Target {
        Target::Background(
            self.background_ids()
                .position(|known| known == id)
                .unwrap_or(0),
        )
    }

    pub fn wallpaper_deleted(&mut self, id: String, result: Result<(), String>) {
        if self.deleting_wallpaper.as_deref() != Some(&id) {
            return;
        }
        self.deleting_wallpaper = None;
        if let Err(error) = result {
            eprintln!("Luma background deletion: {error}");
            self.notify(self.preferences.language.t("deleteBackgroundError"));
            return;
        }
        self.finish_background_delete(&id);
    }

    /// Deletes one of the owner's own photos for a paired phone or computer, like confirming
    /// Delete in the TV's background options.
    pub fn delete_web_photo(&mut self, id: &str) -> Vec<Effect> {
        if !crate::wallpapers::valid_id(id)
            || !self.can_delete_background(id)
            || self.deleting_wallpaper.is_some()
        {
            return vec![];
        }
        self.deleting_wallpaper = Some(id.to_owned());
        vec![Effect::DeleteWallpaper(id.to_owned())]
    }

    /// Opens an app or sends a shortcut tile chosen on a paired phone or computer, as selecting
    /// it on Home does.
    pub fn web_launch(&mut self, id: &str) -> Vec<Effect> {
        if self.launching {
            return vec![];
        }
        if let Some(tile) = self.preferences.custom_tile(id).cloned() {
            return self.send_tile(tile);
        }
        if self.inventory.apps.iter().any(|app| app.id == id)
            || self.inventory.inputs.iter().any(|input| input.id == id)
        {
            return vec![Effect::Launch(id.to_owned())];
        }
        vec![]
    }

    /// A Settings › System action from a paired phone or computer, as pressing its row does.
    pub fn web_action(&mut self, action: &str) -> Vec<Effect> {
        match action {
            "refresh" => vec![Effect::Reload],
            "reset_icons" => {
                self.reset_icon_cache = true;
                vec![]
            }
            "update" if !self.update_busy() => {
                if let Some(version) = self.available_update.clone() {
                    self.update = crate::update::InstallStatus::Downloading(0);
                    vec![Effect::UpdateDownload(version)]
                } else {
                    self.update = crate::update::InstallStatus::Idle;
                    self.update_status = crate::update::Status::Checking;
                    self.update_check_requested = true;
                    self.dirty = true;
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    fn finish_background_delete(&mut self, id: &str) {
        let index = self
            .background_ids()
            .position(|known| known == id)
            .unwrap_or(0);
        let focused = match self.focus {
            Target::Background(index) => self.background_id(index).map(str::to_owned),
            _ => None,
        };
        self.wallpapers.retain(|item| item.id != id);
        if BACKGROUNDS.iter().any(|(known, _)| *known == id) {
            self.preferences.deleted_backgrounds.push(id.to_owned());
        }
        if matches!(&self.overlay, Some(Overlay::BackgroundOptions { id: current, .. } | Overlay::BackgroundDelete { id: current, .. }) if current == id)
        {
            self.overlay = None;
            self.panel_started = None;
            self.focus = Target::Background(index.min(self.background_count().saturating_sub(1)));
            self.pointer = false;
        } else if let Some(focused) = focused {
            self.focus = self.background_target(&focused);
        }
        self.pressed = None;
        self.notify(self.preferences.language.t("backgroundDeleted"));
    }

    pub fn set_wallpapers(&mut self, wallpapers: Vec<crate::wallpapers::Wallpaper>) {
        // A newly uploaded ID can sort before existing entries. Keep the focused
        // image, rather than moving the pointer's target to a different photograph.
        let focused_id = if let Target::Background(index) = self.focus {
            self.background_id(index).map(str::to_owned)
        } else {
            None
        };
        self.wallpapers = wallpapers;
        if let Some(
            Overlay::BackgroundOptions { id, restore } | Overlay::BackgroundDelete { id, restore },
        ) = &self.overlay
            && !self.background_ids().any(|known| known == id)
        {
            let index = if let Target::Background(index) = restore {
                *index
            } else {
                0
            };
            self.focus = Target::Background(index.min(self.background_count().saturating_sub(1)));
            self.overlay = None;
            self.panel_started = None;
            self.pressed = None;
            self.pointer = false;
        }
        if let Some(id) = focused_id {
            let index = (0..self.background_count())
                .find(|&i| self.background_id(i) == Some(id.as_str()))
                .unwrap_or(0);
            self.focus = Target::Background(index);
        }
        self.dirty = true;
    }

    fn open_widget_catalog(&mut self, cell: usize) {
        self.overlay = Some(Overlay::WidgetCatalog {
            cell,
            restore: self.focus.clone(),
        });
        self.focus = HomeWidget::ALL
            .into_iter()
            .find_map(|kind| self.preferences.available_widget(kind))
            .map_or(Target::CloseDialog, Target::AddWidget);
        self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
    }

    fn section_entry(&self) -> Target {
        match self.section {
            1 => Target::Background(
                (0..self.background_count())
                    .position(|index| {
                        self.background_id(index) == Some(self.preferences.background.as_str())
                    })
                    .unwrap_or(0),
            ),
            WIDGETS_SECTION => Target::WidgetAdd,
            INPUTS_SECTION => self
                .inventory
                .inputs
                .first()
                .map_or(Target::Setting(3), |input| {
                    Target::InputMode(input.id.clone())
                }),
            APPS_SECTION => self
                .apps()
                .first()
                .map_or(Target::Sidebar(APPS_SECTION), |app| {
                    Target::App(app.id.clone())
                }),
            SYSTEM_SECTION if self.available_update.is_some() => Target::InstallUpdate,
            SYSTEM_SECTION => Target::CheckUpdates,
            _ => Target::Setting(1),
        }
    }

    fn close_subpage(&mut self) {
        if self.subpage == Some(Subpage::WeatherLocation) {
            self.weather.cancel_search();
        }
        self.panel_started = None;
        self.pointer = false;
        self.focus = match self.subpage.take() {
            Some(Subpage::Market(kind)) => {
                Target::WidgetCell(self.preferences.widget_position(kind).cell())
            }
            Some(Subpage::WidgetPosition(widget)) => {
                Target::WidgetCell(self.preferences.widget_position(widget).cell())
            }
            Some(Subpage::Greeting) => {
                Target::WidgetCell(self.preferences.greeting_position.cell())
            }
            Some(Subpage::DateFormat) => {
                self.subpage = Some(Subpage::Greeting);
                Target::DateFormat
            }
            Some(Subpage::Weather) => {
                Target::WidgetCell(self.preferences.widget_position(self.active_weather).cell())
            }
            Some(Subpage::Shortcuts) => Target::Widget(2),
            Some(Subpage::Clock) => Target::Setting(1),
            Some(Subpage::ClockFormat) => {
                self.subpage = Some(Subpage::Clock);
                Target::ClockFormat
            }
            Some(Subpage::TimeFormat) => {
                self.subpage = Some(Subpage::Clock);
                Target::ClockTimeFormat
            }
            Some(Subpage::InputIcons(id)) => {
                self.subpage = Some(Subpage::Input(id));
                Target::InputIcon
            }
            Some(Subpage::Input(id)) => {
                self.section = INPUTS_SECTION;
                Target::InputMode(id)
            }
            Some(Subpage::WeatherLocation) => {
                self.subpage = Some(Subpage::Weather);
                Target::WeatherLocation
            }
            Some(Subpage::Temperature) => {
                self.subpage = Some(Subpage::Weather);
                Target::WeatherTemperature
            }
            Some(Subpage::WeatherStyle) => {
                self.subpage = Some(Subpage::Weather);
                Target::WeatherStyle
            }
            Some(Subpage::WeatherIcons) => {
                self.subpage = Some(Subpage::WeatherStyle);
                Target::WeatherIcons
            }
            Some(Subpage::Attributions) => Target::Attributions,
            Some(Subpage::Language) => Target::Setting(4),
            Some(Subpage::TextRendering) => Target::Setting(7),
            Some(Subpage::AppSorting) => {
                self.subpage = Some(Subpage::AppDrawer);
                Target::Setting(10)
            }
            Some(Subpage::AppDrawerStyle) => {
                self.subpage = Some(Subpage::AppDrawer);
                Target::Setting(15)
            }
            Some(Subpage::WebAccess) => Target::Setting(WEB_ACCESS_SETTING),
            Some(Subpage::AppDrawer) => Target::Setting(13),
            Some(Subpage::RecentApps) => Target::Setting(12),
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
        if self.overlay.is_none()
            && self.page == Page::Settings
            && self.subpage == Some(Subpage::Attributions)
            && self.focus == Target::AttributionText
        {
            match direction {
                Direction::Up => self.scroll_attributions(controls, -180.),
                Direction::Down => self.scroll_attributions(controls, 180.),
                Direction::Left => self.close_subpage(),
                Direction::Right => {}
            }
            return vec![];
        }
        if self.overlay.is_none()
            && self.page == Page::Settings
            && self.section == WIDGETS_SECTION
            && self.subpage.is_none()
            && let Target::WidgetCell(index) = self.focus
        {
            let target = match direction {
                Direction::Left if index % HOME_GRID_COLUMNS == 0 => {
                    Target::Sidebar(WIDGETS_SECTION)
                }
                Direction::Left => Target::WidgetCell(index - 1),
                Direction::Right if index % HOME_GRID_COLUMNS + 1 < HOME_GRID_COLUMNS => {
                    Target::WidgetCell(index + 1)
                }
                Direction::Up if index >= HOME_GRID_COLUMNS => {
                    Target::WidgetCell(index - HOME_GRID_COLUMNS)
                }
                Direction::Up => {
                    if index % HOME_GRID_COLUMNS == 0 {
                        Target::Widget(2)
                    } else if index % HOME_GRID_COLUMNS == 1 {
                        Target::WidgetReset
                    } else {
                        Target::WidgetAdd
                    }
                }
                Direction::Down if index + HOME_GRID_COLUMNS < HOME_GRID_CELLS => {
                    Target::WidgetCell(index + HOME_GRID_COLUMNS)
                }
                _ => self.focus.clone(),
            };
            self.focus(target, controls);
            return vec![];
        }
        if self.overlay.is_none()
            && self.page == Page::Settings
            && self.section == WIDGETS_SECTION
            && self.subpage.is_none()
            && matches!(
                self.focus,
                Target::WidgetAdd | Target::WidgetReset | Target::Widget(2)
            )
        {
            let target = match direction {
                Direction::Left if self.focus == Target::WidgetAdd => Target::WidgetReset,
                Direction::Left if self.focus == Target::WidgetReset => Target::Widget(2),
                Direction::Left => Target::Sidebar(WIDGETS_SECTION),
                Direction::Right if self.focus == Target::Widget(2) => Target::WidgetReset,
                Direction::Right => Target::WidgetAdd,
                Direction::Up => Target::SettingsMenu,
                Direction::Down => Target::WidgetCell(0),
            };
            self.focus(target, controls);
            return vec![];
        }
        if self.overlay.is_none()
            && matches!(self.subpage, Some(Subpage::WidgetPosition(_)))
            && let Target::Choice(index) = self.focus
        {
            let next = match direction {
                Direction::Left if index % HOME_GRID_COLUMNS > 0 => index - 1,
                Direction::Right if index % HOME_GRID_COLUMNS + 1 < HOME_GRID_COLUMNS => index + 1,
                Direction::Up if index >= HOME_GRID_COLUMNS => index - HOME_GRID_COLUMNS,
                Direction::Down if index + HOME_GRID_COLUMNS < HOME_GRID_CELLS => {
                    index + HOME_GRID_COLUMNS
                }
                _ => index,
            };
            self.focus(Target::Choice(next), controls);
            self.widget_position_cell = next;
            return vec![];
        }
        if matches!(self.overlay, Some(Overlay::Inputs | Overlay::Settings)) {
            let targets: Vec<_> = if matches!(self.overlay, Some(Overlay::Inputs)) {
                self.inventory
                    .inputs
                    .iter()
                    .filter(|input| self.preferences.input_visible(input))
                    .map(|input| Target::Input(input.id.clone()))
                    .collect()
            } else {
                vec![Target::LumaSettings, Target::TvSettings]
            };
            if let Some(index) = targets.iter().position(|target| *target == self.focus) {
                let next = match direction {
                    Direction::Up => index.saturating_sub(1),
                    Direction::Down => (index + 1).min(targets.len() - 1),
                    _ => index,
                };
                self.focus = targets[next].clone();
            }
            return vec![];
        }
        if matches!(self.overlay, Some(Overlay::SystemKeyboard)) {
            // The TV keyboard owns its arrows until Done/Back dismisses it.
            return vec![];
        }
        if matches!(
            self.overlay,
            Some(Overlay::WallpaperUpload | Overlay::TileEditor | Overlay::WebAccessPair)
        ) {
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
            if self.page == Page::Apps
                && let Target::DockApp(id) = &self.focus
            {
                let apps = self.dock_apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    let target = match direction {
                        Direction::Up => {
                            if index < 3 {
                                self.first_input_target()
                            } else {
                                Target::SettingsMenu
                            }
                        }
                        Direction::Down => self
                            .apps()
                            .get(index.min(self.apps().len().saturating_sub(1)))
                            .map_or_else(|| self.focus.clone(), |app| Target::App(app.id.clone())),
                        Direction::Left => {
                            Target::DockApp(apps[(index + apps.len() - 1) % apps.len()].id.clone())
                        }
                        Direction::Right => {
                            Target::DockApp(apps[(index + 1) % apps.len()].id.clone())
                        }
                    };
                    if matches!(direction, Direction::Up) {
                        self.open(0);
                    }
                    self.focus(target, controls);
                    return vec![];
                }
            }
            if matches!(
                self.focus,
                Target::Input(_) | Target::InputsMenu | Target::SettingsMenu
            ) {
                let mut targets: Vec<_> = self
                    .inventory
                    .inputs
                    .iter()
                    .filter(|input| self.preferences.input_visible(input))
                    .map(|input| Target::Input(input.id.clone()))
                    .collect();
                targets.push(Target::SettingsMenu);
                let index = targets
                    .iter()
                    .position(|target| *target == self.focus)
                    .unwrap_or(0);
                match direction {
                    Direction::Left => {
                        self.focus(targets[index.saturating_sub(1)].clone(), controls)
                    }
                    Direction::Right => self.focus(
                        targets[(index + 1).min(targets.len() - 1)].clone(),
                        controls,
                    ),
                    Direction::Down if self.page == Page::Home && self.dock_apps().is_empty() => {
                        self.open(1);
                        self.focus = self.first_content();
                    }
                    Direction::Down => self.focus(
                        if self.page == Page::Home
                            && self.preferences.weather_visible()
                            && self.preferences.weather_location.is_none()
                        {
                            Target::WeatherConfigure
                        } else {
                            self.first_content()
                        },
                        controls,
                    ),
                    Direction::Up => {}
                }
                return vec![];
            }
            if self.managing_apps()
                && let Target::App(id) = &self.focus
            {
                let apps = self.apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    let target = match direction {
                        Direction::Left if index % 3 == 0 => Target::Sidebar(APPS_SECTION),
                        Direction::Left => Target::App(apps[index - 1].id.clone()),
                        Direction::Up if index < 3 => Target::SettingsMenu,
                        Direction::Up => Target::App(apps[index - 3].id.clone()),
                        Direction::Down if (index / 3 + 1) * 3 < apps.len() => {
                            Target::App(apps[(index + 3).min(apps.len() - 1)].id.clone())
                        }
                        Direction::Right => {
                            Target::App(apps[(index + 1).min(apps.len() - 1)].id.clone())
                        }
                        _ => return vec![],
                    };
                    self.focus(target, controls);
                    return vec![];
                }
            }
            if self.page == Page::Home && self.focus == Target::WeatherConfigure {
                match direction {
                    Direction::Down
                        if self.focus == Target::WeatherConfigure
                            && !self.dock_apps().is_empty() =>
                    {
                        self.focus(self.first_content(), controls)
                    }
                    Direction::Down => {
                        self.open(1);
                        self.focus = self.first_content();
                    }
                    Direction::Up => self.focus(self.first_input_target(), controls),
                    _ => {}
                }
                return vec![];
            }
            if self.page == Page::Home
                && let Target::App(id) = &self.focus
            {
                let apps = self.apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    match direction {
                        Direction::Left | Direction::Right => {
                            let next = if matches!(direction, Direction::Left) {
                                (index + apps.len() - 1) % apps.len()
                            } else {
                                (index + 1) % apps.len()
                            };
                            let id = apps[next].id.clone();
                            self.focus(Target::App(id), controls);
                        }
                        Direction::Up => self.focus(
                            if index < 3 {
                                self.first_input_target()
                            } else {
                                Target::SettingsMenu
                            },
                            controls,
                        ),
                        Direction::Down if self.drawer_apps().is_empty() => {}
                        Direction::Down => {
                            self.open(1);
                            self.focus = self.first_content();
                        }
                    }
                    return vec![];
                }
            }
            let setting_rows: Option<&[Target]> = match self.subpage {
                Some(Subpage::Market(kind)) if kind.kind() == HomeWidget::Crypto => {
                    Some(&[Target::MarketSymbol(kind), Target::MarketCurrency])
                }
                Some(Subpage::Market(kind)) => Some(&[Target::MarketSymbol(kind)]),
                Some(Subpage::Greeting) => Some(&[
                    Target::DateToggle,
                    Target::DateFormat,
                    Target::GreetingName,
                    Target::GreetingNameReset,
                ]),
                Some(Subpage::Weather) => Some(&[
                    Target::WeatherLocation,
                    Target::WeatherTemperature,
                    Target::WeatherStyle,
                    Target::WeatherSize,
                ]),
                Some(Subpage::WeatherStyle) => Some(&[
                    Target::WeatherShowLocation,
                    Target::WeatherShowIcon,
                    Target::WeatherIcons,
                    Target::Choice(0),
                    Target::Choice(1),
                    Target::Choice(2),
                ]),
                Some(Subpage::Clock) => Some(&[Target::ClockFormat, Target::ClockTimeFormat]),
                _ => None,
            };
            if let Some(rows) = setting_rows
                && let Some(index) = rows.iter().position(|target| *target == self.focus)
            {
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::SettingsMenu
                        } else {
                            rows[index - 1].clone()
                        },
                        controls,
                    ),
                    Direction::Down if index + 1 < rows.len() => {
                        self.focus(rows[index + 1].clone(), controls)
                    }
                    Direction::Right
                        if matches!(
                            self.focus,
                            Target::ClockFormat
                                | Target::ClockTimeFormat
                                | Target::DateFormat
                                | Target::GreetingName
                                | Target::WeatherLocation
                                | Target::WeatherTemperature
                                | Target::WeatherStyle
                                | Target::WeatherIcons
                                | Target::MarketSymbol(_)
                                | Target::MoveWidget(_)
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
                        | Subpage::TextRendering
                        | Subpage::AppSorting
                        | Subpage::AppDrawerStyle,
                    ) => 2,
                    Some(Subpage::RecentApps) => crate::model::RecentAppsBehavior::ALL.len(),
                    Some(Subpage::WeatherIcons) => crate::model::WeatherIconStyle::ALL.len(),
                    Some(Subpage::InputIcons(_)) => crate::model::INPUT_ICONS.len() + 1,
                    Some(Subpage::Language) => Language::ALL.len(),
                    Some(Subpage::DateFormat) => crate::model::DateFormat::ALL.len(),
                    _ => 3,
                };
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::SettingsMenu
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
            if matches!(
                self.focus,
                Target::InputRename | Target::InputNameReset | Target::InputIcon
            ) {
                let mut rows = vec![Target::InputRename];
                if controls.iter().any(|c| c.target == Target::InputNameReset) {
                    rows.push(Target::InputNameReset);
                }
                rows.push(Target::InputIcon);
                let index = rows
                    .iter()
                    .position(|target| *target == self.focus)
                    .unwrap();
                match direction {
                    Direction::Left => self.close_subpage(),
                    Direction::Up => self.focus(
                        if index == 0 {
                            Target::Choice(2)
                        } else {
                            rows[index - 1].clone()
                        },
                        controls,
                    ),
                    Direction::Down if index + 1 < rows.len() => {
                        self.focus(rows[index + 1].clone(), controls)
                    }
                    Direction::Right if self.focus == Target::InputIcon => return self.activate(),
                    _ => {}
                }
                return vec![];
            }
            if let Target::Sidebar(index) = self.focus {
                let position = SETTINGS_ORDER
                    .iter()
                    .position(|section| *section == index)
                    .unwrap_or(0);
                match direction {
                    Direction::Right => {
                        let entry = if self.section == 1 {
                            Target::UploadWallpaper
                        } else {
                            self.section_entry()
                        };
                        self.focus(entry, controls);
                    }
                    Direction::Up => self.focus(
                        if position == 0 {
                            Target::SettingsMenu
                        } else {
                            Target::Sidebar(SETTINGS_ORDER[position - 1])
                        },
                        controls,
                    ),
                    Direction::Down => self.focus(
                        Target::Sidebar(SETTINGS_ORDER[(position + 1) % SECTION_COUNT]),
                        controls,
                    ),
                    Direction::Left => {}
                }
                return vec![];
            }
            if (matches!(
                self.focus,
                Target::TileEditor
                    | Target::Tile(_)
                    | Target::WebAccessToggle
                    | Target::TvRemoteToggle
                    | Target::WebAccessPair
                    | Target::WebAccessDevice(_)
                    | Target::InputMode(_)
                    | Target::WeatherLocation
                    | Target::WeatherTemperature
                    | Target::WeatherStyle
                    | Target::Widget(_)
                    | Target::MoveWidget(_)
            ) || (self.focus == Target::Setting(3)
                && self.section == INPUTS_SECTION
                && self.subpage.is_none()))
                && matches!(direction, Direction::Left)
            {
                self.close_subpage();
                return vec![];
            }
            if matches!(
                self.focus,
                Target::Setting(0 | 1 | 4 | 7 | 10 | 13 | 15 | WEB_ACCESS_SETTING)
                    | Target::Widget(_)
                    | Target::MoveWidget(_)
                    | Target::InputMode(_)
                    | Target::WeatherLocation
                    | Target::WeatherTemperature
                    | Target::WeatherStyle
                    | Target::Attributions
            ) && matches!(direction, Direction::Right)
            {
                return self.activate();
            }
            if self.subpage == Some(Subpage::AppDrawer)
                && matches!(self.focus, Target::Setting(_))
                && matches!(direction, Direction::Left)
            {
                self.close_subpage();
                return vec![];
            }
            if matches!(self.focus, Target::Setting(_)) && matches!(direction, Direction::Right) {
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
                self.focus(Target::SettingsMenu, controls);
                return vec![];
            }
            if self.focus == Target::Setting(1) && matches!(direction, Direction::Up) {
                self.focus(Target::SettingsMenu, controls);
                return vec![];
            }
            if self.focus == Target::WebAccessToggle && matches!(direction, Direction::Up) {
                self.focus(Target::SettingsMenu, controls);
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
                            if self
                                .preferences
                                .weather_settings(self.active_weather)
                                .location
                                .is_some()
                                && self.focus != Target::WeatherClear
                            {
                                Target::WeatherClear
                            } else {
                                Target::SettingsMenu
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
                        self.focus(Target::Filter(4), controls);
                        return vec![];
                    }
                    Direction::Up => {
                        let target = self
                            .dock_apps()
                            .last()
                            .map_or(Target::SettingsMenu, |app| Target::DockApp(app.id.clone()));
                        self.focus(target, controls);
                        return vec![];
                    }
                    _ => {}
                }
            }
            if let Target::Filter(index) = self.focus {
                let target = match direction {
                    Direction::Left if index > 1 => Target::Filter(index - 1),
                    Direction::Left => Target::InputsMenu,
                    Direction::Right if index < 4 => Target::Filter(index + 1),
                    Direction::Right => Target::Search,
                    Direction::Down => self.first_content(),
                    Direction::Up => self
                        .dock_apps()
                        .first()
                        .map_or(Target::InputsMenu, |app| Target::DockApp(app.id.clone())),
                };
                self.focus(target, controls);
                return vec![];
            }
            if self.page == Page::Apps
                && let Target::App(id) = &self.focus
            {
                let apps = self.apps();
                if let Some(index) = apps.iter().position(|app| &app.id == id) {
                    // A preceding row may be above the visible scroll region.
                    // Its grid index still defines the route; reveal_focus then
                    // scrolls it into view, without letting the fixed nav steal Up.
                    let next = match direction {
                        Direction::Left => Some(Target::App(
                            apps[(index + apps.len() - 1) % apps.len()].id.clone(),
                        )),
                        Direction::Right => {
                            Some(Target::App(apps[(index + 1) % apps.len()].id.clone()))
                        }
                        Direction::Up if index < APPS_COLUMNS => {
                            let target = self
                                .dock_apps()
                                .get(index.min(self.dock_apps().len().saturating_sub(1)))
                                .map_or(
                                    if index < 3 {
                                        self.first_input_target()
                                    } else {
                                        Target::SettingsMenu
                                    },
                                    |app| Target::App(app.id.clone()),
                                );
                            // Reaching the favorites row returns to Home immediately,
                            // preserving the selected column instead of requiring another Up.
                            self.open(0);
                            Some(target)
                        }
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
                    Direction::Right if index + 1 < self.background_count() => {
                        self.focus(Target::Background(index + 1), controls);
                        return vec![];
                    }
                    Direction::Right => return vec![],
                    Direction::Left if index % 3 != 0 => {
                        self.focus(Target::Background(index - 1), controls);
                        return vec![];
                    }
                    Direction::Up => {
                        self.focus(
                            if index == 2 {
                                Target::WallpaperCycle
                            } else if index == 1 {
                                Target::WallpaperGradient
                            } else if index < 3 {
                                Target::UploadWallpaper
                            } else {
                                Target::Background(index - 3)
                            },
                            controls,
                        );
                        return vec![];
                    }
                    Direction::Down => {
                        if (index / 3 + 1) * 3 < self.background_count() {
                            self.focus(
                                Target::Background((index + 3).min(self.background_count() - 1)),
                                controls,
                            );
                        }
                        return vec![];
                    }
                    _ => {}
                }
            }
            if self.focus == Target::UploadWallpaper {
                match direction {
                    Direction::Down => self.focus(Target::Background(0), controls),
                    Direction::Up => self.focus(Target::SettingsMenu, controls),
                    Direction::Left => self.focus(Target::Sidebar(1), controls),
                    Direction::Right => self.focus(Target::WallpaperGradient, controls),
                }
                return vec![];
            }
            if self.focus == Target::WallpaperGradient {
                match direction {
                    Direction::Right => self.focus(Target::WallpaperCycle, controls),
                    Direction::Left => self.focus(Target::UploadWallpaper, controls),
                    Direction::Up => self.focus(Target::SettingsMenu, controls),
                    Direction::Down => self.focus(
                        Target::Background(1.min(self.background_count().saturating_sub(1))),
                        controls,
                    ),
                }
                return vec![];
            }
            if self.focus == Target::WallpaperCycle {
                match direction {
                    Direction::Right => {}
                    Direction::Left => self.focus(Target::WallpaperGradient, controls),
                    Direction::Up => self.focus(Target::SettingsMenu, controls),
                    Direction::Down => self.focus(
                        Target::Background(2.min(self.background_count().saturating_sub(1))),
                        controls,
                    ),
                }
                return vec![];
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
        let next = controls
            .iter()
            .filter(|candidate| candidate.target != self.focus)
            .filter(|candidate| {
                self.page != Page::Home
                    || !matches!(candidate.target, Target::App(_))
                    || (candidate.rect.x >= HOME_STRIP_LEFT - 0.1
                        && candidate.rect.x + candidate.rect.w <= HOME_STRIP_RIGHT + 0.1)
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
        if let Target::Background(index) = self.focus {
            if let Some(id) = self.background_id(index).map(str::to_owned) {
                self.overlay = Some(Overlay::BackgroundOptions {
                    id,
                    restore: self.focus.clone(),
                });
                self.focus = Target::UseBackground;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
                self.dirty = true;
            }
        } else if let Target::Tile(id) = &self.focus {
            if self.preferences.custom_tile(id).is_none() {
                return;
            }
            self.overlay = Some(Overlay::Options {
                id: id.clone(),
                restore: self.focus.clone(),
            });
            self.focus = Target::TileTest(id.clone());
            self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
            self.dirty = true;
        } else if let Target::App(id) | Target::DockApp(id) = &self.focus {
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
            if let Target::App(id) = &self.focus
                && self
                    .preferences
                    .favorites(&self.inventory.apps)
                    .contains(id)
            {
                self.moving = Some(id.clone());
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
            if matches!(
                self.keyboard_field,
                TextField::InputName | TextField::GreetingName | TextField::Market(_)
            ) {
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
            } else if matches!(
                self.keyboard_field,
                TextField::InputName | TextField::GreetingName | TextField::Market(_)
            ) {
                self.input_name.pop();
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
        if self.keyboard_field == TextField::GreetingName {
            let name = clean_input_name(&std::mem::take(&mut self.input_name));
            if !name.is_empty() && self.preferences.greeting_name != name {
                self.preferences.greeting_name = name;
                return vec![Effect::Save];
            }
            return vec![];
        }
        if let TextField::Market(kind) = self.keyboard_field {
            let draft = std::mem::take(&mut self.input_name);
            if draft.trim().is_empty() {
                return vec![];
            }
            if let Some(symbol) = crate::markets::normalize_symbol(kind, &draft) {
                let config = crate::markets::settings_mut(&mut self.preferences, kind);
                if config.symbol != symbol {
                    config.symbol = symbol;
                    return vec![Effect::Save];
                }
            } else {
                self.notify(self.preferences.language.t("invalidMarketSymbol"));
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

    /// Favorites in order and hidden apps from a paired phone or computer. Ids the launcher does
    /// not know are dropped; saved entries for apps that are not installed right now are kept.
    pub fn apply_web_arrangement(
        &mut self,
        favorites: Vec<String>,
        hidden: Vec<String>,
    ) -> Vec<Effect> {
        let known = |id: &String| self.inventory.apps.iter().any(|app| &app.id == id);
        let mut next_favorites: Vec<String> = Vec::new();
        for id in favorites.into_iter().filter(known) {
            if !next_favorites.contains(&id) {
                next_favorites.push(id);
            }
        }
        let current = self.preferences.favorites(&self.inventory.apps);
        next_favorites.extend(current.iter().filter(|id| !known(id)).cloned());
        let mut next_hidden: Vec<String> = Vec::new();
        for id in hidden.into_iter().filter(known) {
            if !next_hidden.contains(&id) && !next_favorites.contains(&id) {
                next_hidden.push(id);
            }
        }
        next_hidden.extend(
            self.preferences
                .hidden
                .iter()
                .filter(|id| !known(id))
                .cloned(),
        );
        if current == next_favorites && self.preferences.hidden == next_hidden {
            return vec![];
        }
        self.preferences.favorites = Some(next_favorites);
        self.preferences.hidden = next_hidden;
        if self.filter == 5 && self.hidden_count() == 0 {
            self.filter = 0;
        }
        self.reconcile_focus();
        self.dirty = true;
        vec![Effect::Save]
    }

    /// One input's name, visibility and icon from a paired phone or computer, saved like the same
    /// choices in Settings › Widgets › Inputs.
    pub fn apply_web_input(&mut self, change: crate::web_settings::InputChange) -> Vec<Effect> {
        if !self
            .inventory
            .inputs
            .iter()
            .any(|input| input.id == change.id)
        {
            return vec![];
        }
        let mut changed = self.preferences.set_input_name(&change.id, &change.name);
        changed |= self.preferences.set_input_icon(&change.id, &change.icon);
        if self
            .preferences
            .input_modes
            .get(&change.id)
            .copied()
            .unwrap_or_default()
            != change.mode
        {
            self.preferences.input_modes.insert(change.id, change.mode);
            changed = true;
        }
        if !changed {
            return vec![];
        }
        self.dirty = true;
        vec![Effect::Save]
    }

    /// A weather place chosen on a paired phone or computer, or `None` to remove it, with the
    /// same effects as choosing or removing it in Settings › Widgets › Weather.
    pub fn apply_web_weather_location(
        &mut self,
        place: Option<crate::weather::Location>,
    ) -> Vec<Effect> {
        if self.preferences.weather_location == place {
            return vec![];
        }
        self.dirty = true;
        match place {
            Some(place) => {
                self.preferences.weather_location = Some(place);
                self.weather.invalidate_forecast();
                self.weather.cancel_search();
                vec![Effect::Save]
            }
            None => {
                self.preferences.weather_location = None;
                self.weather.remove_location();
                vec![Effect::Save, Effect::Weather(weather::Command::ClearCache)]
            }
        }
    }

    /// A Home widget layout change from a paired phone or computer, with the TV's placement rules:
    /// nothing overlaps or leaves the 6×4 grid, and removing keeps a widget's configuration.
    pub fn apply_web_widget(&mut self, change: crate::web_settings::WidgetChange) -> Vec<Effect> {
        use crate::model::WidgetPosition;
        use crate::web_settings::WidgetChange;
        let changed = match change {
            WidgetChange::Move {
                widget,
                column,
                row,
            } => {
                self.preferences.has_widget(widget)
                    && self
                        .preferences
                        .place_widget(widget, WidgetPosition { column, row })
            }
            WidgetChange::Add {
                widget,
                column,
                row,
            } => {
                let position = WidgetPosition { column, row };
                if self.preferences.has_widget(widget)
                    || !self.preferences.can_place_widget(widget, position)
                {
                    false
                } else {
                    self.preferences.place_widget(widget, position);
                    self.preferences.set_widget_present(widget, true);
                    true
                }
            }
            WidgetChange::Remove { widget } => {
                let present = self.preferences.has_widget(widget);
                self.preferences.set_widget_present(widget, false);
                present
            }
            WidgetChange::Reset => self.preferences.reset_widgets(),
        };
        if !changed {
            return vec![];
        }
        // Weather starts or stops fetching with its widget, as on the TV.
        self.weather.invalidate_forecast();
        self.dirty = true;
        vec![Effect::Save]
    }

    /// Shows `id` as the background, fading from the previous one like a choice in Settings.
    fn use_background(&mut self, id: String) {
        if !self.preferences.reduced_motion && self.preferences.background != id {
            self.previous_background = Some((self.preferences.background.clone(), Instant::now()));
            self.background_fade = BACKGROUND_FADE;
        }
        self.preferences.background = id;
        self.preferences.wallpaper_changed = unix_now();
    }

    /// A change made on a paired phone or computer, with the same side effects as on the TV.
    pub fn apply_web_settings(&mut self, settings: crate::web_settings::Settings) -> Vec<Effect> {
        if crate::web_settings::Settings::of(&self.preferences) == settings {
            return vec![];
        }
        if settings.background != self.preferences.background
            && self.deleting_wallpaper.as_deref() != Some(settings.background.as_str())
            && self
                .background_ids()
                .any(|known| known == settings.background)
        {
            self.use_background(settings.background.clone());
        }
        self.preferences.tv_remote = settings.tv_remote;
        if settings.text_rendering != self.preferences.text_rendering {
            self.preferences.text_rendering = settings.text_rendering;
            self.notify(self.preferences.language.t("textRenderingRestart"));
        }
        if self.preferences.language != settings.language {
            self.weather.cancel_search();
        }
        if self.preferences.wallpaper_cycle != settings.wallpaper_cycle {
            self.preferences.wallpaper_changed = unix_now();
        }
        // Style and size change the weather widget's footprint: the TV's own paths move it to a
        // free spot, or keep the old value when none is left.
        if settings.weather_style != self.preferences.weather_style {
            self.preferences
                .set_weather_widget_style(HomeWidget::Weather, settings.weather_style);
        }
        if settings.weather_small != self.preferences.weather_small {
            self.preferences
                .toggle_weather_widget_size(HomeWidget::Weather);
        }
        let preferences = &mut self.preferences;
        preferences.hour_24 = settings.hour_24;
        preferences.show_seconds = settings.show_seconds;
        preferences.language = settings.language;
        preferences.app_sorting = settings.app_sorting;
        preferences.app_drawer_style = settings.app_drawer_style;
        preferences.show_drawer_app_names = settings.show_drawer_app_names;
        preferences.recent_apps_behavior = settings.recent_apps_behavior;
        preferences.reduced_motion = settings.reduced_motion;
        preferences.ui_sounds = settings.ui_sounds;
        preferences.show_date = settings.show_date;
        preferences.date_format = settings.date_format;
        preferences.greeting_name = settings.greeting_name;
        preferences.temperature_unit = settings.temperature_unit;
        preferences.weather_show_location = settings.weather_show_location;
        preferences.weather_show_icon = settings.weather_show_icon;
        preferences.weather_icons = settings.weather_icons;
        preferences.wallpaper_cycle = settings.wallpaper_cycle;
        preferences.wallpaper_gradient = settings.wallpaper_gradient;
        self.dirty = true;
        vec![Effect::Save]
    }

    /// Applies the tile list edited on a phone or computer. New tiles join
    /// the end of the Home favorites and removed ones leave favorites and hidden apps.
    pub fn replace_tiles(&mut self, tiles: Vec<CustomTile>) -> Vec<Effect> {
        let removed: Vec<_> = self
            .preferences
            .custom_tiles
            .iter()
            .filter(|old| !tiles.iter().any(|tile| tile.id == old.id))
            .map(|old| old.id.clone())
            .collect();
        for id in &removed {
            self.preferences.remove_tile(id);
        }
        let added: Vec<_> = tiles
            .iter()
            .filter(|tile| self.preferences.custom_tile(&tile.id).is_none())
            .map(|tile| tile.id.clone())
            .collect();
        self.preferences.custom_tiles = tiles;
        self.preferences.sanitize();
        self.sync_custom_tiles();
        if let Target::Tile(id) | Target::TileRemove(id) | Target::TileTest(id) = &self.focus
            && self.preferences.custom_tile(id).is_none()
        {
            if matches!(
                &self.overlay,
                Some(Overlay::Options {
                    restore: Target::Tile(_),
                    ..
                })
            ) {
                self.overlay = None;
                self.panel_started = None;
            }
            self.focus = Target::TileEditor;
        }
        if !added.is_empty() {
            let mut favorites = self.preferences.favorites(&self.inventory.apps);
            for id in added {
                if self.preferences.custom_tile(&id).is_some() && !favorites.contains(&id) {
                    favorites.push(id);
                }
            }
            self.preferences.favorites = Some(favorites);
        }
        vec![Effect::Save]
    }

    fn send_tile(&mut self, tile: CustomTile) -> Vec<Effect> {
        self.notify(
            self.preferences
                .language
                .format("tileSending", &[("tile", &tile.title)]),
        );
        vec![Effect::FireWebhook(tile)]
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
        } else if matches!(
            self.keyboard_field,
            TextField::InputName | TextField::GreetingName | TextField::Market(_)
        ) {
            self.input_name = query;
        } else {
            self.query = query;
            self.scroll = 0.;
        }
        self.dirty = true;
    }

    pub fn editing_text(&self) -> &str {
        match self.keyboard_field {
            TextField::Location => &self.weather.query,
            TextField::InputName => &self.input_name,
            TextField::GreetingName => &self.input_name,
            TextField::Market(_) => &self.input_name,
            TextField::Apps => &self.query,
        }
    }
    pub fn open_keyboard(&mut self, field: TextField) {
        self.keyboard_field = field;
        if matches!(
            field,
            TextField::InputName | TextField::GreetingName | TextField::Market(_)
        ) {
            // The TV keyboard starts empty, so a new name replaces the old one.
            self.input_name.clear();
        }
        if cfg!(all(
            feature = "webos",
            target_os = "linux",
            target_arch = "arm"
        )) {
            self.overlay = Some(Overlay::SystemKeyboard);
            self.focus = match field {
                TextField::Location => Target::WeatherQuery,
                TextField::InputName => Target::InputRename,
                TextField::GreetingName => Target::GreetingName,
                TextField::Market(kind) => Target::MarketSymbol(kind),
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
            TextField::InputName => Target::InputRename,
            TextField::GreetingName => Target::GreetingName,
            TextField::Market(kind) => Target::MarketSymbol(kind),
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
        if self.moving.take().is_some() {
            // Order was saved at every move. A fresh OK release finishes the
            // mode and keeps the selected favorite, rather than launching it.
            self.dirty = true;
            return vec![];
        }
        self.dirty = true;
        if !self.managing_apps()
            && let Target::App(id) | Target::DockApp(id) = &self.focus
            && let Some(tile) = self.preferences.custom_tile(id).cloned()
        {
            return self.send_tile(tile);
        }
        match self.focus.clone() {
            Target::Nav(index) => self.open(if index == 2 { 0 } else { index }),
            Target::InputsMenu => {
                self.overlay = Some(Overlay::Inputs);
                self.focus = self
                    .inventory
                    .inputs
                    .iter()
                    .find(|input| self.preferences.input_visible(input))
                    .map_or(Target::InputsMenu, |input| Target::Input(input.id.clone()));
            }
            Target::SettingsMenu => {
                self.overlay = Some(Overlay::Settings);
                self.focus = Target::LumaSettings;
            }
            Target::LumaSettings => {
                self.open(2);
                self.section = 0;
                self.focus = Target::Sidebar(0);
            }
            Target::Retry => return vec![Effect::Reload],
            Target::DismissNotice => self.notice = None,
            Target::CloseDialog => return self.back(),
            Target::UploadWallpaper => {
                self.wallpaper_upload = None;
                self.overlay = Some(Overlay::WallpaperUpload);
                self.focus = Target::CloseDialog;
                self.pressed = None;
            }
            Target::TileEditor => {
                self.tile_editor = None;
                self.overlay = Some(Overlay::TileEditor);
                self.focus = Target::CloseDialog;
                self.pressed = None;
            }
            Target::TvRemoteToggle => {
                self.preferences.tv_remote = !self.preferences.tv_remote;
                return vec![Effect::Save];
            }
            Target::WebAccessToggle => {
                self.preferences.web_access = !self.preferences.web_access;
                return vec![Effect::Save];
            }
            Target::WebAccessPair => {
                // Pairing is the way in, so asking to pair turns web access on.
                let effects = if self.preferences.web_access {
                    vec![]
                } else {
                    self.preferences.web_access = true;
                    vec![Effect::Save]
                };
                self.web_access_pairing = None;
                self.overlay = Some(Overlay::WebAccessPair);
                self.focus = Target::CloseDialog;
                self.pressed = None;
                return effects;
            }
            Target::WebAccessDevice(id) => {
                if let Some(index) = self.web_devices.iter().position(|device| device.id == id) {
                    let device = self.web_devices.remove(index);
                    self.forget_web_devices.push(device.id);
                    self.notify(
                        self.preferences
                            .language
                            .format("webAccessRemoved", &[("device", &device.name)]),
                    );
                    self.focus = self
                        .web_devices
                        .get(index.min(self.web_devices.len().saturating_sub(1)))
                        .map_or(Target::WebAccessPair, |device| {
                            Target::WebAccessDevice(device.id.clone())
                        });
                }
            }
            Target::TvSettings => {
                if matches!(self.overlay, Some(Overlay::Settings)) {
                    self.back();
                }
                return vec![Effect::Launch("com.palm.app.settings".into())];
            }
            Target::App(_) if self.managing_apps() && !self.launching => self.options(),
            Target::App(id) | Target::DockApp(id) | Target::Input(id) if !self.launching => {
                if matches!(self.overlay, Some(Overlay::Inputs)) {
                    self.back();
                }
                return vec![Effect::Launch(id)];
            }
            Target::OpenApp if !self.launching => {
                if let Some(Overlay::Options { id, .. }) = &self.overlay
                    && self.inventory.apps.iter().any(|app| &app.id == id)
                {
                    let id = id.clone();
                    self.back();
                    if let Some(tile) = self.preferences.custom_tile(&id).cloned() {
                        return self.send_tile(tile);
                    }
                    return vec![Effect::Launch(id)];
                }
            }
            Target::Filter(index) => {
                self.filter = if self.filter == index { 0 } else { index };
                self.scroll = 0.;
            }
            Target::Search => {
                self.open_keyboard(TextField::Apps);
            }
            Target::MarketSymbol(kind) => self.open_keyboard(TextField::Market(kind)),
            Target::MarketCurrency => {
                let kind = if let Some(Subpage::Market(kind)) = self.subpage {
                    kind
                } else {
                    HomeWidget::Crypto
                };
                let currencies = crate::markets::CURRENCIES;
                let current = currencies
                    .iter()
                    .position(|currency| {
                        *currency == crate::markets::settings(&self.preferences, kind).currency
                    })
                    .unwrap_or(0);
                crate::markets::settings_mut(&mut self.preferences, kind).currency =
                    currencies[(current + 1) % currencies.len()].into();
                return vec![Effect::Save];
            }
            Target::WeatherConfigure => {
                self.active_weather = HomeWidget::Weather;
                self.open(2);
                self.section = WIDGETS_SECTION;
                self.open_location();
                self.open_keyboard(TextField::Location);
            }
            Target::WeatherLocation => self.open_location(),
            Target::WeatherTemperature => {
                self.subpage = Some(Subpage::Temperature);
                self.focus = Target::Choice(usize::from(
                    self.preferences.weather_settings(self.active_weather).unit == Unit::Fahrenheit,
                ));
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
            }
            Target::WeatherSize => {
                if let Some(Overlay::WidgetOptions { cell }) = self.overlay
                    && let Some(widget) =
                        WidgetPosition::from_cell(cell).and_then(|p| self.preferences.widget_at(p))
                {
                    self.active_weather = widget;
                }
                self.preferences
                    .toggle_weather_widget_size(self.active_weather);
                if matches!(self.overlay, Some(Overlay::WidgetOptions { .. })) {
                    // A non-anchor cell may disappear on shrink, and expanding
                    // at an edge may relocate Weather. Keep its popup attached.
                    self.overlay = Some(Overlay::WidgetOptions {
                        cell: self.preferences.widget_position(self.active_weather).cell(),
                    });
                }
                return vec![Effect::Save];
            }
            Target::WeatherStyle => {
                self.subpage = Some(Subpage::WeatherStyle);
                self.focus = Target::Choice(
                    self.preferences
                        .weather_settings(self.active_weather)
                        .style
                        .index(),
                );
                self.scroll = 0.;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
            }
            Target::WeatherShowLocation => {
                self.preferences
                    .edit_weather(self.active_weather, |config| {
                        config.show_location = !config.show_location;
                    });
                return vec![Effect::Save];
            }
            Target::WeatherShowIcon => {
                self.preferences
                    .edit_weather(self.active_weather, |config| {
                        config.show_icon = !config.show_icon;
                    });
                return vec![Effect::Save];
            }
            Target::WeatherIcons => {
                self.subpage = Some(Subpage::WeatherIcons);
                self.focus = Target::Choice(
                    self.preferences
                        .weather_settings(self.active_weather)
                        .icons
                        .index(),
                );
                self.scroll = 0.;
                self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
            }
            Target::Attributions => {
                self.subpage = Some(Subpage::Attributions);
                self.focus = Target::AttributionText;
                self.scroll = 0.;
                self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
            }
            Target::AttributionText => {}
            Target::WeatherQuery => self.open_keyboard(TextField::Location),
            Target::DateToggle => {
                self.preferences.show_date = !self.preferences.show_date;
                return vec![Effect::Save];
            }
            Target::DateFormat => {
                self.subpage = Some(Subpage::DateFormat);
                self.focus = Target::Choice(self.preferences.date_format.index());
                self.scroll = 0.;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
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
            Target::InputIcon => {
                if let Some(Subpage::Input(id)) = self.subpage.clone() {
                    let selected = self
                        .preferences
                        .input_icon(&id)
                        .and_then(|saved| {
                            crate::model::INPUT_ICONS
                                .iter()
                                .position(|(icon, _)| *icon == saved)
                        })
                        .map_or(0, |index| index + 1);
                    self.subpage = Some(Subpage::InputIcons(id));
                    self.focus = Target::Choice(selected);
                    self.scroll = 0.;
                    if !self.preferences.reduced_motion {
                        self.panel_started = Some(Instant::now());
                    }
                }
            }
            Target::InputRename => self.open_keyboard(TextField::InputName),
            Target::GreetingName => self.open_keyboard(TextField::GreetingName),
            Target::GreetingNameReset => {
                self.focus = Target::GreetingName;
                if !self.preferences.greeting_name.is_empty() {
                    self.preferences.greeting_name.clear();
                    return vec![Effect::Save];
                }
            }
            Target::WidgetCell(cell) if cell < HOME_GRID_CELLS => {
                if let Some(widget) = WidgetPosition::from_cell(cell)
                    .and_then(|position| self.preferences.widget_at(position))
                {
                    self.overlay = Some(Overlay::WidgetOptions { cell });
                    self.focus = Target::ConfigureWidget(widget);
                } else {
                    self.open_widget_catalog(cell);
                }
                self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
            }
            Target::WidgetAdd => {
                self.open_widget_catalog(0);
            }
            Target::WidgetReset => {
                if self.preferences.reset_widgets() {
                    return vec![Effect::Save];
                }
            }
            Target::AddWidget(widget) => {
                if self.preferences.has_widget(widget) {
                    return vec![];
                }
                if let Some(Overlay::WidgetCatalog { cell, .. }) = self.overlay.take() {
                    self.preferences.prepare_widget(widget);
                    self.subpage = Some(Subpage::WidgetPosition(widget));
                    let cell = WidgetPosition::from_cell(cell)
                        .filter(|position| self.preferences.can_place_widget(widget, *position))
                        .or_else(|| {
                            (0..HOME_GRID_CELLS)
                                .filter_map(WidgetPosition::from_cell)
                                .find(|position| {
                                    self.preferences.can_place_widget(widget, *position)
                                })
                        })
                        .map_or(cell, WidgetPosition::cell);
                    self.focus = Target::Choice(cell);
                    self.widget_position_cell = cell;
                    self.panel_started = None;
                    self.scroll = 0.;
                }
            }
            Target::RemoveWidget(widget) => {
                if let Some(Overlay::WidgetOptions { cell }) = self.overlay.take() {
                    self.preferences.set_widget_present(widget, false);
                    if widget.kind() == HomeWidget::Weather {
                        self.widget_weather_mut(widget).invalidate_forecast();
                    }
                    self.focus = Target::WidgetCell(cell);
                    self.panel_started = None;
                    return vec![Effect::Save];
                }
            }
            Target::ConfigureWidget(widget) => {
                if self.page != Page::Settings {
                    self.open(2);
                }
                self.section = WIDGETS_SECTION;
                self.overlay = None;
                if widget.kind() == HomeWidget::Weather {
                    self.active_weather = widget;
                }
                self.subpage = Some(match widget.kind() {
                    HomeWidget::Greeting => Subpage::Greeting,
                    HomeWidget::Weather => Subpage::Weather,
                    HomeWidget::Crypto | HomeWidget::Stocks => Subpage::Market(widget),
                    _ => unreachable!(),
                });
                self.focus = match widget.kind() {
                    HomeWidget::Greeting => Target::DateToggle,
                    HomeWidget::Weather => Target::WeatherLocation,
                    HomeWidget::Crypto | HomeWidget::Stocks => Target::MarketSymbol(widget),
                    _ => unreachable!(),
                };
                self.scroll = 0.;
                self.panel_started = None;
            }
            Target::Widget(index) => {
                self.active_weather = HomeWidget::Weather;
                let (subpage, focus) = match index {
                    0 => (Subpage::Greeting, Target::DateToggle),
                    1 => (Subpage::Weather, Target::WeatherLocation),
                    2 => (Subpage::Shortcuts, Target::TileEditor),
                    _ => return vec![],
                };
                self.section = WIDGETS_SECTION;
                self.subpage = Some(subpage);
                self.focus = focus;
                self.scroll = 0.;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
            }
            Target::MoveWidget(widget) => {
                self.overlay = None;
                let position = self.preferences.widget_position(widget);
                self.subpage = Some(Subpage::WidgetPosition(widget));
                self.focus = Target::Choice(position.cell());
                self.widget_position_cell = position.cell();
                self.scroll = 0.;
                self.panel_started = None;
            }
            Target::InstallUpdate => {
                if !self.update_busy()
                    && let Some(version) = &self.available_update
                {
                    let version = version.clone();
                    self.update = crate::update::InstallStatus::Downloading(0);
                    return vec![Effect::UpdateDownload(version)];
                }
            }
            Target::CheckUpdates => {
                if !self.update_busy() {
                    self.update = crate::update::InstallStatus::Idle;
                    self.update_status = crate::update::Status::Checking;
                    self.update_check_requested = true;
                    self.dirty = true;
                }
            }
            Target::Tile(_) => self.options(),
            Target::TileTest(id) => {
                if matches!(&self.overlay, Some(Overlay::Options { id: current, restore: Target::Tile(_), .. }) if current == &id)
                    && let Some(tile) = self.preferences.custom_tile(&id).cloned()
                {
                    return self.send_tile(tile);
                }
            }
            Target::TileRemove(id) => {
                if matches!(&self.overlay, Some(Overlay::Options { id: current, restore: Target::Tile(_), .. }) if current == &id)
                    && let Some(tile) = self.preferences.remove_tile(&id)
                {
                    self.overlay = None;
                    self.notify(
                        self.preferences
                            .language
                            .format("tileRemoved", &[("tile", &tile.title)]),
                    );
                    self.subpage = Some(Subpage::Shortcuts);
                    self.panel_started = None;
                    self.scroll = 0.;
                    self.sync_custom_tiles();
                    self.focus = Target::TileEditor;
                    return vec![Effect::Save];
                }
            }
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
                self.preferences
                    .edit_weather(self.active_weather, |config| config.location = None);
                self.widget_weather_mut(self.active_weather)
                    .remove_location();
                self.weather.cancel_search();
                self.focus = Target::WeatherSearch;
                return vec![
                    Effect::Save,
                    Effect::Weather(match self.active_weather {
                        HomeWidget::ExtraWeather(index) => {
                            weather::Command::ClearWidgetCache(index)
                        }
                        _ => weather::Command::ClearCache,
                    }),
                ];
            }
            Target::WeatherResult(index) => {
                if let Some(place) = self.weather.results.get(index).cloned() {
                    self.preferences
                        .edit_weather(self.active_weather, |config| config.location = Some(place));
                    self.widget_weather_mut(self.active_weather)
                        .invalidate_forecast();
                    self.weather.cancel_search();
                    self.focus = Target::WeatherSearch;
                    return vec![Effect::Save];
                }
            }
            Target::Sidebar(index) => {
                self.section = index;
                self.scroll = 0.;
                self.subpage = None;
                self.focus = self.section_entry();
            }
            Target::Setting(index) => {
                match index {
                    WEB_ACCESS_SETTING => {
                        self.subpage = Some(Subpage::WebAccess);
                        self.focus = Target::WebAccessToggle;
                        self.scroll = 0.;
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    14 => {
                        self.preferences.show_drawer_app_names =
                            !self.preferences.show_drawer_app_names
                    }
                    13 => {
                        self.subpage = Some(Subpage::AppDrawer);
                        self.focus = Target::Setting(15);
                        self.scroll = 0.;
                        self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
                        return vec![];
                    }
                    15 => {
                        self.subpage = Some(Subpage::AppDrawerStyle);
                        self.focus = Target::Choice(usize::from(
                            self.preferences.app_drawer_style
                                == crate::model::AppDrawerStyle::Minimal,
                        ));
                        self.scroll = 0.;
                        self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
                        return vec![];
                    }
                    10 => {
                        self.subpage = Some(Subpage::AppSorting);
                        self.focus = Target::Choice(usize::from(
                            self.preferences.app_sorting == crate::model::AppSorting::Usage,
                        ));
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
                    11 => self.preferences.ui_sounds = !self.preferences.ui_sounds,
                    12 => {
                        self.subpage = Some(Subpage::RecentApps);
                        self.focus = Target::Choice(
                            crate::model::RecentAppsBehavior::ALL
                                .iter()
                                .position(|mode| *mode == self.preferences.recent_apps_behavior)
                                .unwrap_or(1),
                        );
                        self.scroll = 0.;
                        self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
                        return vec![];
                    }
                    4 => {
                        self.subpage = Some(Subpage::Language);
                        self.focus = Target::Choice(self.preferences.language.index());
                        if !self.preferences.reduced_motion {
                            self.panel_started = Some(Instant::now());
                        }
                        return vec![];
                    }
                    5 => return vec![Effect::Quit],
                    9 => {
                        self.reset_icon_cache = true;
                        return vec![];
                    }
                    7 => {
                        self.subpage = Some(Subpage::TextRendering);
                        self.focus = Target::Choice(self.preferences.text_rendering.index());
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
            Target::Background(_) => {
                self.options();
            }
            Target::WallpaperCycle => {
                self.preferences.wallpaper_cycle = !self.preferences.wallpaper_cycle;
                self.preferences.wallpaper_changed = unix_now();
                return vec![Effect::Save];
            }
            Target::WallpaperGradient => {
                self.preferences.wallpaper_gradient = !self.preferences.wallpaper_gradient;
                return vec![Effect::Save];
            }
            Target::UseBackground => {
                let Some(Overlay::BackgroundOptions { id, .. }) = self.overlay.clone() else {
                    return vec![];
                };
                if self.deleting_wallpaper.as_deref() == Some(&id) {
                    return vec![];
                }
                if !self.background_ids().any(|known| known == id) {
                    return self.back();
                }
                self.use_background(id);
                self.back();
                return vec![Effect::Save];
            }
            Target::DeleteBackground => {
                if let Some(Overlay::BackgroundOptions { id, restore }) = self.overlay.clone()
                    && self.can_delete_background(&id)
                    && self.deleting_wallpaper.is_none()
                {
                    self.overlay = Some(Overlay::BackgroundDelete { id, restore });
                    self.focus = Target::CancelDelete;
                    self.pointer = false;
                    self.pressed = None;
                    self.panel_started = None;
                }
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
                    Some(Subpage::WidgetPosition(widget)) => {
                        let Some(position) = WidgetPosition::from_cell(index) else {
                            return vec![];
                        };
                        if !self.preferences.can_place_widget(widget, position) {
                            return vec![];
                        }
                        let added = !self.preferences.has_widget(widget);
                        let changed = self.preferences.place_widget(widget, position);
                        self.preferences.set_widget_present(widget, true);
                        if added && widget.kind() == HomeWidget::Weather {
                            self.widget_weather_mut(widget).invalidate_forecast();
                        }
                        self.close_subpage();
                        return if changed || added {
                            vec![Effect::Save]
                        } else {
                            vec![]
                        };
                    }
                    Some(Subpage::WeatherStyle) => {
                        self.preferences.set_weather_widget_style(
                            self.active_weather,
                            crate::model::WeatherStyle::ALL
                                [index.min(crate::model::WeatherStyle::ALL.len() - 1)],
                        );
                    }
                    Some(Subpage::WeatherIcons) => {
                        let Some(style) = crate::model::WeatherIconStyle::ALL.get(index) else {
                            return vec![];
                        };
                        self.preferences
                            .edit_weather(self.active_weather, |config| config.icons = *style);
                    }
                    Some(Subpage::AppDrawerStyle) => {
                        self.preferences.app_drawer_style =
                            crate::model::AppDrawerStyle::ALL[index.min(1)];
                    }
                    Some(Subpage::AppSorting) => {
                        self.preferences.app_sorting = crate::model::AppSorting::ALL[index.min(1)];
                    }
                    Some(Subpage::RecentApps) => {
                        self.preferences.recent_apps_behavior =
                            crate::model::RecentAppsBehavior::ALL[index.min(2)];
                    }
                    Some(Subpage::ClockFormat) => self.preferences.hour_24 = index == 1,
                    Some(Subpage::TimeFormat) => self.preferences.show_seconds = index == 1,
                    Some(Subpage::DateFormat) => {
                        self.preferences.date_format = crate::model::DateFormat::ALL[index.min(3)];
                    }
                    Some(Subpage::InputIcons(id)) => {
                        let icon = if index == 0 {
                            ""
                        } else if let Some((icon, _)) = crate::model::INPUT_ICONS.get(index - 1) {
                            *icon
                        } else {
                            return vec![];
                        };
                        let changed = self.preferences.set_input_icon(&id, icon);
                        self.close_subpage();
                        return if changed { vec![Effect::Save] } else { vec![] };
                    }
                    Some(Subpage::Input(id)) => {
                        self.preferences.input_modes.insert(
                            id,
                            [InputMode::On, InputMode::Off, InputMode::Auto][index.min(2)],
                        );
                    }
                    Some(Subpage::Temperature) => {
                        self.preferences
                            .edit_weather(self.active_weather, |config| {
                                config.unit = if index == 1 {
                                    Unit::Fahrenheit
                                } else {
                                    Unit::Celsius
                                }
                            });
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
                        | Subpage::Market(_)
                        | Subpage::Greeting
                        | Subpage::Clock
                        | Subpage::AppDrawer
                        | Subpage::Weather
                        | Subpage::Shortcuts
                        | Subpage::WebAccess
                        | Subpage::Attributions,
                    ) => {
                        return vec![];
                    }
                    None => return vec![],
                }
                self.close_subpage();
                return vec![Effect::Save];
            }
            Target::MoveApp => {
                if let Some(Overlay::Options { id, .. }) = &self.overlay
                    && self.managing_apps()
                    && self.favorite_apps().iter().any(|app| &app.id == id)
                {
                    let id = id.clone();
                    self.open(0);
                    self.focus = Target::App(id.clone());
                    if let Some(index) = self.dock_apps().iter().position(|app| app.id == id) {
                        self.scroll_favorites(index as f32 * HOME_TILE_PITCH);
                    }
                    self.moving = Some(id);
                    self.notice = None;
                    self.pointer = false;
                    self.pressed = None;
                }
                return vec![];
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
                self.reconcile_focus();
                return if index < 2 {
                    vec![Effect::Save]
                } else {
                    vec![]
                };
            }
            Target::CancelDelete => return self.back(),
            Target::ConfirmDelete => {
                if let Some(Overlay::BackgroundDelete { id, .. }) = self.overlay.clone() {
                    if self.can_delete_background(&id) && self.deleting_wallpaper.is_none() {
                        if crate::wallpapers::valid_id(&id) {
                            self.deleting_wallpaper = Some(id.clone());
                            self.focus = Target::CancelDelete;
                            return vec![Effect::DeleteWallpaper(id)];
                        }
                        // Packaged assets remain intact; persist removal from the gallery.
                        self.finish_background_delete(&id);
                        return vec![Effect::Save];
                    }
                    return vec![];
                }
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
                    TextField::InputName => self.input_name.clear(),
                    TextField::GreetingName => self.input_name.clear(),
                    TextField::Market(_) => self.input_name.clear(),
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
        if self.moving.take().is_some() {
            return vec![];
        }
        if let Some(overlay) = self.overlay.take() {
            self.panel_started = None;
            if let Overlay::WidgetCatalog { restore, .. } = overlay {
                self.focus = restore;
                self.pressed = None;
                return vec![];
            }
            if let Overlay::WidgetOptions { cell } = overlay {
                self.focus = Target::WidgetCell(cell);
                self.pressed = None;
                return vec![];
            }
            if matches!(overlay, Overlay::Inputs | Overlay::Settings) {
                self.focus = if matches!(overlay, Overlay::Inputs) {
                    Target::InputsMenu
                } else {
                    Target::SettingsMenu
                };
                self.pressed = None;
                return vec![];
            }
            if let Overlay::BackgroundDelete { id, restore } = overlay {
                self.overlay = Some(Overlay::BackgroundOptions { id, restore });
                self.focus = Target::DeleteBackground;
                self.pressed = None;
                return vec![];
            }
            if let Overlay::BackgroundOptions { id, .. } = overlay {
                self.focus = self.background_target(&id);
                self.pressed = None;
                return vec![];
            }
            if matches!(overlay, Overlay::WallpaperUpload) {
                self.wallpaper_upload = None;
                self.focus = Target::UploadWallpaper;
                self.pressed = None;
                return vec![];
            }
            if matches!(overlay, Overlay::TileEditor) {
                self.tile_editor = None;
                self.focus = Target::TileEditor;
                self.pressed = None;
                return vec![];
            }
            if matches!(overlay, Overlay::WebAccessPair) {
                self.web_access_pairing = None;
                self.focus = Target::WebAccessPair;
                self.pressed = None;
                return vec![];
            }
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
            if let Overlay::Options { restore, .. } | Overlay::Recents { restore } = overlay {
                self.focus = restore;
                self.recents.pending = None;
                self.reconcile_focus();
                return vec![];
            }
            self.focus = self.keyboard_return();
            return self.commit_text_input();
        }
        if self.page == Page::Settings && matches!(self.focus, Target::Sidebar(_)) {
            self.open(0);
            return vec![];
        }
        if self.page == Page::Settings && self.subpage.is_some() {
            self.close_subpage();
            return vec![];
        }
        if self.page == Page::Settings && !matches!(self.focus, Target::Nav(_)) {
            self.focus = Target::Sidebar(self.section);
            self.scroll = 0.;
            return vec![];
        }
        if self.page != Page::Home {
            self.open(0);
            self.focus = self.first_content();
        }
        vec![]
    }
}
