use super::*;
use crate::{
    model::WeatherStyle,
    renderer::Draw,
    state::{Rect, Subpage, WIDGETS_SECTION},
    weather,
};

fn configured() -> State {
    let mut state = ready();
    // Layout-specific checks retain an explicitly configured Standard widget.
    state.preferences.weather_style = WeatherStyle::Standard;
    state.preferences.weather_position = crate::model::WidgetPosition { column: 4, row: 0 };
    let location = weather::Location {
        name: "Vienna".into(),
        region: String::new(),
        country: String::new(),
        latitude: 48.2085,
        longitude: 16.3721,
    };
    state.weather.cache = Some(weather::Cache {
        key: location.key(),
        updated_at: crate::state::unix_now() as i64,
        reading: weather::Reading {
            temperature: 14.2,
            code: 2,
            is_day: true,
        },
    });
    state.preferences.weather_location = Some(location);
    state
}

fn weather_icons(state: &State) -> Vec<Rect> {
    scene(state)
        .draw
        .iter()
        .filter_map(|draw| match draw {
            Draw::Image(path, rect, ..) | Draw::TintedImage(path, rect, ..)
                if path.contains("ui-icons/weather-") || path.contains("weather/") =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn appearance_controls_live_inside_widget_style_and_update_previews_immediately() {
    let mut state = configured();
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::Weather);
    let parent = controls(&state);
    assert!(
        !parent
            .iter()
            .any(|c| matches!(c.target, Target::WeatherShowLocation | Target::WeatherIcons))
    );
    state.focus = Target::WeatherStyle;
    state.activate();
    let rendered = scene(&state);
    for target in [Target::WeatherShowLocation, Target::WeatherIcons] {
        assert!(rendered.controls.iter().any(|c| c.target == target));
    }
    state.direction(Direction::Up, &rendered.controls);
    assert_eq!(state.focus, Target::WeatherIcons);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WeatherShowIcon);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WeatherShowLocation);
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::WeatherStyle));
    assert!(
        !scene(&state)
            .draw
            .iter()
            .any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains("Vienna")))
    );
    state.direction(Direction::Down, &controls(&state));
    state.direction(Direction::Down, &controls(&state));
    state.activate();
    state.focus = Target::Choice(3);
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::WeatherStyle));
    assert_eq!(state.focus, Target::WeatherIcons);
    assert_eq!(weather_icons(&state).len(), 3);
    assert!(scene(&state).draw.iter().any(|draw| matches!(draw, Draw::TintedImage(path, ..) if path.ends_with("weather-kickstand-partly-cloudy.lvec"))));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Choice(0));
    state.back();
    assert_eq!(state.subpage, Some(Subpage::Weather));
    assert_eq!(state.focus, Target::WeatherStyle);
}

