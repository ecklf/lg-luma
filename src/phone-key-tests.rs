use super::*;

fn texts() -> PageTexts {
    PageTexts {
        title: "PandaScore key".into(),
        steps: vec!["Sign in".into(), "Copy the token".into()],
        field: "Key".into(),
        send: "Send".into(),
        saved: "Saved".into(),
        invalid: "Invalid".into(),
        accepts: None,
    }
}

fn reply(request: &str) -> (String, Option<String>) {
    let (bytes, key) = respond(request.as_bytes(), "a1b2c3", &texts());
    (String::from_utf8(bytes).unwrap(), key)
}

#[test]
fn the_token_page_serves_a_form_with_the_steps() {
    let (page, key) = reply("GET /a1b2c3 HTTP/1.1\r\nHost: tv\r\n\r\n");
    assert!(page.starts_with("HTTP/1.1 200"));
    assert!(page.contains("<form method=\"post\""));
    assert!(page.contains("Copy the token"));
    assert_eq!(key, None);
}

#[test]
fn posting_the_form_hands_over_a_decoded_key() {
    let body = "key=+ps_Ab-12.x%5Fz+";
    let request = format!(
        "POST /a1b2c3 HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let (page, key) = reply(&request);
    assert!(page.starts_with("HTTP/1.1 200"));
    assert!(page.contains("Saved"));
    assert_eq!(key.as_deref(), Some("ps_Ab-12.x_z"));
}

#[test]
fn other_paths_and_unusable_keys_are_refused() {
    assert!(
        reply("GET / HTTP/1.1\r\n\r\n")
            .0
            .starts_with("HTTP/1.1 404")
    );
    assert!(
        reply("GET /other HTTP/1.1\r\n\r\n")
            .0
            .starts_with("HTTP/1.1 404")
    );
    let (page, key) = reply("POST /a1b2c3 HTTP/1.1\r\nContent-Length: 13\r\n\r\nkey=<script>");
    assert!(page.contains("Invalid"));
    assert_eq!(key, None);
    let (_, key) = reply("POST /a1b2c3 HTTP/1.1\r\nContent-Length: 4\r\n\r\nkey=");
    assert_eq!(key, None, "an empty key changes nothing");
}

#[test]
fn page_text_is_escaped() {
    let mut texts = texts();
    texts.title = "<b>&".into();
    let (bytes, _) = respond(b"GET /a1b2c3 HTTP/1.1\r\n\r\n", "a1b2c3", &texts);
    let page = String::from_utf8(bytes).unwrap();
    assert!(page.contains("&lt;b&gt;&amp;"));
}

#[test]
fn the_qr_code_is_a_square_image_with_a_quiet_zone() {
    let image = qr_image("http://192.168.0.218:8765/a1b2c3").unwrap();
    assert_eq!(image.width(), image.height());
    assert!(image.width() >= 25 * 8);
    // The corner is the white quiet zone; the finder pattern starts dark after it.
    assert_eq!(image.get_pixel(2, 2)[0], 255);
    assert_eq!(image.get_pixel(4 * 8 + 2, 4 * 8 + 2)[0], 0);
}

#[test]
fn a_store_widget_secret_is_checked_by_its_own_rule() {
    let mut texts = texts();
    // E.g. a manifest's pattern: exactly eight digits, spaces allowed around.
    texts.accepts = Some(std::sync::Arc::new(|key: &str| {
        key.len() == 8 && key.bytes().all(|b| b.is_ascii_digit())
    }));
    let post = |body: &str| {
        format!(
            "POST /a1b2c3 HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    };
    let (page, key) = respond(post("key=12345678").as_bytes(), "a1b2c3", &texts);
    assert!(String::from_utf8(page).unwrap().contains("Saved"));
    assert_eq!(key.as_deref(), Some("12345678"));
    let (page, key) = respond(post("key=ps_Ab-12").as_bytes(), "a1b2c3", &texts);
    assert!(String::from_utf8(page).unwrap().contains("Invalid"));
    assert_eq!(key, None);
}
