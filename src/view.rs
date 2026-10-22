//! Screen geometry and styling mirror the web UI's 1920×1080 / 16px-rem layout.
use crate::{
    i18n::Language,
    model::{App, BACKGROUNDS, CATEGORIES, Input, InputMode},
    renderer::{Color, Draw, TextStyle, WHITE},
    state::{
        APPS_COLUMNS, Control, HOME_TILE_PITCH, HOME_TILE_WIDTH, KEYS, Overlay, Page, Rect, State,
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

pub struct Assets {
    pub root: PathBuf,
    pub icons: BTreeMap<String, String>,
    pub colors: BTreeMap<String, [u8; 3]>,
    pub version: String,
    pub metrics: BTreeMap<String, BTreeMap<char, f32>>,
}
impl Assets {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        let load = |file: &str| {
            std::fs::read(root.join(file))
                .map_err(|e| format!("Native assets are missing: {e}. Run pnpm assets."))
        };
        let icons = serde_json::from_slice(&load("icons.json")?).map_err(|e| e.to_string())?;
        let colors = serde_json::from_slice(&load("colors.json")?).map_err(|e| e.to_string())?;
        let version = String::from_utf8(load("version.txt")?).map_err(|e| e.to_string())?;
        let metrics =
            serde_json::from_slice(&load("font-metrics.json")?).map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            icons,
            colors,
            version,
            metrics,
        })
    }
    fn path(&self, path: impl AsRef<Path>) -> String {
        self.root.join(path).to_string_lossy().into_owned()
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
                | Draw::TintedImage(_, _, _, _, color) => apply(color),
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
                Draw::Clip(_) | Draw::EdgeFade(_) => {}
            }
        }
    }
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
            assets.path(format!("wallpapers/{previous}.jpg")),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
        let alpha = (start.elapsed().as_secs_f32() / 0.4).min(1.);
        out.draw.push(Draw::TintedImage(
            assets.path(format!("wallpapers/{}.jpg", state.preferences.background)),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
            [1., 1., 1., alpha],
        ));
    } else {
        out.image(
            assets.path(format!("wallpapers/{}.jpg", state.preferences.background)),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
    }
    let main_bottom = if state.page == Page::Home && state.moving.is_none() {
        1053.6
    } else {
        994.4
    };
    out.draw.push(Draw::Clip(Some(Rect::new(
        64.,
        MAIN_TOP,
        1792.,
        main_bottom - MAIN_TOP,
    ))));
    if state.page == Page::Apps {
        out.draw.push(Draw::EdgeFade(Some([
            MAIN_TOP,
            152.8,
            main_bottom - 24.,
            main_bottom,
        ])));
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
    footer(&mut out, state, assets);
    if let Some(overlay @ (Overlay::Options { .. } | Overlay::Keyboard)) = &state.overlay {
        out.controls.clear();
        let start = out.draw.len();
        out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x080b12, 0.70));
        match overlay {
            Overlay::Options { id, .. } => options(&mut out, state, assets, id),
            Overlay::Keyboard => keyboard(&mut out, state, assets),
            Overlay::SystemKeyboard => {}
        }
        if let Some(opened) = state.panel_started {
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
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
    let clock = language.clock(now, state.preferences.hour_24);
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
    if rect.y + rect.h + label_height < MAIN_TOP
        || rect.y > 1054.
        || rect.x + rect.w + 40. < 64.
        || rect.x - 40. > 1856.
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
    let logos = [
        "netflix",
        "youtube",
        "apple-tv",
        "disney-plus",
        "prime-video",
        "jellyfin",
        "arte",
        "zdf",
        "das-erste",
    ];
    let logo = logos.contains(&app.brand.as_str());
    let installed = (!logo).then(|| assets.icons.get(&app.id)).flatten();
    if let Some(rgb) = installed.and_then(|_| assets.colors.get(&app.id)) {
        out.shape(
            rect,
            19.2,
            [
                rgb[0] as f32 / 255.,
                rgb[1] as f32 / 255.,
                rgb[2] as f32 / 255.,
                1.,
            ],
        );
    } else {
        let brand = if app.brand == "live-tv" {
            "generic"
        } else {
            &app.brand
        };
        out.image(
            assets.path(format!("surfaces/{brand}.png")),
            rect,
            19.2,
            false,
        );
    }
    out.border(rect, 19.2, 1., c(0xffffff, 0.063));
    if logo {
        let ratio = match app.brand.as_str() {
            "netflix" => 0.76,
            "youtube" => 0.74,
            "apple-tv" => 0.43,
            "disney-plus" | "prime-video" => 0.72,
            "jellyfin" => 0.83,
            "arte" => 0.58,
            "zdf" => 0.50,
            _ => 0.70,
        };
        let h = if app.brand == "disney-plus" {
            105.6
        } else {
            88.
        };
        out.image(
            assets.path(format!("logos/{}.lvec", app.brand)),
            Rect::new(
                rect.x + rect.w * (1. - ratio) / 2.,
                rect.y + (rect.h - h) / 2.,
                rect.w * ratio,
                h,
            ),
            0.,
            true,
        );
    } else if let Some(icon) = installed {
        out.image(
            assets.path(icon),
            Rect::new(
                rect.x + rect.w * 0.16,
                rect.y + rect.h * 0.16,
                rect.w * 0.68,
                rect.h * 0.68,
            ),
            0.,
            true,
        );
    } else {
        brand_mark(out, assets, app, rect, state.preferences.language);
    }
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
    out.text(
        &greeting,
        Rect::new(84., 265. - shift - s, 1500., 95.),
        text(90.4, 500, -0.065).leading(1.05),
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
    let names = state.preferences.show_names;
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
        let icon = input_icon(input);
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
            &input.port,
            Rect::new(rect.x + 93., rect.y + 24.3, 450., 28.8),
            text(24., 500, -0.03),
            WHITE,
            false,
        );
        out.text(
            input_status(input, language),
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
    for row in apps.chunks(APPS_COLUMNS) {
        let row_height = FACE_HEIGHT
            + 18.4
            + row
                .iter()
                .map(|app| caption_height(state, assets, app, 270.))
                .fold(49.28, f32::max);
        for (column, app) in row.iter().enumerate() {
            tile(
                out,
                state,
                assets,
                app,
                Rect::new(84. + column as f32 * GRID_PITCH, y - s, 270., FACE_HEIGHT),
                true,
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
fn row(out: &mut Scene, state: &State, assets: &Assets, row: Row<'_>) {
    let focused = state.focus == row.target;
    let bright = focused || row.selected;
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
    let title_width = row.rect.w - (x - row.rect.x) - if row.value.is_some() { 210. } else { 64. };
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
        out.text(
            value,
            Rect::new(
                row.rect.x + row.rect.w - 194.2 - if row.chevron { 34.4 } else { 0. },
                row.rect.y + (row.rect.h - 23.) / 2.,
                170.,
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
    out.control(row.target, row.rect, true);
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
    for (index, key) in ["interface", "background", "weather", "inputs"]
        .into_iter()
        .enumerate()
    {
        let label = language.t(key);
        let rect = Rect::new(160., 164.8 + index as f32 * 84. - s, 403.2, 72.);
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
            let sub = match &state.subpage {
                Some(Subpage::Clock) => language.t("clockFormat"),
                Some(Subpage::Input(id)) => state
                    .inventory
                    .inputs
                    .iter()
                    .find(|input| &input.id == id)
                    .map_or(language.t("inputs"), |input| input.port.as_str()),
                Some(Subpage::WeatherLocation) => language.t("location"),
                Some(Subpage::Temperature) => language.t("temperature"),
                Some(Subpage::Language) => language.t("language"),
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
            out.text(
                sub,
                Rect::new(rect.x + 24.2 + parent_width + 28., rect.y + 23.5, 245., 25.),
                text(20.8, 550, -0.02),
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
        out.control(Target::Sidebar(index), rect, true);
    }
    if let Some(subpage) = &state.subpage {
        let start = out.draw.len();
        if *subpage == Subpage::WeatherLocation {
            location_settings(out, state, assets);
            if let Some(opened) = state.panel_started {
                out.fade(start, (opened.elapsed().as_secs_f32() / 0.15).min(1.), 1.);
            }
            return;
        }
        let options: Vec<(&str, Option<&str>, bool)> = match subpage {
            Subpage::Clock => vec![
                (language.t("hour12"), None, !state.preferences.hour_24),
                (language.t("hour24"), None, state.preferences.hour_24),
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
            Subpage::WeatherLocation => unreachable!(),
            Subpage::Language => Language::ALL
                .into_iter()
                .map(|option| (option.name(), None, option == language))
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
        if let Some(opened) = state.panel_started {
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.15).min(1.), 1.);
        }
        return;
    }
    match state.section {
        0 => {
            for (position, (index, key, icon, value, chevron)) in [
                (
                    0,
                    "showAppNames",
                    "type",
                    if state.preferences.show_names {
                        language.t("stateOn")
                    } else {
                        language.t("stateOff")
                    },
                    false,
                ),
                (
                    1,
                    "clockFormat",
                    "clock",
                    if state.preferences.hour_24 {
                        language.t("hour24")
                    } else {
                        language.t("hour12")
                    },
                    true,
                ),
                (
                    2,
                    "reducedMotion",
                    "motion",
                    if state.preferences.reduced_motion {
                        language.t("stateOn")
                    } else {
                        language.t("stateOff")
                    },
                    false,
                ),
                (4, "language", "language", language.name(), true),
                (
                    6,
                    "greetingName",
                    "type",
                    if state.preferences.greeting_name.trim().is_empty() {
                        language.t("stateOff")
                    } else {
                        state.preferences.greeting_name.as_str()
                    },
                    false,
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
                        value: Some(value),
                        description: None,
                        selected: false,
                        chevron,
                    },
                );
            }
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::Setting(3),
                    rect: Rect::new(640., 600.8 - s, 1120., 72.),
                    title: if state.loading {
                        language.t("refreshNotice")
                    } else {
                        language.t("refreshApps")
                    },
                    icon: Some("refresh"),
                    value: None,
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
                    target: Target::Setting(5),
                    rect: Rect::new(640., 684.8 - s, 1120., 72.),
                    title: language.t("quitApp"),
                    icon: Some("power"),
                    value: None,
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
        }
        1 => {
            for (index, (id, _title)) in BACKGROUNDS.iter().enumerate() {
                let rect = Rect::new(
                    640. + (index % 3) as f32 * 380.,
                    164.8 + (index / 3) as f32 * 264.3 - s,
                    360.,
                    202.5,
                );
                out.shadow(rect, 16., 15., 3., 0.13);
                out.image(
                    assets.path(format!("thumbnails/{id}.jpg")),
                    rect,
                    16.,
                    false,
                );
                out.border(rect, 16., 1., c(0xffffff, 0.086));
                if state.preferences.background == *id {
                    out.border(rect.inset(-2.), 18., 2., c(0xdedede, 1.));
                    out.border(rect.inset(-7.), 23., 3., c(0xdedede, 0.10));
                    let check = Rect::new(rect.x + rect.w - 28., rect.y + rect.h - 28., 21.6, 21.6);
                    out.shape(check, 10.8, c(0xf3f3f3, 1.));
                    out.icon(assets, "check", check.inset(3.6), c(0x333333, 1.));
                }
                if state.focus == Target::Background(index) {
                    out.ring(rect, 16.);
                }
                out.text(
                    language.wallpaper(id),
                    Rect::new(rect.x + 4., rect.y + rect.h + 12., 352., 26.),
                    text(18.4, 500, -0.02).leading(1.4),
                    c(0xf0f0f0, 0.87),
                    false,
                );
                out.control(
                    Target::Background(index),
                    Rect::new(rect.x, rect.y, rect.w, rect.h + 37.8),
                    true,
                );
            }
        }
        WEATHER_SECTION => weather_settings(out, state, assets),
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
                        title: &input.port,
                        icon: Some(&format!("input-{}", input_icon(input))),
                        value: Some(language.t(match mode {
                            InputMode::On => "stateOn",
                            InputMode::Off => "stateOff",
                            InputMode::Auto => "stateAuto",
                        })),
                        description: Some(&input_status(input, language)),
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

fn input_icon(input: &Input) -> &'static str {
    let name = input.title.to_lowercase().replace(' ', "");
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
fn input_status(input: &Input, language: Language) -> String {
    match input.connected {
        Some(false) => language.t("noDevice").into(),
        None => language.t("externalInput").into(),
        Some(true) if input.title == input.port => language.t("connected").into(),
        Some(true) => format!("{} · {}", input.title, language.t("connected")),
    }
}

fn keycap(out: &mut Scene, label: &str, x: f32, y: f32, w: f32, small: bool) {
    let h = if small { 16. } else { 23.2 };
    out.border(
        Rect::new(x, y, w, h),
        4.8,
        1.,
        c(0xffffff, if small { 0.125 } else { 0.20 }),
    );
    out.text(
        label,
        Rect::new(x, y + if small { 2. } else { 4.2 }, w, h),
        text(if small { 8. } else { 11.52 }, 500, 0.),
        c(0xf3f3f3, 0.64),
        true,
    );
}

fn footer(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let style = text(14.4, 400, 0.005);
    if state.page == Page::Home && state.moving.is_none() {
        return;
    }
    let color = c(0xeeeeee, 0.37);
    if state.moving.is_some() {
        keycap(out, "← →", 84., 1030.4, 42., false);
        out.text(
            language.t("move"),
            Rect::new(134., 1033., 100., 20.),
            text(14.4, 400, 0.005),
            color,
            false,
        );
        let second_x = 134. + assets.width(language.t("move"), style) + 25.6;
        keycap(out, "OK/Back", second_x, 1030.4, 62., false);
        out.text(
            language.t("done"),
            Rect::new(second_x + 70., 1033., 150., 20.),
            text(14.4, 400, 0.005),
            color,
            false,
        );
    } else {
        let label = language.t(if state.page == Page::Settings {
            "select"
        } else {
            "open"
        });
        let second_x = 119. + assets.width(label, style) + 25.6;
        keycap(out, "OK", 84., 1030.4, 27., false);
        out.text(
            label,
            Rect::new(119., 1033., 100., 20.),
            text(14.4, 400, 0.005),
            color,
            false,
        );
        if state.page == Page::Settings {
            keycap(out, "Back", second_x, 1030.4, 39., false);
            out.text(
                language.t("homeLabel"),
                Rect::new(second_x + 47., 1033., 180., 20.),
                text(14.4, 400, 0.005),
                color,
                false,
            );
            out.text(
                format!("v{}", assets.version.trim()),
                Rect::new(1775., 1037., 61., 16.),
                text(12.48, 400, 0.),
                c(0xf5f5f5, 0.56),
                false,
            );
        } else {
            out.icon(
                assets,
                "more",
                Rect::new(second_x, 1032., 19.2, 19.2),
                c(0xeeeeee, 0.26),
            );
            out.text(
                language.t("options"),
                Rect::new(second_x + 27.2, 1033., 400., 20.),
                text(14.4, 400, 0.005),
                color,
                false,
            );
        }
    }
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

fn options(out: &mut Scene, state: &State, assets: &Assets, id: &str) {
    let language = state.preferences.language;
    let app = state.inventory.apps.iter().find(|app| app.id == id);
    let title = app.map_or(id, |app| app.title.as_str());
    let extra = assets
        .lines(title, text(37.6, 500, -0.055).leading(1.1), 432.)
        .saturating_sub(1) as f32
        * 41.4;
    let panel = Rect::new(680., 362.9 - extra / 2., 560., 354.2 + extra);
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 120., 25., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.96),
        c(0x24282d, 0.988),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    out.text(
        title,
        Rect::new(717.8, panel.y + 36.2, 432., 42. + extra),
        text(37.6, 500, -0.055).leading(1.1),
        WHITE,
        false,
    );
    out.text(
        language.category(app.map_or("Utilities", |app| app.category.as_str())),
        Rect::new(717.8, panel.y + 87.9 + extra, 440., 23.),
        text(15.2, 400, 0.).leading(1.5),
        c(0xf5f5f5, 0.56),
        false,
    );
    let close = Rect::new(1168.6, panel.y + 39.4, 33.6, 33.6);
    out.shape(close, 16.8, c(0xffffff, 0.035));
    out.border(close, 16.8, 1., c(0xffffff, 0.07));
    out.icon(assets, "close", close.inset(7.6), WHITE);
    if state.focus == Target::CloseDialog {
        out.border(close.inset(1.), 15.8, 2., WHITE);
    }
    out.control(Target::CloseDialog, close, false);
    let favorite = state
        .preferences
        .favorites(&state.inventory.apps)
        .iter()
        .any(|item| item == id);
    let hidden = state.preferences.hidden.iter().any(|item| item == id);
    for (index, label) in [
        if favorite {
            language.t("removeFavorite")
        } else {
            language.t("addFavorite")
        },
        language.t(if hidden { "showApp" } else { "hideApp" }),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            717.8,
            panel.y + 137.9 + extra + index as f32 * 76.8,
            484.4,
            67.2,
        );
        let focused = state.focus == Target::Option(index);
        out.shape(rect, 12., c(0xffffff, if focused { 0.07 } else { 0.016 }));
        out.border(
            rect,
            12.,
            if focused { 2. } else { 1. },
            c(
                if focused { 0xf0f0f0 } else { 0xffffff },
                if focused { 0.95 } else { 0.05 },
            ),
        );
        out.icon(
            assets,
            if index == 0 {
                if favorite { "heart-filled" } else { "heart" }
            } else if hidden {
                "eye"
            } else {
                "eye-off"
            },
            Rect::new(rect.x + 18.6, rect.y + 22.8, 21.6, 21.6),
            if focused { WHITE } else { c(0xe4e4e4, 0.61) },
        );
        out.text(
            label,
            Rect::new(rect.x + 56.2, rect.y + 22., rect.w - 74., 25.),
            text(17.6, 500, -0.015),
            WHITE,
            false,
        );
        out.control(Target::Option(index), rect, false);
    }
    let line_y = panel.y + panel.h - 53.2;
    out.shape(Rect::new(717.8, line_y, 484.4, 1.), 0., c(0xffffff, 0.04));
    let shortcut = text(10.4, 400, 0.);
    let select_width = assets.width(language.t("selectClose"), shortcut);
    let close_width = assets.width(language.t("close"), shortcut);
    let start = 960. - (23. + 6.4 + select_width + 15. + 29. + 6.4 + close_width) / 2.;
    let back = start + 29.4 + select_width + 15.;
    keycap(out, "OK", start, line_y + 15., 23., true);
    out.text(
        language.t("selectClose"),
        Rect::new(start + 29.4, line_y + 18., select_width + 1., 13.),
        text(10.4, 400, 0.),
        c(0xe0e0e0, 0.35),
        false,
    );
    keycap(out, "Back", back, line_y + 15., 29., true);
    out.text(
        language.t("close"),
        Rect::new(back + 35.4, line_y + 18., close_width + 1., 13.),
        text(10.4, 400, 0.),
        c(0xe0e0e0, 0.35),
        false,
    );
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
        if state.keyboard_field == TextField::Location {
            language.t("cityPostcode")
        } else {
            language.t("findApp")
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
    let style = text(16., 400, 0.).leading(1.5);
    let width = (assets.width(notice, style) + 118.).clamp(240., 1856.);
    let height = 34. + assets.lines(notice, style, width - 100.) as f32 * 24.;
    let rect = Rect::new(960. - width / 2., 1080. - 51.2 - height, width, height);
    out.shadow(rect, 16., 40., 8., 0.33);
    out.shape(rect, 16., c(0xededed, 0.96));
    out.border(rect, 16., 1., c(0xffffff, 0.5));
    out.icon(
        assets,
        "sparkles",
        Rect::new(rect.x + 20., rect.y + (height - 20.8) / 2., 20.8, 20.8),
        c(0x727272, 1.),
    );
    out.text(
        notice,
        Rect::new(rect.x + 53.6, rect.y + 17., width - 100., height - 34.),
        text(16., 400, 0.).leading(1.5),
        c(0x303030, 1.),
        false,
    );
    let close = Rect::new(rect.x + width - 33., rect.y + (height - 20.) / 2., 20., 20.);
    out.icon(assets, "close", close.inset(1.2), c(0x6b6b6b, 1.));
    if state.focus == Target::DismissNotice {
        out.border(close.inset(-2.), 5., 2., c(0x303030, 1.));
    }
    if state.overlay.is_none() {
        out.control(Target::DismissNotice, close, false);
    }
    if !state.preferences.reduced_motion
        && let Some((_, opened)) = &state.notice
    {
        out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
    }
}