#[test]
fn icon_picker_matches_style_row_height_and_sidebar_keeps_only_the_last_three_levels() {
    for language in crate::i18n::Language::ALL {
        let mut state = configured();
        state.preferences.language = language;
        state.open(2);
        state.section = WIDGETS_SECTION;
        state.subpage = Some(Subpage::WeatherIcons);
        state.focus = Target::Choice(0);
        let rendered = scene(&state);
        let breadcrumb: Vec<_> = rendered
            .draw
            .iter()
            .filter_map(|draw| match draw {
                Draw::Text(label, rect, ..)
                    if rect.x >= 160. && rect.x < 564. && rect.y > 500. && rect.y < 574. =>
                {
                    Some(label.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            breadcrumb,
            [
                language.t("weather"),
                language.t("weatherStyle"),
                language.t("weatherIconStyle")
            ]
        );
        for index in 0..4 {
            let row = rendered
                .controls
                .iter()
                .find(|c| c.target == Target::Choice(index))
                .unwrap();
            assert_eq!(row.rect.h, 200.);
            let (x, y) = row.rect.center();
            assert_eq!(
                state.hit_test(&rendered.controls, x, y),
                Some(Target::Choice(index))
            );
        }
    }
}

#[test]
fn weather_picker_insets_match_outer_corner_radii() {
    for subpage in [Subpage::WeatherIcons, Subpage::WeatherStyle] {
        let mut state = configured();
        state.open(2);
        state.section = WIDGETS_SECTION;
        state.subpage = Some(subpage);
        let rendered = scene(&state);
        for row in rendered
            .controls
            .iter()
            .filter(|control| matches!(control.target, Target::Choice(_)))
        {
            assert!(rendered.draw.iter().any(|draw| matches!(draw,
                Draw::Shape(inner, radius, ..)
                    if inner.x == 1040.
                    && inner.y == row.rect.y + 20.
                    && inner.w == 700.
                    && inner.h == row.rect.h - 40.
                    && *radius == 48. - 20.
            )));
        }
    }
}

#[test]
fn icon_and_location_visibility_use_matching_row_values() {
    for language in crate::i18n::Language::ALL {
        for enabled in [false, true] {
            let mut state = configured();
            state.preferences.language = language;
            state.preferences.weather_show_icon = enabled;
            state.preferences.weather_show_location = enabled;
            state.open(2);
            state.section = WIDGETS_SECTION;
            state.subpage = Some(Subpage::WeatherStyle);
            state.focus = Target::Choice(0);
            let rendered = scene(&state);
            let label = language.t(if enabled { "stateOn" } else { "stateOff" });
            let values: Vec<_> = rendered
                .draw
                .iter()
                .filter_map(|draw| match draw {
                    Draw::Text(text, rect, font, color, center) if text == label => {
                        Some((rect, font, color, center))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(values.len(), 2);
            let (location, location_font, location_color, location_center) = values[0];
            let (icon, icon_font, icon_color, icon_center) = values[1];
            assert_eq!(
                (location.x, location.w, location.h),
                (icon.x, icon.w, icon.h)
            );
            assert!((icon.y - location.y - 84.).abs() < 0.001);
            assert_eq!(
                (location_font.size, location_font.weight),
                (icon_font.size, icon_font.weight)
            );
            assert_eq!(location_color, icon_color);
            assert_eq!(location_center, icon_center);
        }
    }
}

#[test]
fn show_icon_toggle_preserves_artwork_and_legacy_hidden_icons_migrate() {
    use crate::model::WeatherIconStyle;
    let mut prefs: Preferences = serde_json::from_str(
        r#"{"weather_style":"no_icon","extra_weather":[{"style":"no_icon"}]}"#,
    )
    .unwrap();
    prefs.sanitize();
    assert_eq!(prefs.weather_style, WeatherStyle::Standard);
    assert_eq!(prefs.weather_icons, WeatherIconStyle::Kuankuan);
    assert!(!prefs.weather_show_icon);
    assert_eq!(prefs.extra_weather[0].style, WeatherStyle::Standard);
    assert_eq!(prefs.extra_weather[0].icons, WeatherIconStyle::Kuankuan);
    assert!(!prefs.extra_weather[0].show_icon);
    assert!(!WeatherStyle::ALL.contains(&WeatherStyle::NoIcon));
    let mut state = configured();
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::WeatherStyle);
    state.focus = Target::WeatherShowIcon;
    state.preferences.weather_icons = WeatherIconStyle::Kickstand;
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.preferences.weather_icons, WeatherIconStyle::Kickstand);
    assert!(!state.preferences.weather_show_icon);
    assert_eq!(state.preferences.weather_style, WeatherStyle::Standard);
    for style in WeatherStyle::ALL {
        state.preferences.weather_style = style;
        assert!(weather_icons(&state).is_empty());
    }
    state.activate();
    assert!(state.preferences.weather_show_icon);
    assert_eq!(state.preferences.weather_icons, WeatherIconStyle::Kickstand);
    let cache = state.weather.cache.clone();
    state.pointer_focus(Target::WeatherShowIcon);
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.weather.cache, cache);
    let loaded: Preferences =
        serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
    assert!(!loaded.weather_show_icon);
    assert_eq!(loaded.weather_icons, WeatherIconStyle::Kickstand);
    let mut old_choice: Preferences =
        serde_json::from_str(r#"{"weather_icons":"none","extra_weather":[{"icons":"none"}]}"#)
            .unwrap();
    old_choice.sanitize();
    assert!(!old_choice.weather_show_icon);
    assert!(!old_choice.extra_weather[0].show_icon);
    assert_eq!(old_choice.weather_icons, WeatherIconStyle::Kuankuan);
    assert!(!WeatherIconStyle::ALL.contains(&WeatherIconStyle::None));
    let mut home = configured();
    home.preferences.weather_show_icon = false;
    for small in [false, true] {
        home.preferences.weather_small = small;
        for style in WeatherStyle::ALL {
            home.preferences.weather_style = style;
            assert!(weather_icons(&home).is_empty());
        }
    }
}

#[test]
fn weather_picker_selection_badges_are_inset_and_visible() {
    for subpage in [Subpage::WeatherIcons, Subpage::WeatherStyle] {
        let mut state = configured();
        state.open(2);
        state.section = WIDGETS_SECTION;
        state.subpage = Some(subpage);
        let rendered = scene(&state);
        let row = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::Choice(0))
            .unwrap()
            .rect;
        assert!(rendered.draw.iter().any(|draw| matches!(draw,
            Draw::TintedImage(path, rect, ..) if path.ends_with("check.lvec")
                && rect.w == 28. && rect.h == 28.
                && rect.y == row.y + 44.
                && rect.x + rect.w == row.x + row.w - 44.
        )));
        assert!(rendered.draw.iter().any(|draw| matches!(draw,
            Draw::Shape(rect, radius, ..) if *radius == 22.
                && rect.w == 44. && rect.h == 44. && rect.y == row.y + 36.
        )));
    }
}

#[test]
fn weather_icon_preferences_default_to_kuankuan_and_round_trip_per_instance() {
    use crate::model::{HomeWidget, WeatherIconStyle};
    let old: Preferences = serde_json::from_str(r#"{"extra_weather":[{}]}"#).unwrap();
    assert_eq!(old.weather_icons, WeatherIconStyle::Kuankuan);
    assert_eq!(old.extra_weather[0].icons, WeatherIconStyle::Kuankuan);
    assert_eq!(WeatherIconStyle::ALL[0], WeatherIconStyle::Kuankuan);
    assert_eq!(WeatherIconStyle::Breezy.label(), "Breezy");
    for icons in WeatherIconStyle::ALL {
        let mut state = configured();
        state.preferences.extra_weather.push(Default::default());
        state
            .preferences
            .edit_weather(HomeWidget::ExtraWeather(0), |config| config.icons = icons);
        state.preferences.weather_icons = WeatherIconStyle::Lucide;
        let loaded: Preferences =
            serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
        assert_eq!(loaded.weather_icons, WeatherIconStyle::Lucide);
        assert_eq!(loaded.extra_weather[0].icons, icons);
        assert_eq!(loaded.weather_location, state.preferences.weather_location);
    }
}

#[test]
fn weather_icon_picker_saves_only_on_selection_and_restores_parent_for_remote_and_pointer() {
    use crate::model::WeatherIconStyle;
    let mut state = configured();
    let cache = state.weather.cache.clone();
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::WeatherStyle);
    state.focus = Target::WeatherShowLocation;
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WeatherShowIcon);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WeatherIcons);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(Subpage::WeatherIcons));
    assert_eq!(state.focus, Target::Choice(0));
    for _ in 0..3 {
        state.direction(Direction::Down, &controls(&state));
    }
    assert_eq!(state.focus, Target::Choice(3));
    assert_eq!(state.preferences.weather_icons, WeatherIconStyle::Kuankuan);
    state.back();
    assert_eq!(state.focus, Target::WeatherIcons);
    state.activate();
    let rendered = scene(&state);
    let control = rendered
        .controls
        .iter()
        .find(|control| control.target == Target::Choice(3))
        .unwrap();
    let (x, y) = control.rect.center();
    assert_eq!(
        state.hit_test(&rendered.controls, x, y),
        Some(Target::Choice(3))
    );
    state.pointer_focus(Target::Choice(3));
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.preferences.weather_icons, WeatherIconStyle::Kickstand);
    assert_eq!(state.subpage, Some(Subpage::WeatherStyle));
    assert_eq!(state.focus, Target::WeatherIcons);
    assert_eq!(state.weather.cache, cache);
    state.activate();
    assert_eq!(state.focus, Target::Choice(3));
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::WeatherIcons);
}

