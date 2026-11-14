use super::*;
use crate::recents::{self, Command};

fn tabbed() -> State {
    let mut state = ready();
    state.preferences.tab_navigation = true;
    state.preferences.reduced_motion = true;
    state
}

#[test]
fn general_settings_can_enable_and_disable_tabs_without_changing_favorites() {
    let mut state = ready();
    let favorites = state.preferences.favorites.clone();
    state.open(2);
    state.section = 0;
    state.focus = Target::Setting(12);
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Setting(12))
    );
    assert!(!state.preferences.tab_navigation);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.tab_navigation);
    assert_eq!(state.preferences.favorites, favorites);
    assert_eq!(state.page, Page::Settings);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.preferences.tab_navigation);
    assert_eq!(state.preferences.favorites, favorites);
}

#[test]
fn tabs_open_distinct_pages_and_home_down_does_not_open_a_drawer() {
    let mut state = tabbed();
    state.open(0);
    let first = state.focus.clone();
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.focus, first);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Nav(0));
    state.direction(Direction::Right, &controls(&state));
    state.activate();
    assert_eq!(state.page, Page::Apps);
    assert_eq!(state.drawer_progress(), 0.);
    let rendered = scene(&state);
    assert!(
        !rendered
            .controls
            .iter()
            .any(|c| matches!(c.target, Target::DockApp(_)))
    );
    assert_eq!(state.apps().len(), state.inventory.apps.len());
    for index in 0..3 {
        assert!(
            rendered
                .controls
                .iter()
                .any(|c| c.target == Target::Nav(index))
        );
    }
    state.direction(Direction::Down, &rendered.controls);
    assert!(matches!(state.focus, Target::App(_)));
    let app = rendered
        .controls
        .iter()
        .find(|c| c.target == state.focus)
        .unwrap();
    assert_eq!(
        state.hit_test(&rendered.controls, app.rect.x + 15., app.rect.y + 15.),
        Some(state.focus.clone())
    );
    state.direction(Direction::Up, &rendered.controls);
    assert_eq!(state.focus, Target::Nav(1));
    state.direction(Direction::Right, &rendered.controls);
    state.activate();
    assert_eq!(state.page, Page::Settings);
    assert!(matches!(state.focus, Target::Sidebar(_)));
}

#[test]
fn close_all_is_reachable_busy_guarded_and_refreshes_after_success() {
    let mut state = tabbed();
    state.open_recents();
    state.recent_response(
        state.recents.generation,
        Command::List,
        Ok(Some(recents::demo())),
    );
    let expected: Vec<_> = state
        .recents
        .entries
        .iter()
        .map(|e| e.app.id.clone())
        .collect();
    let rendered = controls(&state);
    let close_all = rendered
        .iter()
        .find(|c| c.target == Target::RecentCloseAll)
        .unwrap();
    assert_eq!(close_all.rect.center().0, 960.);
    assert!(close_all.rect.y >= 900.);
    assert!(close_all.rect.y + close_all.rect.h <= 1080.);
    assert_eq!(
        state.hit_test(
            &rendered,
            close_all.rect.center().0,
            close_all.rect.center().1
        ),
        Some(Target::RecentCloseAll)
    );
    state.direction(Direction::Down, &rendered);
    assert_eq!(state.focus, Target::RecentCloseAll);
    let action = state.activate();
    assert_eq!(
        action,
        vec![Effect::Recent {
            generation: 2,
            command: Command::CloseAll(expected.clone())
        }]
    );
    assert!(state.activate().is_empty());
    let refresh = state.recent_response(2, Command::CloseAll(expected), Ok(None));
    assert_eq!(
        refresh,
        vec![Effect::Recent {
            generation: 3,
            command: Command::List
        }]
    );
    state.recent_response(3, Command::List, Ok(Some(vec![])));
    assert!(state.recents.entries.is_empty());
    assert_eq!(state.focus, Target::RecentEmpty);
}

