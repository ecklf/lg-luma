use super::*;
use crate::{model::AppDrawerStyle, renderer::Draw, state::Subpage};

#[test]
fn app_navigation_wraps_in_list_order_and_reveals_both_ends() {
    for style in AppDrawerStyle::ALL {
        for (page, dock) in [(0, false), (1, true), (1, false)] {
            let mut state = ready();
            state.preferences.app_drawer_style = style;
            state.preferences.reduced_motion = true;
            let template = state.inventory.apps[0].clone();
            for index in 0..40 {
                let mut app = template.clone();
                app.id = format!("wrap-{index:02}");
                app.title = format!("Wrap {index:02}");
                state.inventory.apps.push(app);
            }
            state.preferences.favorites =
                Some((0..12).map(|index| format!("wrap-{index:02}")).collect());
            state.preferences.hidden = vec!["wrap-00".into(), "wrap-39".into()];
            state.open(page);
            let targets: Vec<_> = if page == 0 || dock {
                state.dock_apps()
            } else {
                state.drawer_apps()
            }
            .iter()
            .map(|app| {
                if dock {
                    Target::DockApp(app.id.clone())
                } else {
                    Target::App(app.id.clone())
                }
            })
            .collect();
            assert!(targets.len() > 6);
            let preferences = serde_json::to_value(&state.preferences).unwrap();
            state.focus(targets[0].clone(), &controls(&state));
            for (direction, expected) in [
                (Direction::Left, targets.last().unwrap()),
                (Direction::Right, &targets[0]),
            ] {
                assert!(state.direction(direction, &controls(&state)).is_empty());
                assert_eq!(&state.focus, expected);
                let rendered = controls(&state);
                let card = rendered.iter().find(|c| c.target == state.focus).unwrap();
                if page == 0 || dock {
                    assert!(card.rect.x >= crate::state::HOME_STRIP_LEFT - 0.001);
                    assert!(card.rect.x + card.rect.w <= crate::state::HOME_STRIP_RIGHT + 0.001);
                } else {
                    assert!(card.rect.y >= state.drawer_grid_top() - 8. - 0.001);
                    assert!(card.rect.y + card.rect.h <= 1080.);
                }
            }
            // Horizontal grid navigation crosses rows rather than wrapping each row.
            state.focus(targets[5].clone(), &controls(&state));
            state.direction(Direction::Right, &controls(&state));
            assert_eq!(state.focus, targets[6]);
            state.direction(Direction::Left, &controls(&state));
            assert_eq!(state.focus, targets[5]);
            assert_eq!(
                serde_json::to_value(&state.preferences).unwrap(),
                preferences
            );
        }
    }
}

#[test]
fn app_navigation_with_one_or_no_apps_stays_valid() {
    for page in [0, 1] {
        let mut state = ready();
        state.inventory.apps.truncate(1);
        state.open(page);
        let target = if page == 0 {
            Target::App(state.inventory.apps[0].id.clone())
        } else {
            Target::DockApp(state.inventory.apps[0].id.clone())
        };
        state.focus = target.clone();
        for direction in [Direction::Left, Direction::Right] {
            assert!(state.direction(direction, &controls(&state)).is_empty());
            assert_eq!(state.focus, target);
        }
        state.inventory.apps.clear();
        state.open(page);
        for direction in [Direction::Left, Direction::Right] {
            state.direction(direction, &controls(&state));
            assert!(controls(&state).iter().any(|c| c.target == state.focus));
        }
    }

    // All but one app are pinned, leaving a one-item drawer.
    let mut state = ready();
    state.inventory.apps.truncate(7);
    state.preferences.favorites = Some(
        state.inventory.apps[..6]
            .iter()
            .map(|app| app.id.clone())
            .collect(),
    );
    state.open(1);
    let target = Target::App(state.drawer_apps()[0].id.clone());
    state.focus = target.clone();
    for direction in [Direction::Left, Direction::Right] {
        assert!(state.direction(direction, &controls(&state)).is_empty());
        assert_eq!(state.focus, target);
    }
}

