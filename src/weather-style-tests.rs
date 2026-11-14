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
fn home_styles_share_content_and_keep_the_dock_fixed() {
    let mut state = configured();
    let dock_y = state.dock_y();
    for style in WeatherStyle::ALL {
        state.preferences.weather_style = style;
        let rendered = scene(&state);
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
                    Draw::Text(label, rect, ..) if label.contains(" · Vienna") && rect.y == icons[0].y)));
            }
        }
        assert_eq!(state.dock_y(), dock_y);
    }
    state.preferences.show_weather = false;
    assert!(weather_icons(&state).is_empty());
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