#[test]
fn selected_icon_and_location_settings_drive_every_layout_preview_and_home_size() {
    use crate::model::WeatherIconStyle;
    for icons in WeatherIconStyle::ALL {
        for show_location in [false, true] {
            let mut state = configured();
            state.preferences.weather_icons = icons;
            state.preferences.weather_show_location = show_location;
            for small in [false, true] {
                state.preferences.weather_small = small;
                for style in WeatherStyle::ALL {
                    state.preferences.weather_style = style;
                    let rendered = scene(&state);
                    let paths: Vec<_> = rendered
                        .draw
                        .iter()
                        .filter_map(|draw| match draw {
                            Draw::Image(path, ..) | Draw::TintedImage(path, ..)
                                if path.contains("weather/")
                                    || path.contains("ui-icons/weather-") =>
                            {
                                Some(path)
                            }
                            _ => None,
                        })
                        .collect();
                    if icons != WeatherIconStyle::None && (small || style != WeatherStyle::NoIcon) {
                        assert_eq!(paths.len(), 1);
                        assert!(paths[0].ends_with(&icons.asset("partly-cloudy")));
                    } else {
                        assert!(paths.is_empty());
                    }
                    assert_eq!(
                        rendered.draw.iter().any(|draw| matches!(draw,
                        Draw::Text(label, ..) if label.contains("Vienna"))),
                        show_location
                    );
                }
            }
            state.open(2);
            state.section = WIDGETS_SECTION;
            state.subpage = Some(Subpage::WeatherStyle);
            let rendered = scene(&state);
            let count = rendered.draw.iter().filter(|draw| matches!(draw,
                Draw::Image(path, ..) | Draw::TintedImage(path, ..) if path.ends_with(&icons.asset("partly-cloudy")))).count();
            if icons == WeatherIconStyle::None {
                assert!(weather_icons(&state).is_empty());
            } else {
                assert_eq!(count, 3);
            }
            assert_eq!(
                rendered.draw.iter().any(|draw| matches!(draw,
                Draw::Text(label, ..) if label.contains("Vienna"))),
                show_location
            );
            assert!(!state.weather.fetching());
        }
    }
}

