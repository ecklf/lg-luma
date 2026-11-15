use super::*;
use crate::{
    renderer::Draw,
    state::{APPS_SECTION, Rect, Subpage},
};

#[test]
fn inputs_have_their_own_sidebar_section_directly_below_apps() {
    use crate::state::{INPUTS_SECTION, SETTINGS_ORDER, WIDGETS_SECTION};
    assert_eq!(
        SETTINGS_ORDER,
        [
            0,
            APPS_SECTION,
            INPUTS_SECTION,
            1,
            WIDGETS_SECTION,
            crate::state::SYSTEM_SECTION
        ]
    );
    let mut state = ready();
    state.open(2);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(APPS_SECTION));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(INPUTS_SECTION));
    let id = state.inventory.inputs[0].id.clone();
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::InputMode(id.clone()));
    assert!(state.subpage.is_none());
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::Input(id.clone())));
    state.back();
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::InputMode(id));
    assert_eq!(state.section, INPUTS_SECTION);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(INPUTS_SECTION));
    state.pointer_focus(Target::Sidebar(WIDGETS_SECTION));
    assert!(
        !controls(&state)
            .iter()
            .any(|control| matches!(control.target, Target::Widget(3) | Target::InputMode(_)))
    );
}

#[test]
fn widget_toolbar_pills_are_fixed_ordered_and_remote_and_pointer_accessible() {
    let mut state = ready();
    state.open(2);
    state.pointer_focus(Target::Sidebar(crate::state::WIDGETS_SECTION));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::WidgetAdd);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::WidgetReset);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::WidgetAdd);
    state.activate();
    assert_eq!(
        state.focus,
        Target::CloseDialog,
        "all default widgets are already installed"
    );
    state.back();
    assert_eq!(state.focus, Target::WidgetAdd);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WidgetCell(0));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Widget(2));
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::Shortcuts));
    state.back();
    assert_eq!(state.focus, Target::Widget(2));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::WidgetReset);
    for language in crate::i18n::Language::ALL {
        state.preferences.language = language;
        let rendered = scene(&state);
        let reset = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::WidgetReset)
            .unwrap();
        let add = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::WidgetAdd)
            .unwrap();
        let shortcuts = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::Widget(2))
            .unwrap();
        assert_eq!(
            (shortcuts.rect.x, shortcuts.rect.y, shortcuts.rect.h),
            (640., 164.8, 64.)
        );
        assert!(shortcuts.rect.x + shortcuts.rect.w + 20. <= reset.rect.x);
        assert_eq!((reset.rect.y, reset.rect.h), (164.8, 64.));
        assert!(reset.rect.x >= 640.);
        assert!(reset.rect.x + reset.rect.w + 20. <= add.rect.x);
        assert_eq!(add.rect.x + add.rect.w, 1760.);
        assert!(!shortcuts.scrollable && !reset.scrollable && !add.scrollable);
        for control in [shortcuts, reset, add] {
            let (x, y) = control.rect.center();
            assert_eq!(
                state.hit_test(&rendered.controls, x, y),
                Some(control.target.clone())
            );
        }
        state.scroll = 100.;
        let scrolled = scene(&state);
        for control in [shortcuts, reset, add] {
            let next = scrolled
                .controls
                .iter()
                .find(|next| next.target == control.target)
                .unwrap();
            assert_eq!((next.rect.x, next.rect.y), (control.rect.x, control.rect.y));
        }
        state.scroll = 0.;
    }
}

#[test]
fn favorite_rows_save_and_keep_remote_navigation_aligned_with_the_drawer() {
    use crate::model::HomeWidget::Favorites;
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.inventory.apps.extend(
        (0..25).map(|index| {
            crate::model::App::new(&format!("extra.{index}"), &format!("Extra {index}"))
        }),
    );
    let ids: Vec<_> = state
        .inventory
        .apps
        .iter()
        .take(13)
        .map(|app| app.id.clone())
        .collect();
    state.preferences.favorites = Some(ids.clone());
    assert_eq!(state.preferences.home_rows, 1);
    state.open(2);
    state.pointer_focus(Target::Sidebar(crate::state::WIDGETS_SECTION));
    state.direction(Direction::Right, &controls(&state));
    for _ in 0..crate::model::HOME_GRID_ROWS {
        state.direction(Direction::Down, &controls(&state));
    }
    assert_eq!(
        state.focus,
        Target::WidgetCell(state.preferences.widget_position(Favorites).cell())
    );
    state.activate();
    assert_eq!(state.focus, Target::ConfigureWidget(Favorites));
    let menu = controls(&state);
    for action in [
        Target::ConfigureWidget(Favorites),
        Target::MoveWidget(Favorites),
        Target::RemoveWidget(Favorites),
    ] {
        assert!(menu.iter().any(|control| control.target == action));
    }
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::FavoriteRows));
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert!(matches!(state.activate().as_slice(), [Effect::Save]));
    assert_eq!(
        state.focus,
        Target::WidgetCell(state.preferences.widget_position(Favorites).cell())
    );
    assert_eq!(state.preferences.favorites.as_ref(), Some(&ids));
    assert_eq!(state.preferences.home_rows, 2);
    let restored: Preferences = serde_json::from_value(json!(state.preferences)).unwrap();
    assert_eq!(restored.home_rows, 2);

    state.open(0);
    let home = controls(&state);
    let card = |index: usize| {
        home.iter()
            .find(|control| control.target == Target::App(ids[index].clone()))
            .unwrap()
            .rect
    };
    assert_eq!(card(0).x, card(6).x);
    assert!(card(0).y + card(0).h < card(6).y);
    assert_eq!(card(0).y, card(12).y);
    assert!(card(5).x < card(12).x);
    assert_eq!(card(6).y, 820.);
    let (x, y) = card(6).center();
    assert_eq!(
        state.hit_test(&home, x, y),
        Some(Target::App(ids[6].clone()))
    );

    state.direction(Direction::Right, &home);
    state.direction(Direction::Down, &home);
    assert_eq!(state.focus, Target::App(ids[7].clone()));
    state.direction(Direction::Down, &home);
    assert_eq!(state.page, Page::Apps);
    assert_eq!(state.focus, Target::App(state.drawer_apps()[1].id.clone()));
    assert!(state.drawer_grid_top() > crate::state::DRAWER_DOCK_Y + state.dock_layout().height());
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.focus, Target::App(ids[7].clone()));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::App(ids[1].clone()));

    state.moving = Some(ids[1].clone());
    assert!(matches!(
        state.direction(Direction::Down, &home).as_slice(),
        [Effect::Save]
    ));
    assert_eq!(state.dock_apps()[7].id, ids[1]);

    state.open(0);
    state.scroll_favorites(10000.);
    assert!((state.home_scroll - crate::state::HOME_TILE_PITCH).abs() < 0.01);
    assert_eq!(state.first_content(), Target::App(ids[7].clone()));

    // The next page holds a top-row card but no card in the lower row.
    state.focus = Target::App(state.dock_apps()[11].id.clone());
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::App(ids[12].clone()));

    state.open(2);
    state.pointer_focus(Target::Sidebar(crate::state::WIDGETS_SECTION));
    state.pointer_focus(Target::WidgetCell(
        state.preferences.widget_position(Favorites).cell(),
    ));
    state.activate();
    state.activate();
    assert_eq!(state.focus, Target::Choice(1));
    state.direction(Direction::Down, &controls(&state));
    assert!(matches!(state.activate().as_slice(), [Effect::Save]));
    assert_eq!(state.preferences.home_rows, 3);
    assert_eq!(
        state.focus,
        Target::WidgetCell(state.preferences.widget_position(Favorites).cell())
    );

    state.open(0);
    let dock = state.dock_apps();
    assert_eq!(dock.len(), ids.len());
    assert!(
        state
            .drawer_apps()
            .iter()
            .any(|app| app.id.starts_with("extra."))
    );
    let third_row = Target::App(dock[12].id.clone());
    state.direction(Direction::Down, &controls(&state));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, third_row);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.page, Page::Apps);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.focus, third_row);
}

