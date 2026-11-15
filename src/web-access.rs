//! Web access: Luma's settings from a paired phone or computer, without the remote.
//!
//! A device pairs once, by scanning the QR code in the TV's pairing dialog or by typing the shown
//! address and PIN. It then keeps a device key in an HttpOnly cookie and can open the page at any
//! time while web access is on. The TV stores only a SHA-256 hash of each key and lists the paired
//! devices so any of them can be removed.
//!
//! Exposure is kept small: the server is off unless the owner turns web access on, binds only the
//! TV's private network address, answers only private peers, rejects any Host or Origin other than
//! its own address (DNS rebinding, cross-site requests) and needs a custom header on every write.
//! A PIN only exists while the pairing dialog is open and is replaced after a few wrong guesses.
use crate::companion::{
    Request, Session, error_response, local_address, local_peer, qr, read_exact, response,
    response_with,
};
use crate::web_settings::{Arrangement, Catalog, InputChange, Settings, listing, strings};
use serde::{Deserialize, Serialize};
use std::{
    io::BufReader,
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Fixed, so a bookmark keeps working; the address changes only if the TV's IP does.
pub const PORT: u16 = 8787;
const PIN_TIME: Duration = Duration::from_secs(300);
const PIN_ATTEMPTS: u32 = 5;
pub const DEVICE_LIMIT: usize = 16;
const MAX_BODY: usize = 4096;
const REQUEST_TIME: Duration = Duration::from_secs(10);
const ADDRESS_CHECK: Duration = Duration::from_secs(30);
const CONNECTION_LIMIT: usize = 8;
/// How often a waiting live preview asks the launcher whether its screen changed.
const PREVIEW_CHECK: Duration = Duration::from_millis(100);
/// A waiting live preview answers "unchanged" after this long, so connections recycle.
const PREVIEW_WAIT: Duration = Duration::from_secs(20);
const COOKIE: &str = "luma_device";
// Browsers cap cookie lifetimes at about 400 days; removing the device on the TV ends it sooner.
const COOKIE_AGE: u64 = 400 * 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// Hex SHA-256 of the device key; the key itself never touches the TV's storage.
    hash: String,
    /// Unix seconds.
    pub paired: u64,
}

#[derive(Clone, Debug)]
pub struct Pairing {
    /// The QR code opens the page with the PIN already filled in.
    pub session: Session,
    pub pin: String,
}

pub enum Event {
    Pairing(u64, Result<Pairing, String>),
    /// The complete editable settings after an accepted change.
    Settings(u64, Settings),
    Paired(String),
    Devices(Vec<Device>),
    /// The page asks for Luma's Home as it looks now. `known` is the fingerprint of the
    /// picture the server already has; an unchanged scene is answered without drawing.
    Preview {
        known: Option<u64>,
        reply: mpsc::Sender<Result<Frame, String>>,
    },
    /// One sheet of app tiles (see `view::app_sheet`), drawn unless `known` is still current.
    AppSheet {
        page: usize,
        known: Option<u64>,
        reply: mpsc::Sender<Result<Frame, String>>,
    },
    /// New favorites, their order and hidden apps.
    Arrange(Arrangement),
    /// One input's name, visibility and icon.
    Input(InputChange),
    /// The complete shortcut tile list after a change from the page.
    Tiles(Vec<crate::model::CustomTile>),
    /// A place from the page's search, or `None` to remove the weather location.
    WeatherLocation(Option<crate::weather::Location>),
}

/// A preview answer: the scene's fingerprint and, unless unchanged, its pixels.
pub struct Frame {
    pub key: u64,
    /// Whether Luma is what the TV shows; otherwise the picture is Luma's Home behind an app.
    pub on_screen: bool,
    pub image: Option<image::RgbaImage>,
}

struct Pin {
    code: String,
    attempts: u32,
    expires: Instant,
}

#[derive(Default)]
struct Shared {
    stop: bool,
    enabled: bool,
    /// Nonzero while the TV shows the pairing dialog; changes for every dialog opened.
    pairing: u64,
    pin: Option<Pin>,
    settings: Settings,
    revision: u64,
    acknowledged: u64,
    devices: Vec<Device>,
    catalog: Catalog,
    /// The last preview, encoded once and served until the scene changes.
    preview: Option<Picture>,
    /// An arrangement or input change from the page the launcher has not applied yet.
    arranging: bool,
    inputting: bool,
    locating: bool,
    tiling: bool,
    /// The page's last place search; the page chooses among these by position.
    places: Vec<crate::weather::Location>,
    sheets: std::collections::BTreeMap<usize, Picture>,
}

#[derive(Clone)]
struct Picture {
    key: u64,
    on_screen: bool,
    jpeg: Arc<Vec<u8>>,
    /// When the launcher last confirmed this is still the scene.
    checked: Instant,
}

pub struct Service {
    shared: Arc<Mutex<Shared>>,
    path: PathBuf,
    wake: mpsc::Sender<()>,
    pub receiver: mpsc::Receiver<Event>,
    enabled: bool,
    pairing: bool,
    generation: u64,
}

