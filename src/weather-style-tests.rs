use super::*;
use crate::{
    model::WeatherStyle,
    renderer::Draw,
    state::{Rect, Subpage, WIDGETS_SECTION},
    weather,
};

fn configured() -> State {
    let mut state = ready();
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
                if path.contains("weather/") =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn weather_styles_default_and_round_trip_without_changing_other_preferences() {
    let old: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.weather_style, WeatherStyle::Standard);
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
        let top_label = if style == WeatherStyle::Compact {
            "14°C"
        } else {
            "Vienna"
        };
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
        let expected_size = if weather_style == WeatherStyle::Compact {
            88.
        } else {
            112.
        };
        assert_eq!(font.size, expected_size);
        assert_eq!(font.weight, 500);
        assert!(condition.y - (temperature.y + font.size * font.line_height) <= 12.);
    }
}

#[test]
fn style_picker_previews_every_design_offline_and_only_saves_on_selection() {
    let mut state = ready();
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
    for index in 0..3 {
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
    assert_eq!(weather_icons(&state).len(), 2);
    assert!(state.preferences.weather_location.is_none());
    assert!(!state.weather.fetching());
    state.direction(Direction::Down, &controls(&state));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Choice(2));
    assert_eq!(state.preferences.weather_style, WeatherStyle::Standard);
    state.back();
    assert_eq!(state.subpage, Some(Subpage::Weather));
    assert_eq!(state.focus, Target::WeatherStyle);
    state.activate();
    state.focus = Target::Choice(2);
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.preferences.weather_style, WeatherStyle::Compact);
    assert_eq!(state.focus, Target::WeatherStyle);
    state.activate();
    assert_eq!(state.focus, Target::Choice(2));
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