#[test]
fn favorites_widget_uses_grid_placement_can_be_removed_and_restores_its_position() {
    use crate::model::{HOME_GRID_COLUMNS, HomeWidget::Favorites, WidgetPosition};

    let mut state = ready();
    state.preferences.reduced_motion = true;
    let favorite_count = state.dock_apps().len();
    let edit_favorites = |state: &mut State| {
        state.open(2);
        state.pointer_focus(Target::Sidebar(crate::state::WIDGETS_SECTION));
        state.pointer_focus(Target::WidgetCell(
            state.preferences.widget_position(Favorites).cell(),
        ));
        state.activate();
    };

    edit_favorites(&mut state);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::MoveWidget(Favorites));
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::WidgetPosition(Favorites)));
    assert_eq!(state.focus, Target::Choice(4 * HOME_GRID_COLUMNS));
    state.direction(Direction::Up, &controls(&state));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Choice(2 * HOME_GRID_COLUMNS));
    assert!(matches!(state.activate().as_slice(), [Effect::Save]));
    assert_eq!(
        state.preferences.widget_position(Favorites),
        WidgetPosition { column: 0, row: 2 }
    );

    edit_favorites(&mut state);
    state.direction(Direction::Down, &controls(&state));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::RemoveWidget(Favorites));
    assert!(matches!(state.activate().as_slice(), [Effect::Save]));
    assert!(!state.preferences.show_favorites_dock);
    assert!(state.dock_apps().is_empty());
    assert_eq!(state.drawer_apps().len(), state.inventory.apps.len());
    state.open(0);
    assert_eq!(state.focus, Target::OpenDrawer);
    assert!(
        controls(&state)
            .iter()
            .any(|item| item.target == Target::OpenDrawer)
    );
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.page, Page::Apps);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::OpenDrawer);

    state.open(2);
    state.pointer_focus(Target::Sidebar(crate::state::WIDGETS_SECTION));
    state.direction(Direction::Right, &controls(&state));
    state.activate();
    assert_eq!(state.focus, Target::AddWidget(Favorites));
    state.activate();
    assert_eq!(state.focus, Target::Choice(2 * HOME_GRID_COLUMNS));
    assert!(matches!(state.activate().as_slice(), [Effect::Save]));
    assert!(state.preferences.show_favorites_dock);
    assert_eq!(state.preferences.favorites_dock_row, Some(2));
    assert_eq!(state.dock_apps().len(), favorite_count);
}

#[test]
fn resizing_favorites_reserves_real_grid_rows_and_repairs_collisions() {
    use crate::model::{
        HomeWidget::{Favorites, Greeting, Weather},
        WidgetPosition,
    };

    let mut prefs = Preferences::default();
    let original_greeting = prefs.widget_position(Greeting);
    let original_weather = prefs.widget_position(Weather);
    for rows in [1, 2, 3] {
        prefs.set_favorite_rows(rows);
        assert_eq!(prefs.widget_position(Favorites).row, 5 - rows);
        assert_eq!(prefs.widget_span(Favorites), (6, rows));
        assert_eq!(prefs.widget_position(Greeting), original_greeting);
        assert_eq!(prefs.widget_position(Weather), original_weather);
    }
    prefs.set_favorite_rows(1);
    assert!(prefs.place_widget(Weather, WidgetPosition { column: 4, row: 2 }));
    assert!(prefs.place_widget(Favorites, WidgetPosition { column: 0, row: 1 }));
    prefs.set_favorite_rows(3);
    for widget in [Greeting, Weather, Favorites] {
        assert!(prefs.can_place_widget(widget, prefs.widget_position(widget)));
    }
    assert_ne!(
        prefs.widget_position(Favorites).row,
        1,
        "two free adjacent rows needed for Weather"
    );
}

#[test]
fn widget_reset_only_restores_membership_and_positions_and_saves_once() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut state = ready();
    state.preferences.show_date = false;
    state.preferences.date_format = crate::model::DateFormat::YearMonthDay;
    state.preferences.weather_style = crate::model::WeatherStyle::Compact;
    state.preferences.temperature_unit = crate::weather::Unit::Fahrenheit;
    state.preferences.greeting_position = WidgetPosition { column: 1, row: 3 };
    state.preferences.show_weather = false;
    let mut expected = state.preferences.clone();
    expected.greeting_position = HomeWidget::Greeting.default_position();
    expected.weather_position = HomeWidget::Weather.default_position();
    expected.show_weather = true;
    expected.show_greeting = true;
    state.open(2);
    state.pointer_focus(Target::Sidebar(crate::state::WIDGETS_SECTION));
    state.pointer_focus(Target::WidgetReset);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(
        serde_json::to_value(&state.preferences).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(state.focus, Target::WidgetReset);
    assert!(state.activate().is_empty());
    let mut loaded: Preferences =
        serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
    loaded.sanitize();
    for widget in HomeWidget::ALL {
        assert!(loaded.has_widget(widget));
        assert_eq!(loaded.widget_position(widget), widget.default_position());
    }
    state.preferences.show_greeting = false;
    state.preferences.show_weather = false;
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_greeting && state.preferences.show_weather);
}

