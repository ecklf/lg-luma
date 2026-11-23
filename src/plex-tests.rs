use super::*;
use crate::{
    model::demo_inventory,
    state::{Direction, Effect},
};

const TOKEN: &str = "abcdefgh12345678";

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

fn server() -> Server {
    Server {
        id: "machine-0001".into(),
        name: "Living room".into(),
        uri: "http://192.168.1.10:32400".into(),
        token: "servertoken123".into(),
    }
}

fn item(key: usize) -> Item {
    Item {
        key: key.to_string(),
        kind: "movie".into(),
        title: format!("Film {key}"),
        subtitle: "2024".into(),
        thumb: format!("/library/metadata/{key}/thumb/1"),
        progress: 0.5,
    }
}

/// A Home with Continue watching on it, signed in, and `count` items already fetched.
fn signed_in(count: usize) -> State {
    let mut state = State::new(Preferences::default());
    state.set_inventory(demo_inventory(), None);
    state.preferences.plex.continue_watching.enabled = true;
    state.plex.loaded = true;
    state.plex.account = Account {
        client_id: "luma-0123456789abcdef".into(),
        token: TOKEN.into(),
        name: "Owner".into(),
        server: Some(server()),
    };
    state.plex.shelves = Some(Shelves {
        origin: server().id,
        at: now(),
        continue_watching: Some((1..=count).map(item).collect()),
        recently_added: None,
    });
    state.page = Page::Home;
    state
}

fn favorite(state: &State) -> Target {
    state.first_content()
}

#[test]
fn pins_tokens_and_names_are_validated() {
    assert_eq!(
        parse_pin(&json!({"id": 42, "code": "ab12"})),
        Some(Pin {
            id: 42,
            code: "AB12".into()
        })
    );
    for invalid in [
        json!({"id": 0, "code": "AB12"}),
        json!({"id": 1, "code": "A1"}),
        json!({"id": 1, "code": "AB 12"}),
        json!({"id": 1}),
    ] {
        assert!(parse_pin(&invalid).is_none());
    }
    assert_eq!(
        parse_pin_token(&json!({"authToken": TOKEN})).as_deref(),
        Some(TOKEN)
    );
    assert!(parse_pin_token(&json!({"authToken": null})).is_none());
    assert!(parse_pin_token(&json!({"authToken": "short"})).is_none());
    assert!(parse_pin_token(&json!({"authToken": "has space in it"})).is_none());
    assert_eq!(
        parse_account_name(&json!({"friendlyName": "", "username": "owner\u{7}"})),
        "owner"
    );
}

#[test]
fn server_addresses_are_ip_literals_or_plex_direct_and_http_stays_local() {
    assert_eq!(
        checked_connection("https://192-168-1-10.abcdef.plex.direct:32400/", false).as_deref(),
        Some("https://192-168-1-10.abcdef.plex.direct:32400")
    );
    assert_eq!(
        checked_connection("http://192.168.1.10:32400", true).as_deref(),
        Some("http://192.168.1.10:32400")
    );
    assert!(checked_connection("https://203.0.113.5:32400", false).is_some());
    for (uri, local) in [
        // Plain http over the internet, or to a public address, or not marked local.
        ("http://203.0.113.5:32400", true),
        ("http://192.168.1.10:32400", false),
        // Names other than plex.direct, credentials, paths and queries.
        ("https://plex.example.com:32400", false),
        ("https://plex.direct", false),
        ("https://user:pass@1-2-3-4.abc.plex.direct:32400", false),
        ("https://1-2-3-4.abc.plex.direct:32400/web", false),
        ("https://1-2-3-4.abc.plex.direct:32400?x=1", false),
        ("ftp://192.168.1.10", true),
        ("javascript:alert(1)", true),
    ] {
        assert!(checked_connection(uri, local).is_none(), "{uri}");
    }
}

