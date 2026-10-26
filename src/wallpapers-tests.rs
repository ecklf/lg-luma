use super::*;
use std::sync::atomic::AtomicUsize;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "luma-wallpapers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn wallpapers(&self) -> PathBuf {
        self.0.join("wallpapers")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn image(width: u32, height: u32) -> Vec<u8> {
    jpeg(
        &image::RgbImage::from_pixel(width, height, image::Rgb([150, 200, 230])),
        85,
    )
    .unwrap()
}

/// The HTTP tests use only a loopback mock, never TV or weather services.
fn http(path: &Path, request: impl FnOnce(&str) -> Vec<u8>) -> (String, bool) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let path = path.to_owned();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        serve(&mut socket, &address.to_string(), "secret", &path, || true).unwrap_or(false)
    });
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    socket.write_all(&request(&address.to_string())).unwrap();
    let mut response = String::new();
    let _ = socket.read_to_string(&mut response);
    (response, worker.join().unwrap())
}

#[test]
fn generated_ids_are_safe_and_selection_survives_preferences_reload() {
    let id = format!("custom-{}", random_id().unwrap());
    assert!(valid_id(&id));
    assert_ne!(id, format!("custom-{}", random_id().unwrap()));
    let mut preferences = crate::model::Preferences {
        background: id.clone(),
        ..Default::default()
    };
    preferences.sanitize();
    assert_eq!(preferences.background, id);
    for invalid in [
        "custom-../etc",
        "custom-",
        "custom-123",
        "custom-FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        "/etc/passwd",
    ] {
        assert!(!valid_id(invalid));
        preferences.background = invalid.into();
        preferences.sanitize();
        assert_eq!(preferences.background, "dunes");
    }
}

#[test]
fn uploads_are_named_normalized_immutable_and_discoverable_after_restart() {
    let fixture = Fixture::new();
    let bytes = image(1920, 1080);
    let first = store(&fixture.wallpapers(), "  My\n view  ", &bytes, || true).unwrap();
    let second = store(&fixture.wallpapers(), "My view", &bytes, || true).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(first.name, "My view");
    let found = catalog(&fixture.wallpapers());
    assert_eq!(found.len(), 2);
    assert!(
        found
            .iter()
            .any(|item| item.id == first.id && item.name == "My view")
    );
    let background = image::open(first.image()).unwrap();
    let thumbnail = image::open(first.thumbnail()).unwrap();
    assert_eq!((background.width(), background.height()), (1920, 1080));
    assert_eq!((thumbnail.width(), thumbnail.height()), (480, 270));
    assert!(
        background.to_rgb8().get_pixel(960, 1000)[0] < thumbnail.to_rgb8().get_pixel(240, 250)[0]
    );
    assert_eq!(
        fs::read(first.directory.join("metadata.json"))
            .unwrap()
            .len(),
        serde_json::to_vec(&first).unwrap().len()
    );
}

#[test]
fn validation_and_cancellation_leave_no_partial_gallery_entries() {
    let fixture = Fixture::new();
    let root = fixture.wallpapers();
    assert!(store(&root, "name", b"<svg></svg>", || true).is_err());
    assert!(store(&root, "name", &image(320, 240), || true).is_err());
    assert!(store(&root, " \n", &image(1920, 1080), || true).is_err());
    assert!(store(&root, "name", &image(1920, 1080), || false).is_err());
    assert!(catalog(&root).is_empty());
    assert!(!root.exists());
    let mut reader = Cursor::new(b"unfinished");
    assert!(
        read_exact(
            &mut reader,
            &mut [0; 20],
            &|| false,
            Instant::now() + REQUEST_TIME
        )
        .is_err()
    );
    assert!(read_exact(&mut reader, &mut [0; 20], &|| true, Instant::now()).is_err());
}

#[test]
fn cancellation_before_commit_cleans_only_the_new_staging_files() {
    let fixture = Fixture::new();
    let checks = AtomicUsize::new(0);
    let result = store(
        &fixture.wallpapers(),
        "Cancelled",
        &image(1920, 1080),
        || checks.fetch_add(1, Ordering::Relaxed) == 0,
    );
    assert!(result.is_err());
    assert_eq!(checks.load(Ordering::Relaxed), 2);
    assert!(fs::read_dir(fixture.wallpapers()).unwrap().next().is_none());
}