#[test]
fn widget_manager_grid_opens_context_menu_from_every_occupied_cell() {
    use crate::model::HomeWidget;
    let mut state = ready();
    state.open(2);
    state.pointer_focus(Target::Sidebar(crate::state::WIDGETS_SECTION));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::WidgetAdd);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WidgetCell(0));
    assert!(state.subpage.is_none());
    let rendered = scene(&state);
    assert_eq!(
        rendered
            .controls
            .iter()
            .filter(|control| matches!(control.target, Target::WidgetCell(_)))
            .count(),
        crate::model::HOME_GRID_CELLS
    );
    for (cell, widget) in [
        (0, HomeWidget::Greeting),
        (1, HomeWidget::Greeting),
        (2, HomeWidget::Greeting),
        (3, HomeWidget::Greeting),
        (4, HomeWidget::Weather),
        (5, HomeWidget::Weather),
        (10, HomeWidget::Weather),
        (11, HomeWidget::Weather),
        (27, HomeWidget::Favorites),
    ] {
        let control = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::WidgetCell(cell))
            .unwrap();
        let (x, y) = control.rect.center();
        assert_eq!(
            state.hit_test(&rendered.controls, x, y),
            Some(Target::WidgetCell(cell))
        );
        state.pointer_focus(Target::WidgetCell(cell));
        assert!(state.activate().is_empty());
        assert_eq!(state.focus, Target::ConfigureWidget(widget));
        let menu = scene(&state);
        assert!(menu.controls.iter().all(|control| matches!(
            control.target,
            Target::ConfigureWidget(_) | Target::MoveWidget(_) | Target::RemoveWidget(_)
        )));
        assert!(
            menu.controls
                .iter()
                .any(|control| control.target == Target::MoveWidget(widget))
        );
        state.direction(Direction::Down, &menu.controls);
        assert_eq!(state.focus, Target::MoveWidget(widget));
        state.back();
        assert_eq!(state.focus, Target::WidgetCell(cell));
    }
}

#[test]
fn widget_add_is_atomic_cancellable_and_rejects_invalid_placements() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WIDGETS_SECTION;
    state.focus = Target::WidgetCell(4);
    state.activate();
    state.focus = Target::RemoveWidget(HomeWidget::Weather);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.preferences.has_widget(HomeWidget::Weather));
    // Removing releases space immediately and after preference sanitization/restart.
    state.focus = Target::MoveWidget(HomeWidget::Greeting);
    state.activate();
    state.focus = Target::Choice(6);
    assert_eq!(state.activate(), vec![Effect::Save]);
    let mut loaded: Preferences =
        serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
    loaded.sanitize();
    assert_eq!(
        loaded.greeting_position,
        WidgetPosition { column: 0, row: 1 }
    );
    assert!(!loaded.show_weather);
    state.focus = Target::WidgetCell(4);
    state.activate();
    assert_eq!(state.focus, Target::AddWidget(HomeWidget::Weather));
    let catalog = scene(&state);
    assert!(
        !catalog
            .controls
            .iter()
            .any(|control| control.target == Target::AddWidget(HomeWidget::Greeting))
    );
    state.activate();
    assert!(!state.preferences.show_weather);
    state.back();
    assert!(!state.preferences.show_weather);
    state.focus = Target::WidgetCell(4);
    state.activate();
    state.activate();
    for cell in [6, 18, 23] {
        state.focus = Target::Choice(cell);
        assert!(state.activate().is_empty());
        assert!(!state.preferences.show_weather);
    }
    state.pointer_focus(Target::Choice(4));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_weather);
    assert_eq!(
        state.preferences.weather_position,
        WidgetPosition { column: 4, row: 0 }
    );
    assert_eq!(state.focus, Target::WidgetCell(4));
}

#[test]
fn widget_configuration_has_no_visibility_or_position_rows() {
    use crate::model::HomeWidget;
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WIDGETS_SECTION;
    for (cell, expected) in [(0, Subpage::Greeting), (4, Subpage::Weather)] {
        state.focus = Target::WidgetCell(cell);
        state.activate();
        state.activate();
        assert_eq!(state.subpage, Some(expected));
        let rendered = scene(&state);
        assert!(!rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label == state.preferences.language.t("showWidget"))));
        assert!(
            !rendered
                .controls
                .iter()
                .any(|control| matches!(control.target, Target::MoveWidget(_)))
        );
        state.back();
        assert_eq!(state.focus, Target::WidgetCell(cell));
    }
    // All-added empty cells still open a dismissible, focused menu.
    state.focus = Target::WidgetCell(15);
    state.activate();
    assert_eq!(state.focus, Target::CloseDialog);
    assert!(
        scene(&state)
            .controls
            .iter()
            .any(|control| control.target == state.focus)
    );
    state.back();
    assert_eq!(state.focus, Target::WidgetCell(15));
    assert!(state.preferences.has_widget(HomeWidget::Greeting));
}

#[test]
fn widget_position_browsing_cancels_and_valid_selection_saves() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WIDGETS_SECTION;
    state.focus = Target::MoveWidget(HomeWidget::Weather);
    state.activate();
    assert_eq!(state.focus, Target::Choice(4));
    state.direction(Direction::Down, &scene(&state).controls);
    assert_eq!(state.focus, Target::Choice(10));
    assert_eq!(
        state.preferences.weather_position,
        WidgetPosition { column: 4, row: 0 }
    );
    state.back();
    assert_eq!(state.focus, Target::WidgetCell(4));
    state.focus = Target::MoveWidget(HomeWidget::Weather);
    state.activate();
    state.direction(Direction::Down, &scene(&state).controls);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(
        state.preferences.weather_position,
        WidgetPosition { column: 4, row: 1 }
    );
    let loaded: Preferences =
        serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
    assert_eq!(loaded.weather_position, state.preferences.weather_position);
}

#[test]
fn widget_position_picker_blocks_overlaps_and_out_of_bounds_for_remote_and_pointer() {
    use crate::model::HomeWidget;
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WIDGETS_SECTION;
    state.focus = Target::MoveWidget(HomeWidget::Greeting);
    state.activate();
    for cell in [1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 15, 23] {
        state.focus = Target::Choice(cell);
        assert!(state.activate().is_empty());
        assert_eq!(
            state.subpage,
            Some(Subpage::WidgetPosition(HomeWidget::Greeting))
        );
    }
    let rendered = scene(&state);
    for cell in 0..crate::model::HOME_GRID_CELLS {
        let control = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::Choice(cell))
            .unwrap();
        let (x, y) = control.rect.center();
        assert_eq!(
            state.hit_test(&rendered.controls, x, y),
            Some(Target::Choice(cell))
        );
    }
    state.focus = Target::Choice(6);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.greeting_position.row, 1);
}