impl Service {
    /// `weather` looks up places for the page's weather location search.
    pub fn new(
        path: PathBuf,
        loopback_only: bool,
        weather: crate::weather_service::Client,
    ) -> Result<Self, String> {
        let weather = Arc::new(weather);
        let devices = load_devices(&path);
        let shared = Arc::new(Mutex::new(Shared {
            devices,
            ..Shared::default()
        }));
        let worker_shared = shared.clone();
        let worker_path = path.clone();
        let (sender, receiver) = mpsc::sync_channel(8);
        let (wake, updates) = mpsc::channel();
        thread::Builder::new()
            .name("luma-web-access".into())
            .spawn(move || {
                worker(
                    worker_shared,
                    worker_path,
                    sender,
                    weather,
                    &updates,
                    loopback_only,
                )
            })
            .map_err(|e| format!("Could not start web access worker: {e}"))?;
        Ok(Self {
            shared,
            path,
            wake,
            receiver,
            enabled: false,
            pairing: false,
            generation: 0,
        })
    }

    /// Called every frame: whether web access is on, whether the pairing dialog is showing,
    /// and the TV's current settings for pages that load next.
    pub fn sync(&mut self, enabled: bool, pairing: bool, settings: &Settings, catalog: &Catalog) {
        let pairing = enabled && pairing;
        let mut shared = self.shared.lock().unwrap();
        // A change from the page stays visible until the launcher has applied it.
        if shared.catalog.backgrounds != catalog.backgrounds {
            shared.catalog.backgrounds = catalog.backgrounds.clone();
        }
        if !shared.arranging && shared.catalog.library != catalog.library {
            shared.catalog.library = catalog.library.clone();
        }
        if !shared.inputting && shared.catalog.inputs != catalog.inputs {
            shared.catalog.inputs = catalog.inputs.clone();
        }
        if !shared.locating && shared.catalog.weather_location != catalog.weather_location {
            shared.catalog.weather_location = catalog.weather_location.clone();
        }
        if !shared.tiling && shared.catalog.tiles != catalog.tiles {
            shared.catalog.tiles = catalog.tiles.clone();
        }
        // A change from the phone stays authoritative until the launcher has applied it.
        if shared.revision == shared.acknowledged && shared.settings != *settings {
            shared.settings = settings.clone();
        }
        if enabled == self.enabled && pairing == self.pairing {
            return;
        }
        self.enabled = enabled;
        shared.enabled = enabled;
        if pairing != self.pairing {
            self.pairing = pairing;
            if pairing {
                self.generation += 1;
                shared.pairing = self.generation;
            } else {
                shared.pairing = 0;
                shared.pin = None;
            }
        }
        drop(shared);
        let _ = self.wake.send(());
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn acknowledge(&self, revision: u64) {
        let mut shared = self.shared.lock().unwrap();
        shared.acknowledged = shared.acknowledged.max(revision.min(shared.revision));
    }

    /// The launcher applied the page's arrangement; its own state is authoritative again.
    pub fn arranged(&self) {
        self.shared.lock().unwrap().arranging = false;
    }

    pub fn inputs_applied(&self) {
        self.shared.lock().unwrap().inputting = false;
    }

    pub fn location_applied(&self) {
        self.shared.lock().unwrap().locating = false;
    }

    pub fn tiles_applied(&self) {
        self.shared.lock().unwrap().tiling = false;
    }

    pub fn devices(&self) -> Vec<Device> {
        self.shared.lock().unwrap().devices.clone()
    }

    /// Removes a paired device; its key stops working immediately.
    pub fn forget(&self, id: &str) -> Option<Device> {
        let mut shared = self.shared.lock().unwrap();
        let index = shared.devices.iter().position(|device| device.id == id)?;
        let device = shared.devices.remove(index);
        if let Err(error) = save_devices(&self.path, &shared.devices) {
            eprintln!("Luma web access: {error}");
        }
        Some(device)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.shared.lock().unwrap().stop = true;
        let _ = self.wake.send(());
    }
}

fn load_devices(path: &Path) -> Vec<Device> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_devices(path: &Path, devices: &[Device]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(devices).map_err(|e| e.to_string())?;
    crate::storage::write_atomic(path, &bytes)
}

fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| format!("Secure random source unavailable: {e}"))?;
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Compares secrets without stopping at the first difference; lengths are not secret.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn key_hash(key: &str) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, key.as_bytes()).as_ref())
}

/// Six digits, uniformly drawn.
fn new_pin() -> Result<String, String> {
    loop {
        let value = u32::from_le_bytes(random_bytes::<4>()?);
        // Reject the top remainder to avoid modulo bias.
        if value < u32::MAX - u32::MAX % 1_000_000 {
            return Ok(format!("{:06}", value % 1_000_000));
        }
    }
}

fn pairing_for(address: SocketAddr, pin: &str) -> Result<Pairing, String> {
    // The fragment never leaves the phone's browser, so the PIN is not sent in the request line.
    let session = qr(format!("http://{address}/#pin={pin}"))?;
    Ok(Pairing {
        session,
        pin: pin.to_owned(),
    })
}

