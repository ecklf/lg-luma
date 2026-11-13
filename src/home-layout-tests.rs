use super::*;
use crate::home_layout::{Alignment, Kind, Layout, Mode};

fn custom(preset: usize) -> State {
    let mut state = ready();
    state.preferences.home_layout = Some(Layout::preset(preset, &state.preferences));
    state
}

fn action(state: &mut State, index: usize) -> Vec<Effect> {
    state.focus = Target::LayoutAction(index);
    state.activate()
}

#[test]
fn home_layout_editor_is_remote_accessible_and_does_not_save_on_entry() {
    let mut state = ready();
    state.open(2);
    for _ in 0..3 {
        state.direction(Direction::Down, &controls(&state));
    }
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::LayoutEdit);
    let before = serde_json::to_value(&state.preferences).unwrap();
    assert!(state.activate().is_empty());
    assert!(state.layout_editor.is_some());
    assert_eq!(state.page, Page::Home);
    assert_eq!(before, serde_json::to_value(&state.preferences).unwrap());
    assert!(
        controls(&state)
            .iter()
            .all(|c| matches!(c.target, Target::LayoutWidget(_) | Target::LayoutToolbar(_)))
    );
    assert!(state.open_recents().is_empty());
    state.hold();
    state.options();
    assert!(state.overlay.is_none());
    assert!(state.moving.is_none());
}

#[test]
fn home_layout_move_and_resize_confirm_cancel_and_collision_are_transactional() {
    let mut state = ready();
    state.edit_layout();
    let before = state.layout_editor.as_ref().unwrap().draft.clone();
    let id = before.widgets[0].id;
    state.activate();
    action(&mut state, 0);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(
        state.layout_editor.as_ref().unwrap().draft.widgets[0].column,
        1
    );
    assert!(state.preferences.home_layout.is_none());
    state.back();
    assert_eq!(state.layout_editor.as_ref().unwrap().draft, before);
    action(&mut state, 0);
    for _ in 0..12 {
        state.direction(Direction::Right, &controls(&state));
    }
    assert!(state.layout_editor.as_ref().unwrap().draft.valid());
    assert_eq!(
        state.layout_editor.as_ref().unwrap().draft.widgets[0].column,
        2
    );
    assert!(state.activate().is_empty());
    assert!(
        matches!(state.layout_editor.as_ref().unwrap().mode, Mode::Menu(current) if current == id)
    );
    action(&mut state, 1);
    state.direction(Direction::Down, &controls(&state));
    state.back();
    assert_eq!(
        state.layout_editor.as_ref().unwrap().draft.widgets[0].height,
        2
    );
    state.back();
    state.focus = Target::LayoutToolbar(4);
    assert!(state.activate().is_empty());
    assert!(state.preferences.home_layout.is_none());
}

#[test]
fn home_layout_save_round_trips_and_exit_requires_an_explicit_decision() {
    let mut state = ready();
    state.edit_layout();
    state.focus = Target::LayoutToolbar(1);
    state.activate();
    action(&mut state, 1);
    let draft = state.layout_editor.as_ref().unwrap().draft.clone();
    state.back();
    assert!(matches!(
        state.layout_editor.as_ref().unwrap().mode,
        Mode::Exit
    ));
    assert_eq!(state.focus, Target::LayoutAction(2));
    assert_eq!(action(&mut state, 0), vec![Effect::Save]);
    assert_eq!(state.preferences.home_layout, Some(draft.clone()));
    assert_eq!(state.focus, Target::LayoutEdit);
    let restored: Preferences =
        serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
    assert_eq!(restored.home_layout, Some(draft));
    state.edit_layout();
    state.layout_editor.as_mut().unwrap().draft.widgets.clear();
    state.focus = Target::LayoutToolbar(4);
    state.activate();
    assert!(
        !state
            .preferences
            .home_layout
            .as_ref()
            .unwrap()
            .widgets
            .is_empty()
    );
}

