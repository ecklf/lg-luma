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

fn native_key(scancode: u32, keycode: i32, down: bool) -> sdl2::event::Event {
    use sdl2::sys;
    // SAFETY: a zeroed SDL event has initialized scalar keyboard fields. Write
    // the native extension scancode through its integer ABI representation,
    // without constructing an invalid host SDL_Scancode enum discriminant.
    let mut raw: sys::SDL_Event = unsafe { std::mem::zeroed() };
    raw.key.type_ = if down {
        sys::SDL_EventType::SDL_KEYDOWN
    } else {
        sys::SDL_EventType::SDL_KEYUP
    } as u32;
    raw.key.keysym.sym = keycode;
    unsafe {
        std::ptr::addr_of_mut!(raw.key.keysym.scancode)
            .cast::<u32>()
            .write(scancode);
    }
    crate::sdl_events::decode(raw, true)
}

#[test]
fn native_back_and_exit_survive_an_unmapped_or_layout_dependent_keycode() {
    use sdl2::{event::Event, keyboard::Keycode};
    for down in [true, false] {
        for keycode in [0, crate::WEBOS_BACK_KEYCODE, i32::from(Keycode::A)] {
            let event = native_key(482, keycode, down);
            assert!(matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::AcBack),
                    ..
                } | Event::KeyUp {
                    keycode: Some(Keycode::AcBack),
                    ..
                }
            ));
        }
        let event = native_key(505, 0, down);
        let key = match event {
            Event::KeyDown { keycode, .. } | Event::KeyUp { keycode, .. } => keycode.unwrap(),
            _ => panic!("Not a keyboard event"),
        };
        assert_eq!(i32::from(key), crate::sdl_events::WEBOS_EXIT_KEYCODE);
    }
    for scancode in [484, 485] {
        assert!(matches!(
            native_key(scancode, (1 << 30) | scancode as i32, true),
            Event::KeyDown { keycode: None, .. }
        ));
    }
    assert!(matches!(
        native_key(41, i32::from(Keycode::Escape), true),
        Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        }
    ));
}

#[test]
fn back_dismissal_consumes_repeats_native_exit_and_release_without_quitting() {
    use sdl2::keyboard::Keycode;
    let mut state = ready();
    state.open_recents();
    let start = Instant::now();
    let mut button = BackButton::default();
    assert!(
        button
            .key_down(&mut state, Keycode::AcBack, start)
            .is_empty()
    );
    assert!(!state.recents_open());
    // Even a repeat incorrectly marked as a fresh keydown cannot arm Home's
    // quit path after this same physical press dismissed the switcher.
    assert!(
        button
            .key_down(
                &mut state,
                Keycode::AcBack,
                start + Duration::from_millis(50)
            )
            .is_empty()
    );
    assert!(!button.native_hold(start + Duration::from_millis(700)));
    assert!(!button.held(start + Duration::from_millis(800)));
    assert_eq!(
        button.release(Keycode::AcBack, start + Duration::from_secs(1)),
        None
    );
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.focus, Target::Nav(0));
}

#[test]
fn native_hold_opens_once_and_the_next_physical_back_press_dismisses() {
    use sdl2::keyboard::Keycode;
    let start = Instant::now();
    let mut state = ready();
    let mut button = BackButton::default();
    assert!(
        button
            .key_down(&mut state, Keycode::AcBack, start)
            .is_empty()
    );
    assert!(button.held(start + Duration::from_millis(600)));
    state.open_recents();
    assert!(
        button
            .key_down(
                &mut state,
                Keycode::AcBack,
                start + Duration::from_millis(650)
            )
            .is_empty()
    );
    assert!(state.recents_open());
    assert!(!button.native_hold(start + Duration::from_millis(700)));
    assert_eq!(
        button.release(Keycode::AcBack, start + Duration::from_secs(1)),
        None
    );
    button.cancel(); // native Exit keyup
    assert!(state.recents_open());
    assert!(
        button
            .key_down(&mut state, Keycode::AcBack, start + Duration::from_secs(2))
            .is_empty()
    );
    assert!(!state.recents_open());
    assert_eq!(
        button.release(Keycode::AcBack, start + Duration::from_secs(3)),
        None
    );
    // Firmware may emit only the already-classified long-Back Exit key.
    assert!(button.native_hold(start + Duration::from_secs(4)));
    assert!(!state.open_recents().is_empty());
    assert!(!button.native_hold(start + Duration::from_secs(5)));
    button.cancel();
    assert!(state.recents_open());
    assert_eq!(
        button.release(Keycode::AcBack, start + Duration::from_secs(6)),
        None
    );
}

