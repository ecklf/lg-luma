//! Screen geometry and styling mirror the web UI's 1920×1080 / 16px-rem layout.
use crate::{
    i18n::Language,
    model::{App, BACKGROUNDS, CATEGORIES, Input, InputMode},
    renderer::{Color, Draw, TextStyle, WHITE},
    state::{
        APPS_COLUMNS, Control, HOME_FADE_LEFT, HOME_FADE_RIGHT, HOME_STRIP_LEFT, HOME_STRIP_RIGHT,
        HOME_TILE_PITCH, HOME_TILE_WIDTH, KEYS, Overlay, Page, Rect, SYSTEM_SECTION, State,
        Subpage, Target, TextField, WEATHER_SECTION,
    },
    weather::{self, Unit},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use time::OffsetDateTime;

const MAIN_TOP: f32 = 108.8;
const FACE_HEIGHT: f32 = 154.4;
const GRID_PITCH: f32 = 296.4;
const RECENTS_BACKDROP_OPACITY: f32 = 0.86;

#[path = "recents-view.rs"]
mod recents_view;

pub struct Assets {
    pub root: PathBuf,
    pub version: String,
    pub metrics: BTreeMap<String, BTreeMap<char, f32>>,
    pub artwork: crate::app_artwork::Service,
}
impl Assets {
    pub fn load(root: PathBuf, webos: bool) -> Result<Self, String> {
        let load = |file: &str| {
            std::fs::read(root.join(file))
                .map_err(|e| format!("Native assets are missing: {e}. Run pnpm assets."))
        };
        let version = String::from_utf8(load("version.txt")?).map_err(|e| e.to_string())?;
        let metrics =
            serde_json::from_slice(&load("font-metrics.json")?).map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            version,
            metrics,
            artwork: crate::app_artwork::Service::new(webos),
        })
    }
    fn path(&self, path: impl AsRef<Path>) -> String {
        self.root.join(path).to_string_lossy().into_owned()
    }

    fn background(&self, state: &State, id: &str) -> String {
        if let Some(item) = state.wallpapers.iter().find(|item| item.id == id) {
            return item.image();
        }
        let id = if BACKGROUNDS.iter().any(|(known, _)| *known == id) {
            id
        } else {
            "dunes"
        };
        self.path(format!("wallpapers/{id}.jpg"))
    }

    fn width(&self, value: &str, style: TextStyle) -> f32 {
        let key = format!(
            "{}{}",
            if style.size >= 32. { "display-" } else { "" },
            style.weight
        );
        let advances = self.metrics.get(&key);
        let fallback = self.metrics.get(if style.weight >= 500 {
            "fallback-500"
        } else {
            "fallback-400"
        });
        value
            .chars()
            .map(|c| {
                (advances
                    .and_then(|a| a.get(&c))
                    .or_else(|| fallback.and_then(|a| a.get(&c)))
                    .copied()
                    .unwrap_or(if c == ' ' { 0.28 } else { 0.53 })
                    + style.tracking)
                    * style.size
            })
            .sum()
    }

    fn fit_text(&self, value: &str, mut style: TextStyle, width: f32) -> TextStyle {
        // Leave a pixel for floating-point differences from the renderer's
        // font advances. Re-measure if shrinking crosses the display-font
        // threshold at 32px, where a different optical font is selected.
        let available = width - 1.;
        for _ in 0..2 {
            let measured = self.width(value, style);
            if measured <= available {
                break;
            }
            style.size *= available / measured;
        }
        style
    }

    fn lines(&self, value: &str, style: TextStyle, width: f32) -> usize {
        let key = format!(
            "{}{}",
            if style.size >= 32. { "display-" } else { "" },
            style.weight
        );
        let advances = self.metrics.get(&key);
        let fallback = self.metrics.get(if style.weight >= 500 {
            "fallback-500"
        } else {
            "fallback-400"
        });
        let advance = |c: char| {
            (advances
                .and_then(|a| a.get(&c))
                .or_else(|| fallback.and_then(|a| a.get(&c)))
                .copied()
                .unwrap_or(if c == ' ' { 0.28 } else { 0.53 })
                + style.tracking)
                * style.size
        };
        let mut lines = 1;
        let mut used = 0.;
        for word in value.split_inclusive(char::is_whitespace) {
            let w: f32 = word.chars().map(advance).sum();
            if used > 0. && used + w > width {
                lines += 1;
                used = 0.;
            }
            for c in word.chars() {
                if c == '\n' {
                    lines += 1;
                    used = 0.;
                    continue;
                }
                let w = advance(c);
                if used > 0. && used + w > width {
                    lines += 1;
                    used = 0.;
                }
                used += w;
            }
        }
        lines
    }
}

pub struct Scene {
    pub draw: Vec<Draw>,
    pub controls: Vec<Control>,
    pub modal: Option<Rect>,
}
impl Scene {
    fn new() -> Self {
        Self {
            draw: Vec::new(),
            controls: Vec::new(),
            modal: None,
        }
    }
    fn shape(&mut self, rect: Rect, radius: f32, fill: Color) {
        self.draw.push(Draw::Shape(rect, radius, 0., fill));
    }
    fn border(&mut self, rect: Rect, radius: f32, width: f32, color: Color) {
        self.draw.push(Draw::Shape(rect, radius, width, color));
    }
    fn text(
        &mut self,
        text: impl Into<String>,
        rect: Rect,
        style: TextStyle,
        color: Color,
        center: bool,
    ) {
        self.draw
            .push(Draw::Text(text.into(), rect, style, color, center));
    }
    fn image(&mut self, path: String, rect: Rect, radius: f32, contain: bool) {
        self.draw.push(Draw::Image(path, rect, radius, contain));
    }
    fn icon(&mut self, assets: &Assets, name: &str, rect: Rect, color: Color) {
        self.draw.push(Draw::TintedImage(
            assets.path(format!("ui-icons/{name}.lvec")),
            rect,
            0.,
            true,
            color,
        ));
    }
    fn control(&mut self, target: Target, rect: Rect, scrollable: bool) {
        self.controls.push(Control {
            target,
            rect,
            scrollable,
        });
    }
    fn ring(&mut self, rect: Rect, radius: f32) {
        self.border(rect.inset(-6.9), radius + 6.9, 3.7, c(0xffffff, 0.10));
        self.border(rect.inset(-3.2), radius + 3.2, 3.2, c(0xf0f0f0, 1.));
    }
    fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, y: f32, alpha: f32) {
        self.draw.push(Draw::Shadow(
            Rect::new(rect.x, rect.y + y, rect.w, rect.h),
            radius,
            blur,
            c(0, alpha),
        ));
    }
    fn pill(&mut self, rect: Rect, focused: bool, selected: bool) {
        self.shadow(rect, rect.h / 2., 18., 4., 0.06);
        if focused {
            self.draw.push(Draw::RoundedGradient(
                rect,
                rect.h / 2.,
                c(0xfafafa, 1.),
                c(0xe5e5e5, 1.),
            ));
        } else {
            self.shape(
                rect,
                rect.h / 2.,
                if selected {
                    c(0x303438, 1.)
                } else {
                    c(0x171c20, 0.72)
                },
            );
            self.border(rect, rect.h / 2., 1., c(0xffffff, 0.07));
        }
    }
    fn fade(&mut self, from: usize, alpha: f32, brightness: f32) {
        for draw in &mut self.draw[from..] {
            let apply = |color: &mut Color| {
                for channel in &mut color[..3] {
                    *channel *= brightness;
                }
                color[3] *= alpha;
            };
            match draw {
                Draw::Shape(_, _, _, color)
                | Draw::Shadow(_, _, _, color)
                | Draw::Text(_, _, _, color, _)
                | Draw::TintedImage(_, _, _, _, color)
                | Draw::AppImage(_, _, color) => apply(color),
                Draw::RoundedGradient(_, _, a, b) => {
                    apply(a);
                    apply(b);
                }
                Draw::Image(path, rect, radius, contain) => {
                    *draw = Draw::TintedImage(
                        path.clone(),
                        *rect,
                        *radius,
                        *contain,
                        [brightness, brightness, brightness, alpha],
                    );
                }
                Draw::Clip(_) | Draw::EdgeFade(_) | Draw::EdgeFadeX(_) => {}
            }
        }
    }
}

fn wallpaper_upload(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let panel = Rect::new(520., 144., 880., 816.);
    out.modal = Some(panel);
    out.shadow(panel, 24., 40., 12., 0.3);
    out.shape(panel, 24., c(0x1b2023, 1.));
    out.border(panel, 24., 1., c(0xffffff, 0.09));
    out.text(
        language.t("uploadWallpaper"),
        Rect::new(584., 198., 752., 48.),
        text(32., 550, -0.02),
        WHITE,
        true,
    );
    out.text(
        language.t("wallpaperUploadHint"),
        Rect::new(604., 264., 712., 32.),
        text(19., 400, 0.).leading(1.5),
        c(0xffffff, 0.65),
        true,
    );
    match &state.wallpaper_upload {
        Some(Ok(session)) => {
            let scale = (336. / (session.qr_size + 8) as f32).floor();
            let size = (session.qr_size + 8) as f32 * scale;
            let x = (1920. - size) / 2.;
            let y = 310.;
            // Invert the normal QR polarity to blend into the dark modal, while
            // retaining a high-contrast foreground and the full quiet zone.
            let background = c(0x1b2023, 1.);
            let modules = c(0xf5f4ef, 1.);
            out.shape(Rect::new(x, y, size, size), 0., background);
            for row in 0..session.qr_size {
                // Merge contiguous modules so QR rendering stays a few hundred quads.
                let mut column = 0;
                while column < session.qr_size {
                    let mark_start = (session.qr_size - 11) / 2;
                    let in_mark = (mark_start..mark_start + 11).contains(&row)
                        && (mark_start..mark_start + 11).contains(&column);
                    if in_mark {
                        column += 1;
                        continue;
                    }
                    if !session.qr[(row * session.qr_size + column) as usize] {
                        column += 1;
                        continue;
                    }
                    let start = column;
                    while column < session.qr_size {
                        let in_mark = (mark_start..mark_start + 11).contains(&row)
                            && (mark_start..mark_start + 11).contains(&column);
                        if in_mark || !session.qr[(row * session.qr_size + column) as usize] {
                            break;
                        }
                        column += 1;
                    }
                    out.shape(
                        Rect::new(
                            x + (start + 4) as f32 * scale,
                            y + (row + 4) as f32 * scale,
                            (column - start) as f32 * scale,
                            scale,
                        ),
                        0.,
                        modules,
                    );
                }
            }
            let mark_start = (session.qr_size - 11) / 2;
            let mark_plate = Rect::new(
                x + (mark_start + 4) as f32 * scale,
                y + (mark_start + 4) as f32 * scale,
                11. * scale,
                11. * scale,
            );
            out.shape(mark_plate, 1.1 * scale, background);
            let mark = Rect::new(
                mark_plate.x + 1.25 * scale,
                mark_plate.y + 1.25 * scale,
                8.5 * scale,
                8.5 * scale,
            );
            out.icon(assets, "luma-mark", mark, WHITE);
            out.text(
                language.t("wallpaperUploadBrowser"),
                Rect::new(604., 670., 712., 32.),
                text(19., 400, 0.).leading(1.5),
                c(0xffffff, 0.65),
                true,
            );
            let display_url = session.url.trim_end_matches('/');
            out.text(
                display_url,
                Rect::new(568., 716., 784., 36.),
                assets.fit_text(display_url, text(26., 600, 0.), 784.),
                WHITE,
                true,
            );
            out.text(
                language.t("wallpaperUploadKeepOpen"),
                Rect::new(604., 766., 712., 52.),
                text(16., 400, 0.).leading(1.4),
                c(0xffffff, 0.75),
                true,
            );
            out.text(
                language.t("wallpaperUploadPrivate"),
                Rect::new(604., 820., 712., 22.),
                text(15., 400, 0.),
                c(0xffffff, 0.5),
                true,
            );
        }
        result => {
            let key = if result.is_some() {
                "wallpaperUploadError"
            } else {
                "wallpaperUploadStarting"
            };
            out.text(
                language.t(key),
                Rect::new(604., 470., 712., 90.),
                text(22., 450, 0.).leading(1.5),
                c(0xffffff, 0.75),
                true,
            );
        }
    }
    let close = Rect::new(812., 862., 296., 56.);
    out.pill(close, state.focus == Target::CloseDialog, false);
    out.text(
        language.t("close"),
        Rect::new(close.x, close.y + 15., close.w, 28.),
        text(20., 550, 0.),
        c(0x171c20, 1.),
        true,
    );
    out.control(Target::CloseDialog, close, false);
}

