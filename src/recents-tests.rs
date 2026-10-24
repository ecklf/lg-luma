use super::*;
use crate::{
    BackButton,
    recents::{self, Command},
};
use std::time::{Duration, Instant};

fn accept(state: &mut State, entries: Vec<recents::Entry>) {
    let pending = state.recent_response(state.recents.generation, Command::List, Ok(Some(entries)));
    assert!(pending.is_empty());
}

#[test]
fn system_history_preserves_order_and_inputs_and_rejects_invalid_entries() {
    let entries = recents::normalize(&json!({"ready": true, "recentsAppList": [
        {"appId":"youtube.leanback.v4", "title":" YouTube\n "},
        {"appId":"com.ecklf.luma"}, {"appId":"com.webos.app.home"},
        {"appId":"com.luma.launcher"}, {"appId":"invalid; command"},
        {"appId":"com.webos.app.hdmi1", "title":"Console"},
        {"appId":"netflix", "title":"Netflix"},
        {"appId":"youtube.leanback.v4", "title":"Duplicate"}
    ]}))
    .unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|e| e.app.id.as_str())
            .collect::<Vec<_>>(),
        ["youtube.leanback.v4", "com.webos.app.hdmi1", "netflix"]
    );
    assert_eq!(entries[0].app.title, "YouTube");
    assert!(recents::normalize(&json!({"ready":false,"recentsAppList":[]})).is_err());
    assert!(recents::normalize(&json!({})).is_err());
    assert!(recents::normalize(&json!({"recentsAppList":{}})).is_err());
}

#[test]
fn actions_match_stock_home_without_synthetic_process_kills() {
    assert_eq!(
        Command::Close("netflix".into()).request().unwrap(),
        (
            "luna://com.webos.service.recent/removeRecent",
            json!({"appId":"netflix"})
        )
    );
    assert_eq!(
        Command::CloseAll.request().unwrap(),
        (
            "luna://com.webos.service.recent/removeAllRecents",
            json!({})
        )
    );
    assert_eq!(
        Command::Launch("netflix".into()).request().unwrap(),
        (
            "luna://com.webos.service.applicationmanager/launch",
            json!({
                "id":"netflix", "params":{}, "reason":"recent",
                "checkUpdateOnLaunch":true, "autoInstallation":true
            })
        )
    );
    for id in [
        "com.ecklf.luma",
        "com.webos.app.home",
        "invalid'; command",
        "",
    ] {
        assert!(Command::Close(id.into()).request().is_err());
        assert!(Command::Launch(id.into()).request().is_err());
    }
}

#[test]
fn back_distinguishes_taps_holds_delayed_releases_and_cancelled_presses() {
    use sdl2::keyboard::Keycode;
    let start = Instant::now();
    let mut button = BackButton::default();
    button.down(Keycode::Escape, start);
    assert!(!button.held(start + Duration::from_millis(599)));
    assert_eq!(
        button.release(Keycode::Escape, start + Duration::from_millis(100)),
        Some(false)
    );
    button.down(Keycode::Escape, start);
    // A repeated down cannot reset the timer.
    button.down(Keycode::Escape, start + Duration::from_millis(500));
    assert!(button.held(start + Duration::from_millis(600)));
    assert!(!button.held(start + Duration::from_millis(900)));
    assert_eq!(
        button.release(Keycode::Escape, start + Duration::from_secs(1)),
        None
    );
    let webos = Keycode::from_i32(crate::WEBOS_BACK_KEYCODE).unwrap();
    assert!(crate::is_back(webos, true));
    button.down(webos, start);
    assert_eq!(
        button.release(Keycode::Escape, start + Duration::from_secs(1)),
        None
    );
    assert_eq!(
        button.release(webos, start + Duration::from_secs(1)),
        Some(true)
    );
    button.down(webos, start);
    button.cancel();
    assert!(!button.held(start + Duration::from_secs(1)));
    assert_eq!(button.release(webos, start + Duration::from_secs(1)), None);
}

#[test]
fn recents_is_modal_and_restores_the_exact_settings_parent() {
    let mut state = ready();
    state.open(2);
    state.subpage = Some(crate::state::Subpage::Clock);
    state.focus = Target::ClockFormat;
    assert_eq!(
        state.open_recents(),
        vec![Effect::Recent {
            generation: 1,
            command: Command::List
        }]
    );
    accept(&mut state, recents::demo());
    assert!(state.recents_open());
    assert_eq!(state.focus, Target::Recent("netflix".into()));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Recent("youtube.leanback.v4".into()));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(
        state.focus,
        Target::RecentClose("youtube.leanback.v4".into())
    );
    state.direction(Direction::Down, &controls(&state));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::RecentCloseAll);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Recent("youtube.leanback.v4".into()));
    assert!(state.back().is_empty());
    assert_eq!(state.focus, Target::ClockFormat);
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.page, Page::Settings);
}