#[test]
fn six_by_five_widget_navigation_reaches_last_row_and_column_without_wrapping() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut state = ready();
    state.open(2);
    state.focus = Target::MoveWidget(HomeWidget::Greeting);
    state.activate();
    for direction in [
        Direction::Right,
        Direction::Right,
        Direction::Right,
        Direction::Right,
        Direction::Right,
        Direction::Down,
        Direction::Down,
        Direction::Down,
        Direction::Down,
    ] {
        state.direction(direction, &scene(&state).controls);
    }
    assert_eq!(state.focus, Target::Choice(29));
    for direction in [Direction::Right, Direction::Down] {
        state.direction(direction, &scene(&state).controls);
        assert_eq!(state.focus, Target::Choice(29));
    }
    assert!(
        state.activate().is_empty(),
        "Greeting cannot extend past the right edge"
    );
    state.direction(Direction::Left, &scene(&state).controls);
    assert_eq!(state.focus, Target::Choice(28));
    assert!(state.activate().is_empty(), "Greeting spans four columns");
    state.direction(Direction::Left, &scene(&state).controls);
    assert_eq!(state.focus, Target::Choice(27));
    assert!(state.activate().is_empty(), "Greeting spans four columns");
    state.direction(Direction::Left, &scene(&state).controls);
    assert_eq!(state.focus, Target::Choice(26));
    assert!(
        state.activate().is_empty(),
        "Favorites occupies the bottom row"
    );
    state.direction(Direction::Up, &scene(&state).controls);
    assert_eq!(state.focus, Target::Choice(20));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(
        state.preferences.greeting_position,
        WidgetPosition { column: 2, row: 3 }
    );
    state.focus = Target::MoveWidget(HomeWidget::Weather);
    state.activate();
    state.pointer_focus(Target::Choice(6));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(
        state.preferences.weather_position,
        WidgetPosition { column: 0, row: 1 }
    );
    assert!(WidgetPosition::from_cell(crate::model::HOME_GRID_CELLS).is_none());
}

#[test]
fn widget_positions_default_to_requested_spans_and_sanitize_safely() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut preferences: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(HomeWidget::Greeting.span(1), (4, 1));
    assert_eq!(HomeWidget::Weather.span(1), (2, 2));
    assert_eq!(
        preferences.greeting_position,
        WidgetPosition { column: 0, row: 0 }
    );
    assert_eq!(
        preferences.weather_position,
        WidgetPosition { column: 4, row: 0 }
    );
    assert!(preferences.weather_visible());
    for greeting in 0..crate::model::HOME_GRID_CELLS {
        for weather in 0..crate::model::HOME_GRID_CELLS {
            preferences.greeting_position = WidgetPosition::from_cell(greeting).unwrap();
            preferences.weather_position = WidgetPosition::from_cell(weather).unwrap();
            preferences.sanitize();
            for widget in HomeWidget::ALL {
                assert!(preferences.can_place_widget(widget, preferences.widget_position(widget)));
            }
        }
    }
    preferences.greeting_position = WidgetPosition {
        column: 255,
        row: 255,
    };
    preferences.weather_position = WidgetPosition {
        column: 255,
        row: 255,
    };
    preferences.sanitize();
    assert!(preferences.can_place_widget(HomeWidget::Weather, preferences.weather_position));
}

#[test]
fn combined_greeting_upgrades_separate_date_defaults_and_preserves_custom_positions() {
    use crate::model::{HomeWidget, WidgetPosition};
    let mut old: Preferences = serde_json::from_str(
        r#"{"date_position":{"column":0,"row":0},"greeting_position":{"column":0,"row":1},"weather_position":{"column":0,"row":2}}"#,
    )
    .unwrap();
    old.sanitize();
    for widget in HomeWidget::ALL {
        assert_eq!(old.widget_position(widget), widget.default_position());
    }
    let mut custom: Preferences = serde_json::from_str(
        r#"{"greeting_position":{"column":1,"row":3},"weather_position":{"column":3,"row":0}}"#,
    )
    .unwrap();
    custom.sanitize();
    assert_eq!(
        custom.greeting_position,
        WidgetPosition { column: 1, row: 3 }
    );
    assert_eq!(
        custom.weather_position,
        WidgetPosition { column: 3, row: 0 }
    );
    assert!(
        serde_json::to_value(&old)
            .unwrap()
            .get("date_position")
            .is_none()
    );
    let mut current: Preferences = serde_json::from_str(
        r#"{"greeting_position":{"column":0,"row":1},"weather_position":{"column":0,"row":2}}"#,
    )
    .unwrap();
    current.sanitize();
    assert_eq!(
        current.greeting_position,
        WidgetPosition { column: 0, row: 1 }
    );
    assert_eq!(
        current.weather_position,
        WidgetPosition { column: 0, row: 2 }
    );
    let mut previous_default: Preferences = serde_json::from_str(
        r#"{"greeting_position":{"column":0,"row":0},"weather_position":{"column":0,"row":1}}"#,
    )
    .unwrap();
    previous_default.sanitize();
    assert_eq!(
        previous_default.greeting_position,
        WidgetPosition { column: 0, row: 0 }
    );
    assert_eq!(
        previous_default.weather_position,
        WidgetPosition { column: 0, row: 1 }
    );
}

#[test]
fn dock_is_not_a_control_and_its_glass_is_not_clipped_to_the_content_area() {
    let rendered = scene(&ready());
    assert!(
        !rendered
            .controls
            .iter()
            .any(|control| control.rect.x == 60. && control.rect.w == 1800.)
    );
    let mut clip = None;
    let mut dock = false;
    for draw in &rendered.draw {
        match draw {
            Draw::Clip(value) => clip = *value,
            Draw::Glass(rect, _, _) if rect.x == 60. => {
                assert!(clip.is_none());
                dock = true;
            }
            _ => {}
        }
    }
    assert!(dock);
}

#[test]
fn drawer_has_a_compact_gap_even_with_legacy_caption_preferences() {
    for names in [false, true] {
        let mut state = ready();
        state.preferences.reduced_motion = true;
        state.preferences =
            serde_json::from_value(json!({"show_home_names": names, "reduced_motion": true}))
                .unwrap();
        state.open(1);
        let rendered = scene(&state);
        let bottom = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Glass(rect, ..) if rect.x == 60. => Some(rect.y + rect.h),
                _ => None,
            })
            .unwrap();
        assert!((state.drawer_grid_top() - bottom - 24.).abs() < 0.001);
        assert_eq!(state.apps_viewport().y, 164.8);
        state.wheel_scroll(&rendered.controls, Some((900., 900.)), 0, -1000);
        let scrolled = scene(&state);
        let last_bottom = scrolled
            .controls
            .iter()
            .filter(|control| matches!(control.target, Target::App(_)))
            .map(|control| control.rect.y + control.rect.h)
            .fold(0., f32::max);
        assert!((1080. - last_bottom - 24.).abs() < 0.001);
    }
}