#[test]
fn gallery_capacity_is_checked_before_decoding_or_creating_another_upload() {
    let fixture = Fixture::new();
    create_directory(&fixture.wallpapers()).unwrap();
    for index in 0..MAX_WALLPAPERS {
        let id = format!("custom-{index:032x}");
        let directory = fixture.wallpapers().join(&id);
        create_directory(&directory).unwrap();
        write_new(
            &directory.join("metadata.json"),
            &serde_json::to_vec(&Wallpaper {
                id,
                name: format!("Photo {index}"),
                directory: directory.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        write_new(&directory.join("wallpaper.jpg"), b"fixture").unwrap();
        write_new(&directory.join("thumbnail.jpg"), b"fixture").unwrap();
    }
    assert_eq!(catalog(&fixture.wallpapers()).len(), MAX_WALLPAPERS);
    assert!(
        store(&fixture.wallpapers(), "Another", b"not decoded", || true)
            .unwrap_err()
            .contains("full")
    );
    assert_eq!(
        fs::read_dir(fixture.wallpapers()).unwrap().count(),
        MAX_WALLPAPERS
    );
}

#[test]
fn decoder_enforces_transport_dimensions_and_composites_transparent_pngs() {
    assert!(decode(&vec![0; MAX_BODY + 1]).is_err());
    let rgba = image::RgbaImage::from_pixel(1920, 1080, image::Rgba([255, 0, 0, 0]));
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    assert_eq!(
        decode(png.get_ref()).unwrap().get_pixel(0, 0).0,
        [23, 25, 28]
    );
    let too_tall = image::RgbaImage::new(1, 12000);
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(too_tall)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    assert!(decode(png.get_ref()).is_err());
}

#[test]
fn protocol_rejects_oversized_ambiguous_and_chunked_requests() {
    for request_text in [
        format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        ),
        "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n".into(),
        "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n".into(),
        "POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n".into(),
        "POST / HTTP/1.1\r\nContent-Length: +1\r\n\r\n".into(),
        "POST / HTTP/1.1\r\nContent-Length: 9999999999999999999999999\r\n\r\n".into(),
        format!("GET / HTTP/1.1\r\nX-Header: {}", "a".repeat(8192)),
    ] {
        assert!(
            request(
                &mut Cursor::new(request_text),
                &|| true,
                Instant::now() + REQUEST_TIME
            )
            .is_err()
        );
    }
}

#[test]
fn only_the_session_origin_host_token_and_upload_header_allow_writes() {
    let fixture = Fixture::new();
    for (route, headers, expected) in [
        ("/wrong/", "", "404 Not Found"),
        ("/secret/../../etc/passwd", "", "404 Not Found"),
        (
            "/secret/",
            "Origin: https://evil.example\r\n",
            "403 Forbidden",
        ),
        (
            "/secret/",
            "Sec-Fetch-Site: cross-site\r\n",
            "403 Forbidden",
        ),
    ] {
        let (response, changed) = http(&fixture.wallpapers(), |host| {
            format!("GET {route} HTTP/1.1\r\nHost: {host}\r\n{headers}\r\n").into_bytes()
        });
        assert!(
            response.starts_with(&format!("HTTP/1.1 {expected}")),
            "{response}"
        );
        assert!(!changed);
    }
    let (response, changed) = http(&fixture.wallpapers(), |_| {
        b"GET /secret/ HTTP/1.1\r\nHost: attacker.example\r\n\r\n".to_vec()
    });
    assert!(response.starts_with("HTTP/1.1 403"));
    assert!(!changed);
    let (response, changed) = http(&fixture.wallpapers(), |host| {
        format!("POST /secret/upload?name=photo HTTP/1.1\r\nHost: {host}\r\nContent-Length: 0\r\nContent-Type: image/jpeg\r\n\r\n").into_bytes()
    });
    assert!(response.starts_with("HTTP/1.1 403"));
    assert!(!changed);
    assert!(!fixture.wallpapers().exists());
}

#[test]
fn browser_assets_are_local_and_a_real_http_upload_persists_the_name() {
    let fixture = Fixture::new();
    for route in ["", "app.js", "crop.js", "app.css", "luma-mark.svg"] {
        let (response, changed) = http(&fixture.wallpapers(), |host| {
            format!("GET /secret/{route} HTTP/1.1\r\nHost: {host}\r\n\r\n").into_bytes()
        });
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("Cache-Control: no-store"));
        assert!(response.contains("frame-ancestors 'none'"));
        assert!(!changed);
        if route == "luma-mark.svg" {
            assert!(response.starts_with("HTTP/1.1 200"));
            assert!(response.contains("image/svg+xml"));
            assert!(response.contains("<svg"));
        }
    }
    let bytes = image(1920, 1080);
    let (response, changed) = http(&fixture.wallpapers(), |host| {
        let mut wire = format!("POST /secret/upload?name=My%20%26%20view HTTP/1.1\r\nHost: {host}\r\nOrigin: http://{host}\r\nX-Luma-Upload: secret\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", bytes.len()).into_bytes();
        wire.extend_from_slice(&bytes);
        wire
    });
    assert!(response.starts_with("HTTP/1.1 201"), "{response}");
    assert!(response.contains("My & view"));
    assert!(changed);
    assert_eq!(catalog(&fixture.wallpapers())[0].name, "My & view");
}

#[test]
fn unicode_names_and_malformed_escapes_are_handled_without_paths_or_html() {
    assert_eq!(
        decode_name("%E6%97%85%E8%A1%8C+%26+sea").unwrap(),
        "旅行 & sea"
    );
    for name in ["%", "%0", "%gg", "%ff"] {
        assert!(decode_name(name).is_err());
    }
    assert!(decode_name(&"a".repeat(513)).is_err());
    let fixture = Fixture::new();
    let item = store(
        &fixture.wallpapers(),
        "../../<script>view</script>",
        &image(1920, 1080),
        || true,
    )
    .unwrap();
    assert!(item.directory.starts_with(fixture.wallpapers()));
    assert!(valid_id(&item.id));
    assert_eq!(item.name, "../../<script>view</script>");
}

#[cfg(unix)]
#[test]
fn symlink_storage_and_gallery_entries_are_never_followed() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let elsewhere = fixture.0.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    symlink(&elsewhere, fixture.wallpapers()).unwrap();
    assert!(store(&fixture.wallpapers(), "view", &image(1920, 1080), || true).is_err());
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
    assert!(catalog(&fixture.wallpapers()).is_empty());
}