#[test]
fn home_layout_configure_remove_add_and_clock_variants_are_independent() {
    let mut state = ready();
    state.edit_layout();
    state.activate();
    action(&mut state, 2);
    assert_eq!(state.focus, Target::LayoutAction(0));
    action(&mut state, 0);
    action(&mut state, 1);
    action(&mut state, 4);
    let clock = state
        .layout_editor
        .as_ref()
        .unwrap()
        .draft
        .widget(Kind::Clock)
        .unwrap();
    assert_eq!(clock.size, 2);
    assert_eq!(clock.alignment, Alignment::Center);
    assert!(clock.large_clock);
    state.back();
    action(&mut state, 3);
    assert!(
        state
            .layout_editor
            .as_ref()
            .unwrap()
            .draft
            .widget(Kind::Clock)
            .is_none()
    );
    state.focus = Target::LayoutToolbar(0);
    state.activate();
    action(&mut state, 0);
    assert!(matches!(state.focus, Target::LayoutWidget(_)));
    state.focus = Target::LayoutToolbar(0);
    state.activate();
    action(&mut state, 0);
    assert_eq!(
        state
            .layout_editor
            .as_ref()
            .unwrap()
            .draft
            .widgets
            .iter()
            .filter(|w| w.kind == Kind::Clock)
            .count(),
        2
    );
    state.focus = Target::LayoutToolbar(0);
    state.activate();
    action(&mut state, 0);
    assert_eq!(
        state
            .layout_editor
            .as_ref()
            .unwrap()
            .draft
            .widgets
            .iter()
            .filter(|w| w.kind == Kind::Clock)
            .count(),
        2
    );
    state.back();
    state.focus = Target::LayoutToolbar(3);
    state.activate();
    assert_eq!(
        state
            .preferences
            .home_layout
            .as_ref()
            .unwrap()
            .widgets
            .iter()
            .filter(|w| w.kind == Kind::Clock)
            .count(),
        2
    );
}

#[test]
fn home_layout_favorites_resizing_changes_viewport_not_card_size_and_reveals_whole_cards() {
    let mut state = custom(1);
    let width = state.home_widget(Kind::Favorites).unwrap().card_width();
    let apps: Vec<_> = state.apps().iter().map(|app| app.id.clone()).collect();
    state.focus = Target::App(apps[0].clone());
    for _ in 1..apps.len() {
        state.direction(Direction::Right, &controls(&state));
        let scene = scene(&state);
        let card = scene
            .controls
            .iter()
            .find(|c| c.target == state.focus)
            .unwrap();
        let (left, right) = state.favorite_bounds();
        assert!(card.rect.x >= left - 0.1 && card.rect.x + card.rect.w <= right + 0.1);
        assert_eq!(card.rect.w, width);
    }
    assert!(state.home_scroll > 0.);
    let before = state.home_scroll;
    let card = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::App(apps[0].clone()))
        .unwrap();
    assert_ne!(
        state.hit_test(&controls(&state), card.rect.x + 10., card.rect.y + 10.),
        Some(card.target)
    );
    state.pointer_focus(state.focus.clone());
    assert_eq!(before, state.home_scroll);
}

#[test]
fn home_layout_narrow_inputs_scroll_reveal_clip_and_reconcile_after_disconnect() {
    let mut state = custom(1);
    for input in &state.inventory.inputs {
        state
            .preferences
            .input_modes
            .insert(input.id.clone(), InputMode::On);
    }
    let ids: Vec<_> = state
        .inventory
        .inputs
        .iter()
        .map(|input| input.id.clone())
        .collect();
    state.focus = Target::Input(ids[0].clone());
    for id in ids.iter().skip(1) {
        state.direction(Direction::Right, &controls(&state));
        assert_eq!(state.focus, Target::Input(id.clone()));
        let widget = state.home_widget(Kind::Inputs).unwrap().rect();
        let card = controls(&state)
            .into_iter()
            .find(|c| c.target == state.focus)
            .unwrap();
        assert!(
            card.rect.x >= widget.x - 0.1 && card.rect.x + card.rect.w <= widget.x + widget.w + 0.1
        );
    }
    assert!(state.input_scroll > 0.);
    let first = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::Input(ids[0].clone()))
        .unwrap();
    assert_ne!(
        state.hit_test(&controls(&state), first.rect.x + 10., first.rect.y + 10.),
        Some(first.target)
    );
    state.set_inputs(vec![state.inventory.inputs[0].clone()]);
    assert_eq!(state.input_scroll, 0.);
    assert_ne!(state.focus, Target::Input(ids.last().unwrap().clone()));
}