#[test]
fn weather_empty_state_uses_a_large_icon_and_setup_action() {
    let state = ready();
    let rendered = scene(&state);
    assert!(rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Image(path, rect, ..) | Draw::TintedImage(path, rect, ..) if path.ends_with("weather/partly-cloudy.png") && rect.w == 144. && rect.h == 144.)));
    assert!(rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, _, style, ..) if label == state.preferences.language.t("setWeatherLocation") && style.size >= 24.)));
    let setup = rendered
        .controls
        .iter()
        .find(|control| control.target == Target::WeatherConfigure)
        .unwrap();
    assert!(setup.rect.h >= 240.);
}

#[test]
fn scrolling_drawer_continues_behind_dock_without_obscuring_remote_focus() {
    for names in [false, true] {
        let mut state = ready();
        state.preferences.reduced_motion = true;
        state.preferences =
            serde_json::from_value(json!({"show_home_names": names, "reduced_motion": true}))
                .unwrap();
        state.open(1);
        state.scroll = 220.;
        let rendered = scene(&state);
        let viewport = state.apps_viewport();
        assert_eq!(viewport.y, 164.8);
        let grid_clip = rendered.draw.iter().position(|draw| matches!(draw,
            Draw::Clip(Some(rect)) if rect.y == viewport.y && rect.x == viewport.x && rect.w == viewport.w)).unwrap();
        let dock_glass = rendered
            .draw
            .iter()
            .position(|draw| {
                matches!(draw,
            Draw::Glass(rect, ..) if rect.x == 60.)
            })
            .unwrap();
        assert!(
            grid_clip < dock_glass,
            "the dock must composite above the scrolling grid"
        );
        let pinned = rendered
            .controls
            .iter()
            .find(|control| matches!(control.target, Target::DockApp(_)))
            .unwrap();
        let (x, y) = pinned.rect.center();
        assert_eq!(
            state.hit_test(&rendered.controls, x, y),
            Some(pinned.target.clone())
        );
        let grid = rendered
            .controls
            .iter()
            .find(|control| matches!(control.target, Target::App(_)))
            .unwrap();
        assert!(grid.rect.y < state.drawer_grid_top() - 8.);
        state.focus = grid.target.clone();
        assert!(state.reveal_focus(&rendered.controls));
        let revealed = scene(&state);
        let selected = revealed
            .controls
            .iter()
            .find(|control| control.target == state.focus)
            .unwrap();
        assert!(selected.rect.y >= state.drawer_grid_top() - 8.);
    }
}

#[test]
fn taller_favorites_dock_scrolls_with_the_drawer() {
    use std::time::{Duration, Instant};

    for rows in [2, 3] {
        let mut state = ready();
        state.preferences.set_favorite_rows(rows);
        state.inventory.apps.extend((0..60).map(|index| {
            crate::model::App::new(&format!("extra.{index}"), &format!("Extra {index}"))
        }));
        state.open(1);
        state.drawer_visibility = 1.;
        let rendered = scene(&state);
        let initial_dock_y = state.dock_y();
        let dock_card = rendered
            .controls
            .iter()
            .find(|control| matches!(control.target, Target::DockApp(_)))
            .unwrap();
        let grid_app = state.first_content();
        let grid_control = |scene: &crate::view::Scene| {
            scene
                .controls
                .iter()
                .find(|control| control.target == grid_app)
                .unwrap()
                .clone()
        };
        let initial_grid_y = grid_control(&rendered).presented_y;

        assert!(state.wheel_scroll(&rendered.controls, Some(dock_card.rect.center()), 0, -2));
        assert_eq!(state.scroll, 180.);
        assert_eq!(
            state.drawer_scroll(),
            0.,
            "the first frame should keep its position"
        );
        assert!(state.animating());
        state.advance_drawer_scroll(Instant::now() + Duration::from_millis(100));
        let midway = state.drawer_scroll();
        assert!(
            (13. ..14.).contains(&midway),
            "a late frame advances by at most one 50 ms animation step"
        );
        let scrolled = scene(&state);
        let card = grid_control(&scrolled);
        assert!((state.dock_y() - (initial_dock_y - midway)).abs() < 0.01);
        assert!((initial_grid_y - card.presented_y - midway).abs() < 0.01);
        assert_eq!(
            state.hit_test(
                &scrolled.controls,
                card.rect.center().0,
                card.presented_y + card.rect.h / 2.
            ),
            Some(grid_app.clone())
        );

        assert!(state.wheel_scroll(&scrolled.controls, None, 0, -1));
        assert_eq!(state.scroll, 270.);
        assert_eq!(
            state.drawer_scroll(),
            midway,
            "a new target should start at the visible position"
        );
        let started = Instant::now();
        for frame in 1..=16 {
            state.advance_drawer_scroll(started + Duration::from_millis(frame * 20));
        }
        assert_eq!(state.drawer_scroll(), 270.);

        state.focus = grid_app;
        let mut prior_scroll = 270.;
        for _ in 0..8 {
            state.direction(Direction::Down, &controls(&state));
            let selected = scene(&state)
                .controls
                .into_iter()
                .find(|control| control.target == state.focus)
                .unwrap();
            assert!(selected.rect.y >= state.apps_viewport().y);
            assert!(selected.rect.y + selected.rect.h <= 1056.1);
            assert!(state.scroll >= prior_scroll);
            prior_scroll = state.scroll;
        }
        assert!(state.scroll > 270.);
        state.advance_drawer_scroll(Instant::now() + Duration::from_millis(100));
        let scrolled_dock_y = state.dock_y();
        state.back();
        assert_eq!(state.page, Page::Home);
        assert_eq!(state.scroll, 0.);
        assert_eq!(state.dock_y(), scrolled_dock_y);
        state.drawer_visibility = 0.;
        assert_eq!(state.dock_y(), state.home_dock_y());

        state.preferences.reduced_motion = true;
        state.open(1);
        assert!(state.wheel_scroll(&controls(&state), None, 0, -2));
        assert_eq!(state.drawer_scroll(), 180.);
        assert!(!state.animating());
    }
}