#[test]
fn servers_keep_safe_connections_in_order_with_owned_servers_first() {
    let resources = json!([
        {"provides": "client,player", "clientIdentifier": "player-0001", "accessToken": TOKEN},
        {"provides": "server", "name": "Friend", "clientIdentifier": "friend-00001", "accessToken": TOKEN,
         "owned": false, "connections": [{"uri": "https://5-6-7-8.xyz.plex.direct:32400", "local": false}]},
        {"provides": "server", "name": "Home", "clientIdentifier": "home-000001", "accessToken": TOKEN,
         "owned": true, "connections": [
            {"uri": "https://relay.plex.direct:8443", "local": false, "relay": true},
            {"uri": "https://203-0-113-5.abc.plex.direct:32400", "local": false},
            {"uri": "https://192-168-1-10.abc.plex.direct:32400", "local": true, "address": "192.168.1.10", "port": 32400},
            {"uri": "https://evil.example.com", "local": true}
         ]},
        {"provides": "server", "name": "Broken", "clientIdentifier": "x", "accessToken": TOKEN,
         "connections": [{"uri": "https://1-1-1-1.a.plex.direct:1"}]},
        {"provides": "server", "name": "No addresses", "clientIdentifier": "none-000001", "accessToken": TOKEN,
         "connections": [{"uri": "https://example.org"}]}
    ]);
    let servers = parse_servers(&resources);
    assert_eq!(
        servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["Home", "Friend"]
    );
    assert_eq!(
        servers[0]
            .connections
            .iter()
            .map(|c| c.uri.as_str())
            .collect::<Vec<_>>(),
        [
            "http://192.168.1.10:32400",
            "https://192-168-1-10.abc.plex.direct:32400",
            "https://203-0-113-5.abc.plex.direct:32400",
            "https://relay.plex.direct:8443",
        ]
    );
    assert!(!format!("{:?}", servers[0]).contains(TOKEN));
}

#[test]
fn items_map_films_episodes_and_seasons_and_skip_everything_else() {
    let reply = json!({"MediaContainer": {"Metadata": [
        {"type": "episode", "ratingKey": "11", "grandparentTitle": "Show", "title": "Pilot",
         "parentIndex": 2, "index": 5, "grandparentThumb": "/library/metadata/1/thumb/9",
         "duration": 1000, "viewOffset": 250},
        {"type": "season", "ratingKey": "12", "parentTitle": "Show", "title": "Season 3", "thumb": "/s"},
        {"type": "movie", "ratingKey": "13", "title": "Film", "year": 2024, "thumb": "/m"},
        {"type": "track", "ratingKey": "14", "title": "Song"},
        {"type": "movie", "ratingKey": "../15", "title": "Bad key"},
        {"type": "movie", "ratingKey": "13", "title": "Duplicate"},
        {"type": "movie", "ratingKey": "16", "title": "Odd thumb", "thumb": "https://elsewhere/x.jpg"}
    ]}});
    let items = parse_items(&reply);
    assert_eq!(
        items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
        ["11", "12", "13", "16"]
    );
    assert_eq!(items[0].title, "Show");
    assert_eq!(items[0].subtitle, "S2 · E5");
    assert_eq!(items[0].thumb, "/library/metadata/1/thumb/9");
    assert!((items[0].progress - 0.25).abs() < 1e-6);
    assert_eq!(
        (items[1].title.as_str(), items[1].subtitle.as_str()),
        ("Show", "Season 3")
    );
    assert_eq!(items[2].subtitle, "2024");
    assert!(
        items[3].thumb.is_empty(),
        "only server paths become poster requests"
    );
    // The hub shape: the first hub's Metadata.
    let hub = json!({"MediaContainer": {"Hub": [{"Metadata": [{"type": "movie", "ratingKey": "1", "title": "A"}]}]}});
    assert_eq!(parse_items(&hub).len(), 1);
    let many = json!({"MediaContainer": {"Metadata": (1..40).map(|k| json!({"type": "movie", "ratingKey": k.to_string(), "title": "x"})).collect::<Vec<_>>()}});
    assert_eq!(parse_items(&many).len(), MAX_ITEMS);
}

#[test]
fn launch_parameters_follow_plxnative_and_seasons_always_open() {
    let film = item(7);
    assert_eq!(
        launch_params(&film, "machine-0001", Action::Play),
        json!({"plex": {"server": "machine-0001", "ratingKey": "7", "action": "play"}})
    );
    let season = Item {
        kind: "season".into(),
        ..item(8)
    };
    assert_eq!(
        launch_params(&season, "machine-0001", Action::Play)["plex"]["action"],
        "open"
    );
}

#[test]
fn poster_addresses_encode_the_path_and_never_carry_the_token() {
    let url = poster_url("http://192.168.1.10:32400", "/library/metadata/1/thumb/9").unwrap();
    assert!(url.starts_with("http://192.168.1.10:32400/photo/:/transcode?"));
    assert!(url.contains("url=%2Flibrary%2Fmetadata%2F1%2Fthumb%2F9"));
    assert!(!url.contains("Token"));
    for thumb in ["", "relative", "/a?b", "/a#b", "/a%2F", "/a b"] {
        assert!(
            poster_url("http://192.168.1.10:32400", thumb).is_none(),
            "{thumb}"
        );
    }
}