#[test]
fn home_layout_wheel_only_scrolls_the_widget_under_the_pointer() {
    let mut state = custom(1);
    let inputs = state.home_widget(Kind::Inputs).unwrap().rect();
    let favorites = state.home_widget(Kind::Favorites).unwrap().rect();
    state.wheel_scroll(&controls(&state), Some(favorites.center()), 0, -1);
    assert!(state.home_scroll > 0.);
    assert_eq!(state.input_scroll, 0.);
    let scroll = state.home_scroll;
    state.wheel_scroll(&controls(&state), Some(inputs.center()), 0, -1);
    assert_eq!(state.home_scroll, scroll);
    assert!(state.input_scroll > 0.);
    state.wheel_scroll(&controls(&state), Some((1900., 940.)), 0, -10);
    assert_eq!(state.home_scroll, scroll);
    assert_eq!(state.scroll, 0.);
}

#[test]
fn home_layout_remote_crosses_side_by_side_widgets_using_geometry() {
    let mut state = custom(1);
    let last = state.apps().last().unwrap().id.clone();
    state.focus(Target::App(last), &controls(&state));
    state.direction(Direction::Right, &controls(&state));
    assert!(matches!(state.focus, Target::Input(_)));
    state.direction(Direction::Left, &controls(&state));
    assert!(matches!(state.focus, Target::App(_)));
}

#[test]
fn home_layout_empty_layout_keeps_permanent_navigation_recoverable() {
    let mut state = custom(0);
    state
        .preferences
        .home_layout
        .as_mut()
        .unwrap()
        .widgets
        .clear();
    state.open(0);
    assert_eq!(state.focus, Target::Nav(0));
    let scene = scene(&state);
    assert_eq!(scene.controls.len(), 3);
    assert_eq!(scene.controls[0].target, Target::Nav(0));
    state.focus = Target::Nav(2);
    state.activate();
    assert_eq!(state.page, Page::Settings);
    assert_eq!(state.first_content(), Target::Sidebar(0));
}

#[test]
fn home_layout_presets_and_all_editor_panels_have_valid_focus_in_every_language() {
    for language in crate::i18n::Language::ALL {
        for preset in 0..3 {
            let mut state = custom(preset);
            state.preferences.language = language;
            assert!(
                scene(&state)
                    .controls
                    .iter()
                    .all(|c| c.rect.w > 0. && c.rect.h > 0.)
            );
            state.edit_layout();
            let widgets = state.layout_editor.as_ref().unwrap().draft.widgets.clone();
            for widget in widgets {
                state.layout_editor.as_mut().unwrap().mode = Mode::Select;
                state.focus = Target::LayoutWidget(widget.id);
                state.activate();
                action(&mut state, 2);
                let controls = controls(&state);
                assert!(
                    controls.iter().any(|c| c.target == state.focus),
                    "{} {:?}",
                    language.code(),
                    widget.kind
                );
                assert!(
                    controls
                        .iter()
                        .all(|c| matches!(c.target, Target::LayoutAction(_)))
                );
            }
        }
    }
}

#[test]
fn home_layout_legacy_visibility_controls_stay_in_sync_without_losing_formats_or_names() {
    let mut state = custom(0);
    state.preferences.greeting_name = "Ana".into();
    for (kind, target) in [
        (Kind::Greeting, Target::GreetingToggle),
        (Kind::Date, Target::DateToggle),
        (Kind::Weather, Target::WeatherToggle),
    ] {
        state.focus = target.clone();
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert!(
            state
                .preferences
                .home_layout
                .as_ref()
                .unwrap()
                .widget(kind)
                .is_none()
        );
        state.focus = target;
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert!(
            state
                .preferences
                .home_layout
                .as_ref()
                .unwrap()
                .widget(kind)
                .is_some()
        );
    }
    assert_eq!(state.preferences.greeting_name, "Ana");
    assert!(state.preferences.home_layout.as_ref().unwrap().valid());
}

