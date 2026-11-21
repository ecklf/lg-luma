use super::*;
use std::io::{Read, Write};

fn http(request: &str) -> (u16, String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", PORT)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let reply = String::from_utf8_lossy(&bytes).into_owned();
    let (head, body) = reply.split_once("\r\n\r\n").unwrap();
    let status = head[9..12].parse().unwrap();
    (status, head.to_owned(), body.to_owned())
}

fn get(route: &str, cookie: &str) -> (u16, String, String) {
    http(&format!(
        "GET {route} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nCookie: {cookie}\r\n\r\n"
    ))
}

fn post(route: &str, cookie: &str, body: &str) -> (u16, String, String) {
    http(&format!(
        "POST {route} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nOrigin: http://127.0.0.1:{PORT}\r\nCookie: {cookie}\r\nX-Luma-Request: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ))
}

fn next_event(service: &Service) -> Event {
    service
        .receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("web access event")
}

fn pairing(service: &Service) -> Pairing {
    loop {
        if let Event::Pairing(_, result) = next_event(service) {
            return result.unwrap();
        }
    }
}

#[test]
fn pin_digits_are_uniform_width() {
    for _ in 0..200 {
        let pin = new_pin().unwrap();
        assert_eq!(pin.len(), 6);
        assert!(pin.bytes().all(|byte| byte.is_ascii_digit()));
    }
}

#[test]
fn secrets_compare_by_content_and_length() {
    assert!(same(b"123456", b"123456"));
    assert!(!same(b"123456", b"123457"));
    assert!(!same(b"123456", b"12345"));
}

#[test]
fn cookie_must_match_a_stored_key_hash() {
    let devices = vec![Device {
        id: "abc".into(),
        name: "Phone".into(),
        hash: key_hash("secret"),
        paired: 0,
    }];
    let request = |cookie: &str| Request {
        method: "GET".into(),
        target: "/".into(),
        headers: [("cookie".to_owned(), cookie.to_owned())].into(),
        length: 0,
    };
    assert!(device(&request("luma_device=abc.secret"), &devices).is_some());
    assert!(device(&request("other=1; luma_device=abc.secret"), &devices).is_some());
    assert!(device(&request("luma_device=abc.wrong"), &devices).is_none());
    assert!(device(&request("luma_device=xyz.secret"), &devices).is_none());
    assert!(device(&request("luma_device=abc"), &devices).is_none());
}

/// The whole flow over loopback: pair with the PIN, change a setting, unpair.
#[test]
fn pairs_once_and_then_changes_settings() {
    let path = std::env::temp_dir().join(format!("luma-web-devices-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let weather = crate::weather_service::Client::new(None).unwrap();
    let mut service = Service::new(path.clone(), true, weather).unwrap();
    let settings = Settings::of(&crate::model::Preferences::default());
    let thumbnail = path.with_extension("jpg");
    std::fs::write(&thumbnail, b"jpeg").unwrap();
    let backgrounds = vec![crate::web_settings::Background {
        id: "dunes".into(),
        name: "Monument Valley".into(),
        thumbnail: thumbnail.clone(),
    }];
    let catalog = Catalog {
        backgrounds,
        inputs: vec![crate::web_settings::WebInput {
            id: "com.webos.app.hdmi1".into(),
            port: "HDMI 1".into(),
            name: String::new(),
            mode: crate::model::InputMode::Auto,
            icon: String::new(),
            auto_icon: "hdmi".into(),
        }],
        ..Catalog::default()
    };

    service.sync(true, true, &settings, &catalog);
    let first = pairing(&service);
    assert!(first.session.url.ends_with(&format!("/#pin={}", first.pin)));

    // Unpaired devices only get the page and a request to pair.
    assert_eq!(get("/", "").0, 200);
    let (status, _, body) = get("/api/settings", "");
    assert_eq!(status, 401);
    assert!(body.contains("\"pair\":true"));

    // Another site, a rebinding host name, and writes without the custom header are refused.
    let (status, _, _) = http(&format!(
        "GET /api/settings HTTP/1.1\r\nHost: evil.example:{PORT}\r\n\r\n"
    ));
    assert_eq!(status, 403);
    let (status, _, _) = http(&format!(
        "POST /api/pair HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"
    ));
    assert_eq!(status, 403);

    // Guessing is capped: after the fifth wrong PIN pairing locks, even for the right PIN, until
    // someone at the TV reopens the dialog.
    let wrong = if first.pin == "000000" {
        "111111"
    } else {
        "000000"
    };
    for _ in 0..PIN_ATTEMPTS {
        let (status, _, _) = post("/api/pair", "", &format!(r#"{{"pin":"{wrong}"}}"#));
        assert_eq!(status, 403);
    }
    loop {
        if let Event::Pairing(_, result) = next_event(&service) {
            assert_eq!(result.err().as_deref(), Some(LOCKED));
            break;
        }
    }
    let (status, _, _) = post("/api/pair", "", &format!(r#"{{"pin":"{}"}}"#, first.pin));
    assert_eq!(status, 409, "locked until the dialog is reopened");
    service.sync(true, false, &settings, &catalog);
    service.sync(true, true, &settings, &catalog);
    let first = pairing(&service);

    let (status, head, _) = post(
        "/api/pair",
        "",
        &format!(
            r#"{{"pin":"{} {}","name":"Test phone"}}"#,
            &first.pin[..3],
            &first.pin[3..]
        ),
    );
    assert_eq!(status, 200);
    let cookie = head
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: "))
        .unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    let cookie = cookie.split(';').next().unwrap().to_owned();
    // The key itself is never stored, only its hash.
    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(stored.contains("Test phone"));
    assert!(!stored.contains(cookie.split('.').nth(1).unwrap()));
    assert_eq!(service.devices().len(), 1);

    // A PIN pairs one device only.
    let (status, _, _) = post("/api/pair", "", &format!(r#"{{"pin":"{}"}}"#, first.pin));
    assert_eq!(status, 409);

    // Pairing is done; the page now works with the dialog closed.
    service.sync(true, false, &settings, &catalog);
    let (status, _, body) = get("/api/settings", &cookie);
    assert_eq!(status, 200);
    assert!(body.contains("\"device\":\"Test phone\""));

    // Pictures need pairing too: a custom wallpaper can be a private photo.
    assert_eq!(get("/api/thumbnail/dunes", "").0, 401);
    let (status, head, body) = get("/api/thumbnail/dunes", &cookie);
    assert_eq!(status, 200);
    assert!(head.contains("image/jpeg") && body == "jpeg");
    assert_eq!(get("/api/thumbnail/..%2Fpreferences", &cookie).0, 404);
    assert_eq!(get("/inter.woff2", "").0, 200);
    assert!(get("/api/strings", "").2.contains("\"pairTitle\""));

    // The live preview is drawn by the launcher thread; an unchanged scene is not drawn again.
    let ask = |known: &str, answer: Frame| {
        // Past the shared check interval, so the server asks the launcher again.
        thread::sleep(PREVIEW_CHECK);
        let cookie = cookie.clone();
        let route = format!("/api/preview.jpg?known={known}");
        let request = thread::spawn(move || get(&route, &cookie));
        let mut answer = Some(answer);
        loop {
            if let Event::Preview { known, reply } = next_event(&service) {
                let frame = answer.take().unwrap();
                if frame.image.is_none() {
                    assert_eq!(known, Some(frame.key), "the server offers its cached frame");
                }
                reply.send(Ok(frame)).unwrap();
                break;
            }
        }
        request.join().unwrap()
    };
    let pixels = || {
        Some(image::RgbaImage::from_pixel(
            4,
            4,
            image::Rgba([200, 30, 30, 255]),
        ))
    };
    let (status, head, body) = ask(
        "",
        Frame {
            key: 7,
            on_screen: true,
            image: pixels(),
        },
    );
    assert_eq!(status, 200);
    assert!(head.contains("X-Luma-Frame: 7") && body.len() > 100);
    // The page already shows frame 7 and nothing changed: nothing to send.
    let (status, _, body) = ask(
        "7",
        Frame {
            key: 7,
            on_screen: true,
            image: None,
        },
    );
    assert_eq!((status, body.len()), (204, 0));
    // Another device without the frame gets the cached picture without a new drawing.
    let (status, head, _) = ask(
        "",
        Frame {
            key: 7,
            on_screen: true,
            image: None,
        },
    );
    assert!(status == 200 && head.contains("X-Luma-Frame: 7"));
    let (status, head, _) = ask(
        "7",
        Frame {
            key: 8,
            on_screen: false,
            image: pixels(),
        },
    );
    assert!(status == 200 && head.contains("X-Luma-Frame: 8"));
    assert!(head.contains("X-Luma-On-Screen: false"));

    // Long polling: held while the screen is unchanged, answered when it changes.
    thread::sleep(PREVIEW_CHECK);
    let started = Instant::now();
    let request = {
        let cookie = cookie.clone();
        thread::spawn(move || get("/api/preview.jpg?known=8&wait=1", &cookie))
    };
    let mut answers = 0;
    loop {
        if let Event::Preview { reply, .. } = next_event(&service) {
            answers += 1;
            // Unchanged twice, then the screen changes.
            let frame = if answers < 3 {
                Frame {
                    key: 8,
                    on_screen: false,
                    image: None,
                }
            } else {
                Frame {
                    key: 9,
                    on_screen: true,
                    image: pixels(),
                }
            };
            reply.send(Ok(frame)).unwrap();
            if answers == 3 {
                break;
            }
        }
    }
    let (status, head, _) = request.join().unwrap();
    assert!(status == 200 && head.contains("X-Luma-Frame: 9"));
    assert!(
        started.elapsed() >= PREVIEW_CHECK * 2,
        "held while unchanged"
    );

    // Volume, picture and sleep timer go to the launcher, which answers with the TV's state.
    assert_eq!(post("/api/tv", &cookie, r#"{"sleep":999}"#).0, 422);
    assert_eq!(post("/api/tv", &cookie, r#"{"brightness":3}"#).0, 422);
    assert_eq!(post("/api/tv", "", r#"{"volume":20}"#).0, 401);
    let request = {
        let cookie = cookie.clone();
        thread::spawn(move || post("/api/tv", &cookie, r#"{"volume":20,"sleep":30}"#))
    };
    loop {
        if let Event::Tv(change, reply) = next_event(&service) {
            assert_eq!(
                (change.volume, change.screen, change.sleep),
                (Some(20), None, Some(30))
            );
            let (reading, answer) = mpsc::channel();
            reading
                .send(crate::tv_remote::Reading {
                    volume: 20,
                    max_volume: 100,
                    muted: false,
                    screen_off: false,
                })
                .unwrap();
            reply
                .send(TvAnswer {
                    sleep_left: Some(Duration::from_secs(1800)),
                    reading: answer,
                })
                .unwrap();
            break;
        }
    }
    let (status, _, body) = request.join().unwrap();
    assert_eq!(status, 200);
    assert!(body.contains("\"volume\":20") && body.contains("\"sleepLeft\":1800"));

    let mut changed = settings.clone();
    changed.hour_24 = true;
    let (status, _, _) = post(
        "/api/settings",
        &cookie,
        &serde_json::to_string(&changed).unwrap(),
    );
    assert_eq!(status, 200);
    let applied = loop {
        if let Event::Settings(revision, settings) = next_event(&service) {
            service.acknowledge(revision);
            break settings;
        }
    };
    assert!(applied.hour_24);

    // An input change reaches the launcher cleaned; unknown icons and inputs are refused.
    let (status, _, body) = post(
        "/api/inputs",
        &cookie,
        r#"{"id":"com.webos.app.hdmi1","name":"  PS5\u0007 ","mode":"On","icon":"playstation"}"#,
    );
    assert_eq!(status, 200);
    assert!(body.contains("\"name\":\"PS5\""));
    let change = loop {
        if let Event::Input(change) = next_event(&service) {
            break change;
        }
    };
    assert_eq!(change.name, "PS5");
    assert_eq!(change.mode, crate::model::InputMode::On);
    let bad_icon = r#"{"id":"com.webos.app.hdmi1","name":"","mode":"On","icon":"../x"}"#;
    assert_eq!(post("/api/inputs", &cookie, bad_icon).0, 422);
    let unknown = r#"{"id":"com.webos.app.hdmi9","name":"","mode":"On","icon":""}"#;
    assert_eq!(post("/api/inputs", &cookie, unknown).0, 422);
    assert_eq!(get("/icon/input-playstation.svg", "").0, 200);
    assert_eq!(get("/icon/input-..%2Fx.svg", "").0, 404);

    // A market symbol the TV would not accept is refused with the TV's own explanation.
    let symbol =
        r#"{"action":"market","widget":"stocks","symbol":"not a symbol!","currency":"USD"}"#;
    let (status, _, body) = post("/api/widgets", &cookie, symbol);
    assert_eq!(status, 422);
    assert!(
        body.contains(
            &crate::i18n::Language::En
                .t("invalidMarketSymbol")
                .replace('"', "\\\"")
        )
    );
    // Further widgets are named like { "extra_weather": 0 }, and an unknown field is refused.
    let extra = r#"{"action":"remove","widget":{"extra_weather":0},"x":1}"#;
    assert_eq!(post("/api/widgets", &cookie, extra).0, 422);

    // A place is chosen by position in the last search, never by coordinates from the page.
    assert_eq!(post("/api/location", &cookie, r#"{"index":0}"#).0, 422);
    assert_eq!(post("/api/places", &cookie, r#"{"query":"a"}"#).0, 422);
    let (status, _, _) = post("/api/location", &cookie, r#"{"index":null}"#);
    assert_eq!(status, 200);
    loop {
        if let Event::WeatherLocation(widget, place) = next_event(&service) {
            assert_eq!(widget, crate::model::HomeWidget::Weather);
            assert!(place.is_none());
            break;
        }
    }

    // Shortcut tiles follow the TV tile editor's rules and reach the launcher as a full list.
    let tile = r#"{"id":"","title":"Lights","url":"http://homeassistant.local:8123/api/webhook/x","method":"POST","icon":"sparkles"}"#;
    let (status, _, body) = post("/api/tiles/save", &cookie, tile);
    assert_eq!(status, 200);
    let tiles = loop {
        if let Event::Tiles(tiles) = next_event(&service) {
            break tiles;
        }
    };
    assert_eq!(tiles.len(), 1);
    assert!(body.contains(&format!("\"id\":\"{}\"", tiles[0].id)));
    let bad = r#"{"id":"","title":"Bad","url":"ftp://x","method":"POST","icon":""}"#;
    assert_eq!(post("/api/tiles/save", &cookie, bad).0, 422);
    let remove = format!(r#"{{"id":"{}"}}"#, tiles[0].id);
    assert_eq!(post("/api/tiles/remove", &cookie, &remove).0, 200);
    loop {
        if let Event::Tiles(tiles) = next_event(&service) {
            assert!(tiles.is_empty());
            break;
        }
    }
    assert_eq!(get("/icon/tile-sparkles.svg", "").0, 200);
    assert_eq!(get("/icon/tile-nope.svg", "").0, 404);

    // Photos: only paired devices, only JPEG; stored by the wallpaper worker, then listed.
    let upload = |cookie: &str, kind: &str| {
        http(&format!(
            "POST /api/photo HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nCookie: {cookie}\r\nX-Luma-Request: 1\r\nContent-Type: {kind}\r\nContent-Length: 4\r\n\r\njpeg"
        ))
    };
    assert_eq!(upload("", "image/jpeg").0, 401);
    assert_eq!(upload(&cookie, "image/png").0, 415);
    let photo = {
        let cookie = cookie.clone();
        thread::spawn(move || upload(&cookie, "image/jpeg"))
    };
    let id = "custom-0123456789abcdef0123456789abcdef".to_owned();
    loop {
        if let Event::Photo { bytes, reply } = next_event(&service) {
            assert_eq!(bytes, b"jpeg");
            reply.send(Ok(id.clone())).unwrap();
            break;
        }
    }
    let mut with_photo = catalog.clone();
    with_photo
        .backgrounds
        .push(crate::web_settings::Background {
            id: id.clone(),
            name: String::new(),
            thumbnail: thumbnail.clone(),
        });
    service.sync(true, false, &settings, &with_photo);
    let (status, _, body) = photo.join().unwrap();
    assert_eq!(status, 200);
    assert!(body.contains(&format!("\"id\":\"{id}\"")));
    // Packaged backgrounds cannot be deleted from the page.
    assert_eq!(
        post("/api/photo/delete", &cookie, r#"{"id":"dunes"}"#).0,
        422
    );

    // System actions are a fixed set; the page sees the paired devices with itself marked.
    assert_eq!(post("/api/action", &cookie, r#"{"action":"quit"}"#).0, 422);
    let (status, _, body) = post("/api/action", &cookie, r#"{"action":"refresh"}"#);
    assert_eq!(status, 200);
    assert!(body.contains("\"this\":true"));
    loop {
        if let Event::Action(action) = next_event(&service) {
            assert_eq!(action, "refresh");
            break;
        }
    }
    assert_eq!(
        post("/api/devices/remove", &cookie, r#"{"id":"nope"}"#).0,
        422
    );

    // Unknown fields cannot reach other preferences.
    let (status, _, _) = post("/api/settings", &cookie, r#"{"hidden":["x"]}"#);
    assert_eq!(status, 422);

    // Removing the device on the TV ends its access at once.
    let id = service.devices()[0].id.clone();
    assert!(service.forget(&id).is_some());
    assert_eq!(get("/api/settings", &cookie).0, 401);

    // Turning web access off closes the server.
    service.sync(false, false, &settings, &catalog);
    thread::sleep(Duration::from_millis(400));
    assert!(TcpStream::connect(("127.0.0.1", PORT)).is_err());
    drop(service);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&thumbnail);
}

#[test]
fn every_input_icon_the_tv_offers_reaches_the_page() {
    for (icon, _) in crate::model::INPUT_ICONS {
        assert!(
            input_icon(&format!("/icon/input-{icon}.svg")).is_some(),
            "the page cannot load the {icon} input icon"
        );
    }
}
