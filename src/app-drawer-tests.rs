use super::*;
use crate::{model::AppDrawerStyle, renderer::Draw, state::Subpage};

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
fn general_groups_app_sorting_and_app_drawer_in_visual_and_remote_order() {
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
        [1, 4, 13, 10, 2, 11, 12]
    );
    assert!((settings[3].1 - settings[2].1 - 84.).abs() < 0.001);
    state.focus = Target::Setting(13);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Setting(10));
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
        assert_eq!(state.focus, Target::Setting(13));
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
        assert_eq!(state.focus, Target::Setting(13));
        assert!(state.subpage.is_none());
        state.activate();
        assert_eq!(state.focus, Target::Choice(1));
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::Choice(0));
        state.direction(Direction::Left, &controls(&state));
        assert_eq!(state.focus, Target::Setting(13));
        assert_eq!(state.preferences.app_drawer_style, AppDrawerStyle::Minimal);
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