#[test]
fn settings_sidebar_places_apps_immediately_after_general() {
    let mut state = ready();
    state.open(2);
    let rendered = scene(&state);
    state.direction(Direction::Down, &rendered.controls);
    assert_eq!(state.focus, Target::Sidebar(APPS_SECTION));
    let mut sidebar: Vec<_> = rendered
        .controls
        .iter()
        .filter(|control| matches!(control.target, Target::Sidebar(_)))
        .collect();
    sidebar.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
    assert_eq!(sidebar[0].target, Target::Sidebar(0));
    assert_eq!(sidebar[1].target, Target::Sidebar(APPS_SECTION));
}

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
                (control.rect.x, control.rect.y, control.rect.w, control.rect.h) && *radius == 28.
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
    assert_eq!(state.focus, state.first_input_target());
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WeatherConfigure);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::App("netflix".into()));
    state.direction(Direction::Up, &controls(&state));
    state.direction(Direction::Right, &controls(&state));
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
fn input_pills_respect_visibility_and_launch_without_a_dropdown() {
    let mut state = ready();
    let id = state.inventory.inputs[0].id.clone();
    state.inventory.inputs[0].connected = Some(false);
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
    assert!(!rendered.draw.iter().any(
        |draw| matches!(draw, Draw::Shape(rect, radius, _, _) if rect.w == 8. && *radius == 4.)
    ));
    assert!(!rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains("Connected") || label.contains("Disconnected") || label.contains("No signal"))));
    state.focus = Target::Input(id.clone());
    assert_eq!(state.activate(), vec![Effect::Launch(id.clone())]);
    state
        .preferences
        .input_modes
        .insert(id.clone(), InputMode::Off);
    state.reconcile_focus();
    assert_ne!(state.focus, Target::Input(id));
    assert!(state.overlay.is_none());
}

#[test]
fn input_pills_hug_short_labels_and_center_fitted_text() {
    let mut state = ready();
    let id = state.inventory.inputs[0].id.clone();
    state
        .preferences
        .input_modes
        .insert(id.clone(), InputMode::On);
    let mut widths = Vec::new();
    for title in [
        "PS5",
        "PlayStation 5",
        "An unusually long input label for fitting",
    ] {
        state
            .preferences
            .input_names
            .insert(id.clone(), title.into());
        let rendered = scene(&state);
        let pill = rendered
            .controls
            .iter()
            .find(|control| control.target == Target::Input(id.clone()))
            .unwrap()
            .rect;
        widths.push(pill.w);
        let (label_rect, style) = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(_, rect, style, ..)
                    if rect.x > pill.x && rect.x < pill.x + pill.w && rect.y < 100. =>
                {
                    Some((rect, style))
                }
                _ => None,
            })
            .unwrap();
        assert!((label_rect.center().1 - pill.center().1).abs() < 0.001);
        assert!((label_rect.h - style.size * style.line_height).abs() < 0.001);
        assert!((pill.x + pill.w - label_rect.x - label_rect.w - 14.).abs() < 0.001);
    }
    assert!(
        widths[0] < 148.,
        "short labels must not inherit a minimum width"
    );
    assert!(widths[0] < widths[1]);
    assert!(widths[2] <= 240.);
}

#[test]
fn input_connection_dot_only_appears_on_hover_remote_focus_or_press() {
    for (connected, signal) in [Some(true), Some(false), None]
        .into_iter()
        .flat_map(|connected| [Some(true), Some(false), None].map(|signal| (connected, signal)))
    {
        let mut state = ready();
        let id = state.inventory.inputs[0].id.clone();
        state.inventory.inputs[0].connected = connected;
        state.inventory.inputs[0].signal = signal;
        state
            .preferences
            .input_modes
            .insert(id.clone(), InputMode::On);
        let target = Target::Input(id);
        let pill = controls(&state)
            .into_iter()
            .find(|control| control.target == target)
            .unwrap()
            .rect;
        let dot_color = |state: &State| {
            scene(state).draw.iter().find_map(|draw| match draw {
                Draw::Shape(rect, radius, _, color)
                    if rect.w == 8. && *radius == 4. && pill.contains(rect.x, rect.y) =>
                {
                    Some(*color)
                }
                _ => None,
            })
        };
        assert!(dot_color(&state).is_none());
        state.pointer = false;
        state.focus = target.clone();
        let selected = dot_color(&state).unwrap();
        assert_eq!(
            selected,
            if connected == Some(true) && signal == Some(false) {
                [245. / 255., 184. / 255., 75. / 255., 1.]
            } else if connected == Some(true) {
                [96. / 255., 230. / 255., 160. / 255., 1.]
            } else {
                [140. / 255., 148. / 255., 156. / 255., 1.]
            }
        );
        state.pointer = true;
        assert!(
            dot_color(&state).is_none(),
            "stale pointer focus must not keep the dot visible"
        );
        state.hover = Some(target.clone());
        assert_eq!(dot_color(&state), Some(selected));
        state.hover = None;
        state.pressed = Some(target.clone());
        assert_eq!(dot_color(&state), Some(selected));
        state.pressed = None;
        assert!(dot_color(&state).is_none());
        let after = controls(&state)
            .into_iter()
            .find(|control| control.target == target)
            .unwrap()
            .rect;
        assert_eq!(
            (pill.x, pill.y, pill.w, pill.h),
            (after.x, after.y, after.w, after.h)
        );
        state.overlay = Some(Overlay::Inputs);
        let popup = scene(&state);
        let row = popup
            .controls
            .iter()
            .find(|control| control.target == target)
            .unwrap()
            .rect;
        assert!(popup.draw.iter().any(|draw| matches!(draw,
            Draw::Shape(rect, radius, _, color) if rect.w == 10. && *radius == 5.
                && row.contains(rect.x, rect.y) && *color == selected)));
    }
}

#[test]
fn remote_navigation_ignores_stale_input_hover() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    let id = state.inventory.inputs[0].id.clone();
    state
        .preferences
        .input_modes
        .insert(id.clone(), InputMode::On);
    let target = Target::Input(id.clone());
    state.pointer_focus(target.clone());
    state.hover = Some(target);
    assert_eq!(state.input_status_progress(&id), 1.);

    // A stationary mouse does not send a leave event when the remote takes over.
    state.focus(Target::SettingsMenu, &controls(&state));
    assert!(state.hover.is_some());
    assert!(!state.input_status_active(&id));
    assert_eq!(state.input_status_progress(&id), 0.);

    state.focus(Target::Input(id.clone()), &controls(&state));
    assert_eq!(state.input_status_progress(&id), 1.);
    state.focus(Target::SettingsMenu, &controls(&state));
    state.pointer_focus(Target::Input(id.clone()));
    assert_eq!(state.input_status_progress(&id), 1.);
}