#[test]
fn drawer_app_names_default_off_and_toggle_persists_with_compact_layout_restored() {
    let legacy: Preferences = serde_json::from_value(json!({"show_home_names": true})).unwrap();
    assert!(!legacy.show_drawer_app_names);
    for style in AppDrawerStyle::ALL {
        let mut state = ready();
        state.preferences.app_drawer_style = style;
        state.preferences.reduced_motion = true;
        for enabled in [false, true, false] {
            state.open(2);
            state.focus = Target::Setting(13);
            state.activate();
            state.direction(Direction::Down, &controls(&state));
            state.direction(Direction::Down, &controls(&state));
            assert_eq!(state.focus, Target::Setting(14));
            if state.preferences.show_drawer_app_names != enabled {
                assert_eq!(state.activate(), vec![Effect::Save]);
            }
            assert_eq!(state.preferences.show_drawer_app_names, enabled);
            let loaded: Preferences =
                serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
            assert_eq!(loaded.show_drawer_app_names, enabled);
            state.open(1);
            let first = Target::App(state.drawer_apps()[0].id.clone());
            let next = Target::App(state.drawer_apps()[6].id.clone());
            state.focus = first.clone();
            let rendered = scene(&state);
            let card = rendered
                .controls
                .iter()
                .find(|c| c.target == first)
                .unwrap();
            let next = rendered.controls.iter().find(|c| c.target == next).unwrap();
            let gap = if enabled { 64. } else { 38.4 };
            assert!((next.rect.y - card.rect.y - card.rect.h - gap).abs() < 0.001);
            assert_eq!(
                rendered.draw.iter().any(|draw| matches!(draw,
                    Draw::Text(_, rect, _, _, true) if rect.y == card.rect.y + card.rect.h + 12.
                )),
                enabled
            );
            state.wheel_scroll(&rendered.controls, None, 0, -1000);
            let rendered = scene(&state);
            let bottom = rendered
                .controls
                .iter()
                .filter(|c| matches!(c.target, Target::App(_)))
                .map(|c| c.rect.y + c.rect.h)
                .fold(0., f32::max);
            let padding = if enabled { 64. } else { 24. };
            assert!((1080. - bottom - padding).abs() < 0.001);
        }
    }
}

#[test]
fn selected_drawer_app_name_is_centered_without_changing_card_geometry() {
    for style in AppDrawerStyle::ALL {
        for pointer in [false, true] {
            let mut state = ready();
            state.preferences.show_drawer_app_names = true;
            state.preferences.app_drawer_style = style;
            state.pointer = pointer;
            state.preferences.reduced_motion = true;
            state.open(1);
            let apps: Vec<_> = state.drawer_apps()[..2]
                .iter()
                .map(|app| (app.id.clone(), app.title.clone()))
                .collect();
            let before = controls(&state);
            for (id, title) in &apps {
                state.focus = Target::App(id.clone());
                let rendered = scene(&state);
                let card = rendered
                    .controls
                    .iter()
                    .find(|c| c.target == state.focus)
                    .unwrap();
                let original = before.iter().find(|c| c.target == state.focus).unwrap();
                assert_eq!(
                    (card.rect.x, card.rect.y, card.rect.w, card.rect.h),
                    (
                        original.rect.x,
                        original.rect.y,
                        original.rect.w,
                        original.rect.h
                    )
                );
                let labels: Vec<_> = rendered
                    .draw
                    .iter()
                    .filter_map(|draw| match draw {
                        Draw::Text(label, rect, _, _, true)
                            if rect.y == card.rect.y + card.rect.h + 12. =>
                        {
                            Some((label, rect))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(labels.len(), 1);
                assert_eq!(labels[0].0, title);
                assert_eq!(labels[0].1.x, card.rect.x);
                assert_eq!(labels[0].1.w, card.rect.w);
                assert!(labels[0].1.y + labels[0].1.h < card.rect.y + card.rect.h + 64.);
            }
            let (id, _) = &apps[0];
            state
                .inventory
                .apps
                .iter_mut()
                .find(|app| &app.id == id)
                .unwrap()
                .title =
                "A very long application name that cannot fit beneath a single card".into();
            state.focus = Target::App(id.clone());
            let rendered = scene(&state);
            assert!(rendered.draw.iter().any(|draw| matches!(draw,
                Draw::Text(label, rect, font, _, true)
                    if label.starts_with("A very") && label.ends_with('…')
                        && rect.w == 270. && font.size == 22.
            )));
        }
    }
}

#[test]
fn drawer_rows_leave_room_below_selected_app_names() {
    for style in AppDrawerStyle::ALL {
        let mut state = ready();
        state.preferences.show_drawer_app_names = true;
        state.preferences.app_drawer_style = style;
        state.preferences.reduced_motion = true;
        state.open(1);
        let first = Target::App(state.drawer_apps()[0].id.clone());
        let next_row = Target::App(state.drawer_apps()[6].id.clone());
        state.focus = first.clone();
        let rendered = scene(&state);
        let card = rendered
            .controls
            .iter()
            .find(|c| c.target == first)
            .unwrap();
        let next = rendered
            .controls
            .iter()
            .find(|c| c.target == next_row)
            .unwrap();
        assert!((next.rect.y - card.rect.y - card.rect.h - 64.).abs() < 0.001);
        let label = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(_, rect, _, _, true) if rect.y == card.rect.y + card.rect.h + 12. => {
                    Some(rect)
                }
                _ => None,
            })
            .unwrap();
        assert!((next.rect.y - label.y - label.h - 24.).abs() < 0.001);
        assert_eq!(next.rect.x, card.rect.x);
        assert_eq!(next.rect.w, card.rect.w);
    }
}

#[test]
fn drawer_scroll_reveals_last_app_name_with_bottom_padding() {
    for style in AppDrawerStyle::ALL {
        for wheel in [false, true] {
            let mut state = ready();
            state.preferences.show_drawer_app_names = true;
            state.preferences.app_drawer_style = style;
            state.preferences.reduced_motion = true;
            let template = state.inventory.apps[0].clone();
            for index in 0..40 {
                let mut app = template.clone();
                app.id = format!("scroll-label-{index}");
                app.title = format!("Scroll label {index}");
                state.inventory.apps.push(app);
            }
            state.open(1);
            state.focus = Target::App(state.drawer_apps().last().unwrap().id.clone());
            let rendered = scene(&state);
            if wheel {
                state.wheel_scroll(&rendered.controls, None, 0, -1000);
            } else {
                assert!(state.reveal_focus(&rendered.controls));
            }
            let rendered = scene(&state);
            let card = rendered
                .controls
                .iter()
                .find(|c| c.target == state.focus)
                .unwrap();
            let label = rendered
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(_, rect, _, _, true)
                        if (rect.y - card.rect.y - card.rect.h - 12.).abs() < 0.001 =>
                    {
                        Some(rect)
                    }
                    _ => None,
                })
                .unwrap();
            assert!((1080. - label.y - label.h - 24.).abs() < 0.001);
            let previous = state.scroll;
            state.wheel_scroll(&rendered.controls, None, 0, -1000);
            assert!((state.scroll - previous).abs() < 0.001);
        }
    }
}