fn issue_pin(shared: &mut Shared) -> Result<String, String> {
    let code = new_pin()?;
    shared.pin = Some(Pin {
        code: code.clone(),
        attempts: 0,
        expires: Instant::now() + PIN_TIME,
    });
    Ok(code)
}

fn bind(ip: std::net::Ipv4Addr) -> Result<(TcpListener, SocketAddr), String> {
    let listener = TcpListener::bind((ip, PORT))
        .map_err(|e| format!("Port {PORT} on the TV is unavailable: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    Ok((listener, address))
}

fn worker(
    shared: Arc<Mutex<Shared>>,
    path: PathBuf,
    sender: mpsc::SyncSender<Event>,
    weather: Arc<crate::weather_service::Client>,
    wake: &mpsc::Receiver<()>,
    loopback_only: bool,
) {
    let mut listener: Option<(TcpListener, SocketAddr)> = None;
    let mut bind_error: Option<String> = None;
    let mut checked: Option<Instant> = None;
    let mut handled_pairing = 0;
    let mut busy_until = Instant::now();
    let active = Arc::new(AtomicUsize::new(0));
    loop {
        let (stop, enabled, pairing) = {
            let shared = shared.lock().unwrap();
            (shared.stop, shared.enabled, shared.pairing)
        };
        if stop {
            break;
        }
        if !enabled {
            listener = None;
            checked = None;
            handled_pairing = 0;
            if wake.recv().is_err() {
                break;
            }
            continue;
        }
        // Follow the TV to a new address (DHCP renewal, Wi-Fi to Ethernet).
        if checked.is_none_or(|at| at.elapsed() >= ADDRESS_CHECK) {
            checked = Some(Instant::now());
            match local_address(loopback_only) {
                Ok(ip)
                    if listener
                        .as_ref()
                        .is_none_or(|(_, address)| address.ip() != IpAddr::V4(ip)) =>
                {
                    listener = None;
                    match bind(ip) {
                        Ok(bound) => {
                            listener = Some(bound);
                            bind_error = None;
                        }
                        Err(error) => {
                            eprintln!("Luma web access: {error}");
                            bind_error = Some(error);
                        }
                    }
                    // A dialog that is open shows the new address.
                    handled_pairing = 0;
                }
                Ok(_) => {}
                Err(error) => {
                    listener = None;
                    bind_error = Some(error);
                    handled_pairing = 0;
                }
            }
        }
        if pairing == 0 {
            handled_pairing = 0;
        } else if pairing != handled_pairing {
            handled_pairing = pairing;
            let result = match &listener {
                Some((_, address)) => {
                    let mut current = shared.lock().unwrap();
                    issue_pin(&mut current).and_then(|pin| pairing_for(*address, &pin))
                }
                None => Err(bind_error
                    .clone()
                    .unwrap_or_else(|| "Web access is unavailable".into())),
            };
            if sender.send(Event::Pairing(pairing, result)).is_err() {
                break;
            }
        }
        let Some((server, address)) = &listener else {
            match wake.recv_timeout(Duration::from_secs(5)) {
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => checked = None,
            }
            continue;
        };
        match server.accept() {
            Ok((mut stream, peer)) => {
                if !local_peer(peer.ip()) {
                    continue;
                }
                busy_until = Instant::now() + Duration::from_secs(5);
                // A waiting live preview must not hold up saving, so each connection gets its
                // own thread, a few at a time.
                if active.load(Ordering::Acquire) >= CONNECTION_LIMIT {
                    let _ = error_response(&mut stream, "503 Service Unavailable", "Busy");
                    continue;
                }
                active.fetch_add(1, Ordering::AcqRel);
                let context = Context {
                    shared: shared.clone(),
                    path: path.clone(),
                    sender: sender.clone(),
                    weather: weather.clone(),
                    address: *address,
                };
                let finished = active.clone();
                let spawned = thread::Builder::new()
                    .name("luma-web-request".into())
                    .spawn(move || {
                        if let Err(error) = serve(&mut stream, &context) {
                            eprintln!("Luma web access: {error}");
                        }
                        finished.fetch_sub(1, Ordering::AcqRel);
                    });
                if spawned.is_err() {
                    active.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                // Poll quickly while a page is talking to the TV, rarely when idle.
                let pause = if Instant::now() < busy_until {
                    Duration::from_millis(25)
                } else {
                    Duration::from_millis(250)
                };
                if let Err(mpsc::RecvTimeoutError::Disconnected) = wake.recv_timeout(pause) {
                    break;
                }
            }
            Err(error) => {
                eprintln!("Luma web access server: {error}");
                listener = None;
                checked = None;
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

struct Context {
    shared: Arc<Mutex<Shared>>,
    path: PathBuf,
    sender: mpsc::SyncSender<Event>,
    weather: Arc<crate::weather_service::Client>,
    address: SocketAddr,
}

/// The bundled input icons, by route.
fn input_icon(route: &str) -> Option<&'static [u8]> {
    macro_rules! icons {
        ($($name:literal),*) => {
            match route {
                $(concat!("/icon/input-", $name, ".svg") => {
                    Some(include_bytes!(concat!("../assets/ui-icons/input-", $name, ".svg")))
                })*
                _ => None,
            }
        };
    }
    icons!(
        "hdmi",
        "usb",
        "apple-tv",
        "computer",
        "console",
        "playstation",
        "shield",
        "streamer",
        "switch",
        "switch2",
        "sony",
        "dvd",
        "blu-ray",
        "disk",
        "xbox"
    )
}

fn json(stream: &mut TcpStream, status: &str, body: &serde_json::Value) -> Result<(), String> {
    response(
        stream,
        status,
        "application/json; charset=utf-8",
        body.to_string().as_bytes(),
    )
}

/// The paired device a request's cookie belongs to.
fn device<'a>(request: &Request, devices: &'a [Device]) -> Option<&'a Device> {
    let value = request
        .headers
        .get("cookie")?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)?
        .1;
    let (id, key) = value.split_once('.')?;
    let device = devices.iter().find(|device| device.id == id)?;
    same(key_hash(key).as_bytes(), device.hash.as_bytes()).then_some(device)
}

fn serve(stream: &mut TcpStream, context: &Context) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + REQUEST_TIME;
    let valid = || true;
    let mut reader = BufReader::new(&mut *stream);
    let request = match crate::companion::request(
        &mut reader,
        &valid,
        deadline,
        MAX_BODY,
        "Request too large",
    ) {
        Ok(request) => request,
        Err(error) => {
            drop(reader);
            return error_response(stream, "400 Bad Request", &error);
        }
    };
    // Only the TV's own address: a rebinding domain or another site is turned away.
    if request.foreign(&context.address.to_string()) {
        drop(reader);
        return error_response(stream, "403 Forbidden", "Open the address shown on your TV");
    }
    let route = request
        .target
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_owned();
    if request.method == "GET" && request.length == 0 {
        drop(reader);
        if route == "/api/strings" {
            // The pairing page speaks the TV's language before the device is trusted.
            let language = context.shared.lock().unwrap().settings.language;
            return json(stream, "200 OK", &strings(language));
        }
        if let Some(api) = route.strip_prefix("/api/") {
            let shared = context.shared.lock().unwrap();
            let Some(device) = device(&request, &shared.devices).cloned() else {
                drop(shared);
                return json(
                    stream,
                    "401 Unauthorized",
                    &serde_json::json!({ "error": "Pair this device first.", "pair": true }),
                );
            };
            if api == "settings" {
                let mut body = listing(&shared.settings, &shared.catalog);
                body["device"] = serde_json::json!(device.name);
                drop(shared);
                return json(stream, "200 OK", &body);
            }
            if let Some(id) = api.strip_prefix("thumbnail/") {
                // Only files from the catalog; the id is never used as a path.
                let path = shared
                    .catalog
                    .backgrounds
                    .iter()
                    .find(|background| background.id == id)
                    .map(|background| background.thumbnail.clone());
                drop(shared);
                return match path.and_then(|path| std::fs::read(path).ok()) {
                    Some(bytes) => response(stream, "200 OK", "image/jpeg", &bytes),
                    None => error_response(stream, "404 Not Found", "Not found"),
                };
            }
            drop(shared);
            if api == "apps.jpg" {
                let query = request
                    .target
                    .split_once('?')
                    .map_or("", |(_, query)| query);
                let value = |name: &str| {
                    query
                        .split('&')
                        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
                };
                let page = value("page")
                    .and_then(|page| page.parse().ok())
                    .unwrap_or(0);
                let known = value("known").and_then(|key| key.parse().ok());
                return sheet(stream, context, page, known);
            }
            if api == "preview.jpg" {
                let query = request
                    .target
                    .split_once('?')
                    .map_or("", |(_, query)| query);
                let value = |name: &str| {
                    query
                        .split('&')
                        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
                };
                let known = value("known").and_then(|key| key.parse().ok());
                return preview(stream, context, known, value("wait") == Some("1"));
            }
            return error_response(stream, "404 Not Found", "Not found");
        }
        let (kind, body): (&str, &[u8]) = match route.as_str() {
            "/" => (
                "text/html; charset=utf-8",
                include_bytes!("../assets/web-access/index.html"),
            ),
            "/web.css" => (
                "text/css; charset=utf-8",
                include_bytes!("../assets/web-access/web.css"),
            ),
            "/inter.woff2" => (
                "font/woff2",
                include_bytes!("../assets/fonts/inter-latin.woff2"),
            ),
            "/app.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/web-access/app.js"),
            ),
            "/luma-mark.svg" => ("image/svg+xml", include_bytes!("../assets/app-icon.svg")),
            // Luma's own interface icons for the page's buttons.
            "/icon/heart.svg" => (
                "image/svg+xml",
                include_bytes!("../assets/ui-icons/heart.svg"),
            ),
            "/icon/heart-filled.svg" => (
                "image/svg+xml",
                include_bytes!("../assets/ui-icons/heart-filled.svg"),
            ),
            "/icon/eye.svg" => (
                "image/svg+xml",
                include_bytes!("../assets/ui-icons/eye.svg"),
            ),
            "/icon/eye-off.svg" => (
                "image/svg+xml",
                include_bytes!("../assets/ui-icons/eye-off.svg"),
            ),
            "/icon/left.svg" => (
                "image/svg+xml",
                include_bytes!("../assets/ui-icons/left.svg"),
            ),
            route if route.starts_with("/icon/tile-") => {
                let icon = route
                    .strip_prefix("/icon/tile-")
                    .and_then(|icon| icon.strip_suffix(".svg"))
                    .unwrap_or_default();
                match crate::tile_editor::icon_svg(icon) {
                    "" => return error_response(stream, "404 Not Found", "Not found"),
                    svg => ("image/svg+xml", svg.as_bytes()),
                }
            }
            route if route.starts_with("/icon/input-") => match input_icon(route) {
                Some(svg) => ("image/svg+xml", svg),
                None => return error_response(stream, "404 Not Found", "Not found"),
            },
            _ => return error_response(stream, "404 Not Found", "Not found"),
        };
        return response(stream, "200 OK", kind, body);
    }
    // A plain form or image request from another page cannot set this header.
    if request.method != "POST"
        || request.headers.get("x-luma-request").map(String::as_str) != Some("1")
    {
        drop(reader);
        return error_response(stream, "403 Forbidden", "Invalid request");
    }
    if request
        .headers
        .get("content-type")
        .is_none_or(|value| value.split(';').next() != Some("application/json"))
    {
        drop(reader);
        return error_response(stream, "415 Unsupported Media Type", "Send JSON");
    }
    let mut body = vec![0; request.length];
    read_exact(&mut reader, &mut body, &valid, deadline)?;
    drop(reader);
    match route.as_str() {
        "/api/pair" => pair(stream, context, &body),
        "/api/settings" => save(stream, context, &request, &body),
        "/api/apps" => arrange(stream, context, &request, &body),
        "/api/inputs" => change_input(stream, context, &request, &body),
        "/api/places" => search_places(stream, context, &request, &body),
        "/api/location" => choose_place(stream, context, &request, &body),
        "/api/tiles/save" | "/api/tiles/remove" | "/api/tiles/send" => {
            edit_tile(stream, context, &request, &route, &body)
        }
        "/api/forget" => forget(stream, context, &request),
        _ => error_response(stream, "404 Not Found", "Not found"),
    }
}

