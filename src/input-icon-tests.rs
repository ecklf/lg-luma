use super::{controls, ready, scene};
use crate::{
    i18n::Language,
    model::{INPUT_ICONS, Preferences},
    renderer::Draw,
    state::{Direction, Effect, Subpage, Target},
};
use serde_json::json;

#[test]
fn input_icon_preferences_validate_and_round_trip_without_changing_old_defaults() {
    let old: Preferences = serde_json::from_value(json!({"background": "forest"})).unwrap();
    assert!(old.input_icons.is_empty());
    let mut preferences: Preferences = serde_json::from_value(json!({
        "input_icons": {
            "com.webos.app.hdmi1": "xbox",
            "com.webos.app.hdmi2": "../../unknown",
            "bad;id": "hdmi",
            "com.webos.app.hdmi3": ""
        }
    }))
    .unwrap();
    preferences.sanitize();
    assert_eq!(preferences.input_icons.len(), 1);
    assert_eq!(preferences.input_icon("com.webos.app.hdmi1"), Some("xbox"));
    assert!(!preferences.set_input_icon("bad;id", "hdmi"));
    assert!(!preferences.set_input_icon("com.webos.app.hdmi1", "unknown"));
    assert!(!preferences.set_input_icon("com.webos.app.hdmi1", "xbox"));
    for (icon, _) in INPUT_ICONS {
        preferences.set_input_icon("com.webos.app.hdmi2", icon);
        let mut loaded: Preferences =
            serde_json::from_value(serde_json::to_value(&preferences).unwrap()).unwrap();
        loaded.sanitize();
        assert_eq!(loaded.input_icon("com.webos.app.hdmi2"), Some(icon));
        assert_eq!(loaded.input_icon("com.webos.app.hdmi1"), Some("xbox"));
    }
    assert!(preferences.set_input_icon("com.webos.app.hdmi2", ""));
    assert_eq!(preferences.input_icon("com.webos.app.hdmi2"), None);
}

#[test]
fn remote_input_icon_picker_saves_resets_and_restores_its_parent() {
    for renamed in [false, true] {
        let mut state = ready();
        let id = state.inventory.inputs[0].id.clone();
        if renamed {
            state.preferences.set_input_name(&id, "Custom name");
        }
        state.open(2);
        state.focus = Target::Widget(3);
        state.activate();
        state.focus = Target::InputMode(id.clone());
        state.activate();
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::InputRename);
        state.direction(Direction::Down, &controls(&state));
        if renamed {
            assert_eq!(state.focus, Target::InputNameReset);
            state.direction(Direction::Down, &controls(&state));
        }
        assert_eq!(state.focus, Target::InputIcon);
        state.activate();
        assert_eq!(state.subpage, Some(Subpage::InputIcons(id.clone())));
        assert_eq!(state.focus, Target::Choice(0));
        assert_eq!(
            controls(&state)
                .iter()
                .filter(|c| matches!(c.target, Target::Choice(_)))
                .count(),
            INPUT_ICONS.len() + 1
        );
        for index in 1..=INPUT_ICONS.len() {
            state.direction(Direction::Down, &controls(&state));
            assert_eq!(state.focus, Target::Choice(index));
        }
        state.reveal_focus(&controls(&state));
        let selected = controls(&state)
            .into_iter()
            .find(|c| c.target == state.focus)
            .unwrap();
        assert!(selected.rect.y + selected.rect.h <= 1080.);
        assert!(state.scroll > 0.);
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert_eq!(state.preferences.input_icon(&id), Some("xbox"));
        assert_eq!(state.subpage, Some(Subpage::Input(id.clone())));
        assert_eq!(state.focus, Target::InputIcon);
        state.activate();
        assert_eq!(state.focus, Target::Choice(INPUT_ICONS.len()));
        assert!(state.back().is_empty());
        assert_eq!(state.focus, Target::InputIcon);
        assert_eq!(state.preferences.input_icon(&id), Some("xbox"));
        state.activate();
        state.pointer_focus(Target::Choice(0));
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert_eq!(state.preferences.input_icon(&id), None);
        assert_eq!(state.focus, Target::InputIcon);
        state.back();
        assert_eq!(state.subpage, Some(Subpage::Inputs));
        assert_eq!(state.focus, Target::InputMode(id));
    }
}

#[test]
fn input_icon_overrides_render_on_home_and_settings_and_automatic_restores_detection() {
    let mut state = ready();
    let id = state.inventory.inputs[0].id.clone();
    let has_icon = |state: &crate::state::State, icon: &str| {
        scene(state).draw.iter().any(|d| matches!(d, Draw::TintedImage(path, ..) if path.ends_with(&format!("ui-icons/input-{icon}.lvec"))))
    };
    assert!(has_icon(&state, "playstation"));
    state.preferences.set_input_icon(&id, "xbox");
    state.preferences.set_input_name(&id, "PS5");
    assert!(has_icon(&state, "xbox"));
    assert!(!has_icon(&state, "playstation"));
    let mut inputs = state.inventory.inputs.clone();
    inputs[0].title = "Nintendo Switch".into();
    state.set_inputs(inputs);
    assert!(has_icon(&state, "xbox"));
    state.open(2);
    state.focus = Target::Widget(3);
    state.activate();
    assert!(has_icon(&state, "xbox"));
    state.preferences.set_input_icon(&id, "");
    assert!(has_icon(&state, "playstation"));
    state.preferences.set_input_name(&id, "");
    assert!(has_icon(&state, "switch"));
}

#[test]
fn input_icon_picker_covers_bundled_assets_and_every_language() {
    let available: std::collections::BTreeSet<_> = std::fs::read_dir("assets/ui-icons")
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter_map(|name| {
            name.strip_prefix("input-")
                .and_then(|name| name.strip_suffix(".svg"))
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(
        available,
        INPUT_ICONS
            .iter()
            .map(|(icon, _)| icon.to_string())
            .collect()
    );
    let messages: serde_json::Value =
        serde_json::from_str(include_str!("../assets/messages.json")).unwrap();
    let mut state = ready();
    state.open(2);
    state.focus = Target::Widget(3);
    state.activate();
    state.focus = Target::InputMode(state.inventory.inputs[0].id.clone());
    state.activate();
    state.focus = Target::InputIcon;
    state.activate();
    for language in Language::ALL {
        state.preferences.language = language;
        for key in ["inputIcon", "inputIconAutomatic"]
            .into_iter()
            .chain(INPUT_ICONS.iter().map(|(_, key)| *key))
        {
            assert!(
                messages[key][language.code()]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
            );
        }
        let rendered = scene(&state);
        for (icon, key) in INPUT_ICONS {
            assert!(
                rendered
                    .draw
                    .iter()
                    .any(|d| matches!(d, Draw::Text(value, ..) if value == language.t(key)))
            );
            assert!(rendered.draw.iter().any(|d| matches!(d, Draw::TintedImage(path, ..) if path.ends_with(&format!("ui-icons/input-{icon}.lvec")))));
        }
    }
    state.set_inputs(vec![]);
    assert_eq!(state.subpage, Some(Subpage::Inputs));
    assert_eq!(state.focus, Target::Sidebar(crate::state::WIDGETS_SECTION));
}