#[test]
fn location_visibility_defaults_on_and_round_trips_independently_per_widget() {
    use crate::model::HomeWidget;
    let mut prefs: Preferences = serde_json::from_str(r#"{"extra_weather":[{}]}"#).unwrap();
    assert!(prefs.weather_show_location);
    assert!(prefs.extra_weather[0].show_location);
    prefs.edit_weather(HomeWidget::ExtraWeather(0), |config| {
        config.show_location = false
    });
    let loaded: Preferences = serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();
    assert!(loaded.weather_show_location);
    assert!(!loaded.extra_weather[0].show_location);
    prefs.edit_weather(HomeWidget::Weather, |config| config.show_location = false);
    prefs.edit_weather(HomeWidget::ExtraWeather(0), |config| {
        config.show_location = true
    });
    let loaded: Preferences = serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();
    assert!(!loaded.weather_show_location);
    assert!(loaded.extra_weather[0].show_location);
}

#[test]
fn location_toggle_supports_remote_and_pointer_without_changing_forecast_or_cache() {
    use crate::model::HomeWidget;
    let mut state = configured();
    state.preferences.extra_weather.push(Default::default());
    let cache = state.weather.cache.clone();
    let location = state.preferences.weather_location.clone();
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::WeatherStyle);
    state.focus = Target::Choice(0);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WeatherIcons);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WeatherShowIcon);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WeatherShowLocation);
    assert_eq!(state.activate(), [Effect::Save]);
    assert!(!state.preferences.weather_show_location);
    assert_eq!(state.weather.cache, cache);
    assert_eq!(state.preferences.weather_location, location);
    state.active_weather = HomeWidget::ExtraWeather(0);
    let rendered = scene(&state);
    let control = rendered
        .controls
        .iter()
        .find(|c| c.target == Target::WeatherShowLocation)
        .unwrap();
    let (x, y) = control.rect.center();
    assert_eq!(
        state.hit_test(&rendered.controls, x, y),
        Some(Target::WeatherShowLocation)
    );
    state.pointer_focus(Target::WeatherShowLocation);
    assert_eq!(state.activate(), [Effect::Save]);
    assert!(!state.preferences.extra_weather[0].show_location);
    assert!(!state.preferences.weather_show_location);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::SettingsMenu);
}

#[test]
fn hidden_location_never_leaks_in_any_style_size_or_weather_state() {
    for style in WeatherStyle::ALL {
        for small in [false, true] {
            for mode in 0..4 {
                let mut state = configured();
                state.preferences.weather_style = style;
                state.preferences.weather_small = small;
                state.preferences.weather_show_location = false;
                state.weather.failed = mode == 1 || mode == 3;
                if mode >= 2 {
                    state.weather.cache = None;
                    state.weather.loading = mode == 2;
                }
                let rendered = scene(&state);
                assert!(!rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(label, ..) if label.contains("Vienna"))));
                let label = match mode {
                    0 | 1 => "14°C",
                    2 => state.preferences.language.t("loadingWeather"),
                    _ => state.preferences.language.t("weatherUnavailable"),
                };
                assert!(rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(text, ..) if text == label)));
                if mode == 1 {
                    assert!(rendered.draw.iter().any(|draw| matches!(draw,
                        Draw::Text(label, ..) if label == state.preferences.language.t("lastReading"))));
                }
            }
        }
    }
}