#[test]
fn selected_favorites_and_home_preview_do_not_get_drawer_names() {
    for style in AppDrawerStyle::ALL {
        let mut state = ready();
        state.preferences.show_drawer_app_names = true;
        state.preferences.app_drawer_style = style;
        state.preferences.reduced_motion = true;
        let id = state.dock_apps()[0].id.clone();
        state
            .inventory
            .apps
            .iter_mut()
            .find(|app| app.id == id)
            .unwrap()
            .title = "Unique favorite label".into();
        state.focus = Target::App(id.clone());
        assert!(!scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, ..) if label == "Unique favorite label"
        )));
        state.open(1);
        state.focus = Target::DockApp(id);
        assert!(!scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, ..) if label == "Unique favorite label"
        )));
        let id = state.drawer_apps()[0].id.clone();
        state
            .inventory
            .apps
            .iter_mut()
            .find(|app| app.id == id)
            .unwrap()
            .title = "Unique drawer label".into();
        state.open(0);
        state.focus = Target::App(id);
        assert!(!scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, ..) if label == "Unique drawer label"
        )));
    }
}

#[test]
fn app_drawer_defaults_to_tvos_and_round_trips_both_styles() {
    let legacy: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.app_drawer_style, AppDrawerStyle::Tvos);
    for (style, value) in [
        (AppDrawerStyle::Tvos, "tvos"),
        (AppDrawerStyle::Minimal, "minimal"),
    ] {
        let mut preferences = Preferences {
            app_drawer_style: style,
            ..Preferences::default()
        };
        preferences.sanitize();
        let json = serde_json::to_value(&preferences).unwrap();
        assert_eq!(json["app_drawer_style"], value);
        let loaded: Preferences = serde_json::from_value(json).unwrap();
        assert_eq!(loaded.app_drawer_style, style);
    }
}

#[test]
fn general_groups_drawer_settings_in_a_submenu() {
    let mut state = ready();
    state.open(2);
    let settings: Vec<_> = controls(&state)
        .into_iter()
        .filter_map(|control| match control.target {
            Target::Setting(index) => Some((index, control.rect.y)),
            _ => None,
        })
        .collect();
    assert_eq!(
        settings.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
        [1, 4, 13, 2, 11, 12]
    );
    assert!((settings[3].1 - settings[2].1 - 84.).abs() < 0.001);
    state.focus = Target::Setting(13);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Setting(2));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Setting(13));
}