#[test]
fn clock_is_time_only_and_corner_actions_use_luma_and_settings_icons() {
    let mut state = ready();
    let rendered = scene(&state);
    let clock = rendered
        .controls
        .iter()
        .find(|control| control.target == Target::SettingsMenu)
        .unwrap()
        .rect;
    assert!(!rendered.draw.iter().any(|draw| matches!(draw,
        Draw::TintedImage(_, rect, ..) if clock.contains(rect.x, rect.y))));
    let time = rendered
        .draw
        .iter()
        .find_map(|draw| match draw {
            Draw::Text(_, rect, ..) if clock.contains(rect.x, rect.y) => Some(rect),
            _ => None,
        })
        .unwrap();
    assert!((time.center().0 - clock.center().0).abs() < 0.001);
    assert!((time.center().1 - clock.center().1).abs() < 0.001);
    state.focus = Target::SettingsMenu;
    state.activate();
    let menu = scene(&state);
    for (target, icon) in [
        (Target::LumaSettings, "luma-mark.lvec"),
        (Target::TvSettings, "settings.lvec"),
    ] {
        let action = menu
            .controls
            .iter()
            .find(|control| control.target == target)
            .unwrap();
        assert!(menu.draw.iter().any(|draw| matches!(draw,
            Draw::TintedImage(path, rect, ..) if path.ends_with(icon) && action.rect.contains(rect.x, rect.y))));
    }
}

#[test]
fn input_status_slot_animates_width_in_and_out_without_refitting_the_label() {
    for reduced_motion in [false, true] {
        let mut state = ready();
        state.preferences.reduced_motion = reduced_motion;
        let id = state.inventory.inputs[0].id.clone();
        let target = Target::Input(id.clone());
        let geometry = |state: &State| {
            let rendered = scene(state);
            let pill = rendered
                .controls
                .iter()
                .find(|control| control.target == target)
                .unwrap()
                .rect;
            let label = rendered
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(label, rect, style, ..)
                        if rect.x > pill.x && rect.x < pill.x + pill.w && rect.y < 100. =>
                    {
                        Some((label.clone(), rect.x, rect.w, style.size))
                    }
                    _ => None,
                })
                .unwrap();
            (pill, label)
        };
        state.animate_input_status(0.);
        let (collapsed, label) = geometry(&state);
        state.pointer = false;
        state.focus = target.clone();
        state.animate_input_status(0.04);
        let (growing, fitted) = geometry(&state);
        assert_eq!(label, fitted);
        assert!(growing.w > collapsed.w);
        if !reduced_motion {
            assert!(growing.w < collapsed.w + 18.);
        }
        state.animate_input_status(0.16);
        let (expanded, fitted) = geometry(&state);
        assert_eq!(label, fitted);
        assert!((expanded.w - collapsed.w - 18.).abs() < 0.001);
        state.focus = Target::SettingsMenu;
        state.animate_input_status(0.08);
        let (shrinking, _) = geometry(&state);
        if !reduced_motion {
            assert!(shrinking.w > collapsed.w && shrinking.w < expanded.w);
        }
        state.animate_input_status(0.16);
        let (restored, fitted) = geometry(&state);
        assert_eq!(restored.w, collapsed.w);
        assert_eq!(label, fitted);
        assert_eq!(state.input_status_progress(&id), 0.);
    }
}

#[test]
fn reaching_favorites_from_the_drawer_closes_it_and_preserves_the_column() {
    for reduced_motion in [false, true] {
        for favorites in [0, 3, 9] {
            for column in 0..6 {
                let mut state = ready();
                state.preferences.reduced_motion = reduced_motion;
                state.preferences.favorites = Some(
                    state
                        .inventory
                        .apps
                        .iter()
                        .take(favorites)
                        .map(|app| app.id.clone())
                        .collect(),
                );
                // Enough apps to exercise all columns and offscreen rows.
                state.inventory.apps.extend((0..40).map(|index| {
                    crate::model::App::new(&format!("extra.{index}"), &format!("Extra {index}"))
                }));
                let dock = state.dock_apps()[column].id.clone();
                state.open(1);
                state.drawer_visibility = 1.;
                let grid: Vec<_> = state.apps().iter().map(|app| app.id.clone()).collect();
                state.focus = Target::App(grid[12 + column].clone());
                state.scroll = 800.;
                state.direction(Direction::Up, &controls(&state));
                assert_eq!(state.page, Page::Apps);
                assert_eq!(state.focus, Target::App(grid[6 + column].clone()));
                state.direction(Direction::Up, &controls(&state));
                assert_eq!(state.page, Page::Apps);
                assert_eq!(state.focus, Target::App(grid[column].clone()));
                assert!(state.direction(Direction::Up, &controls(&state)).is_empty());
                assert_eq!(state.page, Page::Home);
                assert_eq!(state.focus, Target::App(dock.clone()));
                assert_eq!(state.scroll, 0.);
                assert!(!state.pointer);
                assert!(state.overlay.is_none());
                assert_eq!(
                    state.drawer_visibility,
                    if reduced_motion { 0. } else { 1. }
                );
                assert!(
                    controls(&state)
                        .iter()
                        .any(|control| control.target == state.focus)
                );
                assert_eq!(state.activate(), vec![Effect::Launch(dock)]);
            }
        }
    }
}