#[test]
fn hidden_location_style_previews_are_private_but_configuration_keeps_city() {
    let mut state = configured();
    state.preferences.weather_show_location = false;
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::WeatherStyle);
    assert!(!scene(&state).draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, ..) if label.contains("Vienna"))));
    state.subpage = Some(Subpage::Weather);
    assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, ..) if label == "Vienna")));
}

#[test]
fn unset_weather_defaults_to_one_line_below_greeting_without_overwriting_saved_choices() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut defaults: Preferences = serde_json::from_str(r#"{"extra_weather":[{}]}"#).unwrap();
    defaults.sanitize();
    assert_eq!(defaults.weather_style, WeatherStyle::OneLine);
    assert_eq!(
        defaults.weather_position,
        WidgetPosition { column: 0, row: 1 }
    );
    assert_eq!(defaults.widget_span(HomeWidget::Weather), (2, 1));
    assert!(defaults.can_place_widget(HomeWidget::Weather, defaults.weather_position));
    assert_eq!(defaults.extra_weather[0].style, WeatherStyle::OneLine);
    for (json, style, position) in [
        (
            r#"{"weather_style":"standard","weather_position":{"column":4,"row":0}}"#,
            WeatherStyle::Standard,
            WidgetPosition { column: 4, row: 0 },
        ),
        (
            r#"{"weather_style":"compact","weather_position":{"column":3,"row":2}}"#,
            WeatherStyle::Compact,
            WidgetPosition { column: 3, row: 2 },
        ),
        (
            r#"{"weather_style":"compact"}"#,
            WeatherStyle::Compact,
            WidgetPosition { column: 0, row: 1 },
        ),
        (
            r#"{"weather_position":{"column":4,"row":0}}"#,
            WeatherStyle::OneLine,
            WidgetPosition { column: 4, row: 0 },
        ),
    ] {
        let mut prefs: Preferences = serde_json::from_str(json).unwrap();
        prefs.sanitize();
        assert_eq!(prefs.weather_style, style);
        assert_eq!(prefs.weather_position, position);
    }
}

#[test]
fn weather_styles_default_and_round_trip_without_changing_other_preferences() {
    let old: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.weather_style, WeatherStyle::OneLine);
    for style in WeatherStyle::ALL {
        let mut state = configured();
        state.preferences.weather_style = style;
        state.preferences.temperature_unit = weather::Unit::Fahrenheit;
        let loaded: Preferences =
            serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
        assert_eq!(loaded.weather_style, style);
        assert_eq!(loaded.temperature_unit, weather::Unit::Fahrenheit);
        assert_eq!(loaded.weather_location, state.preferences.weather_location);
    }
}

#[test]
fn small_weather_places_at_edges_and_resizes_without_overlaps_or_losing_configuration() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut state = configured();
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::Weather);
    state.focus = Target::WeatherStyle;
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WeatherSize);
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.preferences.widget_span(HomeWidget::Weather), (1, 1));
    let last = WidgetPosition { column: 5, row: 3 };
    assert!(state.preferences.place_widget(HomeWidget::Weather, last));
    assert_eq!(state.preferences.widget_at(last), Some(HomeWidget::Weather));
    assert_eq!(
        state
            .preferences
            .widget_at(WidgetPosition { column: 4, row: 3 }),
        None
    );
    let mut loaded: Preferences =
        serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
    loaded.sanitize();
    assert!(loaded.weather_small);
    assert_eq!(loaded.weather_position, last);
    assert_eq!(state.activate(), [Effect::Save]);
    assert!(!state.preferences.weather_small);
    assert!(
        state
            .preferences
            .can_place_widget(HomeWidget::Weather, state.preferences.weather_position)
    );
    assert_eq!(state.preferences.weather_location, loaded.weather_location);
    assert_eq!(state.preferences.weather_style, loaded.weather_style);
    assert!(state.weather.cache.is_some());
}