/// The launcher's current picture, asking it at most every `PREVIEW_CHECK` however many pages
/// wait. An unchanged scene is confirmed without drawing; a changed one is encoded here, off the
/// launcher's thread.
fn picture(context: &Context) -> Result<Picture, String> {
    let cached = context.shared.lock().unwrap().preview.clone();
    if let Some(picture) = cached
        .as_ref()
        .filter(|p| p.checked.elapsed() < PREVIEW_CHECK)
    {
        return Ok(picture.clone());
    }
    let (reply, answer) = mpsc::channel();
    let request = Event::Preview {
        known: cached.as_ref().map(|picture| picture.key),
        reply,
    };
    context
        .sender
        .send(request)
        .map_err(|_| "Launcher stopped".to_owned())?;
    let frame = answer
        .recv_timeout(Duration::from_secs(6))
        .map_err(|_| "The TV is busy".to_owned())??;
    let picture = match (frame.image, cached) {
        (Some(image), _) => {
            let started = Instant::now();
            let jpeg = Arc::new(encode(&image)?);
            eprintln!(
                "Luma web preview: encoded {} KiB in {} ms",
                jpeg.len() / 1024,
                started.elapsed().as_millis()
            );
            Picture {
                key: frame.key,
                on_screen: frame.on_screen,
                jpeg,
                checked: Instant::now(),
            }
        }
        (None, Some(picture)) if picture.key == frame.key => Picture {
            checked: Instant::now(),
            ..picture
        },
        (None, _) => return Err("Try again".into()),
    };
    context.shared.lock().unwrap().preview = Some(picture.clone());
    Ok(picture)
}

