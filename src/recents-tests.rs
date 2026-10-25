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
fn recents_has_only_cards_without_action_buttons_or_footer() {
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
    assert!(scene.draw.iter().all(|draw| !matches!(draw,
        Draw::Text(value, ..) if value == "1 / 8" || value.contains("Close app") || value.contains("OK  Open"))));
    assert!(
        scene.draw.iter().all(|draw| !matches!(draw,
        Draw::Text(value, ..) if value == "Netflix" || value == "YouTube" || value == "Close all"))
    );
    assert!(
        scene
            .controls
            .iter()
            .all(|control| matches!(control.target, Target::Recent(_)))
    );
    assert!(state.direction(Direction::Down, &scene.controls).is_empty());
    assert_eq!(state.focus, Target::Recent("netflix".into()));
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
    assert_eq!(
        state.direction(Direction::Up, &controls(&state)),
        vec![Effect::Recent {
            generation: 2,
            command: Command::Close("netflix".into())
        }]
    );
    assert!(state.activate().is_empty());
    assert!(state.direction(Direction::Up, &controls(&state)).is_empty());
    assert!(state.recents.entries.iter().any(|e| e.app.id == "netflix"));
    let next = state.recent_response(2, Command::Close("netflix".into()), Ok(None));
    assert_eq!(
        next,
        vec![Effect::Recent {
            generation: 3,
            command: Command::List
        }]
    );
    assert_eq!(state.focus, Target::Recent("youtube.leanback.v4".into()));
    let remaining = state.recents.entries.clone();
    accept(&mut state, remaining);
    assert_eq!(state.focus, Target::Recent("youtube.leanback.v4".into()));
    state.recents.entries.truncate(1);
    assert_eq!(
        state.direction(Direction::Up, &controls(&state)),
        vec![Effect::Recent {
            generation: 4,
            command: Command::Close("youtube.leanback.v4".into())
        }]
    );
    let next = state.recent_response(4, Command::Close("youtube.leanback.v4".into()), Ok(None));
    assert_eq!(
        next,
        vec![Effect::Recent {
            generation: 5,
            command: Command::List
        }]
    );
    accept(&mut state, vec![]);
    assert_eq!(state.focus, Target::RecentEmpty);
    assert!(controls(&state).is_empty());
    assert!(state.activate().is_empty());
}

#[test]
fn failed_actions_keep_history_and_stale_replies_cannot_reopen_or_replace_a_session() {
    let mut state = ready();
    state.open_recents();
    accept(&mut state, recents::demo());
    state.direction(Direction::Up, &controls(&state));
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
        Target::Recent("netflix".into()),
    ] {
        state.pointer_focus(target.clone());
        let after = controls(&state);
        let a = before.iter().find(|c| c.target == target).unwrap().rect;
        let b = after.iter().find(|c| c.target == target).unwrap().rect;
        assert_eq!((a.x, a.y, a.w, a.h), (b.x, b.y, b.w, b.h));
    }
    assert!(
        controls(&state)
            .iter()
            .all(|c| matches!(c.target, Target::Recent(_)))
    );
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