#[test]
fn tabbed_recents_keeps_up_for_one_card_and_returns_from_close_all_without_closing() {
    let mut state = tabbed();
    state.open_recents();
    let mut entries = recents::demo();
    state.recent_response(1, Command::List, Ok(Some(entries.clone())));
    state.direction(Direction::Right, &controls(&state));
    let selected = state.focus.clone();
    let Target::Recent(id) = selected.clone() else {
        panic!("Expected a focused recent card");
    };

    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::RecentCloseAll);
    assert!(state.recents.pending.is_none());
    assert!(state.direction(Direction::Up, &controls(&state)).is_empty());
    assert_eq!(state.focus, selected);
    assert!(state.recents.pending.is_none());
    assert_eq!(
        state.direction(Direction::Up, &controls(&state)),
        vec![Effect::Recent {
            generation: 2,
            command: Command::Close(id.clone()),
        }]
    );
    assert!(state.direction(Direction::Up, &controls(&state)).is_empty());
    state.recent_response(2, Command::Close(id.clone()), Ok(None));
    entries.retain(|e| e.app.id != id);
    state.recent_response(3, Command::List, Ok(Some(entries.clone())));
    assert_eq!(state.recents.entries.len(), entries.len());
    assert_eq!(entries.len(), recents::demo().len() - 1);
    assert!(matches!(state.focus, Target::Recent(_)));
}

#[test]
fn close_all_remains_reachable_when_empty_or_after_an_error() {
    let mut state = tabbed();
    state.open_recents();
    state.recent_response(1, Command::List, Ok(Some(vec![])));
    assert!(state.direction(Direction::Up, &controls(&state)).is_empty());
    assert_eq!(state.focus, Target::RecentEmpty);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::RecentCloseAll);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::RecentEmpty);
    state.recents.error = Some("Close refused".into());
    state.focus = Target::RecentRetry;
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::RecentCloseAll);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::RecentRetry);
}

#[test]
fn close_all_failure_keeps_cards_available_and_restores_navigation() {
    let mut state = tabbed();
    state.open_recents();
    state.recent_response(
        state.recents.generation,
        Command::List,
        Ok(Some(recents::demo())),
    );
    let ids = state
        .recents
        .entries
        .iter()
        .map(|e| e.app.id.clone())
        .collect();
    state.recent_request(Command::CloseAll(ids));
    let command = state.recents.pending.clone().unwrap();
    state.recent_response(
        state.recents.generation,
        command,
        Err("Close refused".into()),
    );
    assert!(state.recents.pending.is_none());
    assert!(!state.recents.entries.is_empty());
    assert_eq!(state.recents.error.as_deref(), Some("Close refused"));
}

#[test]
fn background_batch_excludes_home_core_services_and_overlays() {
    for id in [
        crate::platform::HOME_ID,
        crate::platform::APP_ID,
        "com.webos.app.livetv",
        "airplay",
    ] {
        assert!(!recents::valid_recent_id(id));
        assert!(Command::Close(id.into()).request().is_err());
    }
    let history = recents::normalize(&json!({"ready":true,"recentsAppList":[
        {"appId":"com.webos.app.livetv"},{"appId":"airplay"},{"appId":"netflix"}
    ]}))
    .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].app.id, "netflix");
    let reply = json!({"running": [
        {"id":"com.webos.app.home","defaultWindowType":"card"},
        {"id":"com.ecklf.luma","defaultWindowType":"card"},
        {"id":"com.webos.app.livetv","defaultWindowType":"card"},
        {"id":"airplay","defaultWindowType":"card"},
        {"id":"com.webos.app.voice","defaultWindowType":"overlay"},
        {"id":"com.webos.app.screensaver","defaultWindowType":"screenSaver"},
        {"id":"netflix","defaultWindowType":"card"},
        {"id":"netflix","defaultWindowType":"card"},
        {"id":"youtube.leanback.v4","defaultWindowType":"card"},
        {"id":"bad';command","defaultWindowType":"card"}
    ]});
    assert_eq!(
        recents::background_app_ids(&reply).unwrap(),
        ["netflix", "youtube.leanback.v4"]
    );
    assert!(recents::background_app_ids(&json!({})).is_err());
}