#[test]
fn small_weather_renders_readings_setup_loading_and_saved_readings() {
    let mut state = configured();
    state.preferences.weather_small = true;
    assert_eq!(weather_icons(&state)[0].w, 48.);
    for unit in [weather::Unit::Celsius, weather::Unit::Fahrenheit] {
        state.preferences.temperature_unit = unit;
        let expected = weather::format_temperature(14.2, unit);
        assert!(
            scene(&state)
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label == &expected))
        );
    }
    state.weather.failed = true;
    assert!(
        scene(&state)
            .draw
            .iter()
            .any(|draw| matches!(draw, Draw::Text(label, ..) if label == state.preferences.language.t("lastReading")))
    );
    state.preferences.weather_location = None;
    let rendered = scene(&state);
    let setup = rendered
        .controls
        .iter()
        .find(|control| control.target == Target::WeatherConfigure)
        .unwrap();
    assert_eq!(setup.rect.w, 272.);
    assert_eq!(setup.rect.h, 128.);
    state.weather.cache = None;
    state.preferences.weather_location = configured().preferences.weather_location;
    state.weather.loading = true;
    assert!(scene(&state).draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label == state.preferences.language.t("loadingWeather"))));
}

#[test]
fn weather_popup_resizes_with_remote_and_pointer_and_stays_attached_to_its_anchor() {
    use crate::{
        model::{HomeWidget, WidgetPosition},
        state::Overlay,
    };
    for cell in [4, 5, 10, 11] {
        let mut state = configured();
        state.open(2);
        state.section = WIDGETS_SECTION;
        state.focus = Target::WidgetCell(cell);
        state.activate();
        state.direction(Direction::Down, &controls(&state));
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::WeatherSize);
        assert!(
            scene(&state)
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label == "Size · 2 × 2"))
        );
        assert_eq!(state.activate(), [Effect::Save]);
        assert!(state.preferences.weather_small);
        assert!(matches!(
            state.overlay,
            Some(Overlay::WidgetOptions { cell: 4 })
        ));
        assert_eq!(state.focus, Target::WeatherSize);
        let rendered = scene(&state);
        assert!(
            rendered
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label == "Size · 1 × 1"))
        );
        let size = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::WeatherSize)
            .unwrap();
        let (x, y) = size.rect.center();
        assert_eq!(
            state.hit_test(&rendered.controls, x, y),
            Some(Target::WeatherSize)
        );
        state.pointer_focus(Target::WeatherSize);
        assert_eq!(state.activate(), [Effect::Save]);
        assert!(!state.preferences.weather_small);
        state.back();
        assert_eq!(state.focus, Target::WidgetCell(4));
        assert!(state.overlay.is_none());

        state.preferences.toggle_weather_size();
        let edge = WidgetPosition { column: 5, row: 3 };
        assert!(state.preferences.place_widget(HomeWidget::Weather, edge));
        state.focus = Target::WidgetCell(edge.cell());
        state.activate();
        state.pointer_focus(Target::WeatherSize);
        assert_eq!(state.activate(), [Effect::Save]);
        let anchor = state.preferences.weather_position;
        assert_ne!(anchor, edge);
        assert!(
            state
                .preferences
                .can_place_widget(HomeWidget::Weather, anchor)
        );
        assert!(
            matches!(state.overlay, Some(Overlay::WidgetOptions { cell }) if cell == anchor.cell())
        );
        assert!(
            controls(&state)
                .iter()
                .any(|control| control.target == Target::WeatherSize)
        );
        state.back();
        assert_eq!(state.focus, Target::WidgetCell(anchor.cell()));

        state.focus = Target::WidgetCell(0);
        state.activate();
        assert!(
            !controls(&state)
                .iter()
                .any(|control| control.target == Target::WeatherSize)
        );
    }
}

#[test]
fn small_weather_setup_is_complete_and_hit_testable_in_every_language_and_grid_cell() {
    use crate::{
        i18n::Language,
        model::{HomeWidget, WidgetPosition},
    };
    for language in Language::ALL {
        let mut state = ready();
        state.preferences.weather_small = true;
        state.preferences.show_greeting = false;
        state.preferences.language = language;
        for cell in 0..crate::model::HOME_GRID_CELLS {
            let position = WidgetPosition::from_cell(cell).unwrap();
            assert!(
                state
                    .preferences
                    .can_place_widget(HomeWidget::Weather, position)
            );
            state.preferences.weather_position = position;
            let rendered = scene(&state);
            assert!(rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label == language.t("setWeatherLocation"))));
            let setup = rendered
                .controls
                .iter()
                .find(|control| control.target == Target::WeatherConfigure)
                .unwrap();
            let (x, y) = setup.rect.center();
            assert_eq!(
                state.hit_test(&rendered.controls, x, y),
                Some(Target::WeatherConfigure)
            );
            assert!(setup.rect.y + setup.rect.h <= state.dock_y());
        }
    }
}

