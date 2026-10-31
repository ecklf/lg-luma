use super::{controls, ready, scene};
use crate::{
    i18n::Language,
    input_refresh::Refresh,
    model::{InputMode, Preferences, demo_inventory},
    platform::normalize_inputs,
    renderer::Draw,
    state::{Overlay, Subpage, Target, WIDGETS_SECTION},
    view::input_status,
};
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn signal_never_overrides_the_official_cable_state_or_auto_visibility() {
    let cases = [
        (
            json!({"connected":true,"hdmiPlugIn":false,"hdmiSignalExist":true,"activate":true}),
            Some(false),
            Some(true),
            "noDevice",
        ),
        (
            json!({"connected":false,"hdmiPlugIn":true,"hdmiSignalExist":true}),
            Some(true),
            Some(true),
            "connected",
        ),
        (
            json!({"connected":true,"hdmiPlugIn":true,"hdmiSignalExist":false,"activate":false}),
            Some(true),
            Some(false),
            "noSignal",
        ),
        (
            json!({"hdmiPlugIn":true,"hdmiSignalExist":false,"activate":true}),
            Some(true),
            Some(false),
            "noSignal",
        ),
        (
            json!({"connected":true,"hdmiSignalExist":false}),
            Some(true),
            Some(false),
            "noSignal",
        ),
        (
            json!({"connected":false,"hdmiPlugIn":"false","hdmiSignalExist":true}),
            Some(false),
            Some(true),
            "noDevice",
        ),
        (
            json!({"hdmiSignalExist":true}),
            None,
            Some(true),
            "externalInput",
        ),
        (
            json!({"hdmiPlugIn":true,"hdmiSignalExist":"false"}),
            Some(true),
            None,
            "connected",
        ),
        (json!({"hdmiPlugIn":true}), Some(true), None, "connected"),
        (json!({"connected":false}), Some(false), None, "noDevice"),
        (json!({}), None, None, "externalInput"),
    ];
    for (mut raw, cable, signal, key) in cases {
        raw["appId"] = json!("com.webos.app.hdmi1");
        let input = normalize_inputs(&json!([raw])).unwrap().remove(0);
        assert_eq!((input.connected, input.signal), (cable, signal));
        let mut preferences = Preferences::default();
        assert_eq!(preferences.input_visible(&input), cable == Some(true));
        for language in Language::ALL {
            assert_eq!(input_status(&input, false, language), language.t(key));
        }
        preferences
            .input_modes
            .insert(input.id.clone(), InputMode::On);
        assert!(preferences.input_visible(&input));
        preferences
            .input_modes
            .insert(input.id.clone(), InputMode::Off);
        assert!(!preferences.input_visible(&input));
    }
}

#[test]
fn live_signal_keeps_card_geometry_names_apps_and_preferences() {
    let mut state = ready();
    let id = state.inventory.inputs[0].id.clone();
    state.preferences.set_input_name(&id, "Mac");
    state.focus = Target::Input(id.clone());
    let previous = controls(&state);
    let preferences = serde_json::to_value(&state.preferences).unwrap();
    let apps = serde_json::to_value(&state.inventory.apps).unwrap();
    let mut inputs = state.inventory.inputs.clone();
    inputs[0].signal = Some(false);
    state.set_inputs(inputs.clone());
    assert_eq!(state.focus, Target::Input(id));
    let next = controls(&state);
    for before in &previous {
        let after = next.iter().find(|c| c.target == before.target).unwrap();
        assert_eq!(
            (before.rect.x, before.rect.y, before.rect.w, before.rect.h),
            (after.rect.x, after.rect.y, after.rect.w, after.rect.h)
        );
    }
    assert!(
        scene(&state)
            .draw
            .iter()
            .any(|d| matches!(d, Draw::Text(t, ..) if t.ends_with("HDMI 1 · No signal")))
    );
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::Inputs);
    inputs[0].signal = Some(true);
    state.set_inputs(inputs.clone());
    assert!(
        scene(&state)
            .draw
            .iter()
            .any(|d| matches!(d, Draw::Text(t, ..) if t.ends_with("HDMI 1 · Connected")))
    );
    assert_eq!(
        serde_json::to_value(&state.preferences).unwrap(),
        preferences
    );
    assert_eq!(serde_json::to_value(&state.inventory.apps).unwrap(), apps);
    state.dirty = false;
    state.set_inputs(inputs);
    assert!(!state.dirty, "unchanged polls must not redraw");
}

#[test]
fn disconnect_reconciles_home_focus_and_preserves_visibility_overrides() {
    let mut state = ready();
    let id = state.inventory.inputs[0].id.clone();
    state.focus = Target::Input(id.clone());
    let mut inputs = state.inventory.inputs.clone();
    inputs[0].connected = Some(false);
    inputs[0].signal = Some(true);
    state.set_inputs(inputs.clone());
    assert_ne!(state.focus, Target::Input(id.clone()));
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::Input(id.clone()))
    );
    state
        .preferences
        .input_modes
        .insert(id.clone(), InputMode::On);
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Input(id.clone()))
    );
    state
        .preferences
        .input_modes
        .insert(id.clone(), InputMode::Off);
    inputs[0].connected = Some(true);
    state.set_inputs(inputs);
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::Input(id.clone()))
    );
}