#[test]
fn up_from_pinned_drawer_row_closes_drawer_and_preserves_header_focus() {
    for reduced_motion in [false, true] {
        for inputs_visible in [false, true] {
            for index in [0, 3] {
                let mut state = ready();
                state.preferences.reduced_motion = reduced_motion;
                if !inputs_visible {
                    for input in &state.inventory.inputs {
                        state
                            .preferences
                            .input_modes
                            .insert(input.id.clone(), InputMode::Off);
                    }
                }
                state.open(1);
                state.drawer_visibility = 1.;
                state.focus = Target::DockApp(state.favorite_apps()[index].id.clone());
                let expected = if index < 3 {
                    state.first_input_target()
                } else {
                    Target::SettingsMenu
                };
                state.direction(Direction::Up, &controls(&state));
                assert_eq!(state.page, Page::Home);
                assert_eq!(state.focus, expected);
                assert_eq!(state.scroll, 0.);
                assert!(state.overlay.is_none());
                if reduced_motion {
                    assert_eq!(state.drawer_visibility, 0.);
                }
                assert!(
                    controls(&state)
                        .iter()
                        .any(|control| control.target == expected)
                );
            }
        }
    }
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
    assert_eq!(state.focus, Target::App(state.dock_apps()[0].id.clone()));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.page, Page::Apps);
    assert!(matches!(state.focus, Target::App(_)));
    state.inventory.apps.clear();
    state.reconcile_focus();
    assert_eq!(state.focus, Target::SettingsMenu);
    state.back();
    assert_eq!(state.focus, Target::WeatherConfigure);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, state.first_input_target());
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
fn dock_backfills_without_promoting_hidden_or_duplicate_apps() {
    for count in [0, 1, 3, 6, 8] {
        let mut state = ready();
        let ids: Vec<_> = state
            .inventory
            .apps
            .iter()
            .take(count)
            .map(|app| app.id.clone())
            .collect();
        state.preferences.favorites = Some(ids.clone());
        state
            .preferences
            .hidden
            .push(state.inventory.apps.last().unwrap().id.clone());
        let saved = state.preferences.favorites.clone();
        let dock = state.dock_apps();
        assert_eq!(dock.len(), count.max(6));
        assert_eq!(
            dock.iter()
                .take(count)
                .map(|app| app.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
        let dock_ids: std::collections::BTreeSet<_> =
            dock.iter().map(|app| app.id.clone()).collect();
        assert_eq!(dock_ids.len(), dock.len());
        assert!(
            !dock
                .iter()
                .any(|app| state.preferences.hidden.contains(&app.id))
        );
        assert!(
            !state
                .drawer_apps()
                .iter()
                .any(|app| dock_ids.contains(&app.id))
        );
        assert_eq!(
            dock.len() + state.drawer_apps().len(),
            state.inventory.apps.len() - 1
        );
        assert_eq!(state.preferences.favorites, saved);
    }
    let mut state = ready();
    state.inventory.apps.truncate(3);
    state.preferences.favorites = Some(vec![]);
    assert_eq!(state.dock_apps().len(), 3);
    assert!(state.drawer_apps().is_empty());
}

#[test]
fn settings_move_action_reorders_only_saved_favorites() {
    let mut state = ready();
    state.preferences.favorites = Some(vec!["netflix".into(), "amazon".into()]);
    manage_apps(&mut state);
    state.focus = Target::App("netflix".into());
    state.options();
    let actions = controls(&state);
    assert_eq!(actions[1].target, Target::Option(0));
    assert_eq!(actions[2].target, Target::MoveApp);
    state.focus = Target::Option(0);
    state.direction(Direction::Down, &actions);
    assert_eq!(state.focus, Target::MoveApp);
    assert!(
        controls(&state)
            .iter()
            .any(|control| control.target == Target::MoveApp)
    );
    state.focus = Target::MoveApp;
    assert!(state.activate().is_empty());
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.focus, Target::App("netflix".into()));
    assert_eq!(state.moving.as_deref(), Some("netflix"));
    assert!(state.overlay.is_none());
    assert_eq!(
        state.direction(Direction::Right, &controls(&state)),
        vec![Effect::Save]
    );
    assert_eq!(
        state.preferences.favorites.as_ref().unwrap(),
        &["amazon", "netflix"]
    );
    state.back();
    assert!(state.moving.is_none());
    let filler = state.dock_apps()[2].id.clone();
    state.focus = Target::App(filler.clone());
    state.hold();
    assert!(
        state.moving.is_none(),
        "backfilled cards cannot enter reorder mode"
    );
    manage_apps(&mut state);
    state.focus = Target::App(filler);
    state.options();
    assert!(
        !controls(&state)
            .iter()
            .any(|control| control.target == Target::MoveApp)
    );
    state.focus = Target::MoveApp;
    state.activate();
    assert!(state.moving.is_none());
    assert_eq!(state.page, Page::Settings);
    assert_eq!(
        state.preferences.favorites.as_ref().unwrap(),
        &["amazon", "netflix"]
    );
}

#[test]
fn glass_surfaces_are_used_on_home_and_settings() {
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
}

#[test]
fn modal_cards_use_opaque_gray_surfaces_instead_of_glass() {
    let restore = Target::App("netflix".into());
    for overlay in [
        Overlay::WallpaperUpload,
        Overlay::TileEditor,
        Overlay::Options {
            id: "netflix".into(),
            restore: restore.clone(),
        },
        Overlay::Delete {
            id: "netflix".into(),
            restore: restore.clone(),
        },
        Overlay::BackgroundOptions {
            id: "dunes".into(),
            restore: Target::Background(0),
        },
        Overlay::BackgroundDelete {
            id: "dunes".into(),
            restore: Target::Background(0),
        },
        Overlay::Keyboard,
    ] {
        let mut state = ready();
        state.preferences.reduced_motion = true;
        state.overlay = Some(overlay);
        let rendered = scene(&state);
        let panel = rendered.modal.unwrap();
        let same_panel = |rect: &crate::state::Rect| {
            (rect.x, rect.y, rect.w, rect.h) == (panel.x, panel.y, panel.w, panel.h)
        };
        assert!(!rendered.draw.iter().any(|draw| matches!(draw,
            Draw::Glass(rect, ..) if same_panel(rect))));
        if matches!(
            state.overlay,
            Some(Overlay::WallpaperUpload | Overlay::TileEditor)
        ) {
            assert!(rendered.draw.iter().any(|draw| matches!(draw,
                Draw::Shape(rect, _, 0., color) if same_panel(rect)
                    && *color == [27. / 255., 32. / 255., 35. / 255., 1.])));
        } else {
            assert!(rendered.draw.iter().any(|draw| matches!(draw,
                Draw::RoundedGradient(rect, _, top, bottom) if same_panel(rect)
                    && *top == [52. / 255., 56. / 255., 59. / 255., 1.]
                    && *bottom == [36. / 255., 40. / 255., 45. / 255., 1.])));
        }
    }
}

#[test]
fn qr_dialogs_share_one_opaque_background_with_the_quiet_zone_and_logo_plate() {
    for overlay in [Overlay::WallpaperUpload, Overlay::TileEditor] {
        let mut state = ready();
        state.preferences.reduced_motion = true;
        state.overlay = Some(overlay);
        let session = crate::companion::Session {
            url: "http://192.168.1.20:12345/AbCd1234/".into(),
            qr_size: 21,
            qr: vec![true; 21 * 21],
        };
        state.wallpaper_upload = Some(Ok(session.clone()));
        state.tile_editor = Some(Ok(session));
        let rendered = scene(&state);
        let fills: Vec<_> = rendered
            .draw
            .iter()
            .filter_map(|draw| match draw {
                Draw::Shape(rect, _, 0., color)
                    if *color == [27. / 255., 32. / 255., 35. / 255., 1.] =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            fills.len(),
            3,
            "panel, QR quiet zone, and logo plate must match"
        );
        assert_eq!((fills[0].w, fills[0].h), (880., 816.));
        let scale = (336_f32 / 29.).floor();
        assert_eq!((fills[1].w, fills[1].h), (29. * scale, 29. * scale));
        assert_eq!((fills[2].w, fills[2].h), (11. * scale, 11. * scale));
    }
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