#[test]
fn app_drawer_picker_saves_only_on_selection_and_restores_its_parent() {
    for pointer in [false, true] {
        let mut state = ready();
        state.preferences.reduced_motion = true;
        state.open(2);
        let row = controls(&state)
            .into_iter()
            .find(|control| control.target == Target::Setting(13))
            .unwrap();
        let (x, y) = row.rect.center();
        assert_eq!(
            state.hit_test(&controls(&state), x, y),
            Some(Target::Setting(13))
        );
        if pointer {
            state.pointer_focus(Target::Setting(13));
        } else {
            state.direction(Direction::Right, &controls(&state));
            for _ in 0..2 {
                state.direction(Direction::Down, &controls(&state));
            }
        }
        assert_eq!(state.focus, Target::Setting(13));
        assert!(state.activate().is_empty());
        assert_eq!(state.subpage, Some(Subpage::AppDrawer));
        assert_eq!(state.focus, Target::Setting(15));
        assert_eq!(
            controls(&state)
                .into_iter()
                .filter_map(|control| match control.target {
                    Target::Setting(index) => Some(index),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [15, 10, 14, crate::state::SMOOTH_DRAWER_FADE_SETTING]
        );
        assert!(state.activate().is_empty());
        assert_eq!(state.subpage, Some(Subpage::AppDrawerStyle));
        assert_eq!(state.focus, Target::Choice(0));
        let rendered = scene(&state);
        for label in ["tvOS", "Minimal"] {
            assert!(rendered.draw.iter().any(|draw| matches!(draw,
                Draw::Text(text, ..) if text == label)));
        }
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::Choice(1));
        assert_eq!(state.preferences.app_drawer_style, AppDrawerStyle::Tvos);
        state.back();
        assert_eq!(state.focus, Target::Setting(15));
        assert_eq!(state.preferences.app_drawer_style, AppDrawerStyle::Tvos);
        state.activate();
        if pointer {
            let option = controls(&state)
                .into_iter()
                .find(|control| control.target == Target::Choice(1))
                .unwrap();
            let (x, y) = option.rect.center();
            assert_eq!(
                state.hit_test(&controls(&state), x, y),
                Some(Target::Choice(1))
            );
            state.pointer_focus(Target::Choice(1));
        } else {
            state.direction(Direction::Down, &controls(&state));
            assert_eq!(state.focus, Target::Choice(1));
        }
        assert_eq!(state.activate(), [Effect::Save]);
        assert_eq!(state.preferences.app_drawer_style, AppDrawerStyle::Minimal);
        assert_eq!(state.focus, Target::Setting(15));
        assert_eq!(state.subpage, Some(Subpage::AppDrawer));
        state.activate();
        assert_eq!(state.focus, Target::Choice(1));
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::Choice(0));
        state.direction(Direction::Left, &controls(&state));
        assert_eq!(state.focus, Target::Setting(15));
        assert_eq!(state.preferences.app_drawer_style, AppDrawerStyle::Minimal);
        state.focus = Target::Setting(14);
        assert_eq!(state.activate(), [Effect::Save]);
        assert!(state.preferences.show_drawer_app_names);
        assert_eq!(state.subpage, Some(Subpage::AppDrawer));
        state.back();
        assert_eq!(state.focus, Target::Setting(13));
        assert!(state.subpage.is_none());
    }
}