/// One sheet of app tiles, cached until the launcher's tiles change (icons load in the
/// background, so the page asks again a few times with the fingerprint it has).
fn sheet(
    stream: &mut TcpStream,
    context: &Context,
    page: usize,
    shown: Option<u64>,
) -> Result<(), String> {
    let cached = context.shared.lock().unwrap().sheets.get(&page).cloned();
    let (reply, answer) = mpsc::channel();
    let request = Event::AppSheet {
        page,
        known: cached.as_ref().map(|picture| picture.key),
        reply,
    };
    if context.sender.send(request).is_err() {
        return error_response(stream, "503 Service Unavailable", "Launcher stopped");
    }
    let frame = match answer.recv_timeout(Duration::from_secs(6)) {
        Ok(Ok(frame)) => frame,
        Ok(Err(error)) => return error_response(stream, "503 Service Unavailable", &error),
        Err(_) => return error_response(stream, "503 Service Unavailable", "The TV is busy"),
    };
    let picture = match (frame.image, cached) {
        (Some(image), _) => Picture {
            key: frame.key,
            on_screen: true,
            jpeg: Arc::new(encode(&image)?),
            checked: Instant::now(),
        },
        (None, Some(picture)) if picture.key == frame.key => picture,
        (None, _) => return error_response(stream, "503 Service Unavailable", "Try again"),
    };
    context
        .shared
        .lock()
        .unwrap()
        .sheets
        .insert(page, picture.clone());
    if shown == Some(picture.key) {
        return response_with(stream, "204 No Content", "image/jpeg", "", b"");
    }
    response_with(
        stream,
        "200 OK",
        "image/jpeg",
        &format!("X-Luma-Frame: {}\r\n", picture.key),
        &picture.jpeg,
    )
}