fn c(rgb: u32, alpha: f32) -> Color {
    [
        ((rgb >> 16) & 255) as f32 / 255.,
        ((rgb >> 8) & 255) as f32 / 255.,
        (rgb & 255) as f32 / 255.,
        alpha,
    ]
}
fn text(size: f32, weight: u16, tracking: f32) -> TextStyle {
    TextStyle::new(size).weight(weight).tracking(tracking)
}

pub struct Navigation {
    pub surface: Rect,
    pub items: [Rect; 3],
}
pub fn navigation(assets: &Assets, language: Language) -> Navigation {
    let widths = ["home", "apps", "settings"]
        .map(|key| (assets.width(language.t(key), text(20.8, 550, -0.02)) + 46.4).max(99.2));
    let width = widths.iter().sum::<f32>() + 25.6 + 19.6;
    let mut x = (1920. - width) / 2. + 9.8;
    let items = widths.map(|width| {
        let rect = Rect::new(x, 52.3, width, 49.);
        x += width + 12.8;
        rect
    });
    Navigation {
        surface: Rect::new((1920. - width) / 2., 42.5, width, 68.6),
        items,
    }
}

/// Fully shown Recents is drawn instead of the page, which is then not part of the scene.
pub fn recents_replaces_page(state: &State) -> bool {
    state.recents_open() && state.recents.visibility == 1.
}

pub fn scene_with_pill(
    state: &State,
    assets: &Assets,
    pill_x: f32,
    pill_width: f32,
    now: OffsetDateTime,
) -> Scene {
    let mut out = Scene::new();
    if let Some((previous, start)) = &state.previous_background {
        out.image(
            assets.background(state, previous),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
        let alpha = (start.elapsed().as_secs_f32() / state.background_fade).min(1.);
        out.draw.push(Draw::TintedImage(
            assets.background(state, &state.preferences.background),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
            [1., 1., 1., alpha],
        ));
    } else {
        out.image(
            assets.background(state, &state.preferences.background),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
    }
    let content_start = out.draw.len();
    if recents_replaces_page(state) {
        out.shape(
            Rect::new(0., 0., 1920., 1080.),
            0.,
            c(0x080b12, RECENTS_BACKDROP_OPACITY),
        );
        recents_view::render(&mut out, state, assets);
        if let Some((notice, _)) = &state.notice {
            toast(&mut out, state, assets, notice);
        }
        return out;
    }
    let viewport = state.content_viewport();
    let main_bottom = viewport.y + viewport.h;
    out.draw.push(Draw::Clip(Some(viewport)));
    if state.page == Page::Apps {
        out.draw
            .push(Draw::EdgeFade(Some([MAIN_TOP, 152.8, 1080., 1104.])));
    }
    match state.page {
        Page::Home => home(&mut out, state, assets, now),
        Page::Apps => library(&mut out, state, assets),
        Page::Settings => settings(&mut out, state, assets),
    }
    if let Some(error) = &state.error
        && !state.inventory.apps.is_empty()
        && !out
            .controls
            .iter()
            .any(|control| control.target == Target::Retry)
    {
        let rect = Rect::new(84., main_bottom - 66., 1752., 54.);
        out.shape(rect, 11.2, c(0xad7c3e, 0.07));
        out.border(rect, 11.2, 1., c(0xedc596, 0.19));
        out.icon(
            assets,
            "alert",
            Rect::new(104., rect.y + 18., 18., 18.),
            c(0xf3ddbd, 1.),
        );
        out.text(
            error,
            Rect::new(138., rect.y + 16., 1500., 35.),
            text(14.4, 400, 0.),
            c(0xf3ddbd, 1.),
            false,
        );
        let retry = Rect::new(1720., rect.y + 8., 96., 38.);
        out.text(
            state.preferences.language.t("retry"),
            Rect::new(retry.x, retry.y + 10., retry.w, 20.),
            text(14.4, 500, 0.),
            c(0xf3ddbd, 1.),
            true,
        );
        if state.focus == Target::Retry {
            out.border(retry, 6.4, 2., WHITE);
        }
        out.control(Target::Retry, retry, false);
    }
    out.draw.push(Draw::EdgeFade(None));
    out.draw.push(Draw::Clip(None));
    header(&mut out, state, assets, now, pill_x, pill_width);
    if let Some(
        overlay @ (Overlay::Options { .. }
        | Overlay::Delete { .. }
        | Overlay::BackgroundOptions { .. }
        | Overlay::BackgroundDelete { .. }
        | Overlay::Keyboard
        | Overlay::WallpaperUpload),
    ) = &state.overlay
    {
        let anchor = match overlay {
            Overlay::BackgroundOptions { id, .. } => out.controls.iter()
                .find(|control| matches!(control.target, Target::Background(index) if state.background_id(index) == Some(id.as_str())))
                .map(|control| Rect::new(control.rect.x, control.rect.y, control.rect.w, 202.5)),
            Overlay::Options { id, .. } => out
                .controls
                .iter()
                .find(|control| control.target == Target::App(id.clone()))
                .map(|control| {
                    // Hit regions include captions, artwork does not. Reusing the
                    // full hit region here stretches the lifted card vertically.
                    let mut rect = control.rect;
                    let names = if state.page == Page::Home {
                        state.preferences.show_home_names
                    } else {
                        state.preferences.show_app_names
                    };
                    if names
                        && let Some(app) = state.inventory.apps.iter().find(|app| &app.id == id)
                    {
                        rect.h -= 18.4 + caption_height(state, assets, app, rect.w);
                    }
                    rect
                }),
            _ => None,
        };
        out.controls.clear();
        let start = out.draw.len();
        out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x080b12, 0.70));
        if matches!(
            overlay,
            Overlay::Options { .. } | Overlay::BackgroundOptions { .. }
        ) && let Some(opened) = state.panel_started
        {
            // The lifted card stays opaque while the backdrop and popup fade.
            // Fading it over its dimmed parent copy causes a brightness dip.
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
        }
        match overlay {
            Overlay::Options { id, .. } | Overlay::BackgroundOptions { id, .. } => {
                options(&mut out, state, assets, id, anchor)
            }
            Overlay::Delete { id, .. } | Overlay::BackgroundDelete { id, .. } => {
                delete_confirmation(&mut out, state, assets, id)
            }
            Overlay::Keyboard => keyboard(&mut out, state, assets),
            Overlay::WallpaperUpload => wallpaper_upload(&mut out, state, assets),
            Overlay::SystemKeyboard | Overlay::Recents { .. } => {}
        }
        if !matches!(
            overlay,
            Overlay::Options { .. } | Overlay::BackgroundOptions { .. }
        ) && let Some(opened) = state.panel_started
        {
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
        }
    }
    if state.recents_open() || state.recents.visibility > 0. {
        // Crossfade the actual parent interface as well as the dimmed stage.
        // Retain outgoing visuals after dismissal, while input immediately
        // belongs to the restored parent. No geometry or hit targets move.
        let parent_controls = std::mem::take(&mut out.controls);
        let parent_modal = out.modal;
        out.fade(content_start, 1. - state.recents.visibility, 1.);
        let start = out.draw.len();
        out.shape(
            Rect::new(0., 0., 1920., 1080.),
            0.,
            c(0x080b12, RECENTS_BACKDROP_OPACITY),
        );
        recents_view::render(&mut out, state, assets);
        out.fade(start, state.recents.visibility, 1.);
        // Dim the wallpaper beneath the outgoing interface rather than over it. While both
        // fade, the backdrop and Recents' full-screen gradient then lie directly on the
        // wallpaper, and the renderer folds them into it instead of two more full-screen
        // passes per frame of the crossfade.
        let fills = out.draw[start..]
            .iter()
            .take_while(|draw| full_screen_fill(draw))
            .count();
        let moved: Vec<_> = out.draw.drain(start..start + fills).collect();
        out.draw.splice(content_start..content_start, moved);
        if !state.recents_open() {
            out.controls = parent_controls;
            out.modal = parent_modal;
        }
    }
    if let Some((notice, _)) = &state.notice {
        toast(&mut out, state, assets, notice);
    }
    out
}

fn header(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    now: OffsetDateTime,
    pill_x: f32,
    pill_width: f32,
) {
    let language = state.preferences.language;
    let clock = language.clock(
        now,
        state.preferences.hour_24,
        state.preferences.show_seconds,
    );
    out.text(
        clock,
        Rect::new(84., 64.8, 300., 24.),
        text(20., 500, -0.03),
        WHITE,
        false,
    );
    let layout = navigation(assets, language);
    let nav = layout.surface;
    out.shadow(nav, 48., 18., 4., 0.06);
    out.shape(nav, 48., c(0x171c20, 0.72));
    out.border(nav, 48., 1., c(0xffffff, 0.07));
    let pill = Rect::new(pill_x, 52.3, pill_width, 49.);
    out.draw.push(Draw::RoundedGradient(
        pill,
        32.,
        c(0xfafafa, 1.),
        c(0xe5e5e5, 1.),
    ));
    if matches!(state.focus, Target::Nav(_)) && !state.pointer {
        // Follow the pill's animated position and width instead of jumping to
        // the destination tab's control rectangle.
        out.border(pill.inset(-5.), 37., 2., c(0xf0f0f0, 0.95));
    }
    for (index, key) in ["home", "apps", "settings"].into_iter().enumerate() {
        let rect = layout.items[index];
        out.text(
            language.t(key),
            Rect::new(rect.x, 64.3, rect.w, 25.),
            text(20.8, 550, -0.02),
            if state.page_index() == index {
                c(0x2c2c2c, 1.)
            } else {
                c(0xeeeeee, 0.73)
            },
            true,
        );
        out.control(Target::Nav(index), rect, false);
    }
    let rect = Rect::new(1786.4, 52., 49.6, 49.6);
    let focused = state.focus == Target::TvSettings;
    out.pill(rect, focused, false);
    out.icon(
        assets,
        "settings",
        Rect::new(1799.6, 65.2, 23.2, 23.2),
        if focused {
            c(0x2c2c2c, 1.)
        } else {
            c(0xeeeeee, 0.73)
        },
    );
    out.control(Target::TvSettings, rect, false);
}

