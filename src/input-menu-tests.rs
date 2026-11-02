use super::*;
use crate::{i18n::Language, model::INPUT_MENU_ENTRIES, state::Subpage};

#[test]
fn old_preferences_leave_the_ribbon_opt_in_and_keep_stock_entries_visible() {
    let preferences: Preferences = serde_json::from_value(json!({
        "input_modes": {"com.webos.app.hdmi3": "Off"},
        "show_greeting": false
    }))
    .unwrap();
    assert!(!preferences.input_menu.enabled);
    for id in INPUT_MENU_ENTRIES {
        assert!(preferences.input_menu.entry_visible(id));
    }
    assert_eq!(
        preferences.input_modes["com.webos.app.hdmi3"],
        InputMode::Off
    );
}

#[test]
fn ribbon_settings_save_independently_and_restore_parent_focus() {
    let mut state = ready();
    state
        .preferences
        .input_modes
        .insert("com.webos.app.hdmi3".into(), InputMode::Off);
    let home_modes = state.preferences.input_modes.clone();
    state.open(2);
    state.section = crate::state::WIDGETS_SECTION;
    state.focus = Target::Widget(5);
    assert!(state.activate().is_empty());
    assert_eq!(state.subpage, Some(Subpage::InputMenu));
    assert_eq!(state.focus, Target::InputMenuStyle);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.input_menu.enabled);
    state.focus = Target::InputMenuEntry("HDMI_3".into());
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.preferences.input_menu.entry_visible("HDMI_3"));
    assert_eq!(state.preferences.input_modes, home_modes);
    let restored: Preferences =
        serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
    assert_eq!(restored.input_menu, state.preferences.input_menu);
    assert!(state.back().is_empty());
    assert_eq!(state.subpage, None);
    assert_eq!(state.focus, Target::Widget(5));
}

#[test]
fn ribbon_settings_are_reachable_and_translated_in_every_language() {
    for language in Language::ALL {
        let mut state = ready();
        state.preferences.language = language;
        state.open(2);
        state.section = crate::state::WIDGETS_SECTION;
        assert!(
            controls(&state)
                .iter()
                .any(|control| control.target == Target::Widget(5))
        );
        state.focus = Target::Widget(5);
        state.direction(Direction::Right, &controls(&state));
        assert_eq!(state.focus, Target::InputMenuStyle);
        let targets = controls(&state);
        for id in INPUT_MENU_ENTRIES {
            assert!(
                targets
                    .iter()
                    .any(|control| control.target == Target::InputMenuEntry(id.into()))
            );
        }
        for key in [
            "inputMenuTitle",
            "inputMenuStyle",
            "inputMenuHint",
            "inputMenuDashboard",
            "inputMenuLiveTv",
            "inputMenuMediaServers",
            "inputMenuApplyError",
        ] {
            assert_ne!(language.t(key), key);
            assert!(!language.t(key).is_empty());
        }
        state.focus = Target::InputMenuEntry("media_server".into());
        state.direction(Direction::Left, &controls(&state));
        assert_eq!(state.subpage, None);
        assert_eq!(state.focus, Target::Widget(5));
    }
}

#[test]
fn unknown_ribbon_entries_do_not_create_preferences() {
    let mut state = ready();
    for id in ["unknown", "airplay", "brazil_tv"] {
        state.focus = Target::InputMenuEntry(id.into());
        assert!(state.activate().is_empty());
    }
    assert!(state.preferences.input_menu.entries.is_empty());
}

#[test]
fn the_whole_ribbon_list_returns_through_every_setting() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WIDGETS_SECTION;
    state.focus = Target::Widget(5);
    state.activate();
    for id in INPUT_MENU_ENTRIES {
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::InputMenuEntry(id.into()));
    }
    for id in INPUT_MENU_ENTRIES.into_iter().rev().skip(1) {
        state.direction(Direction::Up, &controls(&state));
        assert_eq!(state.focus, Target::InputMenuEntry(id.into()));
    }
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::InputMenuStyle);
    assert_eq!(state.scroll, 0.);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Nav(2));
}

#[test]
fn widget_navigation_reaches_the_input_button_customization() {
    for language in Language::ALL {
        let mut state = ready();
        state.preferences.language = language;
        state.open(2);
        state.section = crate::state::WIDGETS_SECTION;
        state.focus = Target::Widget(5);
        state.activate();
        assert_eq!(state.subpage, Some(Subpage::InputMenu));
        assert_eq!(state.focus, Target::InputMenuStyle);
    }
}

#[test]
fn malformed_optional_ribbon_preferences_keep_existing_user_settings() {
    for input_menu in [
        json!(null),
        json!("invalid"),
        json!({"enabled": "invalid", "entries": {"HDMI_1": false, "HDMI_2": "invalid", "unknown": true, "airplay": true, "brazil_tv": true}}),
    ] {
        let preferences: Preferences = serde_json::from_value(json!({
            "favorites": ["app.one"],
            "background": "midnight",
            "input_menu": input_menu
        }))
        .unwrap();
        assert_eq!(preferences.favorites, Some(vec!["app.one".into()]));
        assert_eq!(preferences.background, "midnight");
        assert!(!preferences.input_menu.enabled);
        assert!(!preferences.input_menu.entries.contains_key("unknown"));
        assert!(!preferences.input_menu.entries.contains_key("airplay"));
        assert!(!preferences.input_menu.entries.contains_key("brazil_tv"));
        assert!(preferences.input_menu.entry_visible("HDMI_2"));
    }
}