fn arrange(
    stream: &mut TcpStream,
    context: &Context,
    request: &Request,
    body: &[u8],
) -> Result<(), String> {
    let mut shared = context.shared.lock().unwrap();
    let Some(name) = device(request, &shared.devices).map(|device| device.name.clone()) else {
        drop(shared);
        return json(
            stream,
            "401 Unauthorized",
            &serde_json::json!({ "error": "This device was removed on the TV. Pair it again.", "pair": true }),
        );
    };
    let Ok(mut next) = serde_json::from_slice::<Arrangement>(body) else {
        drop(shared);
        return error_response(stream, "422 Unprocessable Content", "Reload the page.");
    };
    // Show the page what the launcher will keep: known apps, favorites never hidden.
    let known = |id: &String| shared.catalog.library.apps.iter().any(|app| &app.id == id);
    next.favorites.retain(known);
    next.hidden
        .retain(|id| known(id) && !next.favorites.contains(id));
    shared.catalog.library.favorites = next.favorites.clone();
    shared.catalog.library.hidden = next.hidden.clone();
    shared.arranging = true;
    let mut reply = listing(&shared.settings, &shared.catalog);
    drop(shared);
    reply["device"] = serde_json::json!(name);
    context
        .sender
        .send(Event::Arrange(next))
        .map_err(|_| "Launcher stopped".to_owned())?;
    json(stream, "200 OK", &reply)
}

fn change_input(
    stream: &mut TcpStream,
    context: &Context,
    request: &Request,
    body: &[u8],
) -> Result<(), String> {
    let mut shared = context.shared.lock().unwrap();
    let Some(name) = device(request, &shared.devices).map(|device| device.name.clone()) else {
        drop(shared);
        return json(
            stream,
            "401 Unauthorized",
            &serde_json::json!({ "error": "This device was removed on the TV. Pair it again.", "pair": true }),
        );
    };
    let Ok(mut change) = serde_json::from_slice::<InputChange>(body) else {
        drop(shared);
        return error_response(stream, "422 Unprocessable Content", "Reload the page.");
    };
    change.name = crate::model::clean_input_name(&change.name);
    let known_icon = change.icon.is_empty()
        || crate::model::INPUT_ICONS
            .iter()
            .any(|(icon, _)| *icon == change.icon);
    let Some(input) = shared
        .catalog
        .inputs
        .iter_mut()
        .find(|input| input.id == change.id)
        .filter(|_| known_icon)
    else {
        drop(shared);
        return error_response(stream, "422 Unprocessable Content", "Reload the page.");
    };
    input.name = change.name.clone();
    input.mode = change.mode;
    input.icon = change.icon.clone();
    shared.inputting = true;
    let mut reply = listing(&shared.settings, &shared.catalog);
    drop(shared);
    reply["device"] = serde_json::json!(name);
    context
        .sender
        .send(Event::Input(change))
        .map_err(|_| "Launcher stopped".to_owned())?;
    json(stream, "200 OK", &reply)
}

fn unpaired(stream: &mut TcpStream) -> Result<(), String> {
    json(
        stream,
        "401 Unauthorized",
        &serde_json::json!({ "error": "This device was removed on the TV. Pair it again.", "pair": true }),
    )
}

/// Adds, changes, removes or test-sends a shortcut tile, with the TV tile editor's rules.
fn edit_tile(
    stream: &mut TcpStream,
    context: &Context,
    request: &Request,
    route: &str,
    body: &[u8],
) -> Result<(), String> {
    let mut shared = context.shared.lock().unwrap();
    let Some(name) = device(request, &shared.devices).map(|device| device.name.clone()) else {
        drop(shared);
        return unpaired(stream);
    };
    let Ok(edit) = serde_json::from_slice::<crate::tile_editor::Edit>(body) else {
        drop(shared);
        return error_response(stream, "422 Unprocessable Content", "Reload the page.");
    };
    if route == "/api/tiles/send" {
        drop(shared);
        // Sent from the TV, so the test takes the same network path as a tile press.
        let result = edit
            .tile()
            .and_then(|tile| crate::webhook::send(&crate::webhook::agent(), &tile));
        return match result {
            Ok(()) => json(stream, "200 OK", &serde_json::json!({})),
            Err(error) => error_response(stream, "502 Bad Gateway", &error),
        };
    }
    let mut tiles = shared.catalog.tiles.clone();
    let result = if route == "/api/tiles/save" {
        crate::tile_editor::save(&mut tiles, &edit).map(Some)
    } else {
        crate::tile_editor::remove(&mut tiles, &edit.id).map(|()| None)
    };
    let id = match result {
        Ok(id) => id,
        Err(error) => {
            drop(shared);
            return error_response(stream, "422 Unprocessable Content", &error);
        }
    };
    shared.catalog.tiles = tiles.clone();
    shared.tiling = true;
    let mut reply = listing(&shared.settings, &shared.catalog);
    drop(shared);
    reply["device"] = serde_json::json!(name);
    reply["id"] = serde_json::json!(id);
    context
        .sender
        .send(Event::Tiles(tiles))
        .map_err(|_| "Launcher stopped".to_owned())?;
    json(stream, "200 OK", &reply)
}