fn tile(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    app: &App,
    rect: Rect,
    names: bool,
    pinned: bool,
) {
    let caption_height = if names {
        caption_height(state, assets, app, rect.w)
    } else {
        0.
    };
    let label_height = if names { 18.4 + caption_height } else { 0. };
    out.control(
        Target::App(app.id.clone()),
        Rect::new(rect.x, rect.y, rect.w, rect.h + label_height),
        true,
    );
    let viewport = if state.page == Page::Home {
        state.favorites_viewport()
    } else {
        state.content_viewport()
    };
    if rect.y + rect.h + label_height < MAIN_TOP
        || rect.y > 1054.
        || rect.x + rect.w + 40. < viewport.x
        || rect.x - 40. > viewport.x + viewport.w
    {
        return;
    }
    let start = out.draw.len();
    let focused = state.focus == Target::App(app.id.clone());
    out.shadow(
        rect,
        19.2,
        if focused { 34. } else { 22. },
        if focused { 14. } else { 9. },
        if focused { 0.4 } else { 0.165 },
    );
    app_face(out, assets, app, rect, state.preferences.language, 19.2);
    if pinned {
        out.icon(
            assets,
            "heart-filled",
            Rect::new(rect.x + rect.w - 27.2, rect.y + 11.2, 16., 16.),
            c(0xffffff, 0.9),
        );
    }
    if state.moving.as_ref() == Some(&app.id) {
        let indicator = Rect::new(
            rect.x + rect.w / 2. - 37.6,
            rect.y + rect.h - 33.6,
            75.2,
            24.,
        );
        out.shape(indicator, 16., c(0x101318, 0.79));
        out.icon(
            assets,
            "left",
            Rect::new(indicator.x + 8.8, indicator.y + 4., 16., 16.),
            c(0xededed, 1.),
        );
        out.icon(
            assets,
            "arrow",
            Rect::new(indicator.x + 40., indicator.y + 4., 16., 16.),
            c(0xededed, 1.),
        );
    }
    if focused {
        out.ring(rect, 19.2);
    }
    if state.page == Page::Apps && state.filter == 5 {
        out.fade(start, 0.45, 1.);
    }
    if state.pressed.as_ref() == Some(&Target::App(app.id.clone())) {
        out.fade(start, 1., 0.85);
    }
    if names {
        out.text(
            format!(
                "{}{}",
                app.title,
                if state.filter == 5 {
                    format!(" · {}", state.preferences.language.t("hiddenLabel"))
                } else {
                    String::new()
                }
            ),
            Rect::new(
                rect.x + 4.,
                rect.y + rect.h + 18.4,
                rect.w - 8.,
                caption_height,
            ),
            text(17.6, if focused { 550 } else { 450 }, -0.02).leading(1.4),
            if focused {
                c(0xffffff, 1.)
            } else {
                c(0xececec, 0.66)
            },
            true,
        );
    }
    if state.moving.is_some() && state.moving.as_ref() != Some(&app.id) {
        out.fade(start, 0.48, 1.);
    }
}

/// Shared card artwork for Home, Apps and Recents. Curated vectors always take
/// precedence over installed artwork, with the same relative logo proportions.
fn app_face(
    out: &mut Scene,
    assets: &Assets,
    app: &App,
    rect: Rect,
    language: Language,
    radius: f32,
) {
    let logo = crate::branding::for_key(&app.brand);
    let artwork = logo
        .is_none()
        .then(|| {
            app.icon
                .as_ref()
                .and_then(|source| assets.artwork.get(source))
        })
        .flatten();
    if let Some([top, bottom]) = artwork.as_ref().map(|image| image.backdrop) {
        let color = |rgb: [u8; 3]| rgb.map(|c| c as f32 / 255.);
        let [top, bottom] = [color(top), color(bottom)];
        if top == bottom {
            out.shape(rect, radius, [top[0], top[1], top[2], 1.]);
        } else {
            out.draw.push(Draw::RoundedGradient(
                rect,
                radius,
                [top[0], top[1], top[2], 1.],
                [bottom[0], bottom[1], bottom[2], 1.],
            ));
        }
    } else {
        out.image(
            assets.path(format!("surfaces/{}.png", app.brand)),
            rect,
            radius,
            false,
        );
    }
    out.border(rect, radius, 1., c(0xffffff, 0.063));
    if let Some(brand) = logo {
        let (ratio, height) = brand.artwork_size();
        let height = height * rect.h / FACE_HEIGHT;
        out.image(
            assets.path(format!("logos/{}.lvec", app.brand)),
            Rect::new(
                rect.x + rect.w * (1. - ratio) / 2.,
                rect.y + (rect.h - height) / 2.,
                rect.w * ratio,
                height,
            ),
            0.,
            true,
        );
    } else if let Some(image) = artwork {
        out.draw.push(Draw::AppImage(
            image,
            Rect::new(
                rect.x + rect.w * 0.16,
                rect.y + rect.h * 0.16,
                rect.w * 0.68,
                rect.h * 0.68,
            ),
            [1.; 4],
        ));
    } else {
        brand_mark(out, assets, app, rect, language);
    }
}

fn caption_height(state: &State, assets: &Assets, app: &App, width: f32) -> f32 {
    let title = format!(
        "{}{}",
        app.title,
        if state.filter == 5 {
            format!(" · {}", state.preferences.language.t("hiddenLabel"))
        } else {
            String::new()
        }
    );
    let style = text(
        17.6,
        if state.focus == Target::App(app.id.clone()) {
            550
        } else {
            450
        },
        -0.02,
    )
    .leading(1.4);
    (assets.lines(&title, style, width - 8.) as f32 * style.size * style.line_height).max(49.28)
}

fn brand_mark(out: &mut Scene, assets: &Assets, app: &App, rect: Rect, language: Language) {
    let scale = rect.h / FACE_HEIGHT;
    let rect = Rect::new(
        rect.x / scale,
        rect.y / scale,
        rect.w / scale,
        rect.h / scale,
    );
    let start = out.draw.len();
    let (cx, cy) = rect.center();
    match app.brand.as_str() {
        "plex" => {
            out.text(
                "ple",
                Rect::new(cx - 63., cy - 35.2, 130., 70.4),
                text(70.4, 650, -0.08).leading(1.),
                WHITE,
                false,
            );
            out.text(
                "x",
                Rect::new(cx + 20., cy - 35.2, 60., 70.4),
                text(70.4, 800, -0.08).leading(1.),
                c(0xe6aa25, 1.),
                false,
            );
        }
        "spotify" => {
            out.icon(
                assets,
                "spotify",
                Rect::new(cx - 77., cy - 20., 40., 40.),
                c(0x5ae08c, 1.),
            );
            out.text(
                "Spotify",
                Rect::new(cx - 29., cy - 14.4, 120., 36.),
                text(28.8, 650, -0.04).leading(1.),
                c(0x5ae08c, 1.),
                false,
            );
        }
        "kodi" => {
            out.icon(
                assets,
                "kodi",
                Rect::new(cx - 76., cy - 22.5, 45., 45.),
                WHITE,
            );
            out.text(
                "kodi",
                Rect::new(cx - 18., cy - 24.8, 130., 60.),
                text(49.6, 350, -0.04).leading(1.),
                WHITE,
                false,
            );
        }
        brand => {
            let (icon, label) = match brand {
                "live-tv" => ("tv", language.t("liveTV")),
                "homebrew" => ("coffee", language.t("homebrew")),
                "browser" => ("globe", language.t("browser")),
                "moonlight" => ("moonlight", "Moonlight"),
                "gaming" => ("gaming", app.title.as_str()),
                _ => ("generic", app.title.as_str()),
            };
            let font = if brand == "generic" {
                text(18.4, 500, -0.035).leading(1.35)
            } else {
                text(24., 500, -0.035).leading(1.2)
            };
            let w = (assets.width(label, font) + 54.4).min(rect.w - 32.);
            let label_w = rect.w - 54.4 - 32.;
            let label_h =
                (assets.lines(label, font, label_w) as f32 * font.size * font.line_height)
                    .min(rect.h - 32.);
            out.icon(
                assets,
                icon,
                Rect::new(cx - w / 2., cy - 20.8, 41.6, 41.6),
                WHITE,
            );
            out.text(
                label,
                Rect::new(cx - w / 2. + 54.4, cy - label_h / 2., label_w, label_h),
                font,
                WHITE,
                false,
            );
        }
    }
    if scale != 1. {
        for draw in &mut out.draw[start..] {
            let rect = match draw {
                Draw::Text(_, rect, style, _, _) => {
                    style.size *= scale;
                    rect
                }
                Draw::TintedImage(_, rect, _, _, _) => rect,
                _ => continue,
            };
            *rect = Rect::new(
                rect.x * scale,
                rect.y * scale,
                rect.w * scale,
                rect.h * scale,
            );
        }
    }
}

fn home(out: &mut Scene, state: &State, assets: &Assets, now: OffsetDateTime) {
    let language = state.preferences.language;
    let s = state.scroll;
    let summary = state
        .preferences
        .show_weather
        .then(|| weather_summary_layout(state, assets));
    let shift = summary
        .as_ref()
        .map_or(0., |summary| (summary.height + 18.4) / 2.);
    let date = language.date(now);
    out.text(
        date,
        Rect::new(84., 227.2 - shift - s, 1200., 17.),
        text(14.4, 600, 0.12),
        c(0xf5f5f5, 0.72),
        false,
    );
    let greeting = language.greeting(now.hour(), &state.preferences.greeting_name);
    let greeting_style = text(90.4, 500, -0.065).leading(1.05);
    let fitted = assets.fit_text(&greeting, greeting_style, 1500.);
    let height = 95. * fitted.size / greeting_style.size;
    out.text(
        &greeting,
        Rect::new(84., 265. + (95. - height) / 2. - shift - s, 1500., height),
        fitted,
        WHITE,
        false,
    );
    if let Some(summary) = summary {
        weather_summary(out, state, assets, summary, 265. - shift + 94.92 + 18.4 - s);
    }
    out.text(
        language.t("favorites"),
        Rect::new(84., 478.5 - s, 1000., 31.),
        text(23.2, 550, -0.035).leading(1.3),
        WHITE,
        false,
    );
    if state.moving.is_some() {
        out.text(
            language.t("moveApps"),
            Rect::new(1686., 485. - s, 150., 24.),
            text(14.4, 400, 0.),
            c(0xdedede, 1.),
            true,
        );
    }
    let apps = state.apps();
    let names = state.preferences.show_home_names;
    let mut grid_height = FACE_HEIGHT;
    if state.loading && state.inventory.apps.is_empty() {
        for index in 0..6 {
            out.shape(
                Rect::new(
                    84. + index as f32 * GRID_PITCH,
                    544.8 - s,
                    270.,
                    FACE_HEIGHT,
                ),
                19.2,
                c(0xffffff, 0.06),
            );
        }
    } else if apps.is_empty() {
        let error = state.error.is_some() && state.inventory.apps.is_empty();
        empty(
            out,
            state,
            assets,
            EmptyState {
                rect: Rect::new(84., 533.6 - s, 1752., 258.7),
                icon: if error { "alert" } else { "heart" },
                title: if error {
                    language.t("aLittleHelp")
                } else {
                    language.t("makeFavorites")
                },
                hint: if error {
                    state.error.as_deref().unwrap_or("")
                } else {
                    language.t("pinHint")
                },
                action: Some(if error { Target::Retry } else { Target::Browse }),
            },
        );
        grid_height = 247.5;
    }
    if !apps.is_empty() {
        grid_height = FACE_HEIGHT
            + if names {
                18.4 + apps
                    .iter()
                    .map(|app| caption_height(state, assets, app, HOME_TILE_WIDTH))
                    .fold(49.28, f32::max)
            } else {
                0.
            };
    }
    // Like Apple TV's rows, the favourites run to the screen edge: the next tile shows cut off
    // at the right (and a scrolled-away one at the left), so it is clear the row continues.
    out.draw.push(Draw::Clip(Some(state.favorites_viewport())));
    // Soft edges: out to the right from the end of the sixth tile, and in from the left once
    // the row has scrolled.
    let left = if state.home_scroll > 0. {
        [HOME_FADE_LEFT, HOME_STRIP_LEFT]
    } else {
        [0., 0.]
    };
    out.draw.push(Draw::EdgeFadeX(Some([
        left[0],
        left[1],
        HOME_STRIP_RIGHT,
        HOME_FADE_RIGHT,
    ])));
    for (index, app) in apps.iter().enumerate() {
        tile(
            out,
            state,
            assets,
            app,
            Rect::new(
                84. + index as f32 * HOME_TILE_PITCH - state.home_scroll,
                544.8 - s,
                HOME_TILE_WIDTH,
                FACE_HEIGHT,
            ),
            names,
            false,
        );
    }
    out.draw.push(Draw::EdgeFadeX(None));
    out.draw.push(Draw::Clip(Some(state.content_viewport())));
    let input_y = 544.8 + grid_height + 41.6 - s;
    let inputs: Vec<_> = state
        .inventory
        .inputs
        .iter()
        .filter(|input| state.preferences.input_visible(input))
        .collect();
    if !inputs.is_empty() {
        out.text(
            language.t("inputsHeading"),
            Rect::new(84., input_y + 3.3, 1000., 31.),
            text(23.2, 550, -0.035).leading(1.3),
            WHITE,
            false,
        );
    }
    for (index, input) in inputs.iter().enumerate() {
        let rect = Rect::new(
            84. + (index % 3) as f32 * 591.45,
            input_y + 58.4 + (index / 3) as f32 * 126.4,
            569.1,
            104.,
        );
        let focused = state.focus == Target::Input(input.id.clone());
        out.shadow(
            rect,
            18.4,
            if focused { 34. } else { 4. },
            if focused { 14. } else { 1. },
            if focused { 0.4 } else { 0.01 },
        );
        out.draw.push(Draw::RoundedGradient(
            rect,
            18.4,
            if focused {
                c(0x30363c, 0.70)
            } else {
                c(0x20252b, 0.60)
            },
            if focused {
                c(0x242a30, 0.65)
            } else {
                c(0x171c22, 0.55)
            },
        ));
        out.border(
            rect,
            18.4,
            1.,
            c(0xffffff, if focused { 0.24 } else { 0.063 }),
        );
        let art = Rect::new(rect.x + 21., rect.y + 24., 56., 56.);
        out.draw.push(Draw::RoundedGradient(
            art,
            12.8,
            c(0xd6d6d6, 0.24),
            c(0xa0a0a0, 0.10),
        ));
        let name = state.preferences.input_name(input);
        let icon = input_icon(input, name);
        let size = if icon == "apple-tv" { 41.6 } else { 35.2 };
        out.icon(
            assets,
            &format!("input-{icon}"),
            Rect::new(
                art.x + (56. - size) / 2.,
                art.y + (56. - size) / 2.,
                size,
                size,
            ),
            c(0xdedede, 1.),
        );
        out.text(
            state.preferences.input_title(input),
            Rect::new(rect.x + 93., rect.y + 24.3, 450., 28.8),
            text(24., 500, -0.03),
            WHITE,
            false,
        );
        out.text(
            input_status(input, name.is_some(), language),
            Rect::new(rect.x + 93., rect.y + 58.7, 450., 20.),
            text(13.12, 400, 0.).leading(1.5),
            c(0xe8e8e8, 0.48),
            false,
        );
        if focused {
            out.ring(rect, 18.4);
        }
        out.control(Target::Input(input.id.clone()), rect, true);
    }
}