#[test]
fn a_short_back_on_home_opens_recents_and_the_next_press_restores_the_app() {
    use sdl2::keyboard::Keycode;
    let mut state = ready();
    state.focus = Target::App("youtube.leanback.v4".into());
    let mut button = BackButton::default();
    let start = Instant::now();
    assert!(
        button
            .key_down(&mut state, Keycode::AcBack, start)
            .is_empty()
    );
    assert_eq!(
        button.release(Keycode::AcBack, start + Duration::from_millis(100)),
        Some(false)
    );
    assert_eq!(
        state.back(),
        vec![Effect::Recent {
            generation: 1,
            command: Command::List
        }]
    );
    accept(&mut state, recents::demo());
    assert!(state.recents_open());
    assert!(
        button
            .key_down(&mut state, Keycode::AcBack, start + Duration::from_secs(1))
            .is_empty()
    );
    assert_eq!(
        button.release(Keycode::AcBack, start + Duration::from_millis(1100)),
        None
    );
    assert!(!state.recents_open());
    assert_eq!(state.focus, Target::App("youtube.leanback.v4".into()));
}

#[test]
fn home_header_has_only_navigation_and_tv_settings() {
    let state = ready();
    let header: Vec<_> = controls(&state)
        .into_iter()
        .filter(|control| control.rect.y < 120.)
        .collect();
    assert_eq!(header.len(), 4);
    assert!(
        header
            .iter()
            .all(|control| matches!(control.target, Target::Nav(_) | Target::TvSettings))
    );
}

#[test]
fn recents_has_compact_top_right_close_all_without_a_dismiss_button_or_footer() {
    use crate::renderer::Draw;
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_recents();
    accept(&mut state, recents::demo());
    let scene = scene(&state);
    assert!(
        scene
            .controls
            .iter()
            .all(|control| control.target != Target::CloseDialog)
    );
    let all = scene
        .controls
        .iter()
        .find(|control| control.target == Target::RecentCloseAll)
        .unwrap()
        .rect;
    assert_eq!(all.y, 132.);
    assert!((all.x + all.w - 1836.).abs() < 0.01);
    let label = scene
        .draw
        .iter()
        .find_map(|draw| match draw {
            Draw::Text(value, rect, ..) if value == "Close all" => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert!((all.x + all.w - label.x - label.w - 20.).abs() < 0.01);
    assert!(scene.draw.iter().all(|draw| !matches!(draw,
        Draw::Text(value, ..) if value == "1 / 8" || value.contains("Close app") || value.contains("OK  Open"))));
    state.direction(Direction::Up, &scene.controls);
    assert!(matches!(state.focus, Target::RecentClose(_)));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::RecentCloseAll);
    state.direction(Direction::Down, &controls(&state));
    assert!(matches!(state.focus, Target::Recent(_)));
}

#[test]
fn youtube_recents_uses_the_apps_vector_and_the_same_relative_logo_geometry() {
    use crate::renderer::Draw;
    let bounds = |scene: &view::Scene, suffix: &str| {
        scene
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Image(path, rect, ..) | Draw::TintedImage(path, rect, ..)
                    if path.ends_with(suffix) =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap()
    };
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open(1);
    let apps = scene(&state);
    let app_face = bounds(&apps, "surfaces/youtube.png");
    let app_logo = bounds(&apps, "logos/youtube.lvec");
    state.open_recents();
    accept(&mut state, recents::normalize(&json!({"recentsAppList":[{
        "appId":"youtube.leanback.v4", "title":"YouTube",
        "customImageFilePath":"/media/developer/apps/usr/palm/applications/youtube.leanback.v4/imageForRecents.png"
    }]})).unwrap());
    let recent = scene(&state);
    let recent_face = bounds(&recent, "surfaces/youtube.png");
    let recent_logo = bounds(&recent, "logos/youtube.lvec");
    assert!((app_logo.w / app_face.w - recent_logo.w / recent_face.w).abs() < 0.0001);
    assert!((app_logo.h / app_face.h - recent_logo.h / recent_face.h).abs() < 0.0001);
    assert!((recent_logo.center().0 - recent_face.center().0).abs() < 0.01);
    assert!((recent_logo.center().1 - recent_face.center().1).abs() < 0.01);
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
    state.direction(Direction::Up, &controls(&state));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::RecentCloseAll);
    state.direction(Direction::Down, &controls(&state));
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
    assert_eq!(state.focus, Target::RecentCloseAll);
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
        Target::Recent(_) | Target::RecentClose(_) | Target::RecentCloseAll
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