#[test]
fn qr_generation_is_deterministic_and_has_a_reasonable_size() {
    let address = "192.168.1.2:12345".parse().unwrap();
    let info = session(address, "0123456789abcdef0123456789abcdef").unwrap();
    assert_eq!(
        info.url,
        "http://192.168.1.2:12345/0123456789abcdef0123456789abcdef/"
    );
    assert_eq!(info.qr.len(), (info.qr_size * info.qr_size) as usize);
    assert!(info.qr_size <= 65);
    assert_eq!(
        info.qr,
        session(address, "0123456789abcdef0123456789abcdef")
            .unwrap()
            .qr
    );
}

#[test]
fn session_closure_revokes_the_socket_and_reopening_rotates_the_capability() {
    let fixture = Fixture::new();
    let mut service = Service::new(&fixture.0.join("preferences.json"), true).unwrap();
    assert!(matches!(
        service
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        Event::Catalog(_)
    ));
    service.set_active(true);
    let first = match service
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    {
        Event::Session(generation, Ok(session)) => {
            assert_eq!(generation, service.generation());
            session
        }
        _ => panic!("Expected a local upload session"),
    };
    let address = first
        .url
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    assert!(address.starts_with("127.0.0.1:"));
    service.set_active(false);
    assert!(matches!(
        service
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        Event::Ended(_)
    ));
    assert!(TcpStream::connect(address).is_err());
    service.set_active(true);
    let second = match service
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    {
        Event::Session(_, Ok(session)) => session,
        _ => panic!("Expected a new upload session"),
    };
    assert_ne!(first.url, second.url);
    assert_ne!(first.url.split('/').nth(3), second.url.split('/').nth(3));
    service.set_active(false);
    assert!(matches!(
        service
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        Event::Ended(_)
    ));
}

#[test]
fn cancelling_a_slow_request_releases_the_worker_without_waiting_for_the_body() {
    let fixture = Fixture::new();
    let mut service = Service::new(&fixture.0.join("preferences.json"), true).unwrap();
    service
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    service.set_active(true);
    let info = match service
        .receiver
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    {
        Event::Session(_, Ok(session)) => session,
        _ => panic!("Expected a session"),
    };
    let parts: Vec<_> = info.url.split('/').collect();
    let mut socket = TcpStream::connect(parts[2]).unwrap();
    write!(socket, "POST /{}/upload?name=slow HTTP/1.1\r\nHost: {}\r\nX-Luma-Upload: {}\r\nContent-Type: image/jpeg\r\nContent-Length: 100000\r\n\r\npartial", parts[3], parts[2], parts[3]).unwrap();
    service.set_active(false);
    assert!(matches!(
        service
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        Event::Ended(_)
    ));
    assert!(catalog(&fixture.wallpapers()).is_empty());
}