struct WeatherSummary {
    height: f32,
    width: f32,
    icon: String,
    parts: Vec<(String, TextStyle, Color, Rect)>,
}
type WeatherPart = (String, TextStyle, Color, f32, f32);
fn weather_summary_layout(state: &State, assets: &Assets) -> WeatherSummary {
    let language = state.preferences.language;
    let base = text(23.2, 400, -0.025).leading(1.5);
    let mut parts = Vec::new();
    let mut icon = "location".to_owned();
    if let Some(place) = &state.preferences.weather_location {
        parts.push((place.name.clone(), base.weight(450), c(0xf5f5f5, 0.72)));
        parts.push(("·".into(), base, c(0xf5f5f5, 0.324)));
        if let Some(reading) = state.weather.reading(place) {
            let condition = weather::condition(reading);
            icon = format!("weather-{}", condition.icon);
            parts.push((
                weather::format_temperature(
                    reading.temperature,
                    state.preferences.temperature_unit,
                ),
                base.weight(550),
                c(0xf5f5f5, 0.91),
            ));
            parts.push((language.t(condition.key).into(), base, c(0xf5f5f5, 0.58)));
            if state.weather.failed {
                parts.push((
                    language.t("lastReading").into(),
                    text(15.08, 400, 0.).leading(1.5),
                    c(0xf2d5a5, 0.65),
                ));
            }
        } else {
            parts.push((
                if state.weather.loading {
                    language.t("loadingWeather")
                } else {
                    language.t("weatherUnavailable")
                }
                .into(),
                base,
                c(0xf5f5f5, 0.58),
            ));
        }
    } else {
        parts.push((
            language.t("setWeatherLocation").into(),
            base,
            c(0xf5f5f5, 0.72),
        ));
    }
    let mut rows: Vec<Vec<WeatherPart>> = vec![Vec::new()];
    let mut used = 0.;
    for (label, style, color) in parts {
        let measured_width = assets.width(&label, style);
        let width = measured_width.min(1714.4);
        // Use the same extra pixel as the rendered text rectangle. Counting
        // lines at the exact advance width can round into a phantom second
        // line even though the renderer displays the label on one line.
        let text_width = width + 1.;
        let lines = if measured_width <= text_width && !label.contains('\n') {
            1
        } else {
            assets.lines(&label, style, text_width)
        };
        let height = lines as f32 * style.size * style.line_height;
        if used > 0. && used + 11.2 + width > 1714.4 {
            rows.push(Vec::new());
            used = 0.;
        }
        rows.last_mut()
            .unwrap()
            .push((label, style, color, width, height));
        used += width + if used > 0. { 11.2 } else { 0. };
    }
    let mut positioned = Vec::new();
    let mut top = 1.6;
    let mut right: f32 = 0.;
    for row in rows {
        let height = row.iter().map(|part| part.4).fold(34.8, f32::max);
        let mut x = 121.6;
        for (label, style, color, width, item_height) in row {
            positioned.push((
                label,
                style,
                color,
                Rect::new(
                    x,
                    top + (height - item_height) / 2.,
                    width + 1.,
                    item_height,
                ),
            ));
            x += width + 11.2;
        }
        right = right.max(x - 11.2);
        top += height + 11.2;
    }
    WeatherSummary {
        height: top - 11.2 + 1.6,
        width: right - 84.,
        icon,
        parts: positioned,
    }
}
fn weather_summary(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    summary: WeatherSummary,
    y: f32,
) {
    let label_focus = summary.parts.first().map(|(_, style, _, rect)| {
        // The label's layout line box includes leading above and below the
        // text. Keep the focus ring snug to the label rather than the whole
        // weather row, which also contains the location icon.
        let leading = style.size * (style.line_height - 1.).max(0.);
        Rect::new(
            rect.x,
            y + rect.y + leading / 2.,
            rect.w,
            (rect.h - leading).max(style.size),
        )
    });
    out.icon(
        assets,
        &summary.icon,
        Rect::new(84., y + 5.8, 26.4, 26.4),
        c(0xdedede, 0.83),
    );
    for (label, style, color, rect) in summary.parts {
        out.text(
            label,
            Rect::new(rect.x, rect.y + y, rect.w, rect.h),
            style,
            color,
            false,
        );
    }
    if state.preferences.weather_location.is_none() {
        let rect = Rect::new(84., y, summary.width, summary.height);
        if state.focus == Target::WeatherConfigure
            && !state.pointer
            && let Some(label) = label_focus
        {
            out.border(label.inset(-5.), 8.6, 2., c(0xf0f0f0, 0.95));
        }
        out.control(Target::WeatherConfigure, rect, true);
    }
}

fn library(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    let mut x = 84.;
    for (index, label) in CATEGORIES.into_iter().enumerate() {
        let label = language.category(label);
        let width = assets.width(label, text(16., 400, 0.)) + 35.6;
        let rect = Rect::new(x, 171.4 - s, width, 42.8);
        filter(
            out,
            state,
            Target::Filter(index + 1),
            rect,
            label,
            state.filter == index + 1,
        );
        x += width + 7.2;
    }
    if state.hidden_count() > 0 {
        let label = language.format("hidden", &[("count", &state.hidden_count().to_string())]);
        let rect = Rect::new(
            x,
            171.4 - s,
            assets.width(&label, text(16., 400, 0.)) + 35.6,
            42.8,
        );
        filter(
            out,
            state,
            Target::Filter(5),
            rect,
            &label,
            state.filter == 5,
        );
        x += rect.w + 7.2;
    }
    if state.filter != 0 {
        let rect = Rect::new(x, 176.8 - s, 32., 32.);
        out.shape(rect, 16., c(0x23282c, 0.5));
        out.icon(assets, "close", rect.inset(8.), c(0xeeeeee, 0.73));
        if state.focus == Target::ClearFilter {
            out.border(rect.inset(-2.), 18., 2., WHITE);
        }
        out.control(Target::ClearFilter, rect, true);
    }
    let search = Rect::new(1532., 164.8 - s, 304., 56.);
    let focused = matches!(state.focus, Target::Search | Target::ClearSearch);
    out.shape(
        search,
        48.,
        if focused {
            c(0x303438, 1.)
        } else {
            c(0x171c20, 0.72)
        },
    );
    out.border(
        search,
        48.,
        1.,
        c(0xffffff, if focused { 0.70 } else { 0.07 }),
    );
    if focused {
        out.border(search.inset(-3.), 51., 3., c(0xffffff, 0.063));
    }
    out.icon(
        assets,
        "search",
        Rect::new(search.x + 19.4, search.y + 18.4, 19.2, 19.2),
        c(0xeeeeee, 0.73),
    );
    out.text(
        if state.query.is_empty() {
            language.t("findApp")
        } else {
            &state.query
        },
        Rect::new(
            search.x + 49.8,
            search.y + 17.8,
            if state.query.is_empty() { 234. } else { 202. },
            22.,
        ),
        text(16.8, 400, -0.02),
        c(0xeeeeee, if state.query.is_empty() { 0.73 } else { 1. }),
        false,
    );
    out.control(Target::Search, search, true);
    if !state.query.is_empty() {
        let clear = Rect::new(search.x + search.w - 45.8, search.y + 14.8, 26.4, 26.4);
        out.shape(clear, 13.2, c(0xffffff, 0.055));
        out.icon(assets, "close", clear.inset(5.2), WHITE);
        if state.focus == Target::ClearSearch {
            out.border(clear.inset(-2.), 15.2, 2., WHITE);
        }
        out.control(Target::ClearSearch, clear, true);
    }
    let favorites = state.preferences.favorites(&state.inventory.apps);
    let apps = state.apps();
    if apps.is_empty() {
        let title = if state.loading {
            language.t("findingApps")
        } else if !state.query.is_empty() {
            language.t("noMatches")
        } else if state.filter == 5 {
            language.t("noHidden")
        } else {
            language.t("roomForNew")
        };
        let hint = state.error.clone().unwrap_or_else(|| {
            if !state.query.is_empty() {
                language.format("noMatchesHint", &[("query", &state.query)])
            } else if state.filter == 5 {
                language.t("hiddenHint").into()
            } else {
                language.t("noCategoryApps").into()
            }
        });
        empty(
            out,
            state,
            assets,
            EmptyState {
                rect: Rect::new(84., 259.2 - s, 1752., 304.),
                icon: "grid",
                title,
                hint: &hint,
                action: state.error.as_ref().map(|_| Target::Retry),
            },
        );
    }
    let mut y = 270.4;
    let names = state.preferences.show_app_names;
    for row in apps.chunks(APPS_COLUMNS) {
        let row_height = FACE_HEIGHT
            + if names {
                18.4 + row
                    .iter()
                    .map(|app| caption_height(state, assets, app, 270.))
                    .fold(49.28, f32::max)
            } else {
                0.
            };
        for (column, app) in row.iter().enumerate() {
            tile(
                out,
                state,
                assets,
                app,
                Rect::new(84. + column as f32 * GRID_PITCH, y - s, 270., FACE_HEIGHT),
                names,
                favorites.contains(&app.id),
            );
        }
        y += row_height + 38.4;
    }
}