/// Looks up places for the weather location, on this connection's thread.
fn search_places(
    stream: &mut TcpStream,
    context: &Context,
    request: &Request,
    body: &[u8],
) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Search {
        query: String,
    }
    let language = {
        let shared = context.shared.lock().unwrap();
        if device(request, &shared.devices).is_none() {
            drop(shared);
            return unpaired(stream);
        }
        shared.settings.language
    };
    let Ok(search) = serde_json::from_slice::<Search>(body) else {
        return error_response(stream, "422 Unprocessable Content", "Reload the page.");
    };
    if search.query.trim().chars().count() < 2 {
        return error_response(stream, "422 Unprocessable Content", language.t("enterCity"));
    }
    match context.weather.search(&search.query, language) {
        Ok(places) => {
            let body = serde_json::json!({ "places": places });
            context.shared.lock().unwrap().places = places;
            json(stream, "200 OK", &body)
        }
        Err(error) => {
            eprintln!("Luma web access: place search failed: {error}");
            error_response(stream, "502 Bad Gateway", language.t("searchUnavailable"))
        }
    }
}

/// Sets the weather location to one of the last search's places, or removes it.
fn choose_place(
    stream: &mut TcpStream,
    context: &Context,
    request: &Request,
    body: &[u8],
) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Choice {
        index: Option<usize>,
    }
    let mut shared = context.shared.lock().unwrap();
    let Some(name) = device(request, &shared.devices).map(|device| device.name.clone()) else {
        drop(shared);
        return unpaired(stream);
    };
    let Ok(choice) = serde_json::from_slice::<Choice>(body) else {
        drop(shared);
        return error_response(stream, "422 Unprocessable Content", "Reload the page.");
    };
    let place = match choice.index {
        Some(index) => match shared.places.get(index) {
            Some(place) => Some(place.clone()),
            None => {
                drop(shared);
                return error_response(stream, "422 Unprocessable Content", "Search again.");
            }
        },
        None => None,
    };
    shared.catalog.weather_location = place.clone();
    shared.locating = true;
    let mut reply = listing(&shared.settings, &shared.catalog);
    drop(shared);
    reply["device"] = serde_json::json!(name);
    context
        .sender
        .send(Event::WeatherLocation(place))
        .map_err(|_| "Launcher stopped".to_owned())?;
    json(stream, "200 OK", &reply)
}

/// Luma's screen for the page. The page passes the fingerprint it shows; with `wait` the request
/// is held until the screen changes (long polling), so changes arrive as soon as they are drawn.
fn preview(
    stream: &mut TcpStream,
    context: &Context,
    shown: Option<u64>,
    wait: bool,
) -> Result<(), String> {
    let deadline = Instant::now() + PREVIEW_WAIT;
    let picture = loop {
        let picture = match picture(context) {
            Ok(picture) => picture,
            Err(error) => return error_response(stream, "503 Service Unavailable", &error),
        };
        let enabled = {
            let shared = context.shared.lock().unwrap();
            shared.enabled && !shared.stop
        };
        if Some(picture.key) != shown {
            break picture;
        }
        if !wait || !enabled || Instant::now() >= deadline {
            return response_with(stream, "204 No Content", "image/jpeg", "", b"");
        }
        thread::sleep(PREVIEW_CHECK);
    };
    response_with(
        stream,
        "200 OK",
        "image/jpeg",
        &format!(
            "X-Luma-Frame: {}\r\nX-Luma-On-Screen: {}\r\n",
            picture.key, picture.on_screen
        ),
        &picture.jpeg,
    )
}

fn encode(image: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let (width, height) = image.dimensions();
    let mut jpeg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpeg, 75)
        .encode(
            image.as_raw(),
            u16::try_from(width).map_err(|e| e.to_string())?,
            u16::try_from(height).map_err(|e| e.to_string())?,
            jpeg_encoder::ColorType::Rgba,
        )
        .map_err(|e| e.to_string())?;
    Ok(jpeg)
}

#[derive(Deserialize)]
struct PairRequest {
    pin: String,
    #[serde(default)]
    name: String,
}