#[test]
fn home_layout_widget_absence_disables_weather_without_a_network_request() {
    let mut state = custom(2);
    state.preferences.weather_location = Some(crate::weather::Location {
        name: "Berlin".into(),
        latitude: 52.52,
        longitude: 13.405,
        country: String::new(),
        region: String::new(),
    });
    state.preferences.show_weather = true;
    assert!(
        state
            .weather
            .sync(&state.preferences, true, 1000)
            .0
            .is_none()
    );
}

#[test]
fn home_layout_native_back_cancels_once_and_consumes_the_physical_press() {
    use sdl2::keyboard::Keycode;
    use std::time::{Duration, Instant};
    let mut state = ready();
    state.edit_layout();
    state.activate();
    action(&mut state, 0);
    state.direction(Direction::Right, &controls(&state));
    let mut button = crate::BackButton::default();
    let now = Instant::now();
    assert!(button.key_down(&mut state, Keycode::AcBack, now).is_empty());
    assert!(matches!(
        state.layout_editor.as_ref().unwrap().mode,
        Mode::Menu(_)
    ));
    assert_eq!(
        state.layout_editor.as_ref().unwrap().draft.widgets[0].column,
        0
    );
    assert!(!button.native_hold(now + Duration::from_millis(1000)));
    assert_eq!(
        button.release(Keycode::AcBack, now + Duration::from_millis(1100)),
        None
    );
    assert!(matches!(
        state.layout_editor.as_ref().unwrap().mode,
        Mode::Menu(_)
    ));
    assert!(state.overlay.is_none());
}

#[test]
fn home_layout_explicit_presets_and_reset_restore_widgets_hidden_by_minimal() {
    let mut state = custom(2);
    state.preferences.show_weather = false;
    state.preferences.show_date = false;
    state.edit_layout();
    state.focus = Target::LayoutToolbar(1);
    state.activate();
    action(&mut state, 0);
    assert!(
        state
            .layout_editor
            .as_ref()
            .unwrap()
            .draft
            .widget(Kind::Weather)
            .is_some()
    );
    assert!(
        state
            .layout_editor
            .as_ref()
            .unwrap()
            .draft
            .widget(Kind::Date)
            .is_some()
    );
    state.layout_editor.as_mut().unwrap().draft.widgets.clear();
    state.focus = Target::LayoutToolbar(2);
    state.activate();
    assert_eq!(state.layout_editor.as_ref().unwrap().draft.widgets.len(), 7);
    assert!(!state.preferences.show_weather);
    assert!(!state.preferences.show_date);
}

#[test]
fn home_layout_navigation_is_fixed_centered_and_never_in_the_widget_palette() {
    use crate::renderer::Draw;
    for language in crate::i18n::Language::ALL {
        for preset in 0..3 {
            let mut state = custom(preset);
            state.preferences.language = language;
            let rendered = scene(&state);
            let tabs: Vec<_> = rendered
                .controls
                .iter()
                .filter(|c| matches!(c.target, Target::Nav(_)))
                .collect();
            assert_eq!(tabs.len(), 3);
            let left = tabs[0].rect.x;
            let right = tabs[2].rect.x + tabs[2].rect.w;
            assert!(((left + right) / 2. - 960.).abs() < 0.1);
            assert!(left >= 528. && right <= 1392.);
            assert!(!rendered.draw.iter().any(|d| matches!(d, Draw::Text(value, ..) if value == language.t("favorites") || value == language.t("inputs"))));
            state.edit_layout();
            state.focus = Target::LayoutToolbar(0);
            state.activate();
            assert!(
                !scene_text(&scene(&state)).contains(&language.t("layoutNavigation").to_owned())
            );
        }
    }
}