#[test]
fn home_styles_share_content_and_keep_the_dock_fixed() {
    let mut state = configured();
    let dock_y = state.dock_y();
    for style in WeatherStyle::ALL {
        state.preferences.weather_style = style;
        let rendered = scene(&state);
        let top_label = "14°C";
        assert!(
            rendered.draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, rect, ..) if label == top_label && rect.y == 188.)),
            "Weather content must align with the top of its default grid cell"
        );
        assert!(
            rendered
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label == "14°C"))
        );
        assert!(
            rendered
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains("Vienna")))
        );
        let icons = weather_icons(&state);
        match style {
            WeatherStyle::Standard => assert_eq!(icons[0].w, 144.),
            WeatherStyle::NoIcon => assert!(icons.is_empty()),
            WeatherStyle::OneLine => {
                assert_eq!(icons[0].w, 32.);
                assert!(rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(label, rect, ..) if label == "Vienna · Partly cloudy" && rect.x > icons[0].x && rect.y == icons[0].y)));
            }
            WeatherStyle::Compact => {
                assert_eq!(icons[0].w, 32.);
                assert!(rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(label, rect, ..) if label == "Vienna · Partly cloudy" && rect.y == icons[0].y && rect.x > icons[0].x)));
            }
        }
        assert_eq!(state.dock_y(), dock_y);
    }
    state.preferences.show_weather = false;
    assert!(weather_icons(&state).is_empty());
}

#[test]
fn every_style_and_size_combines_city_and_condition_without_a_separate_city_header() {
    for style in WeatherStyle::ALL {
        for small in [false, true] {
            for language in crate::i18n::Language::ALL {
                let mut state = configured();
                state.preferences.language = language;
                state.preferences.weather_style = style;
                state.preferences.weather_small = small;
                let expected = format!("Vienna · {}", language.t("partlyCloudy"));
                let rendered = scene(&state);
                assert!(
                    rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(label, ..) if label == &expected || (small && style == WeatherStyle::OneLine && label.starts_with("Vienna · ")))),
                    "{style:?}, small={small}, {language:?}"
                );
                assert!(!rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(label, ..) if label == "Vienna")));
                state.preferences.weather_show_location = false;
                let rendered = scene(&state);
                assert!(rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(label, ..) if label == language.t("partlyCloudy"))));
                assert!(!rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(label, ..) if label.contains("Vienna") || label.starts_with('·'))));
            }
        }
    }
}

#[test]
fn one_line_uses_one_grid_row_and_taller_styles_relocate_from_the_bottom() {
    use crate::model::{HOME_GRID_ROWS, HomeWidget, WidgetPosition};
    let mut state = configured();
    let widget = HomeWidget::Weather;
    state
        .preferences
        .set_weather_widget_style(widget, WeatherStyle::OneLine);
    assert_eq!(state.preferences.widget_span(widget), (2, 1));
    let bottom = WidgetPosition {
        column: 4,
        row: HOME_GRID_ROWS as u8 - 1,
    };
    assert!(state.preferences.can_place_widget(widget, bottom));
    state.preferences.weather_position = bottom;
    state
        .preferences
        .set_weather_widget_style(widget, WeatherStyle::Standard);
    assert_eq!(state.preferences.widget_span(widget), (2, 2));
    assert_ne!(state.preferences.widget_position(widget), bottom);
    assert!(
        state
            .preferences
            .can_place_widget(widget, state.preferences.widget_position(widget))
    );
    state.preferences.toggle_weather_widget_size(widget);
    state
        .preferences
        .set_weather_widget_style(widget, WeatherStyle::OneLine);
    assert_eq!(state.preferences.widget_span(widget), (1, 1));
    state.preferences.extra_weather.push(Default::default());
    let extra = HomeWidget::ExtraWeather(0);
    state
        .preferences
        .set_weather_widget_style(extra, WeatherStyle::OneLine);
    assert_eq!(state.preferences.widget_span(extra), (2, 1));
}

