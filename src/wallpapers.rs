//! Immutable, app-owned wallpapers and a short-lived, capability-protected LAN upload server.
//! Network I/O, decoding, normalization, metadata discovery and QR generation stay on this worker.
pub use crate::companion::Session;
use crate::companion::{
    Request, error_response, listen, local_peer, random_token, read_exact, redirect_to_page,
    response_header, session,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufReader, Cursor, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_BODY: usize = 12 * 1024 * 1024;
const MAX_WALLPAPERS: usize = 20;
const SESSION_TIME: Duration = Duration::from_secs(600);
const SESSION_REQUEST_LIMIT: usize = 64;
const REQUEST_TIME: Duration = Duration::from_secs(20);
const STOP: u64 = u64::MAX;
// Only this companion page may fetch the pinned model as data. Executable code
// and WASM stay local. Other companion pages keep their original stricter CSP.
const BROWSER_POLICY: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self'; img-src 'self' blob: data:; connect-src 'self' https://huggingface.co https://*.xethub.hf.co; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
#[path = "wallpaper-depth.rs"]
mod depth;
use depth::{decode_mask, mask_placements, png};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Wallpaper {
    pub id: String,
    #[serde(skip)]
    pub directory: PathBuf,
    #[serde(skip)]
    pub has_original: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub clock_placements: BTreeMap<String, Vec<Option<f32>>>,
    #[serde(skip)]
    pub has_foreground: bool,
}

impl Wallpaper {
    pub fn foreground(&self, gradient: bool) -> String {
        self.directory
            .join(if gradient {
                "foreground.png"
            } else {
                "foreground-plain.png"
            })
            .to_string_lossy()
            .into_owned()
    }
    pub fn unshaded_image(&self) -> String {
        // Preserve full resolution for older uploads without an unshaded original.
        self.directory
            .join(if self.has_original {
                "original.jpg"
            } else {
                "wallpaper.jpg"
            })
            .to_string_lossy()
            .into_owned()
    }
    pub fn image(&self) -> String {
        self.directory
            .join("wallpaper.jpg")
            .to_string_lossy()
            .into_owned()
    }
    pub fn thumbnail(&self) -> String {
        self.directory
            .join("thumbnail.jpg")
            .to_string_lossy()
            .into_owned()
    }
}

pub fn valid_id(id: &str) -> bool {
    id.strip_prefix("custom-").is_some_and(|id| {
        id.len() == 32
            && id
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}

pub enum Event {
    Catalog(Vec<Wallpaper>),
    Deleted(String, Result<(), String>),
    Session(u64, Result<Session, String>),
    Ended(u64),
}

enum Command {
    Wake,
    Delete { id: String, selected: String },
}

pub struct Service {
    desired: Arc<AtomicU64>,
    wake: mpsc::Sender<Command>,
    pub receiver: mpsc::Receiver<Event>,
    active: bool,
    generation: u64,
}

impl Service {
    pub fn new(preferences: &Path, loopback_only: bool, assets: &Path) -> Result<Self, String> {
        let assets = assets.to_owned();
        let directory = preferences
            .parent()
            .unwrap_or(Path::new("."))
            .join("wallpapers");
        let desired = Arc::new(AtomicU64::new(0));
        let worker_desired = desired.clone();
        let (sender, receiver) = mpsc::sync_channel(4);
        let (wake, updates) = mpsc::channel();
        thread::Builder::new()
            .name("luma-wallpapers".into())
            .spawn(move || {
                worker(
                    &directory,
                    &worker_desired,
                    &sender,
                    &updates,
                    loopback_only,
                    &assets,
                );
            })
            .map_err(|e| format!("Could not start wallpaper worker: {e}"))?;
        Ok(Self {
            desired,
            wake,
            receiver,
            active: false,
            generation: 0,
        })
    }

    pub fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.generation += 1;
            self.desired
                .store(if active { self.generation } else { 0 }, Ordering::Release);
            let _ = self.wake.send(Command::Wake);
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn delete(&self, id: String, selected: String) {
        let _ = self.wake.send(Command::Delete { id, selected });
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.desired.store(STOP, Ordering::Release);
        let _ = self.wake.send(Command::Wake);
    }
}

fn random_id() -> Result<String, String> {
    let mut bytes = [0; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| format!("Secure random source unavailable: {e}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn small_file(path: &Path, maximum: u64) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > maximum {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= maximum).then_some(bytes)
}

fn catalog(directory: &Path) -> Vec<Wallpaper> {
    if !real_directory(directory) {
        return vec![];
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return vec![];
    };
    let mut result = Vec::new();
    // Bound discovery even if someone manually populates this private directory.
    for entry in entries.take(128).flatten() {
        let path = entry.path();
        let id = entry.file_name().to_string_lossy().into_owned();
        if !valid_id(&id) || !real_directory(&path) {
            continue;
        }
        let Some(bytes) = small_file(&path.join("metadata.json"), 2048) else {
            continue;
        };
        let Ok(mut item) = serde_json::from_slice::<Wallpaper>(&bytes) else {
            continue;
        };
        // Legacy metadata may contain a name; serde ignores it. Identity and
        // selection depend only on the generated ID, never a display label.
        if item.id != id {
            continue;
        }
        if ["wallpaper.jpg", "thumbnail.jpg"].iter().any(|file| {
            !fs::symlink_metadata(path.join(file))
                .is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= MAX_BODY as u64)
        }) {
            continue;
        }
        item.has_original = fs::symlink_metadata(path.join("original.jpg"))
            .is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= MAX_BODY as u64);
        item.has_foreground = item.has_original
            && ["foreground.png", "foreground-plain.png"]
                .iter()
                .all(|file| {
                    fs::symlink_metadata(path.join(file))
                        .is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= MAX_BODY as u64)
                });
        item.directory = path;
        result.push(item);
        if result.len() == MAX_WALLPAPERS {
            break;
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    result
}

fn delete_wallpaper(directory: &Path, id: &str, selected: &str) -> Result<(), String> {
    if id == selected || !valid_id(id) || !real_directory(directory) {
        return Err("Background cannot be deleted".into());
    }
    let path = directory.join(id);
    if !real_directory(&path) {
        return Err("Background directory unavailable".into());
    }
    // Only the strictly validated app-owned directory is removed; symlink
    // entries within it are unlinked, never followed by remove_dir_all.
    fs::remove_dir_all(path).map_err(|error| error.to_string())
}

/// Root Home creates files readable by the same app user after takeover is disabled.
fn create_directory(path: &Path) -> Result<(), String> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => inherit_owner(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && real_directory(path) => {
        }
        Err(error) => return Err(error.to_string()),
    }
    if !real_directory(path) {
        return Err("Wallpaper storage is not a real directory".into());
    }
    Ok(())
}

fn inherit_owner(path: &Path) -> Result<(), String> {
    #[cfg(all(feature = "webos", unix))]
    if unsafe { libc::geteuid() } == 0 {
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
        let parent = path.parent().ok_or("Missing wallpaper storage parent")?;
        let owner = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        if !owner.is_dir() {
            return Err("Invalid wallpaper storage parent".into());
        }
        use std::os::unix::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| e.to_string())?;
        if unsafe { libc::fchown(file.as_raw_fd(), owner.uid(), owner.gid()) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    let _ = path;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    inherit_owner(path)
}

fn decode(bytes: &[u8]) -> Result<image::RgbImage, String> {
    if bytes.len() > MAX_BODY {
        return Err("Image exceeds 12 MiB".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "Choose a JPEG, PNG or WebP image")?;
    if !matches!(
        format,
        image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP
    ) {
        return Err("Choose a JPEG, PNG or WebP image".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1920);
    limits.max_image_height = Some(1080);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("Invalid image: {e}"))?;
    use image::ImageDecoder;
    if decoder.dimensions() != (1920, 1080) {
        return Err("Crop the image to 1920 × 1080 before uploading".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    if image.width() != 1920 || image.height() != 1080 {
        return Err("Unsupported image orientation".into());
    }
    // Composite alpha against the launcher's base instead of exposing transparent pixels as black.
    let rgba = image.to_rgba8();
    Ok(image::RgbImage::from_fn(1920, 1080, |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let alpha = p[3] as f32 / 255.;
        image::Rgb(std::array::from_fn(|i| {
            (p[i] as f32 * alpha + [23., 25., 28.][i] * (1. - alpha)).round() as u8
        }))
    }))
}

fn shade(image: &mut image::RgbImage) {
    // Same base, image opacity and gradients as tools/assets.mjs. Only source-specific
    // saturation adjustments are omitted for owner-supplied photographs.
    let stops = [
        (0., [13., 16., 19.], 0.24),
        (0.19, [14., 17., 21.], 0.08),
        (0.35, [15., 17., 20.], 0.35),
        (0.63, [17., 19., 23.], 0.86),
        (0.85, [23., 25., 29.], 0.92157),
        (1., [23., 25., 29.], 0.96078),
    ];
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let y = y as f32 / 1079.;
        let pair = stops.windows(2).find(|s| y <= s[1].0).unwrap();
        let t = (y - pair[0].0) / (pair[1].0 - pair[0].0);
        let alpha = pair[0].2 * (1. - t) + pair[1].2 * t;
        let horizontal = 0.42 * (1. - x as f32 / (1919. * 0.75)).max(0.);
        for i in 0..3 {
            let base = pixel[i] as f32 * 0.92 + [23., 25., 28.][i] * 0.08;
            let base = base * (1. - horizontal) + [17., 20., 23.][i] * horizontal;
            // SVG gradients interpolate premultiplied colors.
            let gradient = pair[0].1[i] * pair[0].2 * (1. - t) + pair[1].1[i] * pair[1].2 * t;
            pixel[i] = (base * (1. - alpha) + gradient).round() as u8;
        }
    }
}

fn jpeg(image: &image::RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(image)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn store(directory: &Path, bytes: &[u8], valid: impl Fn() -> bool) -> Result<Wallpaper, String> {
    store_with_mask(directory, bytes, None, valid)
}

fn store_with_mask(
    directory: &Path,
    bytes: &[u8],
    mask: Option<&[u8]>,
    valid: impl Fn() -> bool,
) -> Result<Wallpaper, String> {
    if catalog(directory).len() >= MAX_WALLPAPERS {
        return Err("The wallpaper gallery is full (20 images)".into());
    }
    let mut image = decode(bytes)?;
    let original = jpeg(&image, 94)?;
    let thumbnail =
        image::imageops::resize(&image, 480, 270, image::imageops::FilterType::Triangle);
    shade(&mut image);
    let image = jpeg(&image, 94)?;
    let thumbnail = jpeg(&thumbnail, 82)?;
    let mask = mask.map(decode_mask).transpose()?;
    let clock_placements = mask.as_ref().map(mask_placements).unwrap_or_default();
    if !valid() {
        return Err("Upload session ended".into());
    }
    let parent = directory
        .parent()
        .ok_or("Missing wallpaper directory parent")?;
    if !parent.exists() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if !real_directory(parent) {
        return Err("Invalid wallpaper storage parent".into());
    }
    create_directory(directory)?;
    let id = format!("custom-{}", random_id()?);
    let staging = directory.join(format!(".{id}"));
    create_directory(&staging)?;
    let item = Wallpaper {
        id: id.clone(),
        directory: directory.join(&id),
        has_original: true,
        has_foreground: mask.is_some(),
        clock_placements,
    };
    let result = (|| {
        write_new(&staging.join("wallpaper.jpg"), &image)?;
        write_new(&staging.join("original.jpg"), &original)?;
        write_new(&staging.join("thumbnail.jpg"), &thumbnail)?;
        if let Some(mask) = &mask {
            write_new(
                &staging.join("mask.png"),
                &png(mask.as_raw(), image::ExtendedColorType::L8)?,
            )?;
            for (name, photo) in [
                ("foreground.png", &image),
                ("foreground-plain.png", &original),
            ] {
                if !valid() {
                    return Err("Upload session ended".into());
                }
                let rgb = decode(photo)?;
                let rgba = image::RgbaImage::from_fn(1920, 1080, |x, y| {
                    let p = rgb.get_pixel(x, y).0;
                    image::Rgba([p[0], p[1], p[2], mask.get_pixel(x, y).0[0]])
                });
                write_new(
                    &staging.join(name),
                    &png(rgba.as_raw(), image::ExtendedColorType::Rgba8)?,
                )?;
            }
        }
        write_new(
            &staging.join("metadata.json"),
            &serde_json::to_vec(&item).map_err(|e| e.to_string())?,
        )?;
        if !valid() {
            return Err("Upload session ended".into());
        }
        fs::rename(&staging, &item.directory).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        // Only the files and fresh directory created by this operation.
        for file in [
            "wallpaper.jpg",
            "original.jpg",
            "thumbnail.jpg",
            "metadata.json",
            "mask.png",
            "foreground.png",
            "foreground-plain.png",
        ] {
            let _ = fs::remove_file(staging.join(file));
        }
        let _ = fs::remove_dir(&staging);
    }
    result.map(|()| item)
}

fn worker(
    directory: &Path,
    desired: &AtomicU64,
    sender: &mpsc::SyncSender<Event>,
    updates: &mpsc::Receiver<Command>,
    loopback_only: bool,
    assets: &Path,
) {
    if sender.send(Event::Catalog(catalog(directory))).is_err() {
        return;
    }
    let update = |command| match command {
        Command::Wake => true,
        Command::Delete { id, selected } => {
            let result = delete_wallpaper(directory, &id, &selected);
            sender.send(Event::Deleted(id, result)).is_ok()
                && sender.send(Event::Catalog(catalog(directory))).is_ok()
        }
    };
    let mut handled = 0;
    loop {
        // Finish queued mutations before starting another upload session.
        for command in updates.try_iter() {
            if !update(command) {
                return;
            }
        }
        let generation = desired.load(Ordering::Acquire);
        if generation == STOP {
            break;
        }
        if generation == 0 || generation == handled {
            match updates.recv() {
                Ok(command) => {
                    if !update(command) {
                        break;
                    }
                }
                Err(_) => break,
            }
            continue;
        }
        handled = generation;
        let opened = (|| {
            let listener = listen(loopback_only)?;
            let token = random_token()?;
            let address = listener.local_addr().map_err(|e| e.to_string())?;
            let info = session(address, &token)?;
            Ok::<_, String>((listener, token, address, info))
        })();
        let (listener, token, address, info) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                let _ = sender.send(Event::Session(generation, Err(error)));
                continue;
            }
        };
        if sender.send(Event::Session(generation, Ok(info))).is_err() {
            break;
        }
        let deadline = Instant::now() + SESSION_TIME;
        let mut requests = 0;
        while desired.load(Ordering::Acquire) == generation
            && Instant::now() < deadline
            && requests < SESSION_REQUEST_LIMIT
        {
            match listener.accept() {
                Ok((mut stream, peer)) => {
                    if !local_peer(peer.ip()) {
                        continue;
                    }
                    requests += 1;
                    let valid = || {
                        desired.load(Ordering::Acquire) == generation && Instant::now() < deadline
                    };
                    match serve(
                        &mut stream,
                        &address.to_string(),
                        &token,
                        directory,
                        assets,
                        valid,
                    ) {
                        Ok(true) => {
                            if sender.send(Event::Catalog(catalog(directory))).is_err() {
                                return;
                            }
                        }
                        Ok(false) => {}
                        Err(error) => eprintln!("Luma wallpaper upload: {error}"),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25))
                }
                Err(error) => {
                    eprintln!("Luma wallpaper server: {error}");
                    break;
                }
            }
        }
        drop(listener);
        if sender.send(Event::Ended(generation)).is_err() {
            break;
        }
    }
}

fn request(
    reader: &mut impl Read,
    valid: &impl Fn() -> bool,
    deadline: Instant,
) -> Result<Request, String> {
    crate::companion::request(reader, valid, deadline, MAX_BODY, "Image exceeds 12 MiB")
}

fn upload_parts(body: &[u8]) -> Result<(&[u8], &[u8]), String> {
    if body.len() < 16 || &body[..8] != b"LUMAWP01" {
        return Err("Invalid masked wallpaper upload".into());
    }
    let photo = u32::from_be_bytes(body[8..12].try_into().unwrap()) as usize;
    let mask = u32::from_be_bytes(body[12..16].try_into().unwrap()) as usize;
    let end = 16usize.checked_add(photo).and_then(|n| n.checked_add(mask));
    if photo == 0 || mask == 0 || end != Some(body.len()) {
        return Err("Invalid masked wallpaper lengths".into());
    }
    Ok((&body[16..16 + photo], &body[16 + photo..]))
}

fn response(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> Result<(), String> {
    response_header(stream, status, kind, body.len(), BROWSER_POLICY)?;
    stream.write_all(body).map_err(|e| e.to_string())
}

fn serve(
    stream: &mut TcpStream,
    host: &str,
    token: &str,
    directory: &Path,
    assets: &Path,
    valid: impl Fn() -> bool,
) -> Result<bool, String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + REQUEST_TIME;
    let mut reader = BufReader::new(&mut *stream);
    let request = match request(&mut reader, &valid, deadline) {
        Ok(request) => request,
        Err(error) => {
            error_response(stream, "400 Bad Request", &error)?;
            return Ok(false);
        }
    };
    if request.foreign(host) {
        error_response(stream, "403 Forbidden", "Open the address shown on your TV")?;
        return Ok(false);
    }
    if !valid() {
        return Err("Upload session ended".into());
    }
    if request.method == "GET" && request.length == 0 && request.target == format!("/{token}") {
        redirect_to_page(stream, token)?;
        return Ok(false);
    }
    let Some(route) = request.target.strip_prefix(&format!("/{token}/")) else {
        error_response(stream, "404 Not Found", "Upload session not found")?;
        return Ok(false);
    };
    if request.method == "GET" && request.length == 0 {
        if let Some(name) = route.strip_prefix("vendor/") {
            let kind = match name {
                "ort.wasm.min.mjs" | "ort-wasm-simd-threaded.mjs" => {
                    "text/javascript; charset=utf-8"
                }
                "ort-wasm-simd-threaded.wasm" => "application/wasm",
                _ => {
                    error_response(stream, "404 Not Found", "Not found")?;
                    return Ok(false);
                }
            };
            let Some(bytes) = small_file(
                &assets.join("wallpaper-runtime").join(name),
                32 * 1024 * 1024,
            ) else {
                error_response(
                    stream,
                    "404 Not Found",
                    "The browser mask runtime is unavailable. Upload a mask PNG or update Luma.",
                )?;
                return Ok(false);
            };
            response_header(stream, "200 OK", kind, bytes.len(), BROWSER_POLICY)?;
            for chunk in bytes.chunks(64 * 1024) {
                if !valid() || Instant::now() >= deadline {
                    return Err("Upload session ended".into());
                }
                stream.write_all(chunk).map_err(|e| e.to_string())?;
            }
            return Ok(false);
        }
        if route == "info.json" {
            let body = serde_json::json!({"path": directory.to_string_lossy()}).to_string();
            response(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
            )?;
            return Ok(false);
        }
        let (kind, body): (&str, &[u8]) = match route {
            "" => (
                "text/html; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/index.html"),
            ),
            "app.css" => (
                "text/css; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/app.css"),
            ),
            "app.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/app.js"),
            ),
            "crop.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/crop.js"),
            ),
            "storage.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/storage.js"),
            ),
            "mask.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/mask.js"),
            ),
            "depth-worker.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/depth-worker.js"),
            ),
            "sha256.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/wallpaper-upload/sha256.js"),
            ),
            "luma-mark.svg" => ("image/svg+xml", include_bytes!("../assets/app-icon.svg")),
            _ => {
                error_response(stream, "404 Not Found", "Not found")?;
                return Ok(false);
            }
        };
        response(stream, "200 OK", kind, body)?;
        return Ok(false);
    }
    if request.method != "POST"
        || route != "upload"
        || request.headers.get("x-luma-upload").map(String::as_str) != Some(token)
    {
        error_response(stream, "403 Forbidden", "Invalid upload request")?;
        return Ok(false);
    }
    let masked = request.headers.get("content-type").map(String::as_str)
        == Some("application/x-luma-wallpaper");
    if !masked
        && !request.headers.get("content-type").is_some_and(|value| {
            matches!(value.as_str(), "image/jpeg" | "image/png" | "image/webp")
        })
    {
        error_response(
            stream,
            "415 Unsupported Media Type",
            "Choose a JPEG, PNG or WebP image",
        )?;
        return Ok(false);
    }
    let mut body = vec![0; request.length];
    read_exact(&mut reader, &mut body, &valid, deadline)?;
    drop(reader);
    let saved = if masked {
        upload_parts(&body)
            .and_then(|(photo, mask)| store_with_mask(directory, photo, Some(mask), &valid))
    } else {
        store(directory, &body, &valid)
    };
    match saved {
        Ok(item) => {
            // The commit succeeded even if the phone disconnects before receiving its reply.
            let _ = response(
                stream,
                "201 Created",
                "application/json; charset=utf-8",
                serde_json::json!({"id":item.id}).to_string().as_bytes(),
            );
            Ok(true)
        }
        Err(error) => {
            error_response(stream, "422 Unprocessable Content", &error)?;
            Ok(false)
        }
    }
}

#[cfg(test)]
#[path = "wallpapers-tests.rs"]
mod tests;