#[test]
fn posters_are_cropped_to_two_by_three() {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(400, 400, image::Rgba([10, 20, 30, 255]))
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    let poster = decode_poster(&bytes).unwrap();
    assert_eq!(poster.dimensions(), (POSTER_WIDTH, POSTER_HEIGHT));
    assert!(decode_poster(b"not an image").is_none());
}

#[test]
fn the_sign_in_stays_out_of_preferences_debug_output_and_bad_files() {
    let mut preferences = Preferences::default();
    preferences.plex.continue_watching.enabled = true;
    let saved = serde_json::to_string(&preferences).unwrap();
    assert!(saved.contains("\"plex\""));
    assert!(!saved.contains("token"));
    let account = Account {
        client_id: "luma-0123456789abcdef".into(),
        token: TOKEN.into(),
        name: "Owner".into(),
        server: Some(server()),
    };
    let debug = format!("{account:?} {:?}", account.server);
    assert!(!debug.contains(TOKEN) && !debug.contains("servertoken123"));
    // A tampered file loses what does not validate, never the shape checks.
    let directory = std::env::temp_dir().join(format!("luma-plex-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&directory);
    let path = directory.join("preferences.plex.json");
    let mut bad = account.clone();
    bad.server.as_mut().unwrap().uri = "https://evil.example.com".into();
    std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    let loaded = load_account(&path);
    assert!(loaded.signed_in());
    assert!(loaded.server.is_none());
    bad.server.as_mut().unwrap().uri = "http://203.0.113.5:32400".into();
    std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(
        load_account(&path).server.is_none(),
        "plain http only to a private address"
    );
    std::fs::write(&path, serde_json::to_vec(&account).unwrap()).unwrap();
    assert_eq!(load_account(&path), account);
    std::fs::write(&path, b"{not json").unwrap();
    assert!(!load_account(&path).signed_in());
    let _ = std::fs::remove_dir_all(&directory);
    // Older preferences without the field, or with a broken one, still load.
    let old: Preferences = serde_json::from_str("{}").unwrap();
    assert!(!old.plex.continue_watching.enabled);
    let broken: Preferences = serde_json::from_str(r#"{"plex": 5}"#).unwrap();
    assert_eq!(broken.plex, Settings::default());
}

#[test]
fn nothing_loads_or_starts_until_a_widget_is_on_home() {
    let directory = std::env::temp_dir().join(format!("luma-plex-lazy-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&directory);
    let preferences = directory.join("preferences.json");
    std::fs::write(
        account_path(&preferences),
        serde_json::to_vec(&Account {
            client_id: "luma-0123456789abcdef".into(),
            token: TOKEN.into(),
            name: "Owner".into(),
            server: Some(server()),
        })
        .unwrap(),
    )
    .unwrap();
    let mut state = State::new(Preferences::default());
    state.plex.set_paths(&preferences);
    let mut service = None;
    // No widget: no file read, no workers, even with requests allowed.
    for _ in 0..3 {
        assert!(!tick(
            &mut state,
            &mut service,
            Some(&Origin::public()),
            true,
            now()
        ));
    }
    assert!(!state.plex.loaded());
    assert!(!state.plex.account.signed_in());
    assert!(service.is_none());
    // A widget on Home loads the sign-in; previews without network still start nothing.
    state.preferences.plex.recently_added.enabled = true;
    tick(&mut state, &mut service, None, true, now());
    assert!(state.plex.loaded());
    assert!(state.plex.account.ready());
    assert!(service.is_none());
    // Removing it forgets what was loaded and stops the workers.
    let (detached, _jobs, _replies) = Service::detached();
    service = Some(detached);
    state.preferences.plex.recently_added.enabled = false;
    assert!(tick(&mut state, &mut service, None, true, now()));
    assert!(!state.plex.loaded());
    assert!(!state.plex.account.signed_in());
    assert!(service.is_none());
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn lists_are_fetched_only_on_visible_home_and_stale_replies_are_ignored() {
    let mut state = signed_in(0);
    state.plex.shelves = None;
    let (service, jobs, replies) = Service::detached();
    let mut service = Some(service);
    let t = now();
    state.page = Page::Settings;
    tick(&mut state, &mut service, None, true, t);
    state.page = Page::Home;
    tick(&mut state, &mut service, None, false, t);
    assert!(
        jobs.try_recv().is_err(),
        "Settings and a hidden Home fetch nothing"
    );
    tick(&mut state, &mut service, None, true, t);
    let Ok(Job::Shelves {
        generation, lists, ..
    }) = jobs.try_recv()
    else {
        panic!("a visible Home fetches the lists");
    };
    assert_eq!(lists, [List::Continue]);
    tick(&mut state, &mut service, None, true, t + 1);
    assert!(jobs.try_recv().is_err(), "one fetch at a time");
    // Switching Recently added on starts over; the earlier reply is ignored.
    state.preferences.plex.recently_added.enabled = true;
    tick(&mut state, &mut service, None, true, t + 2);
    let Ok(Job::Shelves {
        generation: current,
        lists,
        ..
    }) = jobs.try_recv()
    else {
        panic!("a changed choice fetches at once");
    };
    assert_eq!(lists, [List::Continue, List::Recent]);
    replies
        .send(Reply::Shelves {
            generation,
            result: Ok(Shelves {
                origin: server().id,
                at: t,
                continue_watching: Some(vec![item(1)]),
                recently_added: Some(vec![]),
            }),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + 3);
    assert!(state.plex.items(List::Continue).is_empty());
    replies
        .send(Reply::Shelves {
            generation: current,
            result: Ok(Shelves {
                origin: server().id,
                at: t,
                continue_watching: Some(vec![item(1), item(2)]),
                recently_added: Some(vec![]),
            }),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + 4);
    assert_eq!(state.plex.items(List::Continue).len(), 2);
    assert_eq!(state.plex.status, Status::Ready);
    // Fresh for ten minutes, then fetched again.
    tick(&mut state, &mut service, None, true, t + 4 + FRESH - 1);
    assert!(jobs.try_recv().is_err());
    tick(&mut state, &mut service, None, true, t + 4 + FRESH);
    assert!(matches!(jobs.try_recv(), Ok(Job::Shelves { .. })));
}

#[test]
fn failures_recover_the_address_once_back_off_and_stop_on_an_expired_sign_in() {
    let mut state = signed_in(0);
    state.plex.shelves = None;
    let (service, jobs, replies) = Service::detached();
    let mut service = Some(service);
    let t = now();
    tick(&mut state, &mut service, None, true, t);
    let Ok(Job::Shelves { generation, .. }) = jobs.try_recv() else {
        panic!();
    };
    replies
        .send(Reply::Shelves {
            generation,
            result: Err(Failure::NoAnswer),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + 1);
    assert!(
        matches!(jobs.try_recv(), Ok(Job::Recover { .. })),
        "no answer: ask plex.tv for the server's addresses"
    );
    replies
        .send(Reply::Recovered {
            generation,
            result: Err(Failure::NoAnswer),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + 2);
    assert_eq!(state.plex.status, Status::Failed);
    tick(&mut state, &mut service, None, true, t + 2 + RETRY[0] - 1);
    assert!(jobs.try_recv().is_err());
    tick(&mut state, &mut service, None, true, t + 2 + RETRY[0]);
    let Ok(Job::Shelves { generation, .. }) = jobs.try_recv() else {
        panic!("retried after two minutes");
    };
    replies
        .send(Reply::Shelves {
            generation,
            result: Err(Failure::Status(401)),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + 200);
    assert_eq!(state.plex.status, Status::Expired);
    assert!(state.notice.is_some());
    tick(&mut state, &mut service, None, true, t + 100_000);
    assert!(
        jobs.try_recv().is_err(),
        "an expired sign-in fetches nothing"
    );
}

#[test]
fn a_new_server_address_from_plex_tv_is_saved_and_fetched_at_once() {
    let mut state = signed_in(0);
    state.plex.shelves = None;
    let (service, jobs, replies) = Service::detached();
    let mut service = Some(service);
    let t = now();
    tick(&mut state, &mut service, None, true, t);
    let Ok(Job::Shelves { generation, .. }) = jobs.try_recv() else {
        panic!();
    };
    replies
        .send(Reply::Shelves {
            generation,
            result: Err(Failure::NoAnswer),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + 1);
    assert!(matches!(jobs.try_recv(), Ok(Job::Recover { .. })));
    let moved = Server {
        uri: "http://192.168.1.20:32400".into(),
        ..server()
    };
    replies
        .send(Reply::Recovered {
            generation,
            result: Ok(moved),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + 2);
    tick(&mut state, &mut service, None, true, t + 2);
    let Ok(Job::Shelves { server, .. }) = jobs.try_recv() else {
        panic!("the new address is fetched at once");
    };
    assert_eq!(server.uri, "http://192.168.1.20:32400");
}

#[test]
fn signing_in_polls_the_code_then_picks_the_only_server() {
    let mut state = State::new(Preferences::default());
    state.preferences.plex.continue_watching.enabled = true;
    state.plex.loaded = true;
    let (service, jobs, replies) = Service::detached();
    let mut service = Some(service);
    let t = now();
    state.page = Page::Settings;
    state.subpage = Some(Subpage::Plex(HomeWidget::ContinueWatching));
    state.focus = Target::PlexAccount;
    state.activate();
    assert!(matches!(state.overlay, Some(Overlay::PlexSignIn)));
    tick(&mut state, &mut service, None, true, t);
    assert!(valid_client_id(&state.plex.account.client_id));
    assert!(matches!(jobs.try_recv(), Ok(Job::Pin { .. })));
    replies
        .send(Reply::Pin(Ok(Pin {
            id: 9,
            code: "ABCD".into(),
        })))
        .unwrap();
    tick(&mut state, &mut service, None, true, t);
    assert_eq!(
        state
            .plex
            .sign_in
            .as_ref()
            .unwrap()
            .pin
            .as_ref()
            .unwrap()
            .code,
        "ABCD"
    );
    tick(&mut state, &mut service, None, true, t + PIN_POLL);
    assert!(matches!(jobs.try_recv(), Ok(Job::CheckPin { id: 9, .. })));
    replies
        .send(Reply::CheckPin {
            id: 9,
            token: Ok(Some(TOKEN.into())),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + PIN_POLL + 1);
    assert!(state.plex.account.signed_in());
    assert!(state.overlay.is_none(), "the dialog closes by itself");
    assert_eq!(state.focus, Target::PlexAccount);
    assert!(matches!(jobs.try_recv(), Ok(Job::SignedIn { .. })));
    let choice = ServerChoice {
        id: server().id,
        name: "Living room".into(),
        token: "servertoken123".into(),
        owned: true,
        connections: vec![Connection {
            uri: server().uri,
            local: true,
            relay: false,
        }],
    };
    replies
        .send(Reply::SignedIn {
            token: TOKEN.into(),
            name: "Owner".into(),
            servers: Ok(vec![choice]),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + PIN_POLL + 2);
    assert_eq!(state.plex.account.name, "Owner");
    assert!(matches!(jobs.try_recv(), Ok(Job::Connect { .. })));
    replies
        .send(Reply::Connected {
            id: server().id,
            result: Ok(server()),
        })
        .unwrap();
    tick(&mut state, &mut service, None, true, t + PIN_POLL + 3);
    assert!(state.plex.account.ready());
}

#[test]
fn closing_the_sign_in_dialog_stops_polling() {
    let mut state = State::new(Preferences::default());
    state.preferences.plex.continue_watching.enabled = true;
    state.plex.loaded = true;
    let (service, jobs, _replies) = Service::detached();
    let mut service = Some(service);
    state.page = Page::Settings;
    state.subpage = Some(Subpage::Plex(HomeWidget::ContinueWatching));
    state.focus = Target::PlexAccount;
    state.activate();
    tick(&mut state, &mut service, None, true, now());
    assert!(matches!(jobs.try_recv(), Ok(Job::Pin { .. })));
    state.back();
    assert_eq!(state.focus, Target::PlexAccount);
    tick(&mut state, &mut service, None, true, now());
    assert!(state.plex.sign_in.is_none());
}

#[test]
fn home_rows_are_reachable_from_favorites_and_the_top_and_keep_their_place() {
    let mut state = signed_in(30);
    let favorite = favorite(&state);
    state.focus = favorite.clone();
    state.direction(Direction::Up, &[]);
    assert_eq!(
        state.focus,
        Target::PlexItem(HomeWidget::ContinueWatching, 0)
    );
    state.direction(Direction::Left, &[]);
    assert_eq!(
        state.focus,
        Target::PlexItem(HomeWidget::ContinueWatching, 0),
        "rows stop at their ends"
    );
    for _ in 0..25 {
        state.direction(Direction::Right, &[]);
    }
    assert_eq!(
        state.focus,
        Target::PlexItem(HomeWidget::ContinueWatching, 25)
    );
    assert!(state.plex.first[0] + VISIBLE > 25 && state.plex.first[0] <= 25);
    for _ in 0..10 {
        state.direction(Direction::Right, &[]);
    }
    assert_eq!(
        state.focus,
        Target::PlexItem(HomeWidget::ContinueWatching, 29)
    );
    // Down returns to favorites, Up comes back to the same poster.
    state.direction(Direction::Down, &[]);
    assert_eq!(state.focus, favorite);
    state.direction(Direction::Up, &[]);
    assert_eq!(
        state.focus,
        Target::PlexItem(HomeWidget::ContinueWatching, 29)
    );
    // The default Weather has no location: its setup action sits above the row.
    state.direction(Direction::Up, &[]);
    assert_eq!(state.focus, Target::WeatherConfigure);
    state.direction(Direction::Down, &[]);
    assert_eq!(
        state.focus,
        Target::PlexItem(HomeWidget::ContinueWatching, 29)
    );
    // Back returns to favorites rather than opening Recent apps.
    state.preferences.recent_apps_behavior = crate::model::RecentAppsBehavior::ShortPress;
    state.back_press(false);
    assert!(state.overlay.is_none());
    assert_eq!(state.focus, favorite);
}

#[test]
fn rows_without_items_are_skipped_and_signed_out_rows_offer_sign_in() {
    let mut state = signed_in(0);
    let favorite = favorite(&state);
    state.focus = favorite.clone();
    state.direction(Direction::Up, &[]);
    assert!(
        !matches!(state.focus, Target::PlexItem(..)),
        "an empty row cannot take focus"
    );
    let mut state = signed_in(3);
    state.plex.account.token.clear();
    state.focus = favorite;
    state.direction(Direction::Up, &[]);
    assert_eq!(state.focus, Target::PlexSetup(HomeWidget::ContinueWatching));
    state.activate();
    assert_eq!(state.page, Page::Settings);
    assert_eq!(
        state.subpage,
        Some(Subpage::Plex(HomeWidget::ContinueWatching))
    );
    assert_eq!(state.focus, Target::PlexAccount);
}

#[test]
fn ok_opens_the_item_in_plxnative_or_says_it_is_missing() {
    let mut state = signed_in(3);
    state.focus = Target::PlexItem(HomeWidget::ContinueWatching, 2);
    assert!(state.activate().is_empty());
    assert!(
        state
            .notice
            .as_ref()
            .is_some_and(|(text, _)| text.contains(APP_NAME))
    );
    state.inventory.apps.push(crate::model::App {
        id: APP_ID.into(),
        title: APP_NAME.into(),
        ..state.inventory.apps[0].clone()
    });
    state.preferences.plex.action = Action::Open;
    assert_eq!(
        state.activate(),
        [Effect::LaunchWith {
            id: APP_ID.into(),
            params: json!({"plex": {"server": "machine-0001", "ratingKey": "3", "action": "open"}}),
        }]
    );
}

#[test]
fn a_shrinking_row_keeps_focus_inside_it_or_hands_it_to_favorites() {
    let mut state = signed_in(5);
    state.focus = Target::PlexItem(HomeWidget::ContinueWatching, 4);
    state.plex.shelves.as_mut().unwrap().continue_watching = Some(vec![item(1), item(2)]);
    state.reconcile_plex_focus();
    assert_eq!(
        state.focus,
        Target::PlexItem(HomeWidget::ContinueWatching, 1)
    );
    state.plex.shelves.as_mut().unwrap().continue_watching = Some(vec![]);
    state.reconcile_plex_focus();
    assert_eq!(state.focus, state.first_content());
}

#[test]
fn both_rows_fit_only_with_a_compact_layout_and_reset_removes_them() {
    let mut preferences = Preferences::default();
    assert_eq!(HomeWidget::ContinueWatching.span(), (6, 2));
    let position = WidgetPosition { column: 0, row: 2 };
    assert!(preferences.can_place_widget(HomeWidget::ContinueWatching, position));
    preferences.set_widget_present(HomeWidget::ContinueWatching, true);
    assert!(
        (0..crate::model::HOME_GRID_CELLS)
            .filter_map(WidgetPosition::from_cell)
            .all(|p| !preferences.can_place_widget(HomeWidget::RecentlyAdded, p)),
        "Greeting and Weather leave room for one row"
    );
    preferences.sanitize();
    assert!(preferences.has_widget(HomeWidget::ContinueWatching));
    assert!(preferences.reset_widgets());
    assert!(!preferences.has_widget(HomeWidget::ContinueWatching));
}