fn filter(out: &mut Scene, state: &State, target: Target, rect: Rect, label: &str, selected: bool) {
    if selected {
        out.shape(rect, 32., c(0xffffff, 0.11));
        out.border(rect, 32., 1., c(0xffffff, 0.07));
    }
    if state.focus == target && !state.pointer {
        out.border(rect.inset(-7.), 39., 2., c(0xf0f0f0, 0.95));
    }
    out.text(
        label,
        Rect::new(rect.x, rect.y + 11.4, rect.w, 20.),
        text(16., 400, 0.),
        c(
            if selected { 0xf5f5f5 } else { 0xf0f0f0 },
            if selected { 1. } else { 0.66 },
        ),
        true,
    );
    out.control(target, rect, true);
}

struct Row<'a> {
    target: Target,
    rect: Rect,
    title: &'a str,
    icon: Option<&'a str>,
    value: Option<&'a str>,
    description: Option<&'a str>,
    selected: bool,
    chevron: bool,
}
fn rotation_value_width(assets: &Assets, language: Language) -> f32 {
    ["stateOn", "stateOff"]
        .into_iter()
        .map(|key| assets.width(language.t(key), text(18.4, 450, 0.)))
        .fold(0., f32::max)
        .ceil()
}

fn row(out: &mut Scene, state: &State, assets: &Assets, row: Row<'_>) {
    let focused = state.focus == row.target;
    let bright = focused;
    let radius = if row.description.is_some() { 20. } else { 48. };
    if bright {
        out.draw.push(Draw::RoundedGradient(
            row.rect,
            radius,
            c(0xfafafa, 1.),
            c(0xe5e5e5, 1.),
        ));
    } else {
        out.shape(row.rect, radius, c(0x171c20, 0.72));
        out.border(row.rect, radius, 1., c(0xffffff, 0.07));
    }
    let color = if bright {
        c(0x2c2c2c, 1.)
    } else {
        c(0xeeeeee, 0.73)
    };
    let mut x = row.rect.x + 24.2;
    if let Some(icon) = row.icon {
        out.icon(
            assets,
            icon,
            Rect::new(x, row.rect.y + (row.rect.h - 23.2) / 2., 23.2, 23.2),
            if bright {
                c(0x444444, 1.)
            } else {
                c(0xe4e4e4, 0.61)
            },
        );
        x += 39.2;
    }
    let title_y = row.rect.y
        + if row.description.is_some() {
            17.
        } else {
            (row.rect.h - 29.12) / 2.
        };
    let compact_value_width = (row.target == Target::WallpaperCycle)
        .then(|| rotation_value_width(assets, state.preferences.language));
    let right_inset = if let Some(width) = compact_value_width {
        width + 24.2 + 24.
    } else if row.value.is_some() {
        210.
    } else if matches!(row.target, Target::UploadWallpaper | Target::WallpaperCycle) {
        24.2
    } else {
        64.
    };
    let title_width = row.rect.w - (x - row.rect.x) - right_inset;
    let title_height = if row.description.is_some() {
        assets.lines(row.title, text(20.8, 550, -0.02), title_width) as f32 * 29.12
    } else {
        29.12
    };
    out.text(
        row.title,
        Rect::new(x, title_y, title_width, title_height),
        text(20.8, 550, -0.02).leading(1.4),
        color,
        false,
    );
    if let Some(description) = row.description {
        out.text(
            description,
            Rect::new(
                x,
                title_y + title_height + 4.8,
                row.rect.w - 100.,
                (row.rect.h - 34. - title_height - 4.8).max(22.8),
            ),
            text(15.2, 400, 0.).leading(1.5),
            if bright {
                c(0x555555, 1.)
            } else {
                c(0xf5f5f5, 0.56)
            },
            false,
        );
    }
    if let Some(value) = row.value {
        let value_width = compact_value_width.unwrap_or(170.);
        out.text(
            value,
            Rect::new(
                row.rect.x + row.rect.w - value_width - 24.2 - if row.chevron { 34.4 } else { 0. },
                row.rect.y + (row.rect.h - 23.) / 2.,
                value_width,
                24.,
            ),
            text(18.4, 450, 0.).right(),
            if bright {
                c(0x444444, 1.)
            } else {
                c(0xeeeeee, 0.73)
            },
            false,
        );
    }
    if row.chevron || row.selected {
        out.icon(
            assets,
            if row.selected { "check" } else { "right" },
            Rect::new(
                row.rect.x + row.rect.w - 42.6,
                row.rect.y + (row.rect.h - 18.4) / 2.,
                18.4,
                18.4,
            ),
            if bright {
                c(0x444444, 1.)
            } else {
                c(0xe4e4e4, 0.61)
            },
        );
    }
    let scrollable = !matches!(row.target, Target::UploadWallpaper | Target::WallpaperCycle);
    out.control(row.target, row.rect, scrollable);
}

fn weather_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherToggle,
            rect: Rect::new(640., 164.8 - s, 1120., 72.),
            title: language.t("showWidget"),
            icon: Some("eye"),
            value: Some(if state.preferences.show_weather {
                language.t("stateOn")
            } else {
                language.t("stateOff")
            }),
            description: None,
            selected: false,
            chevron: false,
        },
    );
    let city = state
        .preferences
        .weather_location
        .as_ref()
        .map(|place| place.name.as_str());
    let style = text(18.4, 450, 0.).leading(1.4);
    let city_lines = city.map_or(1, |city| assets.lines(city, style, 450.));
    let height = (34. + city_lines as f32 * 25.76).max(72.);
    let rect = Rect::new(640., 264.8 - s, 1120., height);
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherLocation,
            rect,
            title: language.t("location"),
            icon: Some("location"),
            value: None,
            description: None,
            selected: false,
            chevron: true,
        },
    );
    if let Some(city) = city {
        out.text(
            city,
            Rect::new(
                rect.x + rect.w - 508.6,
                rect.y + (height - city_lines as f32 * 25.76) / 2.,
                450.,
                city_lines as f32 * 25.76,
            ),
            style.right(),
            if state.focus == Target::WeatherLocation {
                c(0x444444, 1.)
            } else {
                c(0xeeeeee, 0.73)
            },
            false,
        );
    }
    let y = rect.y + height + 12.;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherTemperature,
            rect: Rect::new(640., y, 1120., 72.),
            title: language.t("temperature"),
            icon: Some("thermometer"),
            value: Some(state.preferences.temperature_unit.symbol()),
            description: None,
            selected: false,
            chevron: true,
        },
    );
}

fn location_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let mut y = 164.8 - state.scroll;
    if let Some(place) = &state.preferences.weather_location {
        let rect = Rect::new(640., y, 1120., 76.2);
        out.shape(rect, 20., c(0x171c20, 0.72));
        out.border(rect, 20., 1., c(0xffffff, 0.04));
        out.icon(
            assets,
            "location",
            Rect::new(664.2, y + 29.1, 18., 18.),
            c(0xcccccc, 1.),
        );
        out.text(
            &place.name,
            Rect::new(698.2, y + 17., 920., 25.),
            text(18.4, 500, 0.),
            WHITE,
            false,
        );
        out.text(
            place.detail(),
            Rect::new(698.2, y + 42.3, 920., 22.),
            text(14.4, 400, 0.),
            c(0xf5f5f5, 0.56),
            false,
        );
        let clear = Rect::new(1695.8, y + 18.1, 40., 40.);
        out.shape(clear, 20., c(0x23282c, 0.5));
        out.border(clear, 20., 1., c(0xffffff, 0.07));
        out.icon(assets, "close", clear.inset(12.), WHITE);
        if state.focus == Target::WeatherClear {
            out.border(clear.inset(-2.), 22., 2., c(0xf0f0f0, 0.95));
        }
        out.control(Target::WeatherClear, clear, true);
        y += 96.2;
    }
    let query = Rect::new(640., y, 1044., 64.);
    let focused = state.focus == Target::WeatherQuery;
    out.shape(
        query,
        48.,
        if focused {
            c(0x303438, 1.)
        } else {
            c(0x171c20, 0.72)
        },
    );
    out.border(
        query,
        48.,
        1.,
        c(0xffffff, if focused { 0.70 } else { 0.07 }),
    );
    if focused {
        out.border(query.inset(-3.), 51., 3., c(0xffffff, 0.063));
    }
    out.icon(
        assets,
        "search",
        Rect::new(664.2, y + 21.2, 21.6, 21.6),
        c(0xeeeeee, 0.73),
    );
    out.text(
        if state.weather.query.is_empty() {
            language.t("cityPostcode")
        } else {
            &state.weather.query
        },
        Rect::new(699.4, y + 20.9, 950., 24.),
        text(18.4, 400, -0.02),
        c(0xeeeeee, 0.73),
        false,
    );
    out.control(Target::WeatherQuery, query, true);
    let search = Rect::new(1696., y, 64., 64.);
    out.shape(search, 32., c(0x171c20, 0.72));
    out.border(search, 32., 1., c(0xffffff, 0.07));
    out.icon(
        assets,
        if state.weather.searching {
            "refresh"
        } else {
            "search"
        },
        search.inset(20.8),
        WHITE,
    );
    if state.focus == Target::WeatherSearch {
        out.border(search.inset(-7.), 39., 2., c(0xf0f0f0, 0.95));
    }
    out.control(Target::WeatherSearch, search, true);
    y += 84.;
    if state.weather.searching || state.weather.message.is_some() {
        out.text(
            if state.weather.searching {
                language.t("searching")
            } else {
                language.t(state.weather.message.as_deref().unwrap_or(""))
            },
            Rect::new(664.2, y, 1071.6, 58.),
            text(16., 400, 0.).leading(1.5),
            c(0xe0e0e0, 0.68),
            false,
        );
    }
    for (index, place) in state.weather.results.iter().enumerate() {
        let detail = place.detail();
        let height = (34.
            + assets.lines(&place.name, text(20.8, 550, -0.02), 992.6) as f32 * 29.12
            + 4.8
            + assets.lines(&detail, text(15.2, 400, 0.), 1020.) as f32 * 22.8)
            .max(90.7);
        let rect = Rect::new(640., y, 1120., height);
        row(
            out,
            state,
            assets,
            Row {
                target: Target::WeatherResult(index),
                rect,
                title: &place.name,
                icon: Some("location"),
                value: None,
                description: Some(&detail),
                selected: false,
                chevron: false,
            },
        );
        if state
            .preferences
            .weather_location
            .as_ref()
            .is_some_and(|saved| saved.key() == place.key())
        {
            out.icon(
                assets,
                "check",
                Rect::new(1717.4, y + (height - 18.4) / 2., 18.4, 18.4),
                if state.focus == Target::WeatherResult(index) {
                    c(0x444444, 1.)
                } else {
                    c(0xe4e4e4, 0.61)
                },
            );
        }
        y += height + 12.;
    }
}

