use super::*;
use crate::home_layout::{Alignment, Kind, Mode, Widget};

pub(super) fn home(out: &mut Scene, state: &State, assets: &Assets, now: OffsetDateTime) {
    out.draw.push(Draw::Clip(None));
    let layout = state.home_layout().unwrap();
    for widget in &layout.widgets {
        let rect = widget.rect();
        out.draw.push(Draw::Clip(Some(rect.inset(-8.))));
        match widget.kind {
            Kind::Clock => {
                let size = if widget.large_clock {
                    [48., 64., 88.][usize::from(widget.size)]
                } else {
                    [20., 24., 32.][usize::from(widget.size)]
                };
                label(
                    out,
                    assets,
                    &state.preferences.language.clock(
                        now,
                        state.preferences.hour_24,
                        state.preferences.show_seconds,
                    ),
                    rect,
                    size,
                    widget.alignment,
                    WHITE,
                );
            }
            Kind::Date => {
                label(
                    out,
                    assets,
                    &state
                        .preferences
                        .date_format
                        .format(state.preferences.language, now),
                    rect,
                    [16., 18.4, 24.][usize::from(widget.size)],
                    widget.alignment,
                    c(0xf5f5f5, 0.72),
                );
            }
            Kind::Greeting => {
                label(
                    out,
                    assets,
                    &state
                        .preferences
                        .language
                        .greeting(now.hour(), &state.preferences.greeting_name),
                    rect,
                    [64., 90.4, 104.][usize::from(widget.size)],
                    widget.alignment,
                    WHITE,
                );
            }
            Kind::Weather => weather_widget(out, state, assets, widget),
            Kind::Favorites => favorites(out, state, assets, widget),
            Kind::Inputs => inputs(out, state, assets, widget),
            Kind::Settings => {
                let button = Rect::new(
                    rect.x + (rect.w - 60.) / 2.,
                    rect.y + (rect.h - 60.) / 2.,
                    60.,
                    60.,
                );
                let focused = state.focus == Target::TvSettings;
                out.pill(button, focused, false);
                out.icon(
                    assets,
                    "settings",
                    button.inset([20., 18., 14.][usize::from(widget.size)]),
                    if focused { c(0x20252b, 1.) } else { WHITE },
                );
                out.control(Target::TvSettings, button, false);
            }
        }
        out.draw.push(Draw::Clip(None));
    }
}

fn label(
    out: &mut Scene,
    assets: &Assets,
    value: &str,
    rect: Rect,
    size: f32,
    alignment: Alignment,
    color: Color,
) {
    let style = assets.fit_text(value, text(size.min(rect.h / 1.2), 500, -0.035), rect.w);
    let width = assets.width(value, style).min(rect.w);
    let x = match alignment {
        Alignment::Left => rect.x,
        Alignment::Center => rect.x + (rect.w - width) / 2.,
        Alignment::Right => rect.x + rect.w - width,
    };
    out.text(
        value,
        Rect::new(
            x,
            rect.y + (rect.h - style.size * 1.2) / 2.,
            width + 1.,
            style.size * 1.2,
        ),
        style,
        color,
        false,
    );
}

fn favorites(out: &mut Scene, state: &State, assets: &Assets, widget: &Widget) {
    let rect = widget.rect();
    let top = 8.;
    let width = widget.card_width().min(rect.w);
    let apps = state.apps();
    if apps.is_empty() {
        let title = state.preferences.language.t(if state.loading {
            "favorites"
        } else if state.error.is_some() {
            "aLittleHelp"
        } else {
            "makeFavorites"
        });
        label(
            out,
            assets,
            title,
            Rect::new(rect.x, rect.y + top, rect.w, 64.),
            24.,
            Alignment::Left,
            WHITE,
        );
        let button = Rect::new(rect.x, rect.y + top + 72., rect.w.min(280.), 48.);
        let target = if state.error.is_some() {
            Target::Retry
        } else {
            Target::Browse
        };
        out.pill(button, state.focus == target, false);
        label(
            out,
            assets,
            state.preferences.language.t(if state.error.is_some() {
                "retry"
            } else {
                "apps"
            }),
            button,
            20.,
            Alignment::Center,
            WHITE,
        );
        if !state.loading {
            out.control(target, button, false);
        }
    }
    for (index, app) in apps.iter().enumerate() {
        let labels = if widget.labels {
            18.4 + caption_height(state, assets, app, width)
        } else {
            0.
        };
        let height = (width * FACE_HEIGHT / HOME_TILE_WIDTH).min((rect.h - top - labels).max(40.));
        tile(
            out,
            state,
            assets,
            app,
            Rect::new(
                rect.x + index as f32 * (width + 25.6) - state.home_scroll,
                rect.y + top,
                width,
                height,
            ),
            widget.labels,
            false,
        );
    }
}

