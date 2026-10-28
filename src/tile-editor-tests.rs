use super::*;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener},
};

fn tile(id: &str, title: &str) -> CustomTile {
    CustomTile {
        id: id.into(),
        title: title.into(),
        url: "http://homeassistant.local:8123/api/webhook/lights".into(),
        ..Default::default()
    }
}

fn edit(id: &str, title: &str, url: &str) -> Edit {
    Edit {
        id: id.into(),
        title: title.into(),
        url: url.into(),
        ..Default::default()
    }
}

/// The HTTP tests use only a loopback mock, never a TV or webhook.
fn http(tiles: &mut Vec<CustomTile>, request: impl FnOnce(&str) -> Vec<u8>) -> (String, bool) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    socket.write_all(&request(&address.to_string())).unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    let agent = crate::webhook::agent();
    let changed = serve(
        &mut stream,
        &address.to_string(),
        "secret",
        tiles,
        &agent,
        || true,
        |_| Ok(()),
    )
    .unwrap_or(false);
    drop(stream);
    let mut response = String::new();
    let _ = socket.read_to_string(&mut response);
    (response, changed)
}

fn post(route: &str, body: &str, headers: &str) -> impl FnOnce(&str) -> Vec<u8> {
    let route = route.to_owned();
    let body = body.to_owned();
    let headers = headers.to_owned();
    move |host| {
        format!(
            "POST /secret/{route} HTTP/1.1\r\nHost: {host}\r\nOrigin: http://{host}\r\n{headers}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }
}

const WRITE: &str = "X-Luma-Token: secret\r\nContent-Type: application/json\r\n";

fn session_request(session: &Session, method: &str, route: &str, body: &str) -> serde_json::Value {
    let parts: Vec<_> = session.url.split('/').collect();
    let mut socket = TcpStream::connect(parts[2]).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(socket, "{method} /{}/{route} HTTP/1.1\r\nHost: {}\r\nX-Luma-Token: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", parts[3], parts[2], parts[3], body.len()).unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap()
}

fn next_session(service: &Service) -> Session {
    loop {
        match service
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
        {
            Event::Session(generation, Ok(session)) => {
                assert_eq!(generation, service.generation());
                return session;
            }
            Event::Ended(_) => {}
            _ => panic!("Expected a local editing session"),
        }
    }
}

#[test]
fn reopening_preserves_unacknowledged_saves_and_removals() {
    let mut service = Service::new(true).unwrap();
    service.set_active(true, &[]);
    let first = next_session(&service);
    session_request(
        &first,
        "POST",
        "save",
        r#"{"title":"Lamp","url":"http://127.0.0.1:9/hook"}"#,
    );
    // Rotate before SDL has consumed the edit; its preferences still contain no tiles.
    service.set_active(false, &[]);
    service.set_active(true, &[]);
    let Event::Tiles(lamp_revision, lamp_tiles) = service
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    else {
        panic!("Expected saved Lamp")
    };
    let second = next_session(&service);
    let listing = session_request(&second, "GET", "tiles.json", "");
    assert_eq!(listing["tiles"][0]["id"], "custom.lamp");
    session_request(
        &second,
        "POST",
        "save",
        r#"{"title":"Gate","url":"http://127.0.0.1:9/hook"}"#,
    );
    let Event::Tiles(gate_revision, both_tiles) = service
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    else {
        panic!("Expected saved Gate")
    };
    assert_eq!(both_tiles.len(), 2);
    // Acknowledging an older event cannot make a newer pending save replaceable.
    service.acknowledge(lamp_revision);
    service.set_active(false, &[]);
    service.set_active(true, &lamp_tiles);
    let third = next_session(&service);
    assert_eq!(
        session_request(&third, "GET", "tiles.json", "")["tiles"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    service.acknowledge(gate_revision);
    session_request(&third, "POST", "remove", r#"{"id":"custom.lamp"}"#);
    service.set_active(false, &[]);
    service.set_active(true, &both_tiles);
    let Event::Tiles(remove_revision, remaining) = service
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    else {
        panic!("Expected removed Lamp")
    };
    let fourth = next_session(&service);
    let listing = session_request(&fourth, "GET", "tiles.json", "");
    assert_eq!(listing["tiles"].as_array().unwrap().len(), 1);
    assert_eq!(listing["tiles"][0]["id"], "custom.gate");
    service.acknowledge(remove_revision);
    service.acknowledge(lamp_revision);
    // Once every edit is consumed, a later TV-side edit may seed a fresh session.
    let mut tv_tiles = remaining;
    tv_tiles[0].title = "Garage".into();
    service.set_active(false, &[]);
    service.set_active(true, &tv_tiles);
    let fifth = next_session(&service);
    assert_eq!(
        session_request(&fifth, "GET", "tiles.json", "")["tiles"][0]["title"],
        "Garage"
    );
    service.set_active(false, &[]);
}

#[test]
fn saving_adds_a_tile_with_a_unique_id_and_edits_keep_it() {
    let mut tiles = vec![tile("custom.lights", "Lights")];
    let id = save(
        &mut tiles,
        &edit("", "  Lights ", "https://example.com/hook"),
    )
    .unwrap();
    assert_eq!(id, "custom.lights-2");
    assert_eq!(tiles[1].title, "Lights");
    let id = save(
        &mut tiles,
        &Edit {
            method: TileMethod::Get,
            icon: "power".into(),
            ..edit("custom.lights-2", "Garage", "http://10.0.0.2/open")
        },
    )
    .unwrap();
    assert_eq!(id, "custom.lights-2");
    assert_eq!(tiles.len(), 2);
    assert_eq!(tiles[1].title, "Garage");
    assert_eq!(tiles[1].method, TileMethod::Get);
    assert_eq!(tiles[1].icon(), "power");
}

#[test]
fn saving_uses_the_tv_editors_rules() {
    let mut tiles = vec![tile("custom.lights", "Lights")];
    for invalid in [
        edit("", " ", "http://example.com"),
        edit("", "Gate", "example.com"),
        edit("", "Gate", "http:///"),
        edit("", "Gate", "ftp://example.com"),
        edit("custom.gone", "Gate", "http://example.com"),
        Edit {
            icon: "../etc/passwd".into(),
            ..edit("", "Gate", "http://example.com")
        },
    ] {
        assert!(save(&mut tiles, &invalid).is_err(), "{invalid:?}");
    }
    assert_eq!(tiles, vec![tile("custom.lights", "Lights")]);
    let mut full: Vec<_> = (0..TILE_LIMIT)
        .map(|index| tile(&format!("custom.t{index}"), "Tile"))
        .collect();
    assert!(save(&mut full, &edit("", "One more", "http://example.com")).is_err());
    assert_eq!(full.len(), TILE_LIMIT);
    assert!(remove(&mut tiles, "custom.gone").is_err());
    remove(&mut tiles, "custom.lights").unwrap();
    assert!(tiles.is_empty());
}

#[test]
fn every_choosable_icon_ships_its_markup() {
    let listing = listing(&[]);
    let icons = listing["icons"].as_array().unwrap();
    assert_eq!(icons.len(), TILE_ICONS.len());
    for icon in icons {
        assert!(icon["svg"].as_str().unwrap().starts_with("<svg"), "{icon}");
        assert!(!icon["name"].as_str().unwrap().is_empty());
    }
    assert_eq!(listing["limit"], TILE_LIMIT);
}

#[test]
fn page_assets_and_the_tile_list_are_served_under_the_token() {
    let mut tiles = vec![tile("custom.lights", "Lights")];
    for route in ["", "app.js", "app.css", "tiles.js", "luma-mark.svg"] {
        let (response, changed) = http(&mut tiles, |host| {
            format!("GET /secret/{route} HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes()
        });
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "{route}: {response}"
        );
        assert!(response.contains("Content-Security-Policy: default-src 'none'"));
        assert!(!changed);
    }
    let (response, _) = http(&mut tiles, |host| {
        format!("GET /secret/tiles.json HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes()
    });
    let body: serde_json::Value =
        serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["tiles"][0]["id"], "custom.lights");
    let (response, _) = http(&mut tiles, |host| {
        format!("GET /secret HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes()
    });
    assert!(response.contains("\r\nLocation: /secret/\r\n"));
}

#[test]
fn only_the_session_origin_token_and_json_allow_changes() {
    let mut tiles = vec![tile("custom.lights", "Lights")];
    let body = r#"{"title":"Gate","url":"http://10.0.0.2/open"}"#;
    let cross_site = format!("{WRITE}Sec-Fetch-Site: cross-site\r\n");
    for (route, headers, expected) in [
        ("save", "Content-Type: application/json\r\n", "403"),
        (
            "save",
            "X-Luma-Token: wrong\r\nContent-Type: application/json\r\n",
            "403",
        ),
        (
            "save",
            "X-Luma-Token: secret\r\nContent-Type: text/plain\r\n",
            "415",
        ),
        ("delete", WRITE, "403"),
        ("save", cross_site.as_str(), "403"),
    ] {
        let (response, changed) = http(&mut tiles, post(route, body, headers));
        assert!(
            response.starts_with(&format!("HTTP/1.1 {expected}")),
            "{route} {headers}: {response}"
        );
        assert!(!changed);
    }
    let (response, changed) = http(&mut tiles, |host| {
        format!("GET /wrong/tiles.json HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes()
    });
    assert!(response.starts_with("HTTP/1.1 404"));
    assert!(!changed);
    let (response, changed) = http(&mut tiles, |_| {
        format!(
            "POST /secret/save HTTP/1.1\r\nHost: attacker.example\r\n{WRITE}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    });
    assert!(response.starts_with("HTTP/1.1 403"));
    assert!(!changed);
    assert_eq!(tiles, vec![tile("custom.lights", "Lights")]);
}

#[test]
fn a_real_http_save_and_removal_change_the_list() {
    let mut tiles = vec![tile("custom.lights", "Lights")];
    let (response, changed) = http(
        &mut tiles,
        post(
            "save",
            r#"{"title":"Gate","url":"http://10.0.0.2/open","method":"GET","icon":"power"}"#,
            WRITE,
        ),
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(changed);
    assert!(response.contains(r#""id":"custom.gate""#));
    assert_eq!(tiles[1].url, "http://10.0.0.2/open");
    let (response, changed) = http(
        &mut tiles,
        post("save", r#"{"title":"Gate","url":"10.0.0.2/open"}"#, WRITE),
    );
    assert!(response.starts_with("HTTP/1.1 422"), "{response}");
    assert!(!changed);
    let (response, changed) = http(
        &mut tiles,
        post("remove", r#"{"id":"custom.lights"}"#, WRITE),
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(changed);
    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0].id, "custom.gate");
    let (response, changed) = http(&mut tiles, post("remove", "not json", WRITE));
    assert!(response.starts_with("HTTP/1.1 400"));
    assert!(!changed);
}

#[test]
fn a_test_request_is_sent_without_saving() {
    let hook = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let url = format!("http://{}/api/webhook/test", hook.local_addr().unwrap());
    let receiver = thread::spawn(move || {
        let (mut socket, _) = hook.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 1024];
        let read = socket.read(&mut request).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        String::from_utf8_lossy(&request[..read]).into_owned()
    });
    let mut tiles = vec![];
    let (response, changed) = http(
        &mut tiles,
        post(
            "send",
            &format!(r#"{{"title":"Test","url":"{url}","method":"POST"}}"#),
            WRITE,
        ),
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(!changed);
    assert!(tiles.is_empty());
    assert!(
        receiver
            .join()
            .unwrap()
            .starts_with("POST /api/webhook/test ")
    );
}

#[test]
fn session_closure_revokes_the_socket_and_reopening_rotates_the_capability() {
    let mut service = Service::new(true).unwrap();
    let open = |service: &mut Service| {
        service.set_active(true, &[tile("custom.lights", "Lights")]);
        match service
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
        {
            Event::Session(generation, Ok(session)) => {
                assert_eq!(generation, service.generation());
                session
            }
            _ => panic!("Expected a local editing session"),
        }
    };
    let first = open(&mut service);
    let parts: Vec<_> = first.url.split('/').collect();
    assert!(parts[2].starts_with("127.0.0.1:"));
    let mut socket = TcpStream::connect(parts[2]).unwrap();
    write!(
        socket,
        "GET /{}/tiles.json HTTP/1.1\r\nHost: {}\r\n\r\n",
        parts[3], parts[2]
    )
    .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.contains("custom.lights"), "{response}");
    service.set_active(false, &[]);
    assert!(matches!(
        service
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        Event::Ended(_)
    ));
    assert!(TcpStream::connect(parts[2]).is_err());
    let second = open(&mut service);
    assert_ne!(first.url.split('/').nth(3), second.url.split('/').nth(3));
    service.set_active(false, &[]);
}