fn settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    for (index, key) in ["interface", "background", "weather", "inputs", "system"]
        .into_iter()
        .enumerate()
    {
        let label = language.t(key);
        let rect = Rect::new(160., 164.8 + index as f32 * 84., 403.2, 72.);
        let focused = state.focus == Target::Sidebar(index);
        out.pill(rect, focused, state.section == index);
        let color = if focused {
            c(0x2c2c2c, 1.)
        } else if state.section == index {
            WHITE
        } else {
            c(0xeeeeee, 0.73)
        };
        if state.section == index && state.subpage.is_some() {
            let clock_path = match state.subpage {
                Some(Subpage::ClockFormat) => Some(format!(
                    "{} › {}",
                    language.t("clock"),
                    language.t("clockFormat")
                )),
                Some(Subpage::TimeFormat) => Some(format!(
                    "{} › {}",
                    language.t("clock"),
                    language.t("timeFormat")
                )),
                _ => None,
            };
            let sub = match &state.subpage {
                Some(Subpage::AppNames) => language.t("appNames"),
                Some(Subpage::Greeting) => language.t("greeting"),
                Some(Subpage::Clock) => language.t("clock"),
                Some(Subpage::ClockFormat | Subpage::TimeFormat) => clock_path.as_deref().unwrap(),
                Some(Subpage::Input(id)) => state
                    .inventory
                    .inputs
                    .iter()
                    .find(|input| &input.id == id)
                    .map_or(language.t("inputs"), |input| {
                        state.preferences.input_title(input)
                    }),
                Some(Subpage::WeatherLocation) => language.t("location"),
                Some(Subpage::Temperature) => language.t("temperature"),
                Some(Subpage::Language) => language.t("language"),
                Some(Subpage::TextRendering) => language.t("textRendering"),
                None => "",
            };
            let parent_width = assets.width(label, text(20.8, 550, -0.02)) + 2.;
            out.text(
                label,
                Rect::new(rect.x + 24.2, rect.y + 23.5, parent_width, 25.),
                text(20.8, 550, -0.02),
                [color[0], color[1], color[2], color[3] * 0.7],
                false,
            );
            out.icon(
                assets,
                "right",
                Rect::new(rect.x + 24.2 + parent_width + 5., rect.y + 27., 17., 17.),
                [color[0], color[1], color[2], color[3] * 0.6],
            );
            let sub_width = rect.w - 24.2 - parent_width - 28. - 24.2;
            let mut sub_style = text(20.8, 550, -0.02);
            if clock_path.is_some() {
                sub_style.size *= (sub_width / assets.width(sub, sub_style)).min(1.);
            }
            out.text(
                sub,
                Rect::new(
                    rect.x + 24.2 + parent_width + 28.,
                    rect.y + 23.5,
                    sub_width,
                    25.,
                ),
                sub_style,
                color,
                false,
            );
        } else {
            out.text(
                label,
                Rect::new(rect.x + 24.2, rect.y + 23.5, 330., 25.),
                text(20.8, 550, -0.02),
                color,
                false,
            );
            if state.section == index {
                out.icon(
                    assets,
                    "right",
                    Rect::new(rect.x + rect.w - 42.6, rect.y + 26.8, 18.4, 18.4),
                    if focused {
                        c(0x2c2c2c, 0.6)
                    } else {
                        c(0xeeeeee, 0.5)
                    },
                );
            }
        }
        out.control(Target::Sidebar(index), rect, false);
    }
    if let Some(subpage) = &state.subpage {
        let start = out.draw.len();
        if matches!(
            subpage,
            Subpage::WeatherLocation | Subpage::Greeting | Subpage::AppNames | Subpage::Clock
        ) {
            match subpage {
                Subpage::WeatherLocation => location_settings(out, state, assets),
                Subpage::Greeting => greeting_settings(out, state, assets),
                Subpage::AppNames => app_name_settings(out, state, assets),
                Subpage::Clock => clock_settings(out, state, assets),
                _ => unreachable!(),
            }
            if let Some(opened) = state.panel_started {
                out.fade(start, (opened.elapsed().as_secs_f32() / 0.15).min(1.), 1.);
            }
            return;
        }
        let options: Vec<(&str, Option<&str>, bool)> = match subpage {
            Subpage::ClockFormat => vec![
                (language.t("hour12"), None, !state.preferences.hour_24),
                (language.t("hour24"), None, state.preferences.hour_24),
            ],
            Subpage::TimeFormat => vec![
                ("HH:MM", None, !state.preferences.show_seconds),
                ("HH:MM:SS", None, state.preferences.show_seconds),
            ],
            Subpage::Input(id) => {
                let current = state
                    .preferences
                    .input_modes
                    .get(id)
                    .copied()
                    .unwrap_or_default();
                vec![
                    (
                        language.t("stateOn"),
                        Some(language.t("inputAlways")),
                        current == InputMode::On,
                    ),
                    (
                        language.t("stateOff"),
                        Some(language.t("inputNever")),
                        current == InputMode::Off,
                    ),
                    (
                        language.t("stateAuto"),
                        Some(language.t("inputWhenConnected")),
                        current == InputMode::Auto,
                    ),
                ]
            }
            Subpage::Temperature => vec![
                (
                    language.t("celsius"),
                    None,
                    state.preferences.temperature_unit == Unit::Celsius,
                ),
                (
                    language.t("fahrenheit"),
                    None,
                    state.preferences.temperature_unit == Unit::Fahrenheit,
                ),
            ],
            Subpage::WeatherLocation | Subpage::Greeting | Subpage::AppNames | Subpage::Clock => {
                unreachable!()
            }
            Subpage::Language => Language::ALL
                .into_iter()
                .map(|option| (option.name(), None, option == language))
                .collect(),
            Subpage::TextRendering => crate::text_rendering::TextRendering::ALL
                .into_iter()
                .map(|mode| {
                    (
                        language.t(mode.label_key()),
                        Some(language.t("textRenderingRestart")),
                        mode == state.preferences.text_rendering,
                    )
                })
                .collect(),
        };
        for (index, (title, description, selected)) in options.into_iter().enumerate() {
            let h = if description.is_some() { 90.7 } else { 72. };
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::Choice(index),
                    rect: Rect::new(640., 164.8 + index as f32 * (h + 12.) - s, 1120., h),
                    title,
                    icon: None,
                    value: None,
                    description,
                    selected,
                    chevron: false,
                },
            );
        }
        if let Subpage::Input(id) = subpage {
            input_name_rows(out, state, assets, id, 164.8 + 3. * 102.7 + 16. - s);
        }
        if let Some(opened) = state.panel_started {
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.15).min(1.), 1.);
        }
        return;
    }
    match state.section {
        0 => {
            for (position, (index, key, icon, value, chevron)) in [
                (0, "appNames", "type", None, true),
                (1, "clock", "clock", None, true),
                (
                    2,
                    "reducedMotion",
                    "motion",
                    Some(if state.preferences.reduced_motion {
                        language.t("stateOn")
                    } else {
                        language.t("stateOff")
                    }),
                    false,
                ),
                (4, "language", "language", Some(language.name()), true),
                (
                    6,
                    "greeting",
                    "type",
                    Some(if state.preferences.greeting_name.trim().is_empty() {
                        language.t("stateOff")
                    } else {
                        state.preferences.greeting_name.as_str()
                    }),
                    true,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Setting(index),
                        rect: Rect::new(640., 164.8 + position as f32 * 84. - s, 1120., 72.),
                        title: language.t(key),
                        icon: Some(icon),
                        value,
                        description: None,
                        selected: false,
                        chevron,
                    },
                );
            }
        }
        1 => {
            let upload_label = language.t("uploadWallpaper");
            let upload_width = (assets.width(upload_label, text(20.8, 550, -0.02)) + 88.)
                .ceil()
                .clamp(240., 520.);
            let cycle_label = language.t("wallpaperCycle");
            let cycle_width = (assets.width(cycle_label, text(20.8, 550, -0.02))
                + rotation_value_width(assets, language)
                + 111.6)
                .ceil()
                .min(520.);
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::WallpaperCycle,
                    rect: Rect::new(
                        1760. - upload_width - 20. - cycle_width,
                        164.8,
                        cycle_width,
                        64.,
                    ),
                    title: cycle_label,
                    icon: Some(if state.preferences.wallpaper_cycle {
                        "refresh"
                    } else {
                        "refresh-off"
                    }),
                    value: Some(language.t(if state.preferences.wallpaper_cycle {
                        "stateOn"
                    } else {
                        "stateOff"
                    })),
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::UploadWallpaper,
                    rect: Rect::new(1760. - upload_width, 164.8, upload_width, 64.),
                    title: upload_label,
                    icon: Some("upload"),
                    value: None,
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
            out.draw
                .push(Draw::Clip(Some(state.backgrounds_viewport())));
            for index in 0..state.background_count() {
                let id = state.background_id(index).unwrap();
                let custom = state.wallpapers.iter().find(|item| item.id == id);
                let rect = Rect::new(
                    640. + (index % 3) as f32 * 380.,
                    252.8 + (index / 3) as f32 * 226.5 - s,
                    360.,
                    202.5,
                );
                out.shadow(rect, 16., 15., 3., 0.13);
                out.image(
                    custom.map_or_else(
                        || assets.path(format!("thumbnails/{id}.jpg")),
                        |item| item.thumbnail(),
                    ),
                    rect,
                    16.,
                    false,
                );
                out.border(rect, 16., 1., c(0xffffff, 0.086));
                if state.preferences.background == id {
                    out.border(rect.inset(-2.), 18., 2., c(0xdedede, 1.));
                    out.border(rect.inset(-7.), 23., 3., c(0xdedede, 0.10));
                    let check = Rect::new(rect.x + rect.w - 28., rect.y + rect.h - 28., 21.6, 21.6);
                    out.shape(check, 10.8, c(0xf3f3f3, 1.));
                    out.icon(assets, "check", check.inset(3.6), c(0x333333, 1.));
                }
                if state.focus == Target::Background(index) {
                    out.ring(rect, 16.);
                }
                out.control(Target::Background(index), rect, true);
            }
            out.draw.push(Draw::Clip(Some(state.content_viewport())));
        }
        WEATHER_SECTION => weather_settings(out, state, assets),
        SYSTEM_SECTION => {
            let version_rect = Rect::new(640., 164.8 - s, 1120., 72.);
            out.shape(version_rect, 48., c(0x171c20, 0.3));
            out.border(version_rect, 48., 1., c(0xffffff, 0.045));
            out.text(
                format!("Luma v{}", assets.version.trim()),
                Rect::new(664.2, version_rect.y + (72. - 29.12) / 2., 1031.8, 29.12),
                text(20.8, 550, -0.02).leading(1.4),
                c(0xeeeeee, 0.5),
                false,
            );
            for (position, (index, title, icon, value, chevron)) in [
                (
                    3,
                    language.t(if state.loading {
                        "refreshNotice"
                    } else {
                        "refreshApps"
                    }),
                    "refresh",
                    None,
                    false,
                ),
                (
                    7,
                    language.t("textRendering"),
                    "type",
                    Some(language.t(state.preferences.text_rendering.label_key())),
                    true,
                ),
                (5, language.t("quitApp"), "power", None, false),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Setting(index),
                        rect: Rect::new(640., 248.8 + position as f32 * 84. - s, 1120., 72.),
                        title,
                        icon: Some(icon),
                        value,
                        description: None,
                        selected: false,
                        chevron,
                    },
                );
            }
        }
        _ => {
            for (index, input) in state.inventory.inputs.iter().enumerate() {
                let mode = state
                    .preferences
                    .input_modes
                    .get(&input.id)
                    .copied()
                    .unwrap_or_default();
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::InputMode(input.id.clone()),
                        rect: Rect::new(640., 164.8 + index as f32 * 102.7 - s, 1120., 90.7),
                        title: state.preferences.input_title(input),
                        icon: Some(&format!(
                            "input-{}",
                            input_icon(input, state.preferences.input_name(input))
                        )),
                        value: Some(language.t(match mode {
                            InputMode::On => "stateOn",
                            InputMode::Off => "stateOff",
                            InputMode::Auto => "stateAuto",
                        })),
                        description: Some(&input_status(
                            input,
                            state.preferences.input_name(input).is_some(),
                            language,
                        )),
                        selected: false,
                        chevron: true,
                    },
                );
            }
            let mut y = 164.8 + state.inventory.inputs.len() as f32 * 102.7 + 16. - s;
            if state.inventory.inputs.is_empty() {
                out.text(
                    language.t("noInputs"),
                    Rect::new(664., 165. - s, 1000., 26.),
                    text(16.8, 400, 0.).leading(1.5),
                    c(0xf5f5f5, 0.72),
                    false,
                );
                y += 36.;
            }
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::Setting(3),
                    rect: Rect::new(640., y, 1120., 72.),
                    title: if state.loading {
                        language.t("refreshNotice")
                    } else {
                        language.t("refreshInputs")
                    },
                    icon: Some("refresh"),
                    value: None,
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
        }
    }
}