#[test]
fn a_removed_port_restores_valid_input_settings_focus() {
    for detail in [false, true] {
        let mut state = ready();
        let id = state.inventory.inputs[0].id.clone();
        state.open(2);
        state.section = WIDGETS_SECTION;
        state.subpage = Some(if detail {
            Subpage::Input(id.clone())
        } else {
            Subpage::Inputs
        });
        state.focus = if detail {
            Target::Choice(2)
        } else {
            Target::InputMode(id)
        };
        let mut inputs = state.inventory.inputs.clone();
        inputs.remove(0);
        let next = inputs[0].id.clone();
        state.set_inputs(inputs);
        assert_eq!(state.subpage, Some(Subpage::Inputs));
        assert_eq!(state.focus, Target::InputMode(next));
        state.set_inputs(vec![]);
        assert_eq!(state.focus, Target::Sidebar(WIDGETS_SECTION));
    }
}

#[test]
fn polling_only_runs_on_visible_focused_tv_input_screens_outside_interactions() {
    let mut state = ready();
    let active = |s: &crate::state::State| crate::input_refresh_active(s, true, true, true);
    assert!(active(&state));
    assert!(!crate::input_refresh_active(&state, false, true, true));
    assert!(!crate::input_refresh_active(&state, true, false, true));
    assert!(!crate::input_refresh_active(&state, true, true, false));
    state.loading = true;
    assert!(!active(&state));
    state.loading = false;
    state.overlay = Some(Overlay::Recents {
        restore: Target::Nav(0),
    });
    assert!(!active(&state));
    state.overlay = None;
    state.recents.visibility = 0.5;
    assert!(!active(&state));
    state.recents.visibility = 0.;
    state.moving = Some(state.inventory.apps[0].id.clone());
    assert!(!active(&state));
    state.moving = None;
    state.pressed = Some(Target::Input(state.inventory.inputs[0].id.clone()));
    assert!(!active(&state));
    state.pressed = None;
    state.open(1);
    assert!(!active(&state));
    state.open(2);
    assert!(!active(&state));
    state.subpage = Some(Subpage::Inputs);
    assert!(active(&state));
    state.subpage = Some(Subpage::Input(state.inventory.inputs[0].id.clone()));
    assert!(active(&state));
    state.subpage = Some(Subpage::Clock);
    assert!(!active(&state));
}

#[test]
fn slow_reads_are_bounded_and_errors_preserve_the_last_valid_snapshot() {
    let now = Instant::now();
    let mut refresh = Refresh::default();
    assert!(!refresh.due(now, true));
    assert!(refresh.due(now + Duration::from_secs(2), true));
    assert!(!refresh.due(now + Duration::from_secs(60), true));
    let inputs = demo_inventory().inputs;
    refresh
        .completed(now + Duration::from_secs(60), Ok(inputs.clone()))
        .unwrap();
    assert!(!refresh.due(now + Duration::from_secs(61), true));
    assert!(refresh.due(now + Duration::from_secs(62), true));
    assert!(
        refresh
            .completed(now + Duration::from_secs(63), Err("unavailable".into()))
            .is_err()
    );
    assert!(!refresh.due(now + Duration::from_secs(72), true));
    assert_eq!(refresh.take_pending(true).unwrap(), inputs);
    assert!(refresh.due(now + Duration::from_secs(73), true));
}

#[test]
fn snapshots_wait_for_release_and_yield_to_a_newer_full_inventory() {
    let now = Instant::now();
    let mut refresh = Refresh::default();
    let mut inputs = demo_inventory().inputs;
    refresh.completed(now, Ok(inputs.clone())).unwrap();
    assert!(refresh.take_pending(false).is_none());
    inputs[0].connected = Some(false);
    refresh.completed(now, Ok(inputs.clone())).unwrap();
    assert_eq!(refresh.take_pending(true).unwrap(), inputs);
    assert!(refresh.take_pending(true).is_none());
    refresh.completed(now, Ok(inputs)).unwrap();
    refresh.superseded(now);
    assert!(refresh.take_pending(true).is_none());
    assert!(!refresh.due(now + Duration::from_secs(1), true));
    assert!(refresh.due(now + Duration::from_secs(2), true));
}

#[test]
fn a_disconnect_snapshot_cannot_move_the_pressed_card_before_release() {
    let mut state = ready();
    let id = state.inventory.inputs[0].id.clone();
    state.pressed = Some(Target::Input(id.clone()));
    let mut inputs = state.inventory.inputs.clone();
    inputs[0].connected = Some(false);
    let mut refresh = Refresh::default();
    refresh.completed(Instant::now(), Ok(inputs)).unwrap();
    assert!(
        refresh
            .take_pending(crate::input_refresh_active(&state, true, true, true))
            .is_none()
    );
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Input(id.clone()))
    );
    state.pressed = None;
    state.set_inputs(
        refresh
            .take_pending(crate::input_refresh_active(&state, true, true, true))
            .unwrap(),
    );
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::Input(id.clone()))
    );
}

#[test]
fn hidden_screens_cancel_scheduled_reads_without_duplicating_in_flight_requests() {
    let now = Instant::now();
    let mut refresh = Refresh::default();
    assert!(!refresh.due(now, false));
    assert!(!refresh.due(now + Duration::from_secs(10), true));
    assert!(!refresh.due(now + Duration::from_secs(11), false));
    assert!(!refresh.due(now + Duration::from_secs(20), true));
    assert!(refresh.due(now + Duration::from_secs(22), true));
    assert!(!refresh.due(now + Duration::from_secs(30), false));
    assert!(!refresh.due(now + Duration::from_secs(40), true));
    refresh
        .completed(now + Duration::from_secs(41), Ok(vec![]))
        .unwrap();
    assert!(!refresh.due(now + Duration::from_secs(42), true));
    assert!(refresh.due(now + Duration::from_secs(43), true));
}