fn pair(stream: &mut TcpStream, context: &Context, body: &[u8]) -> Result<(), String> {
    let Ok(request) = serde_json::from_slice::<PairRequest>(body) else {
        return error_response(stream, "400 Bad Request", "Invalid request");
    };
    let guess: String = request.pin.chars().filter(char::is_ascii_digit).collect();
    let mut shared = context.shared.lock().unwrap();
    let pairing = shared.pairing;
    let Some(pin) = shared.pin.as_mut().filter(|_| pairing != 0) else {
        drop(shared);
        return error_response(
            stream,
            "409 Conflict",
            "On your TV, open Settings › System › Phone & computer › Pair a device.",
        );
    };
    if Instant::now() >= pin.expires {
        let replaced = issue_pin(&mut shared)
            .and_then(|pin| pairing_for(context.address, &pin))
            .map_err(|error| error.to_string());
        drop(shared);
        let _ = context.sender.send(Event::Pairing(pairing, replaced));
        return error_response(
            stream,
            "409 Conflict",
            "This PIN expired. Enter the new PIN shown on your TV.",
        );
    }
    if !same(pin.code.as_bytes(), guess.as_bytes()) {
        pin.attempts += 1;
        if pin.attempts < PIN_ATTEMPTS {
            drop(shared);
            return error_response(
                stream,
                "403 Forbidden",
                "Wrong PIN. Check the PIN on your TV.",
            );
        }
        let replaced = issue_pin(&mut shared).and_then(|pin| pairing_for(context.address, &pin));
        drop(shared);
        let _ = context.sender.send(Event::Pairing(pairing, replaced));
        return error_response(
            stream,
            "403 Forbidden",
            "Too many wrong PINs. Your TV now shows a new PIN.",
        );
    }
    if shared.devices.len() >= DEVICE_LIMIT {
        drop(shared);
        return error_response(
            stream,
            "409 Conflict",
            "Too many devices are paired. Remove one on your TV first.",
        );
    }
    let id = crate::companion::random_token()?;
    let key = hex(&random_bytes::<32>()?);
    let name = crate::model::clean_input_name(&request.name);
    let device = Device {
        id: id.clone(),
        name: if name.is_empty() {
            "Browser".into()
        } else {
            name
        },
        hash: key_hash(&key),
        paired: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs()),
    };
    shared.devices.push(device.clone());
    // A PIN pairs one device; the TV closes the dialog.
    shared.pin = None;
    let saved = save_devices(&context.path, &shared.devices);
    let devices = shared.devices.clone();
    drop(shared);
    if let Err(error) = saved {
        eprintln!("Luma web access: {error}");
    }
    let _ = context.sender.send(Event::Devices(devices));
    let _ = context.sender.send(Event::Paired(device.name.clone()));
    response_with(
        stream,
        "200 OK",
        "application/json; charset=utf-8",
        &format!(
            "Set-Cookie: {COOKIE}={id}.{key}; Path=/; Max-Age={COOKIE_AGE}; HttpOnly; SameSite=Strict\r\n"
        ),
        serde_json::json!({ "device": device.name })
            .to_string()
            .as_bytes(),
    )
}

fn save(
    stream: &mut TcpStream,
    context: &Context,
    request: &Request,
    body: &[u8],
) -> Result<(), String> {
    let mut shared = context.shared.lock().unwrap();
    let Some(name) = device(request, &shared.devices).map(|device| device.name.clone()) else {
        drop(shared);
        return json(
            stream,
            "401 Unauthorized",
            &serde_json::json!({ "error": "This device was removed on the TV. Pair it again.", "pair": true }),
        );
    };
    let Ok(next) = serde_json::from_slice::<Settings>(body).map(Settings::cleaned) else {
        drop(shared);
        return error_response(
            stream,
            "422 Unprocessable Content",
            "These settings are not valid. Reload the page.",
        );
    };
    let event = (next != shared.settings).then(|| {
        shared.settings = next.clone();
        shared.revision += 1;
        Event::Settings(shared.revision, next)
    });
    let mut reply = listing(&shared.settings, &shared.catalog);
    drop(shared);
    reply["device"] = serde_json::json!(name);
    // Queue the change before replying; it applies even if the phone misses the reply.
    if let Some(event) = event {
        context
            .sender
            .send(event)
            .map_err(|_| "Launcher stopped".to_owned())?;
    }
    json(stream, "200 OK", &reply)
}

fn forget(stream: &mut TcpStream, context: &Context, request: &Request) -> Result<(), String> {
    let mut shared = context.shared.lock().unwrap();
    let Some(id) = device(request, &shared.devices).map(|device| device.id.clone()) else {
        drop(shared);
        return error_response(stream, "401 Unauthorized", "This device is not paired.");
    };
    shared.devices.retain(|device| device.id != id);
    let saved = save_devices(&context.path, &shared.devices);
    let devices = shared.devices.clone();
    drop(shared);
    if let Err(error) = saved {
        eprintln!("Luma web access: {error}");
    }
    let _ = context.sender.send(Event::Devices(devices));
    response_with(
        stream,
        "200 OK",
        "application/json; charset=utf-8",
        &format!("Set-Cookie: {COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict\r\n"),
        b"{}",
    )
}

#[cfg(test)]
#[path = "web-access-tests.rs"]
mod tests;
