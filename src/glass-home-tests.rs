use super::*;
use crate::{
    renderer::Draw,
    state::{APPS_SECTION, Rect},
};

#[test]
fn home_keeps_pinned_order_without_extra_labels_even_while_reordering() {
    let mut state = ready();
    state.preferences.favorites = Some(vec![
        "amazon".into(),
        "netflix".into(),
        "youtube.leanback.v4".into(),
    ]);
    state.open(0);
    assert_eq!(
        state
            .favorite_apps()
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        ["amazon", "netflix", "youtube.leanback.v4"]
    );
    assert_eq!(state.focus, Target::App("amazon".into()));
    state.hold();
    let rendered = scene(&state);
    for label in ["apps", "moveApps"] {
        assert!(!rendered.draw.iter().any(|draw| matches!(draw,
            Draw::Text(value, ..) if value == state.preferences.language.t(label))));
    }
    assert!(state.moving.is_some());
}

#[test]
fn back_from_every_settings_sidebar_returns_home_without_a_home_button() {
    for section in 0..crate::state::SECTION_COUNT {
        let mut state = ready();
        state.open(2);
        state.focus(Target::Sidebar(section), &controls(&state));
        let rendered = scene(&state);
        assert!(
            !rendered
                .controls
                .iter()
                .any(|control| matches!(control.target, Target::Nav(_)))
        );
        assert!(!rendered.draw.iter().any(|draw| matches!(draw,
            Draw::Text(value, ..) if value == state.preferences.language.t("home"))));
        state.back();
        assert_eq!(state.page, Page::Home);
        assert_eq!(state.focus, Target::App("netflix".into()));
    }
}

#[test]
fn app_settings_gallery_navigates_three_columns_and_opens_existing_popup() {
    let mut state = ready();
    state.open(2);
    state.focus(Target::Sidebar(APPS_SECTION), &controls(&state));
    state.activate();
    let ids: Vec<_> = state.apps().iter().map(|app| app.id.clone()).collect();
    let rendered = scene(&state);
    let cards: Vec<_> = rendered
        .controls
        .iter()
        .filter(|control| matches!(control.target, Target::App(_)))
        .collect();
    assert_eq!(cards.len(), state.inventory.apps.len());
    assert_eq!(cards[0].rect.w, 360.);
    assert_eq!(cards[0].rect.y, cards[2].rect.y);
    assert!(cards[3].rect.y > cards[0].rect.y);
    state.direction(Direction::Right, &rendered.controls);
    assert_eq!(state.focus, Target::App(ids[1].clone()));
    assert!(state.overlay.is_none());
    state.direction(Direction::Down, &rendered.controls);
    assert_eq!(state.focus, Target::App(ids[4].clone()));
    state.activate();
    assert_eq!(state.focus, Target::OpenApp);
    assert!(matches!(state.overlay, Some(Overlay::Options { .. })));
    state.back();
    assert_eq!(state.focus, Target::App(ids[4].clone()));
    state.focus = Target::App(ids[3].clone());
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(APPS_SECTION));
}

#[test]
fn app_gallery_clips_pointer_hits_and_reveals_remote_focus_without_scrolling_sidebar() {
    let mut state = ready();
    state.preferences.show_app_names = false;
    state.preferences.hidden.push("netflix".into());
    state.open(2);
    state.section = APPS_SECTION;
    let rendered = scene(&state);
    let first = rendered
        .controls
        .iter()
        .find(|control| control.target == Target::App("netflix".into()))
        .unwrap();
    assert_eq!(first.rect.h, 202.5);
    assert!(rendered.draw.iter().any(|draw| matches!(draw,
        Draw::TintedImage(path, rect, ..) if path.ends_with("eye-off.lvec") && first.rect.contains(rect.x, rect.y))));
    state.wheel_scroll(&rendered.controls, Some((300., 500.)), 0, -3);
    assert_eq!(state.scroll, 0.);
    state.wheel_scroll(&rendered.controls, Some((800., 500.)), 0, -3);
    assert!(state.scroll > 0.);
    let scrolled = scene(&state);
    assert_ne!(
        state.hit_test(&scrolled.controls, 800., 170.),
        Some(first.target.clone())
    );
    let last = Target::App(state.apps().last().unwrap().id.clone());
    state.focus(last.clone(), &scrolled.controls);
    let revealed = controls(&state)
        .into_iter()
        .find(|control| control.target == last)
        .unwrap();
    assert!(revealed.rect.y >= state.app_settings_viewport().y);
    assert!(revealed.rect.y + revealed.rect.h <= 1032.1);
}

#[test]
fn control_center_uses_separate_floating_actions_and_preserves_navigation() {
    let mut state = ready();
    state.focus = Target::SettingsMenu;
    state.activate();
    let rendered = scene(&state);
    assert_eq!(rendered.controls.len(), 2);
    for control in &rendered.controls {
        assert_eq!(control.rect.x + control.rect.w, 1836.);
        assert!(rendered.draw.iter().any(|draw| matches!(draw,
            Draw::Glass(rect, radius, _) if (rect.x, rect.y, rect.w, rect.h) ==
                (control.rect.x, control.rect.y, control.rect.w, control.rect.h) && *radius == 44.
        )));
    }
    let panel = rendered.modal.unwrap();
    assert!(!rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Glass(rect, ..) if (rect.x, rect.y, rect.w, rect.h) == (panel.x, panel.y, panel.w, panel.h)
    )));
    assert_eq!(state.focus, Target::LumaSettings);
    state.direction(Direction::Down, &rendered.controls);
    assert_eq!(state.focus, Target::TvSettings);
    state.back();
    assert!(state.overlay.is_none());
    assert_eq!(state.focus, Target::SettingsMenu);
}

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
    assert_eq!(state.focus, Target::SettingsMenu);
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