fn inputs(out: &mut Scene, state: &State, assets: &Assets, widget: &Widget) {
    let rect = widget.rect();
    let top = 8.;
    let width = widget.card_width().min(rect.w);
    let inputs: Vec<_> = state
        .inventory
        .inputs
        .iter()
        .filter(|input| state.preferences.input_visible(input))
        .collect();
    if inputs.is_empty() {
        label(
            out,
            assets,
            state.preferences.language.t("layoutNoInputs"),
            Rect::new(rect.x, rect.y + top, rect.w, 64.),
            20.,
            Alignment::Left,
            c(0xeeeeee, 0.7),
        );
    }
    for (index, input) in inputs.iter().enumerate() {
        input_card(
            out,
            state,
            assets,
            input,
            Rect::new(
                rect.x + index as f32 * (width + 22.35) - state.input_scroll,
                rect.y + top,
                width,
                104.,
            ),
        );
    }
}

fn weather_widget(out: &mut Scene, state: &State, assets: &Assets, widget: &Widget) {
    let rect = widget.rect();
    let language = state.preferences.language;
    let mut icon = "location".to_owned();
    let value = if let Some(place) = &state.preferences.weather_location {
        if let Some(reading) = state.weather.reading(place) {
            let condition = weather::condition(reading);
            icon = format!("weather-{}", condition.icon);
            format!(
                "{} · {} · {}{}",
                place.name,
                weather::format_temperature(
                    reading.temperature,
                    state.preferences.temperature_unit
                ),
                language.t(condition.key),
                if state.weather.failed {
                    format!(" · {}", language.t("lastReading"))
                } else {
                    String::new()
                }
            )
        } else {
            format!(
                "{} · {}",
                place.name,
                language.t(if state.weather.loading {
                    "loadingWeather"
                } else {
                    "weatherUnavailable"
                })
            )
        }
    } else {
        language.t("setWeatherLocation").to_owned()
    };
    out.icon(
        assets,
        &icon,
        Rect::new(rect.x, rect.y + (rect.h - 28.) / 2., 28., 28.),
        WHITE,
    );
    // Wrap at a readable size rather than reducing a narrow forecast to tiny text.
    let style = text([18., 23.2, 28.][usize::from(widget.size)], 450, -0.025).leading(1.4);
    let height =
        (assets.lines(&value, style, rect.w - 40.) as f32 * style.size * style.line_height)
            .min(rect.h);
    let text_rect = Rect::new(
        rect.x + 40.,
        rect.y + (rect.h - height) / 2.,
        rect.w - 40.,
        height,
    );
    out.text(
        value,
        text_rect,
        style,
        c(0xf5f5f5, 0.8),
        widget.alignment == Alignment::Center,
    );
    if state.preferences.weather_location.is_none() {
        if state.focus == Target::WeatherConfigure {
            out.ring(rect.inset(6.), 10.);
        }
        out.control(Target::WeatherConfigure, rect, false);
    }
}