fn greeting_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let description = if state.text_input_active() && state.keyboard_field == TextField::Name {
        format!("{}|", state.greeting_name_draft)
    } else if state.preferences.greeting_name.is_empty() {
        language.t("stateOff").into()
    } else {
        state.preferences.greeting_name.clone()
    };
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingName,
            rect: Rect::new(640., 164.8 - state.scroll, 1120., 90.7),
            title: language.t("name"),
            icon: Some("type"),
            value: None,
            description: Some(&description),
            selected: false,
            chevron: true,
        },
    );
    // Always present so completing name entry cannot move a clicked control.
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingNameClear,
            rect: Rect::new(640., 267.5 - state.scroll, 1120., 72.),
            title: language.t("clearName"),
            icon: Some("refresh"),
            value: None,
            description: None,
            selected: false,
            chevron: false,
        },
    );
}

fn clock_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, (target, key, value)) in [
        (
            Target::ClockFormat,
            "clockFormat",
            language.t(if state.preferences.hour_24 {
                "hour24"
            } else {
                "hour12"
            }),
        ),
        (
            Target::ClockTimeFormat,
            "timeFormat",
            if state.preferences.show_seconds {
                "HH:MM:SS"
            } else {
                "HH:MM"
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - state.scroll, 1120., 72.),
                title: language.t(key),
                icon: None,
                value: Some(value),
                description: None,
                selected: false,
                chevron: true,
            },
        );
    }
}

fn app_name_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, (target, title, enabled)) in [
        (
            Target::AppNamesHome,
            "showOnHome",
            state.preferences.show_home_names,
        ),
        (
            Target::AppNamesApps,
            "showOnApps",
            state.preferences.show_app_names,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - state.scroll, 1120., 72.),
                title: language.t(title),
                icon: None,
                value: Some(language.t(if enabled { "stateOn" } else { "stateOff" })),
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
}

fn input_name_rows(out: &mut Scene, state: &State, assets: &Assets, id: &str, y: f32) {
    let language = state.preferences.language;
    let custom = state.preferences.input_names.get(id);
    let editing = state.text_input_active() && state.keyboard_field == TextField::InputName;
    let description = if editing {
        format!("{}|", state.input_name)
    } else {
        custom
            .cloned()
            .unwrap_or_else(|| language.t("inputNameHint").into())
    };
    row(
        out,
        state,
        assets,
        Row {
            target: Target::InputRename,
            rect: Rect::new(640., y, 1120., 90.7),
            title: language.t("renameInput"),
            icon: Some("type"),
            value: None,
            description: Some(&description),
            selected: false,
            chevron: false,
        },
    );
    if custom.is_some() {
        row(
            out,
            state,
            assets,
            Row {
                target: Target::InputNameReset,
                rect: Rect::new(640., y + 102.7, 1120., 72.),
                title: language.t("clearName"),
                icon: Some("refresh"),
                value: None,
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
}

/// A custom name such as "PS5" can choose the icon; otherwise the TV's label.
fn input_icon(input: &Input, custom: Option<&str>) -> &'static str {
    custom
        .map(device_icon)
        .filter(|icon| *icon != "hdmi")
        .unwrap_or_else(|| device_icon(&input.title))
}

fn device_icon(name: &str) -> &'static str {
    let name = name.to_lowercase().replace(' ', "");
    if name.contains("playstation") || ["ps1", "ps2", "ps3", "ps4", "ps5"].contains(&name.as_str())
    {
        "playstation"
    } else if name.contains("appletv") {
        "apple-tv"
    } else if name.contains("usb") {
        "usb"
    } else if ["xbox", "switch", "console"]
        .iter()
        .any(|part| name.contains(part))
    {
        "console"
    } else if ["pc", "computer", "laptop"]
        .iter()
        .any(|part| name.contains(part))
    {
        "computer"
    } else if ["stream", "fire", "roku"]
        .iter()
        .any(|part| name.contains(part))
    {
        "streamer"
    } else {
        "hdmi"
    }
}
/// Detail under an input title: the TV's own label (when connected and not just
/// the port), the port when a custom name replaces it, then the status.
pub fn input_status(input: &Input, renamed: bool, language: Language) -> String {
    let mut parts = vec![];
    if input.connected == Some(true) && input.title != input.port {
        parts.push(input.title.as_str());
    }
    if renamed {
        parts.push(input.port.as_str());
    }
    parts.push(match input.connected {
        Some(false) => language.t("noDevice"),
        None => language.t("externalInput"),
        Some(true) => language.t("connected"),
    });
    parts.join(" · ")
}

struct EmptyState<'a> {
    rect: Rect,
    icon: &'a str,
    title: &'a str,
    hint: &'a str,
    action: Option<Target>,
}
fn empty(out: &mut Scene, state: &State, assets: &Assets, content: EmptyState<'_>) {
    let EmptyState {
        rect,
        icon,
        title,
        hint,
        action,
    } = content;
    out.shape(rect, 17.6, c(0xffffff, 0.02));
    out.border(rect, 17.6, 1., c(0xffffff, 0.05));
    let center = rect.x + rect.w / 2.;
    let top = rect.y
        + if action.is_some() {
            33.
        } else {
            (rect.h - 123.2) / 2.
        };
    out.icon(
        assets,
        icon,
        Rect::new(center - 17.6, top, 35.2, 35.2),
        c(0xd8d8d8, 0.60),
    );
    out.text(
        title,
        Rect::new(rect.x + 32., top + 52., rect.w - 64., 31.),
        text(24., 550, -0.035),
        WHITE,
        true,
    );
    out.text(
        hint,
        Rect::new(center - 352., top + 97.6, 704., 64.),
        text(16., 400, 0.).leading(1.6),
        c(0xf5f5f5, 0.72),
        true,
    );
    if let Some(target) = action {
        let label = state.preferences.language.t(if target == Target::Retry {
            "retry"
        } else {
            "exploreApps"
        });
        let width = assets.width(label, text(15.2, 600, 0.)) + 72.;
        let button = Rect::new(center - width / 2., top + 144.8, width, 46.8);
        out.shape(button, 11.2, c(0xededed, 1.));
        if state.focus == target {
            out.border(button.inset(-7.), 18.2, 2., WHITE);
        }
        out.text(
            label,
            Rect::new(button.x + 20.8, button.y + 14.4, button.w - 72., 18.4),
            text(15.2, 600, 0.),
            c(0x242424, 1.),
            false,
        );
        out.icon(
            assets,
            "arrow",
            Rect::new(button.x + button.w - 38.4, button.y + 14.6, 17.6, 17.6),
            c(0x242424, 1.),
        );
        out.control(target, button, true);
    }
}

fn options(out: &mut Scene, state: &State, assets: &Assets, id: &str, anchor: Option<Rect>) {
    let language = state.preferences.language;
    let background = matches!(state.overlay, Some(Overlay::BackgroundOptions { .. }));
    let app = state.inventory.apps.iter().find(|app| app.id == id);
    let favorite = state
        .preferences
        .favorites(&state.inventory.apps)
        .iter()
        .any(|item| item == id);
    let deletable = if background {
        state.can_delete_background(id) && state.deleting_wallpaper.is_none()
    } else {
        crate::platform::can_uninstall(id)
    };
    let height = if background {
        if deletable { 176. } else { 99.2 }
    } else if deletable {
        329.6
    } else {
        252.8
    };
    let card = anchor.unwrap_or(Rect::new(460., 380., 256., 153.6));
    let x = if card.x + card.w + 32. + 520. <= 1836. {
        card.x + card.w + 32.
    } else {
        (card.x - 552.).max(84.)
    };
    let panel = Rect::new(x, card.y.clamp(164., 974. - height), 520., height);
    // Lift the selected card out of the dimmed grid without moving its hit region.
    if background {
        let path = state
            .wallpapers
            .iter()
            .find(|item| item.id == id)
            .map_or_else(
                || assets.path(format!("thumbnails/{id}.jpg")),
                |item| item.thumbnail(),
            );
        out.image(path, card, 16., false);
        out.border(card, 16., 1., c(0xffffff, 0.086));
    } else if let Some(app) = app {
        app_face(out, assets, app, card, language, 19.2);
        if favorite {
            out.icon(
                assets,
                "heart-filled",
                Rect::new(card.x + card.w - 27.2, card.y + 11.2, 16., 16.),
                c(0xffffff, 0.9),
            );
        }
    }
    let panel_start = out.draw.len();
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 120., 25., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.96),
        c(0x24282d, 0.988),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    let hidden = state.preferences.hidden.iter().any(|item| item == id);
    let actions = if background {
        let mut actions = vec![(Target::UseBackground, language.t("useBackground"))];
        if deletable {
            actions.push((Target::DeleteBackground, language.t("deleteBackground")));
        }
        actions
    } else {
        vec![
            (Target::OpenApp, language.t("open")),
            (
                Target::Option(0),
                if favorite {
                    language.t("removeFavorite")
                } else {
                    language.t("addFavorite")
                },
            ),
            (
                Target::Option(1),
                language.t(if hidden { "showApp" } else { "hideApp" }),
            ),
            (Target::Option(2), language.t("deleteApp")),
        ]
        .into_iter()
        .take(if deletable { 4 } else { 3 })
        .collect()
    };
    for (row, (target, label)) in actions.into_iter().enumerate() {
        let rect = Rect::new(
            panel.x + 16.,
            panel.y + 16. + row as f32 * 76.8,
            panel.w - 32.,
            67.2,
        );
        let focused = state.focus == target;
        let destructive = matches!(target, Target::Option(2) | Target::DeleteBackground);
        let label_style = text(20.8, 600, -0.02);
        let label_height = label_style.size * label_style.line_height;
        let label_color = if destructive || !focused {
            WHITE
        } else {
            c(0x242424, 1.)
        };
        out.shape(
            rect,
            12.,
            if destructive {
                c(0xd63b35, if focused { 1. } else { 0.25 })
            } else {
                c(0xffffff, if focused { 0.96 } else { 0.10 })
            },
        );
        out.text(
            label,
            Rect::new(
                rect.x + 24.,
                rect.y + (rect.h - label_height) / 2.,
                rect.w - 88.,
                label_height,
            ),
            label_style,
            label_color,
            false,
        );
        out.icon(
            assets,
            match &target {
                Target::OpenApp => "arrow",
                Target::UseBackground => "check",
                Target::Option(0) => {
                    if favorite {
                        "heart-filled"
                    } else {
                        "heart"
                    }
                }
                Target::Option(1) => {
                    if hidden {
                        "eye"
                    } else {
                        "eye-off"
                    }
                }
                _ => "circle-minus",
            },
            Rect::new(rect.x + rect.w - 45.6, rect.y + 22.8, 21.6, 21.6),
            label_color,
        );
        out.control(target, rect, false);
    }
    if let Some(opened) = state.panel_started {
        out.fade(
            panel_start,
            (opened.elapsed().as_secs_f32() / 0.12).min(1.),
            1.,
        );
    }
}