#[test]
fn one_line_style_is_remote_selectable_and_keeps_content_side_by_side() {
    let mut state = configured();
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::WeatherStyle);
    state.focus = Target::Choice(1);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Choice(2));
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.preferences.weather_style, WeatherStyle::OneLine);
    state.back();
    state.back();
    state.back();
    for small in [false, true] {
        state.preferences.weather_small = small;
        let rendered = scene(&state);
        let rect_for = |expected: &str| {
            rendered
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(label, rect, ..) if label == expected => Some(*rect),
                    _ => None,
                })
                .unwrap()
        };
        let temperature = rect_for("14°C");
        let detail = rect_for("Vienna · Partly cloudy");
        let icon = weather_icons(&state)[0];
        assert!(icon.x + icon.w <= temperature.x);
        assert!(temperature.x + temperature.w <= detail.x);
        assert_eq!(detail.y, temperature.y);
        let fonts: Vec<_> = rendered
            .draw
            .iter()
            .filter_map(|draw| match draw {
                Draw::Text(label, _, font, ..)
                    if label == "14°C" || label == "Vienna · Partly cloudy" =>
                {
                    Some(*font)
                }
                _ => None,
            })
            .collect();
        assert_eq!(fonts.len(), 2);
        assert_eq!(fonts[0].size, fonts[1].size);
        assert_eq!(fonts[0].weight, fonts[1].weight);
    }
}

#[test]
fn weather_grid_keeps_original_temperature_typography_and_spacing() {
    let mut state = configured();
    for weather_style in WeatherStyle::ALL {
        state.preferences.weather_style = weather_style;
        let rendered = scene(&state);
        let (temperature, font) = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(label, rect, font, ..) if label == "14°C" => Some((rect, font)),
                _ => None,
            })
            .unwrap();
        let condition = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(label, rect, ..) if label.contains("Partly cloudy") => Some(rect),
                _ => None,
            })
            .unwrap();
        let expected_size = match weather_style {
            WeatherStyle::Compact => 88.,
            WeatherStyle::OneLine => 26.,
            _ => 112.,
        };
        assert_eq!(font.size, expected_size);
        assert_eq!(
            font.weight,
            if weather_style == WeatherStyle::OneLine {
                450
            } else {
                500
            }
        );
        assert!(condition.y - (temperature.y + font.size * font.line_height) <= 12.);
    }
}

#[test]
fn style_picker_previews_every_design_offline_and_only_saves_on_selection() {
    let mut state = ready();
    state.preferences.weather_style = WeatherStyle::Standard;
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::Weather);
    state.focus = Target::WeatherStyle;
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::WeatherStyle));
    assert_eq!(state.focus, Target::Choice(0));
    let rendered = scene(&state);
    assert!(rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, ..) if label == "Compact")));
    assert!(rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, ..) if label == "Vienna · Partly cloudy")));
    assert!(!rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, ..) if label == "Compact inline")));
    assert!(!rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, ..) if label.contains("Sample weather preview"))));
    for index in 0..WeatherStyle::ALL.len() {
        state.focus = Target::Choice(index);
        state.reveal_focus(&controls(&state));
        let rendered = scene(&state);
        assert!(
            rendered
                .controls
                .iter()
                .any(|c| c.target == Target::Choice(index))
        );
        let control = rendered
            .controls
            .iter()
            .find(|c| c.target == Target::Choice(index))
            .unwrap();
        let (x, y) = control.rect.center();
        assert_eq!(
            state.hit_test(&rendered.controls, x, y),
            Some(Target::Choice(index))
        );
    }
    assert_eq!(weather_icons(&state).len(), 3);
    state.focus = Target::Choice(0);
    assert!(state.preferences.weather_location.is_none());
    assert!(!state.weather.fetching());
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Choice(1));
    assert_eq!(state.preferences.weather_style, WeatherStyle::Standard);
    state.back();
    assert_eq!(state.subpage, Some(Subpage::Weather));
    assert_eq!(state.focus, Target::WeatherStyle);
    state.activate();
    state.focus = Target::Choice(1);
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.preferences.weather_style, WeatherStyle::Compact);
    assert_eq!(state.focus, Target::WeatherStyle);
    state.activate();
    assert_eq!(state.focus, Target::Choice(1));
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.subpage, Some(Subpage::Weather));
}

#[test]
fn every_style_keeps_setup_and_error_messages_accessible() {
    for style in WeatherStyle::ALL {
        let mut state = ready();
        state.preferences.weather_style = style;
        assert!(
            controls(&state)
                .iter()
                .any(|c| c.target == Target::WeatherConfigure)
        );
        state = configured();
        state.preferences.weather_style = style;
        state.weather.failed = true;
        assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, ..) if label == state.preferences.language.t("lastReading"))));
        state.weather.cache = None;
        assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, ..) if label == state.preferences.language.t("weatherUnavailable"))));
    }
}