#[test]
fn closing_is_serialized_refreshes_on_success_and_keeps_a_neighbor_focused() {
    let mut state = ready();
    state.open_recents();
    accept(&mut state, recents::demo());
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(
        state.activate(),
        vec![Effect::Recent {
            generation: 2,
            command: Command::Close("netflix".into())
        }]
    );
    assert!(state.activate().is_empty());
    assert!(state.recents.entries.iter().any(|e| e.app.id == "netflix"));
    let next = state.recent_response(2, Command::Close("netflix".into()), Ok(None));
    assert_eq!(
        next,
        vec![Effect::Recent {
            generation: 3,
            command: Command::List
        }]
    );
    assert_eq!(
        state.focus,
        Target::RecentClose("youtube.leanback.v4".into())
    );
    let remaining = state.recents.entries.clone();
    accept(&mut state, remaining);
    assert_eq!(
        state.focus,
        Target::RecentClose("youtube.leanback.v4".into())
    );
    state.focus = Target::RecentCloseAll;
    state.activate();
    let next = state.recent_response(4, Command::CloseAll, Ok(None));
    assert_eq!(
        next,
        vec![Effect::Recent {
            generation: 5,
            command: Command::List
        }]
    );
    accept(&mut state, vec![]);
    assert_eq!(state.focus, Target::CloseDialog);
    assert!(
        controls(&state)
            .iter()
            .all(|c| c.target != Target::RecentCloseAll)
    );
}

#[test]
fn failed_actions_keep_history_and_stale_replies_cannot_reopen_or_replace_a_session() {
    let mut state = ready();
    state.open_recents();
    accept(&mut state, recents::demo());
    state.direction(Direction::Up, &controls(&state));
    state.activate();
    state.recent_response(2, Command::Close("netflix".into()), Err("denied".into()));
    assert_eq!(state.recents.entries[0].app.id, "netflix");
    assert!(state.recents.error.is_some());
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::RecentRetry)
    );
    state.back();
    state.open_recents();
    assert!(
        state
            .recent_response(2, Command::Close("netflix".into()), Ok(None))
            .is_empty()
    );
    assert!(state.recents.entries.is_empty());
    state.recent_response(
        state.recents.generation,
        Command::List,
        Err("inventory unavailable".into()),
    );
    assert!(
        state.recents.entries.is_empty(),
        "TV failures must never populate demo history"
    );
    state.focus = Target::RecentRetry;
    assert!(!state.activate().is_empty());
    accept(&mut state, recents::demo());
    state.focus = Target::Recent("netflix".into());
    state.activate();
    let generation = state.recents.generation;
    state.recent_response(generation, Command::Launch("netflix".into()), Ok(None));
    assert!(!state.recents_open());
    assert_eq!(state.focus, Target::Nav(0));
}

#[test]
fn recents_pointer_focus_never_moves_hit_regions_and_reduced_motion_snaps() {
    let mut state = ready();
    state.open_recents();
    accept(&mut state, recents::demo());
    let before = controls(&state);
    for target in [
        Target::Recent("youtube.leanback.v4".into()),
        Target::RecentClose("netflix".into()),
    ] {
        state.pointer_focus(target.clone());
        let after = controls(&state);
        let a = before.iter().find(|c| c.target == target).unwrap().rect;
        let b = after.iter().find(|c| c.target == target).unwrap().rect;
        assert_eq!((a.x, a.y, a.w, a.h), (b.x, b.y, b.w, b.h));
    }
    assert!(controls(&state).iter().all(|c| matches!(
        c.target,
        Target::Recent(_) | Target::RecentClose(_) | Target::RecentCloseAll | Target::CloseDialog
    )));
    state.recents.selected = 3;
    state.recents.position = 1.5;
    state.tick();
    assert_eq!(
        state.recents.position, 1.5,
        "Pointer input interrupts carousel motion"
    );
    state.preferences.reduced_motion = true;
    state.pointer = false;
    state.recents.selected = 7;
    state.tick();
    assert_eq!(state.recents.position, 7.);
    assert!(
        controls(&state)
            .iter()
            .any(|c| matches!(c.target, Target::Recent(_)))
    );
}