fn delete_confirmation(out: &mut Scene, state: &State, assets: &Assets, id: &str) {
    let language = state.preferences.language;
    let background = matches!(state.overlay, Some(Overlay::BackgroundDelete { .. }));
    let deleting = if background {
        state.deleting_wallpaper.as_deref() == Some(id)
    } else {
        state.uninstalling.as_deref() == Some(id)
    };
    let warning_key = if background {
        "deleteBackgroundWarning"
    } else {
        "deleteAppWarning"
    };
    let deleting_key = if background {
        "deletingBackground"
    } else {
        "deletingApp"
    };
    let delete_key = if background {
        "deleteBackground"
    } else {
        "deleteApp"
    };
    let title = state
        .inventory
        .apps
        .iter()
        .find(|app| app.id == id)
        .map_or(id, |app| app.title.as_str());
    let body_style = text(20.8, 600, -0.02).leading(1.5);
    let body = language.format(
        if deleting { deleting_key } else { warning_key },
        &[("app", title)],
    );
    // Status changes must not move the confirmation buttons under the pointer.
    let warning = language.format(warning_key, &[("app", title)]);
    let body_height = assets
        .lines(&warning, body_style, 568.)
        .max(assets.lines(language.t(deleting_key), body_style, 568.))
        .max(1) as f32
        * 31.2;
    let height = 244. + body_height;
    let panel = Rect::new(640., (1080. - height) / 2., 640., height);
    out.modal = Some(panel);
    out.shadow(panel, 28., 100., 24., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        28.,
        c(0x34383b, 0.98),
        c(0x24282d, 0.99),
    ));
    out.border(panel, 28., 1., c(0xffffff, 0.12));
    out.text(
        body,
        Rect::new(panel.x + 36., panel.y + 36., 568., body_height),
        body_style,
        c(0xf5f5f5, 0.75),
        true,
    );
    for (index, (target, label)) in [
        (Target::ConfirmDelete, delete_key),
        (Target::CancelDelete, "cancel"),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            panel.x + 28.,
            panel.y + panel.h - 184. + index as f32 * 80.,
            panel.w - 56.,
            68.,
        );
        let focused = state.focus == target;
        let disabled = target == Target::ConfirmDelete && deleting;
        let label_style = text(20.8, 600, -0.02);
        let label_height = label_style.size * label_style.line_height;
        out.shape(
            rect,
            14.,
            if index == 0 {
                c(
                    0xd63b35,
                    if disabled {
                        0.15
                    } else if focused {
                        1.
                    } else {
                        0.25
                    },
                )
            } else {
                c(0xffffff, if focused { 0.96 } else { 0.10 })
            },
        );
        out.text(
            language.t(label),
            Rect::new(
                rect.x + 20.,
                rect.y + (rect.h - label_height) / 2.,
                rect.w - 40.,
                label_height,
            ),
            label_style,
            if index == 1 && focused {
                c(0x242424, 1.)
            } else {
                WHITE
            },
            true,
        );
        if !disabled {
            out.control(target, rect, false);
        }
    }
}

fn keyboard(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let panel = Rect::new(350., 290., 1220., 530.);
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 90., 20., 0.45);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.98),
        c(0x24282d, 1.),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    out.text(
        match state.keyboard_field {
            TextField::Location => language.t("cityPostcode"),
            TextField::Name => language.t("greetingName"),
            TextField::InputName => language.t("inputName"),
            TextField::Apps => language.t("findApp"),
        },
        Rect::new(389., 324., 1000., 44.),
        text(37.6, 500, -0.055).leading(1.1),
        WHITE,
        false,
    );
    let close = Rect::new(1497., 328., 33.6, 33.6);
    out.icon(assets, "close", close.inset(7.6), WHITE);
    out.control(Target::CloseDialog, close, false);
    if state.focus == Target::CloseDialog {
        out.border(close, 16.8, 2., WHITE);
    }
    out.shape(Rect::new(389., 390., 1142., 56.), 28., c(0x171c20, 0.72));
    out.icon(
        assets,
        "search",
        Rect::new(409., 408., 20., 20.),
        c(0xeeeeee, 0.73),
    );
    out.text(
        format!("{}|", state.editing_text()),
        Rect::new(446., 406., 1058., 25.),
        text(20.8, 400, -0.02),
        WHITE,
        false,
    );
    for (index, key) in KEYS.iter().enumerate() {
        let label = match *key {
            "Space" => language.t("space"),
            "Delete" => language.t("delete"),
            "Clear" => language.t("clear"),
            "Done" => language.t("done"),
            _ => key,
        };
        let rect = Rect::new(
            389. + (index % 10) as f32 * 115.6,
            474. + (index / 10) as f32 * 74.,
            101.6,
            60.,
        );
        let focused = state.focus == Target::Key(index);
        out.pill(rect, focused, false);
        let mut style = text(18.4, 550, -0.02);
        let width = assets.width(label, style);
        if width > rect.w - 12. {
            style.size *= (rect.w - 12.) / width;
        }
        out.text(
            label,
            Rect::new(rect.x, rect.y + 18., rect.w, 25.),
            style,
            if focused { c(0x2c2c2c, 1.) } else { WHITE },
            true,
        );
        out.control(Target::Key(index), rect, false);
    }
}

fn toast(out: &mut Scene, state: &State, assets: &Assets, notice: &str) {
    let start = out.draw.len();
    let notice = notice.trim_end_matches(['.', '。']);
    let style = text(20.8, 550, -0.02);
    let width = (assets.width(notice, style).ceil() + 42.).clamp(42., 1856.);
    let text_height =
        assets.lines(notice, style, width - 40.) as f32 * style.size * style.line_height;
    let height = 20. + text_height;
    let rect = Rect::new(960. - width / 2., 1080. - 51.2 - height, width, height);
    out.shadow(rect, 16., 18., 4., 0.06);
    out.shape(rect, 16., c(0x171c20, 0.72));
    out.border(rect, 16., 1., c(0xffffff, 0.07));
    out.text(
        notice,
        Rect::new(rect.x + 20., rect.y + 10., width - 40., text_height),
        style,
        WHITE,
        true,
    );
    if state.focus == Target::DismissNotice {
        out.border(rect, 16., 2., WHITE);
    }
    if state.overlay.is_none() {
        out.control(Target::DismissNotice, rect, false);
    }
    if !state.preferences.reduced_motion
        && let Some((_, opened)) = &state.notice
    {
        out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
    }
}

/// A corner-less, borderless fill of the whole logical screen.
fn full_screen_fill(draw: &Draw) -> bool {
    let whole = |r: &Rect| r.x <= 0. && r.y <= 0. && r.x + r.w >= 1920. && r.y + r.h >= 1080.;
    match draw {
        Draw::Shape(rect, radius, border, _) => whole(rect) && *radius <= 0. && *border <= 0.,
        Draw::RoundedGradient(rect, radius, ..) => whole(rect) && *radius <= 0.,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Preferences, demo_inventory};

    fn assets() -> Assets {
        Assets {
            root: PathBuf::new(),
            version: "test".into(),
            metrics: BTreeMap::from([
                ("display-500".into(), BTreeMap::from([('W', 1.)])),
                ("500".into(), BTreeMap::from([('W', 1.2)])),
                (
                    "fallback-500".into(),
                    BTreeMap::from([('界', 1.), ('한', 1.)]),
                ),
            ]),
            artwork: crate::app_artwork::Service::new(false),
        }
    }

    #[test]
    fn auto_rotation_keeps_a_compact_gap_and_right_aligned_state() {
        let assets = assets();
        let mut state = State::new(Preferences::default());
        state.open(2);
        state.section = 1;
        let label = state.preferences.language.t("wallpaperCycle");
        let label_width = assets.width(label, text(20.8, 550, -0.02));
        for enabled in [false, true] {
            state.preferences.wallpaper_cycle = enabled;
            let mut scene = Scene::new();
            settings(&mut scene, &state, &assets);
            let button = scene
                .controls
                .iter()
                .find(|control| control.target == Target::WallpaperCycle)
                .unwrap()
                .rect;
            let title = scene
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(value, rect, ..) if value == label => Some(rect),
                    _ => None,
                })
                .unwrap();
            let state_label =
                state
                    .preferences
                    .language
                    .t(if enabled { "stateOn" } else { "stateOff" });
            let (value, style) = scene
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(label, rect, style, ..) if label == state_label => {
                        Some((rect, style))
                    }
                    _ => None,
                })
                .unwrap();
            assert!(style.align_right);
            assert!((button.x + button.w - value.x - value.w - 24.2).abs() < 0.001);
            assert!((24. ..25.).contains(&(value.x - title.x - label_width)));
            assert!(button.w < 320.);
        }
    }

    #[test]
    fn short_greetings_keep_their_original_style_and_geometry() {
        let assets = assets();
        for name in ["", "Ana"] {
            let state = State::new(Preferences {
                greeting_name: name.into(),
                show_weather: false,
                ..Preferences::default()
            });
            let mut scene = Scene::new();
            home(&mut scene, &state, &assets, OffsetDateTime::UNIX_EPOCH);
            let greeting = state.preferences.language.greeting(0, name);
            let (rect, style) = scene
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(value, rect, style, ..) if value == &greeting => Some((rect, style)),
                    _ => None,
                })
                .unwrap();
            assert_eq!((rect.x, rect.y, rect.w, rect.h), (84., 265., 1500., 95.));
            assert_eq!(style.size, 90.4);
            assert_eq!(style.weight, 500);
            assert_eq!(style.tracking, -0.065);
        }
    }

    #[test]
    fn long_localized_greetings_fit_on_one_line_without_changing_the_name() {
        let assets = assets();
        let base = text(90.4, 500, -0.065).leading(1.05);
        for language in Language::ALL {
            for hour in [2, 9, 15, 20] {
                for name in [
                    "W".repeat(40),
                    "界".repeat(40),
                    "한".repeat(40),
                    "Alexandra Catherine Elizabeth Montgomery".into(),
                ] {
                    let greeting = language.greeting(hour, &name);
                    let fitted = assets.fit_text(&greeting, base, 1500.);
                    assert!(fitted.size > 0. && fitted.size < base.size);
                    assert!(assets.width(&greeting, fitted) < 1500.);
                    assert_eq!(assets.lines(&greeting, fitted, 1500.), 1);
                    assert!(greeting.contains(&name));
                }
            }
        }
    }

    #[test]
    fn fitting_remeasures_when_shrinking_changes_the_optical_font() {
        let assets = assets();
        let greeting = "W".repeat(100);
        let fitted = assets.fit_text(&greeting, text(90.4, 500, -0.065), 1500.);
        assert!(fitted.size < 32.);
        assert!(assets.width(&greeting, fitted) < 1500.);
        assert_eq!(assets.lines(&greeting, fitted, 1500.), 1);
    }

    #[test]
    fn shrinking_a_greeting_preserves_weather_favorites_and_control_positions() {
        let assets = assets();
        for show_weather in [false, true] {
            let mut state = State::new(Preferences {
                greeting_name: "Ana".into(),
                show_weather,
                ..Preferences::default()
            });
            state.set_inventory(demo_inventory(), None);
            let mut original = Scene::new();
            home(&mut original, &state, &assets, OffsetDateTime::UNIX_EPOCH);
            state.preferences.greeting_name = "W".repeat(40);
            let mut fitted = Scene::new();
            home(&mut fitted, &state, &assets, OffsetDateTime::UNIX_EPOCH);
            for (before, after) in original.draw.iter().zip(&fitted.draw) {
                if let (
                    Draw::Text(old, rect, old_style, ..),
                    Draw::Text(new, fitted_rect, style, ..),
                ) = (before, after)
                {
                    if old == new {
                        assert_eq!(
                            (rect.x, rect.y, rect.w, rect.h),
                            (fitted_rect.x, fitted_rect.y, fitted_rect.w, fitted_rect.h)
                        );
                        assert_eq!(old_style.size, style.size);
                    } else {
                        assert_eq!(
                            new,
                            &state
                                .preferences
                                .language
                                .greeting(0, &state.preferences.greeting_name)
                        );
                        assert!(
                            (rect.y + rect.h / 2. - fitted_rect.y - fitted_rect.h / 2.).abs()
                                < 0.001
                        );
                        assert!(style.size * style.line_height <= fitted_rect.h);
                    }
                }
            }
            assert_eq!(original.controls.len(), fitted.controls.len());
            for (before, after) in original.controls.iter().zip(&fitted.controls) {
                assert_eq!(before.target, after.target);
                assert_eq!(
                    (before.rect.x, before.rect.y, before.rect.w, before.rect.h),
                    (after.rect.x, after.rect.y, after.rect.w, after.rect.h)
                );
            }
        }
    }
}