fn scene_text(scene: &view::Scene) -> Vec<String> {
    scene
        .draw
        .iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(value, ..) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn home_layout_grid_toggle_only_changes_guides_and_keeps_pointer_targets_still() {
    use crate::renderer::Draw;
    let mut state = custom(0);
    state.edit_layout();
    let draft = state.layout_editor.as_ref().unwrap().draft.clone();
    let before = scene(&state);
    let guide_count = |scene: &view::Scene| {
        scene.draw.iter().filter(|draw| matches!(draw, Draw::Shape(_, radius, width, color) if *radius == 4. && *width == 0. && (color[3] - 0.035).abs() < 0.0001)).count()
    };
    assert!(guide_count(&before) > 0);
    state.pointer_focus(Target::LayoutToolbar(5));
    assert!(state.activate().is_empty());
    assert!(!state.layout_editor.as_ref().unwrap().show_grid);
    let after = scene(&state);
    assert_eq!(guide_count(&after), 0);
    assert_eq!(draft, state.layout_editor.as_ref().unwrap().draft);
    for control in before.controls {
        let current = after
            .controls
            .iter()
            .find(|c| c.target == control.target)
            .unwrap();
        assert_eq!(
            (
                control.rect.x,
                control.rect.y,
                control.rect.w,
                control.rect.h
            ),
            (
                current.rect.x,
                current.rect.y,
                current.rect.w,
                current.rect.h
            )
        );
    }
    state.activate();
    assert!(guide_count(&scene(&state)) > 0);
}

#[test]
fn home_layout_reserves_navigation_cells_and_keeps_compact_clock_tight() {
    let mut state = custom(0);
    let clock = state.home_widget(Kind::Clock).unwrap();
    assert_eq!(clock.width, 1);
    assert_eq!(clock.rect().w, 124.);
    let mut candidate = clock.clone();
    candidate.column = 3;
    assert!(
        !state
            .preferences
            .home_layout
            .as_mut()
            .unwrap()
            .replace(candidate)
    );
    state.preferences.show_seconds = true;
    assert_eq!(
        Layout::preset(0, &state.preferences)
            .widget(Kind::Clock)
            .unwrap()
            .width,
        2
    );
    for column in 0..12 {
        let cell = crate::home_layout::cell_rect(column, 3);
        assert_eq!(cell.w, 124.);
        assert_eq!(cell.h, 32.);
        if column < 11 {
            assert_eq!(
                crate::home_layout::cell_rect(column + 1, 3).x - cell.x - cell.w,
                24.
            );
        }
    }
}

#[test]
fn home_layout_old_navigation_and_heading_fields_migrate_without_losing_other_widgets() {
    let mut layout = serde_json::to_value(Layout::preset(0, &Preferences::default())).unwrap();
    let widgets = layout["widgets"].as_array_mut().unwrap();
    widgets[0]["width"] = 3.into();
    widgets.push(json!({"id": 99, "kind": "navigation", "column": 8, "row": 0, "width": 3, "height": 2, "compact_navigation": true}));
    for widget in widgets.iter_mut() {
        widget["heading"] = true.into();
    }
    let restored: Preferences =
        serde_json::from_value(json!({"home_layout": layout, "greeting_name": "Ana"})).unwrap();
    let restored_layout = restored.home_layout.as_ref().unwrap();
    assert_eq!(restored_layout.widgets.len(), 7);
    assert_eq!(restored_layout.widget(Kind::Clock).unwrap().width, 1);
    assert_eq!(restored.greeting_name, "Ana");
    let output = serde_json::to_string(&restored).unwrap();
    assert!(!output.contains("\"navigation\""));
    assert!(!output.contains("heading"));
    assert!(!output.contains("compact_navigation"));
    // A newly resized compact clock must not be mistaken for the old default on reload.
    let mut custom_layout = restored_layout.clone();
    custom_layout.widgets[0].width = 3;
    assert!(custom_layout.valid());
    let saved: Preferences = serde_json::from_value(json!({"home_layout": custom_layout})).unwrap();
    assert_eq!(
        saved
            .home_layout
            .unwrap()
            .widget(Kind::Clock)
            .unwrap()
            .width,
        3
    );
}

#[test]
fn home_layout_strip_bounds_end_at_the_content_without_changing_card_positions() {
    for size in 0..3 {
        for labels in [false, true] {
            let mut state = custom(0);
            let layout = state.preferences.home_layout.as_mut().unwrap();
            let mut favorite = layout.widget(Kind::Favorites).unwrap().clone();
            favorite.size = size;
            favorite.labels = labels;
            assert!(layout.replace(favorite));
            let rendered = scene(&state);
            for (kind, is_target) in [(Kind::Favorites, true), (Kind::Inputs, false)] {
                let widget = state.home_widget(kind).unwrap();
                let rect = widget.rect();
                let card = rendered
                    .controls
                    .iter()
                    .find(|control| {
                        if is_target {
                            matches!(control.target, Target::App(_))
                        } else {
                            matches!(control.target, Target::Input(_))
                        }
                    })
                    .unwrap();
                assert!((card.rect.y - (48. + f32::from(widget.row) * 40. + 8.)).abs() < 0.001);
                assert!(
                    (card.rect.y + card.rect.h - rect.y - rect.h).abs() < 0.001,
                    "{kind:?} size {size}, labels {labels}"
                );
                assert_eq!(widget.height, ((rect.h + 8.) / 40.).ceil() as u8);
                if kind == Kind::Favorites {
                    assert!(
                        (card.rect.h
                            - if labels { 76.64 } else { 0. }
                            - widget.card_width() * 154.4 / 256.)
                            .abs()
                            < 0.001
                    );
                }
            }
        }
    }
}

#[test]
fn home_layout_old_padded_strips_migrate_in_place_and_release_grid_rows() {
    let mut old = Layout::preset(0, &Preferences::default());
    for widget in &mut old.widgets {
        if widget.kind == Kind::Favorites {
            widget.height = 7;
        }
        if widget.kind == Kind::Inputs {
            widget.height = 4;
        }
    }
    let preferences: Preferences = serde_json::from_value(json!({"home_layout": old})).unwrap();
    let mut layout = preferences.home_layout.unwrap();
    for kind in [Kind::Favorites, Kind::Inputs] {
        let before = old.widget(kind).unwrap();
        let after = layout.widget(kind).unwrap();
        assert_eq!(
            (
                before.id,
                before.column,
                before.row,
                before.width,
                before.size
            ),
            (after.id, after.column, after.row, after.width, after.size)
        );
        assert_eq!(after.height, if kind == Kind::Favorites { 5 } else { 3 });
    }
    let mut input = layout.widget(Kind::Inputs).unwrap().clone();
    input.row = 16;
    assert!(
        layout.replace(input),
        "The two grid rows beneath Favorites are available again"
    );
}

#[test]
fn home_layout_strip_configuration_rejects_growing_into_a_neighbor() {
    let mut state = custom(0);
    let layout = state.preferences.home_layout.as_mut().unwrap();
    let mut input = layout.widget(Kind::Inputs).unwrap().clone();
    input.row = 16;
    assert!(layout.replace(input));
    let before = state.preferences.home_layout.clone();
    state.focus = Target::AppNamesHome;
    assert!(state.activate().is_empty());
    assert!(!state.preferences.show_home_names);
    assert_eq!(state.preferences.home_layout, before);
    state.edit_layout();
    let id = state.home_widget(Kind::Favorites).unwrap().id;
    state.layout_editor.as_mut().unwrap().mode = Mode::Configure(id);
    action(&mut state, 3);
    assert!(!state.home_widget(Kind::Favorites).unwrap().labels);
    assert!(state.layout_editor.as_ref().unwrap().draft.valid());
    state
        .layout_editor
        .as_mut()
        .unwrap()
        .draft
        .widgets
        .iter_mut()
        .find(|widget| widget.kind == Kind::Inputs)
        .unwrap()
        .row = 18;
    action(&mut state, 3);
    assert!(state.home_widget(Kind::Favorites).unwrap().labels);
    assert_eq!(state.home_widget(Kind::Favorites).unwrap().height, 7);
    assert!(state.layout_editor.as_ref().unwrap().draft.valid());
}

#[test]
fn home_layout_strip_resize_changes_width_not_empty_vertical_space() {
    let mut state = custom(1);
    state.edit_layout();
    let widget = state.home_widget(Kind::Favorites).unwrap().clone();
    state.layout_editor.as_mut().unwrap().mode = Mode::Resize(widget.clone());
    state.focus = Target::LayoutWidget(widget.id);
    for direction in [Direction::Up, Direction::Down, Direction::Down] {
        state.direction(direction, &controls(&state));
    }
    assert_eq!(state.home_widget(Kind::Favorites), Some(&widget));
    state.direction(Direction::Left, &controls(&state));
    let after = state.home_widget(Kind::Favorites).unwrap();
    assert_eq!(after.width, widget.width - 1);
    assert_eq!(after.rect().h, widget.rect().h);
    assert_eq!(after.card_width(), widget.card_width());
}