#[test]
fn recents_crossfade_reverses_without_jumping_and_restores_parent_input_immediately() {
    use crate::renderer::Draw;
    let mut state = ready();
    state.open(2);
    state.subpage = Some(crate::state::Subpage::Clock);
    state.focus = Target::ClockFormat;
    let parent = controls(&state);
    state.open_recents();
    accept(&mut state, recents::demo());
    let start = Instant::now();
    state.recents.updated = start;
    state
        .recents
        .animate(true, false, false, start + Duration::from_millis(40));
    let entering = state.recents.visibility;
    assert!(entering > 0. && entering < 1.);
    let incoming = scene(&state);
    assert!(incoming.modal.is_some());
    assert!(
        incoming
            .controls
            .iter()
            .all(|c| matches!(c.target, Target::Recent(_)))
    );
    state.back();
    assert!(!state.recents_open());
    assert_eq!(state.recents.visibility, entering);
    assert_eq!(state.focus, Target::ClockFormat);
    let outgoing = scene(&state);
    assert!(outgoing.modal.is_none());
    assert_eq!(
        outgoing
            .controls
            .iter()
            .map(|c| &c.target)
            .collect::<Vec<_>>(),
        parent.iter().map(|c| &c.target).collect::<Vec<_>>()
    );
    assert!(outgoing.draw.iter().any(|draw| matches!(draw,
        Draw::TintedImage(path, _, _, _, tint) if path.ends_with("logos/netflix.lvec") && tint[3] > 0. && tint[3] < 1.)));
    state.recents.updated = start + Duration::from_millis(40);
    state
        .recents
        .animate(false, false, false, start + Duration::from_millis(56));
    let leaving = state.recents.visibility;
    assert!(leaving > 0. && leaving < entering);
    state.open_recents();
    assert_eq!(state.recents.visibility, leaving);
    assert_eq!(
        state.recents.entries.len(),
        8,
        "Reversing dismissal must retain the visible cards"
    );
    state.recents.updated = start + Duration::from_millis(56);
    state
        .recents
        .animate(true, false, false, start + Duration::from_millis(72));
    assert!(state.recents.visibility > leaving);
    for frame in 1..40 {
        state.recents.animate(
            true,
            false,
            false,
            start + Duration::from_millis(72 + frame * 16),
        );
    }
    assert_eq!(state.recents.visibility, 1.);
    state.back();
    state.preferences.reduced_motion = true;
    state.tick();
    assert_eq!(state.recents.visibility, 0.);
    assert!(state.recents.closing_focus.is_none());
    state.open_recents();
    assert_eq!(state.recents.visibility, 1.);
    state.back();
    assert_eq!(state.recents.visibility, 0.);
}

#[test]
fn pointer_interruption_freezes_cards_without_freezing_the_crossfade() {
    let mut recent = recents::Recents::default();
    let start = Instant::now();
    recent.updated = start;
    recent.selected = 3;
    recent.position = 1.5;
    for frame in 1..40 {
        recent.animate(true, false, true, start + Duration::from_millis(frame * 16));
    }
    assert_eq!(recent.visibility, 1.);
    assert_eq!(recent.position, 1.5);
}

fn advance_recents(state: &mut State, millis: u64) {
    state.recents.animate(
        state.recents_open(),
        state.preferences.reduced_motion,
        state.pointer,
        state.recents.updated + Duration::from_millis(millis),
    );
}

fn shows_recent_message(state: &State, key: &str) -> bool {
    use crate::renderer::Draw;
    scene(state).draw.iter().any(
        |draw| matches!(draw, Draw::Text(value, ..) if value == state.preferences.language.t(key)),
    )
}

#[test]
fn closing_recent_cards_keeps_the_close_and_follow_up_refresh_quiet() {
    for reduced in [false, true] {
        for count in [1, 2] {
            let mut state = ready();
            state.preferences.reduced_motion = reduced;
            state.open_recents();
            assert!(shows_recent_message(&state, "recentLoading"));
            accept(
                &mut state,
                recents::demo().into_iter().take(count).collect(),
            );
            state.recents.visibility = 1.;
            state.direction(Direction::Up, &controls(&state));
            assert!(!shows_recent_message(&state, "recentLoading"));
            state.recent_response(2, Command::Close("netflix".into()), Ok(None));
            assert_eq!(state.recents.pending, Some(Command::List));
            assert!(state.recents.error.is_none());
            assert!(!shows_recent_message(&state, "recentLoading"));
            // Keep the refresh pending longer than the exit animation: the last
            // card must not be replaced by the initial-loading message either.
            for _ in 0..50 {
                advance_recents(&mut state, 16);
            }
            assert!(!shows_recent_message(&state, "recentLoading"));
            assert_eq!(shows_recent_message(&state, "recentEmpty"), count == 1);
            let remaining = state.recents.entries.clone();
            accept(&mut state, remaining);
            assert!(!shows_recent_message(&state, "recentLoading"));
            if count == 2 {
                state.activate();
                assert!(shows_recent_message(&state, "opening"));
                state.back();
                assert!(!shows_recent_message(&state, "opening"));
            }
        }
    }
}

