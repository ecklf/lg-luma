use super::*;
use crate::{
    renderer::Draw,
    state::{APPS_SECTION, Rect},
};

#[test]
fn down_opens_drawer_and_back_resets_to_first_favorite() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.preferences.favorites = Some(
        state
            .inventory
            .apps
            .iter()
            .take(9)
            .map(|app| app.id.clone())
            .collect(),
    );
    let first = state.favorite_apps()[0].id.clone();
    state.focus = Target::App("youtube.leanback.v4".into());
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.page, Page::Apps);
    assert_eq!(state.drawer_visibility, 1.);
    assert!(!state.apps().iter().any(|app| app.id == "netflix"));
    assert!(
        controls(&state)
            .iter()
            .any(|control| control.target == Target::DockApp("netflix".into()))
    );
    state.scroll = 500.;
    state.home_scroll = 300.;
    state.back();
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.drawer_visibility, 0.);
    assert_eq!(state.focus, Target::App(first));
    assert_eq!((state.scroll, state.home_scroll), (0., 0.));
}

#[test]
fn corners_and_weather_setup_are_remote_accessible() {
    let mut state = ready();
    state.focus = Target::App("netflix".into());
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::InputsMenu);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WeatherConfigure);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::App("netflix".into()));
    state.direction(Direction::Up, &controls(&state));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::SettingsMenu);
    state.activate();
    assert!(matches!(state.overlay, Some(Overlay::Settings)));
    assert_eq!(controls(&state).len(), 2);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::TvSettings);
    assert_eq!(
        state.activate(),
        vec![Effect::Launch("com.palm.app.settings".into())]
    );
    assert!(state.overlay.is_none());
    assert_eq!(state.focus, Target::SettingsMenu);
}

#[test]
fn inputs_menu_respects_visibility_and_uses_dots_not_status_text() {
    let mut state = ready();
    let id = state.inventory.inputs[0].id.clone();
    state.inventory.inputs[0].connected = Some(false);
    state.focus = Target::InputsMenu;
    state.activate();
    assert!(
        !controls(&state)
            .iter()
            .any(|control| control.target == Target::Input(id.clone()))
    );
    state
        .preferences
        .input_modes
        .insert(id.clone(), InputMode::On);
    state.reconcile_focus();
    let rendered = scene(&state);
    assert!(
        rendered
            .controls
            .iter()
            .any(|control| control.target == Target::Input(id.clone()))
    );
    assert!(rendered.draw.iter().any(
        |draw| matches!(draw, Draw::Shape(rect, radius, _, _) if rect.w == 10. && *radius == 5.)
    ));
    assert!(!rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains("Connected") || label.contains("Disconnected") || label.contains("No signal"))));
    state.preferences.input_modes.insert(id, InputMode::Off);
    state.back();
    assert_eq!(state.focus, Target::InputsMenu);
    assert!(state.overlay.is_none());
}

#[test]
fn all_apps_remain_manageable_when_hidden_and_favoriting_restores_them() {
    let mut state = ready();
    let id = "netflix".to_string();
    state.preferences.hidden.push(id.clone());
    manage_apps(&mut state);
    assert!(state.apps().iter().any(|app| app.id == id));
    state.focus = Target::App(id.clone());
    state.activate();
    assert!(matches!(state.overlay, Some(Overlay::Options { .. })));
    state.focus = Target::Option(0);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.preferences.hidden.contains(&id));
    assert_eq!(state.section, APPS_SECTION);
    assert_eq!(state.focus, Target::App(id));
}

#[test]
fn empty_favorites_and_empty_inventory_still_offer_drawer_and_settings() {
    let mut state = ready();
    state.preferences.favorites = Some(vec![]);
    state.open(0);
    assert_eq!(state.focus, Target::Browse);
    state.activate();
    assert_eq!(state.page, Page::Apps);
    assert!(matches!(state.focus, Target::App(_)));
    state.inventory.apps.clear();
    state.reconcile_focus();
    assert_eq!(state.focus, Target::Search);
    state.back();
    assert_eq!(state.focus, Target::Browse);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::InputsMenu);
}

#[test]
fn drawer_animation_does_not_change_grid_layout_or_scroll_on_open() {
    let mut state = ready();
    state.open(1);
    state.focus = state.first_content();
    let target = state.focus.clone();
    let mut previous: Option<Rect> = None;
    for progress in [0., 0.25, 0.5, 0.75, 1.] {
        state.drawer_visibility = progress;
        let rendered = scene(&state);
        assert!(!state.reveal_focus(&rendered.controls));
        let rect = rendered
            .controls
            .iter()
            .find(|control| control.target == target)
            .unwrap()
            .rect;
        if let Some(previous) = previous {
            assert_eq!(
                (rect.x, rect.y, rect.w, rect.h),
                (previous.x, previous.y, previous.w, previous.h)
            );
        }
        previous = Some(rect);
        assert_eq!(state.scroll, 0.);
        let (x, y) = rect.center();
        if progress < 1. {
            assert_ne!(
                state.hit_test(&rendered.controls, x, y),
                Some(target.clone())
            );
        } else {
            assert_eq!(
                state.hit_test(&rendered.controls, x, y),
                Some(target.clone())
            );
        }
    }
}

#[test]
fn glass_surfaces_are_used_on_home_settings_and_keyboard() {
    let mut state = ready();
    for page in [0, 1, 2] {
        state.open(page);
        assert!(
            scene(&state)
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Glass(..)))
        );
    }
    state.open_keyboard(crate::state::TextField::Apps);
    assert!(
        scene(&state)
            .draw
            .iter()
            .any(|draw| matches!(draw, Draw::Glass(rect, ..) if rect.w == 1220.))
    );
}

#[test]
fn hiding_a_docked_app_cannot_leave_focus_on_an_invisible_card() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open(1);
    state.focus = Target::DockApp("netflix".into());
    state.options();
    state.focus = Target::Option(1);
    state.activate();
    assert!(state.preferences.hidden.contains(&"netflix".into()));
    assert_ne!(state.focus, Target::DockApp("netflix".into()));
    assert!(
        controls(&state)
            .iter()
            .any(|control| control.target == state.focus)
    );
}

#[test]
fn connecting_the_first_input_gives_an_empty_dropdown_a_valid_remote_target() {
    let mut state = ready();
    for input in &mut state.inventory.inputs {
        input.connected = Some(false);
    }
    state.focus = Target::InputsMenu;
    state.activate();
    assert_eq!(state.focus, Target::InputsMenu);
    let mut inputs = state.inventory.inputs.clone();
    inputs[0].connected = Some(true);
    let id = inputs[0].id.clone();
    state.set_inputs(inputs);
    assert_eq!(state.focus, Target::Input(id.clone()));
    assert_eq!(state.activate(), vec![Effect::Launch(id)]);
    assert_eq!(state.focus, Target::InputsMenu);
    assert!(state.overlay.is_none());
}
