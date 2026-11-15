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
    let mut service = Service::new(path.clone(), true).unwrap();
    let settings = Settings::of(&crate::model::Preferences::default());
    let thumbnail = path.with_extension("jpg");
    std::fs::write(&thumbnail, b"jpeg").unwrap();
    let catalog = vec![Background {
        id: "dunes".into(),
        name: "Monument Valley".into(),
        thumbnail: thumbnail.clone(),
    }];

    service.sync(true, true, &settings, &catalog, &Library::default());
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

    // Guessing is capped: the fifth wrong PIN replaces the PIN shown on the TV.
    let wrong = if first.pin == "000000" {
        "111111"
    } else {
        "000000"
    };
    for _ in 0..PIN_ATTEMPTS {
        let (status, _, _) = post("/api/pair", "", &format!(r#"{{"pin":"{wrong}"}}"#));
        assert_eq!(status, 403);
    }
    let replaced = pairing(&service);
    let (status, _, _) = post("/api/pair", "", &format!(r#"{{"pin":"{}"}}"#, first.pin));
    assert_eq!(status, if replaced.pin == first.pin { 200 } else { 403 });
    assert_ne!(replaced.pin, first.pin, "a new PIN is drawn");
    let first = replaced;

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
    service.sync(true, false, &settings, &catalog, &Library::default());
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

    let mut changed = settings.clone();
    changed.hour_24 = true;
    changed.greeting_name = "  Ana\u{7}  ".into();
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
    assert_eq!(applied.greeting_name, "Ana");

    // Unknown fields cannot reach other preferences.
    let (status, _, _) = post("/api/settings", &cookie, r#"{"hidden":["x"]}"#);
    assert_eq!(status, 422);

    // Removing the device on the TV ends its access at once.
    let id = service.devices()[0].id.clone();
    assert!(service.forget(&id).is_some());
    assert_eq!(get("/api/settings", &cookie).0, 401);

    // Turning web access off closes the server.
    service.sync(false, false, &settings, &catalog, &Library::default());
    thread::sleep(Duration::from_millis(400));
    assert!(TcpStream::connect(("127.0.0.1", PORT)).is_err());
    drop(service);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&thumbnail);
}