#[test]
fn dismissing_an_initial_read_does_not_flash_loading_or_an_unconfirmed_empty_state() {
    let mut state = ready();
    state.open_recents();
    state.recents.visibility = 0.5;
    assert!(shows_recent_message(&state, "recentLoading"));
    state.back();
    assert!(!shows_recent_message(&state, "recentLoading"));
    assert!(!shows_recent_message(&state, "recentEmpty"));
    state.open_recents();
    assert!(shows_recent_message(&state, "recentLoading"));
    accept(&mut state, recents::demo());
    state.recent_request(Command::List);
    assert!(!shows_recent_message(&state, "recentLoading"));
    state.back();
    for _ in 0..50 {
        advance_recents(&mut state, 16);
    }
    state.open_recents();
    assert!(shows_recent_message(&state, "recentLoading"));
    accept(&mut state, vec![]);
    assert!(!shows_recent_message(&state, "recentLoading"));
    assert!(shows_recent_message(&state, "recentEmpty"));
}

fn recent_bounds(state: &State, id: &str) -> crate::state::Rect {
    controls(state)
        .into_iter()
        .find(|control| control.target == Target::Recent(id.into()))
        .unwrap()
        .rect
}

fn netflix_logo(state: &State) -> crate::state::Rect {
    use crate::renderer::Draw;
    scene(state)
        .draw
        .into_iter()
        .find_map(|draw| match draw {
            Draw::Image(path, rect, ..) | Draw::TintedImage(path, rect, ..)
                if path.ends_with("logos/netflix.lvec") =>
            {
                Some(rect)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn stacked_cards_hit_the_front_card_in_overlaps_and_rear_cards_only_where_exposed() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_recents();
    accept(&mut state, recents::demo());
    let front = recent_bounds(&state, "netflix");
    let rear = recent_bounds(&state, "youtube.leanback.v4");
    assert!(rear.w < front.w && rear.h < front.h);
    assert!(rear.x < front.x + front.w);
    let controls = controls(&state);
    assert_eq!(
        state.hit_test(&controls, 1200., 540.),
        Some(Target::Recent("netflix".into()))
    );
    assert_eq!(
        state.hit_test(&controls, 1600., 540.),
        Some(Target::Recent("youtube.leanback.v4".into()))
    );
}

#[test]
fn closing_lifts_the_card_and_fills_its_slot_without_a_refresh_jump() {
    let mut state = ready();
    state.open_recents();
    accept(&mut state, recents::demo());
    state.recents.visibility = 1.;
    let before_logo = netflix_logo(&state);
    let before_next = recent_bounds(&state, "youtube.leanback.v4");
    state.direction(Direction::Up, &controls(&state));
    assert!(state.animating());
    assert!(
        controls(&state)
            .iter()
            .all(|control| control.target != Target::Recent("netflix".into()))
    );
    advance_recents(&mut state, 80);
    assert!(netflix_logo(&state).y < before_logo.y);
    assert_eq!(
        recent_bounds(&state, "youtube.leanback.v4").x,
        before_next.x,
        "History remains intact until the service confirms the close"
    );
    state.recent_response(2, Command::Close("netflix".into()), Ok(None));
    let remaining = state.recents.entries.clone();
    accept(&mut state, remaining);
    assert_eq!(
        recent_bounds(&state, "youtube.leanback.v4").x,
        before_next.x,
        "Neither removal nor the history refresh may snap presentation slots"
    );
    advance_recents(&mut state, 80);
    let entering = recent_bounds(&state, "youtube.leanback.v4");
    assert!(entering.x < before_next.x);
    assert!(entering.w > before_next.w && entering.w < 900.);
    for _ in 0..50 {
        advance_recents(&mut state, 16);
    }
    let settled = recent_bounds(&state, "youtube.leanback.v4");
    assert_eq!(settled.center(), (960., 540.));
    assert_eq!(settled.w, 900.);
    assert!(state.recents.dismissals.is_empty());
    assert!(!state.animating());
}

#[test]
fn a_failed_close_returns_the_same_card_and_restores_its_hit_target() {
    let mut state = ready();
    state.open_recents();
    accept(&mut state, recents::demo());
    state.recents.visibility = 1.;
    let original = netflix_logo(&state);
    state.direction(Direction::Up, &controls(&state));
    for _ in 0..3 {
        advance_recents(&mut state, 80);
    }
    let lifted = netflix_logo(&state);
    assert!(lifted.y < original.y);
    state.recent_response(2, Command::Close("netflix".into()), Err("denied".into()));
    advance_recents(&mut state, 60);
    assert!(netflix_logo(&state).y > lifted.y);
    for _ in 0..20 {
        advance_recents(&mut state, 16);
    }
    assert_eq!(netflix_logo(&state).y, original.y);
    assert_eq!(recent_bounds(&state, "netflix").center(), (960., 540.));
    assert!(state.recents.dismissals.is_empty());
    assert_eq!(state.recents.entries.len(), 8);
    assert!(state.recents.error.is_some());
    assert!(!state.animating());
}

#[test]
fn closing_the_last_slot_brings_the_previous_card_forward_and_reduced_motion_snaps() {
    for reduced in [false, true] {
        let mut state = ready();
        state.preferences.reduced_motion = reduced;
        state.open_recents();
        accept(&mut state, recents::demo().into_iter().take(2).collect());
        state.recents.visibility = 1.;
        state.recents.selected = 1;
        state.recents.position = 1.;
        state.focus = Target::Recent("youtube.leanback.v4".into());
        let before = recent_bounds(&state, "netflix");
        state.direction(Direction::Up, &controls(&state));
        state.recent_response(2, Command::Close("youtube.leanback.v4".into()), Ok(None));
        let remaining = state.recents.entries.clone();
        accept(&mut state, remaining);
        assert_eq!(state.focus, Target::Recent("netflix".into()));
        advance_recents(&mut state, 80);
        let entering = recent_bounds(&state, "netflix");
        assert!(entering.center().0 > before.center().0);
        assert!(entering.w > before.w);
        assert_eq!(state.recents.position == 0., reduced);
        if reduced {
            assert!(state.recents.dismissals.is_empty());
        }
        for _ in 0..50 {
            advance_recents(&mut state, 16);
        }
        assert_eq!(recent_bounds(&state, "netflix").center(), (960., 540.));
        assert!(!state.animating());
    }
}

#[test]
fn close_selects_the_hovered_cards_neighbor_and_preserves_browsing_during_the_request() {
    let mut state = ready();
    state.open_recents();
    accept(&mut state, recents::demo());
    let neighbor = state.recents.entries[3].app.id.clone();
    state.pointer_focus(Target::Recent("com.apple.appletv".into()));
    state.direction(Direction::Up, &controls(&state));
    state.recent_response(2, Command::Close("com.apple.appletv".into()), Ok(None));
    assert_eq!(state.focus, Target::Recent(neighbor));
    let remaining = state.recents.entries.clone();
    accept(&mut state, remaining);

    state.focus = Target::Recent("netflix".into());
    state.recents.selected = 0;
    state.direction(Direction::Up, &controls(&state));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Recent("youtube.leanback.v4".into()));
    state.recent_response(4, Command::Close("netflix".into()), Ok(None));
    assert_eq!(state.focus, Target::Recent("youtube.leanback.v4".into()));
    assert_eq!(state.recents.selected, 0);
}

#[test]
fn the_last_card_exits_before_the_empty_state_and_reduced_motion_skips_the_exit() {
    use crate::renderer::Draw;
    for reduced in [false, true] {
        let mut state = ready();
        state.preferences.reduced_motion = reduced;
        state.open_recents();
        accept(&mut state, recents::demo().into_iter().take(1).collect());
        state.recents.visibility = 1.;
        state.direction(Direction::Up, &controls(&state));
        state.recent_response(2, Command::Close("netflix".into()), Ok(None));
        accept(&mut state, vec![]);
        assert!(controls(&state).is_empty());
        let empty_visible = |state: &State| {
            scene(state)
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(value, ..) if value == "No recent apps"))
        };
        assert_eq!(empty_visible(&state), reduced);
        for _ in 0..30 {
            advance_recents(&mut state, 16);
        }
        assert!(empty_visible(&state));
        assert!(state.recents.dismissals.is_empty());
        assert!(!state.animating());
    }
}

#[test]
fn empty_recents_has_no_explanatory_subtitle() {
    use crate::renderer::Draw;
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open_recents();
    accept(&mut state, vec![]);
    let texts: Vec<_> = scene(&state)
        .draw
        .into_iter()
        .filter_map(|draw| match draw {
            Draw::Text(value, ..) => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["No recent apps"]);
}