pub(super) fn editor(out: &mut Scene, state: &State, assets: &Assets) {
    let editor = state.layout_editor.as_ref().unwrap();
    let language = state.preferences.language;
    out.controls.clear();
    out.draw.push(Draw::Clip(None));
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x080b12, 0.22));
    if editor.show_grid {
        for row in 0..crate::home_layout::ROWS {
            for column in 0..crate::home_layout::COLUMNS {
                let cell = Widget {
                    column,
                    row,
                    width: 1,
                    height: 1,
                    ..Widget::default()
                };
                if !cell.overlaps_navigation()
                    && !editor
                        .draft
                        .widgets
                        .iter()
                        .any(|widget| cell.overlaps(widget))
                {
                    let rect = crate::home_layout::cell_rect(column, row);
                    out.shape(rect, 4., c(0xffffff, 0.035));
                    out.border(rect, 4., 1., c(0xffffff, 0.10));
                }
            }
        }
    }
    for widget in &editor.draft.widgets {
        let rect = widget.rect();
        let selected = state.focus == Target::LayoutWidget(widget.id);
        out.border(
            rect,
            8.,
            if selected { 3. } else { 1. },
            c(0xffffff, if selected { 1. } else { 0.45 }),
        );
        let name = language.t(widget.kind.key());
        let tag = Rect::new(
            rect.x + 8.,
            rect.y + 4.,
            (assets.width(name, text(14., 500, 0.)) + 20.).min(rect.w - 16.),
            24.,
        );
        out.shape(tag, 5., c(0x101720, 0.9));
        out.text(name, tag.inset(4.), text(14., 500, 0.), WHITE, false);
        out.control(Target::LayoutWidget(widget.id), rect, false);
    }
    out.shape(Rect::new(0., 944., 1920., 136.), 0., c(0x101720, 0.96));
    let instruction = match editor.mode {
        Mode::Move(_) => "layoutMoveHint",
        Mode::Resize(_) => "layoutResizeHint",
        _ => "layoutSelectHint",
    };
    out.text(
        language.t(instruction),
        Rect::new(84., 952., 1430., 28.),
        text(18., 450, 0.),
        WHITE,
        false,
    );
    editor_button(
        out,
        state,
        assets,
        Target::LayoutToolbar(5),
        &format!(
            "{} · {}",
            language.t("layoutGrid"),
            language.t(if editor.show_grid {
                "stateOn"
            } else {
                "stateOff"
            })
        ),
        Rect::new(1556., 948., 280., 36.),
    );
    for (index, key) in [
        "layoutAdd",
        "layoutPresets",
        "layoutReset",
        "layoutSave",
        "layoutDiscard",
    ]
    .into_iter()
    .enumerate()
    {
        editor_button(
            out,
            state,
            assets,
            Target::LayoutToolbar(index),
            language.t(key),
            Rect::new(84. + index as f32 * 354., 996., 336., 56.),
        );
    }
    match &editor.mode {
        Mode::Select => {}
        Mode::Move(_) | Mode::Resize(_) => {
            out.controls.retain(|c| c.target == state.focus);
        }
        Mode::Menu(_) => panel(
            out,
            state,
            assets,
            "layoutEdit",
            vec![
                (0, "layoutMove", None),
                (1, "layoutResize", None),
                (2, "layoutConfigure", None),
                (3, "layoutRemove", None),
            ],
        ),
        Mode::Add => panel(
            out,
            state,
            assets,
            "layoutAdd",
            Kind::ALL
                .iter()
                .enumerate()
                .map(|(i, k)| (i, k.key(), None))
                .collect(),
        ),
        Mode::Presets => panel(
            out,
            state,
            assets,
            "layoutPresets",
            vec![
                (0, "layoutClassic", None),
                (1, "layoutSplit", None),
                (2, "layoutMinimal", None),
            ],
        ),
        Mode::Exit => panel(
            out,
            state,
            assets,
            "layoutExit",
            vec![
                (0, "layoutSave", None),
                (1, "layoutDiscard", None),
                (2, "layoutContinue", None),
            ],
        ),
        Mode::Configure(id) => {
            let widget = editor.draft.widgets.iter().find(|w| w.id == *id).unwrap();
            let mut items =
                vec![(
                    0,
                    "layoutSize",
                    Some(
                        language
                            .t(["layoutSmall", "layoutNormal", "layoutLarge"]
                                [usize::from(widget.size)])
                            .to_owned(),
                    ),
                )];
            if matches!(widget.kind, Kind::Clock | Kind::Date | Kind::Greeting) {
                items.push((
                    1,
                    "layoutAlignment",
                    Some(
                        language
                            .t(match widget.alignment {
                                Alignment::Left => "layoutLeft",
                                Alignment::Center => "layoutCenter",
                                Alignment::Right => "layoutRight",
                            })
                            .to_owned(),
                    ),
                ));
            }
            let on_off = |value| {
                Some(
                    language
                        .t(if value { "stateOn" } else { "stateOff" })
                        .to_owned(),
                )
            };
            if widget.kind == Kind::Favorites {
                items.push((3, "appNames", on_off(widget.labels)));
            }
            if widget.kind == Kind::Clock {
                items.push((4, "layoutLargeClock", on_off(widget.large_clock)));
            }
            panel(out, state, assets, "layoutConfigure", items);
        }
    }
}

fn editor_button(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    target: Target,
    title: &str,
    rect: Rect,
) {
    let focused = state.focus == target;
    out.pill(rect, focused, false);
    label(
        out,
        assets,
        title,
        rect.inset(10.),
        20.,
        Alignment::Center,
        if focused { c(0x20252b, 1.) } else { WHITE },
    );
    out.control(target, rect, false);
}

fn panel(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    title: &str,
    items: Vec<(usize, &str, Option<String>)>,
) {
    out.controls.clear();
    out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x080b12, 0.65));
    let height = 106. + items.len() as f32 * 76.;
    let surface = Rect::new(600., (1080. - height) / 2., 720., height);
    settings_surface(out, surface, 24.);
    out.text(
        state.preferences.language.t(title),
        Rect::new(surface.x + 24., surface.y + 24., 672., 36.),
        text(28., 550, -0.02),
        WHITE,
        false,
    );
    for (position, (index, key, value)) in items.into_iter().enumerate() {
        row(
            out,
            state,
            assets,
            Row {
                target: Target::LayoutAction(index),
                rect: Rect::new(
                    surface.x + 24.,
                    surface.y + 82. + position as f32 * 76.,
                    672.,
                    64.,
                ),
                title: state.preferences.language.t(key),
                icon: None,
                value: value.as_deref(),
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
}