#[test]
fn tvos_home_peeks_at_one_noninteractive_app_row_and_minimal_does_not() {
    for (style, expected_peek, gap) in [
        (AppDrawerStyle::Tvos, 6, 57.6),
        (AppDrawerStyle::Minimal, 0, 48.),
    ] {
        let mut state = ready();
        state.preferences.app_drawer_style = style;
        let rendered = scene(&state);
        let glass = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Glass(rect, ..) if rect.x == 60. => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert!((1080. - glass.y - glass.h - gap).abs() < 0.001);
        let preview_cards: Vec<_> = rendered
            .draw
            .iter()
            .filter_map(|draw| match draw {
                Draw::Image(path, rect, ..) if path.starts_with("surfaces/") && rect.y > 1000. => {
                    Some(*rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(preview_cards.len(), expected_peek);
        for rect in preview_cards {
            assert!((1080. - rect.y - 24.).abs() < 0.001);
            assert!((rect.y - glass.y - glass.h - 33.6).abs() < 0.001);
            assert!(rect.y + rect.h > 1080.);
        }
        assert_eq!(
            rendered
                .controls
                .iter()
                .filter(|control| matches!(control.target, Target::App(_)))
                .count(),
            state.dock_apps().len()
        );
        assert!(state.hit_test(&rendered.controls, 120., 1068.).is_none());
        let favorites = state.preferences.favorites.clone();
        state.focus = Target::App(state.dock_apps()[0].id.clone());
        state.direction(Direction::Down, &rendered.controls);
        assert_eq!(state.page, crate::state::Page::Apps);
        state.back();
        assert_eq!(state.page, crate::state::Page::Home);
        assert_eq!(state.focus, Target::App(state.dock_apps()[0].id.clone()));
        assert_eq!(state.preferences.favorites, favorites);
    }
}

#[test]
fn both_app_drawer_styles_share_open_geometry_and_smooth_monotonic_motion() {
    let mut open_geometry = None;
    for style in AppDrawerStyle::ALL {
        let mut state = ready();
        state.preferences.app_drawer_style = style;
        state.open(1);
        let home_y = state.dock_y();
        let mut previous = home_y;
        for step in 1..=20 {
            state.drawer_visibility = step as f32 / 20.;
            let y = state.dock_y();
            assert!(y < previous);
            previous = y;
        }
        assert!((state.dock_y() - 164.8).abs() < 0.001);
        let rendered = scene(&state);
        let geometry: Vec<_> = rendered
            .controls
            .iter()
            .map(|control| {
                (
                    control.target.clone(),
                    control.rect.x,
                    control.rect.y,
                    control.rect.w,
                    control.rect.h,
                )
            })
            .collect();
        if let Some(expected) = &open_geometry {
            assert_eq!(&geometry, expected);
        } else {
            open_geometry = Some(geometry);
        }
        state.preferences.reduced_motion = true;
        state.open(0);
        assert!((state.dock_y() - home_y).abs() < 0.001);
        state.open(1);
        assert!((state.dock_y() - 164.8).abs() < 0.001);
    }
}

#[test]
fn app_drawer_picker_labels_are_localized_and_fit_all_languages() {
    for language in crate::i18n::Language::ALL {
        let mut state = ready();
        state.preferences.language = language;
        state.preferences.reduced_motion = true;
        state.open(2);
        assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(text, ..) if text == language.t("appDrawer"))));
        state.focus = Target::Setting(13);
        state.activate();
        state.activate();
        for (index, style) in AppDrawerStyle::ALL.into_iter().enumerate() {
            let rendered = scene(&state);
            assert!(
                rendered
                    .controls
                    .iter()
                    .any(|control| control.target == Target::Choice(index))
            );
            assert!(rendered.draw.iter().any(|draw| matches!(draw,
                Draw::Text(text, _, font, ..)
                    if text == language.t(style.label_key()) && font.size >= 18.)));
        }
    }
}

#[test]
fn drawer_grid_turns_opaque_before_the_motion_ends() {
    let mut state = ready();
    let mut previous = 0.;
    for step in 0..=100 {
        state.drawer_visibility = step as f32 / 100.;
        let alpha = state.drawer_content_alpha();
        assert!(alpha >= previous);
        previous = alpha;
        // The grid fades with the motion, then rides its last 30% fully opaque.
        if state.drawer_progress() >= 0.7 {
            assert_eq!(alpha, 1.);
        } else {
            assert!(alpha < 1.);
        }
    }
    state.drawer_visibility = 0.;
    assert_eq!(state.drawer_content_alpha(), 0.);
    // Off, the grid fades in over the whole motion, as before.
    state.preferences.smooth_drawer_fade = false;
    for step in 0..=20 {
        state.drawer_visibility = step as f32 / 20.;
        assert_eq!(state.drawer_content_alpha(), state.drawer_progress());
    }
}

#[test]
fn smooth_drawer_fade_toggles_from_the_app_drawer_settings_and_persists() {
    let mut state = ready();
    assert!(state.preferences.smooth_drawer_fade);
    state.open(2);
    state.focus = Target::Setting(crate::state::SMOOTH_DRAWER_FADE_SETTING);
    state.subpage = Some(Subpage::AppDrawer);
    assert!(
        controls(&state)
            .iter()
            .any(|control| control.target == state.focus)
    );
    let effects = state.activate();
    assert!(!state.preferences.smooth_drawer_fade);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Save)));
    let saved = serde_json::to_string(&state.preferences).unwrap();
    let loaded: crate::model::Preferences = serde_json::from_str(&saved).unwrap();
    assert!(!loaded.smooth_drawer_fade);
    // Older preference files without the field get the smooth fade.
    let legacy: crate::model::Preferences = serde_json::from_str("{}").unwrap();
    assert!(legacy.smooth_drawer_fade);
}
