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
use crate::web_settings::{Settings, listing};
use serde::{Deserialize, Serialize};
use std::{
    io::BufReader,
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
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
    pub fn new(path: PathBuf, loopback_only: bool) -> Result<Self, String> {
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
                    &worker_shared,
                    &worker_path,
                    &sender,
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
    pub fn sync(&mut self, enabled: bool, pairing: bool, settings: &Settings) {
        let pairing = enabled && pairing;
        let mut shared = self.shared.lock().unwrap();
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
    shared: &Mutex<Shared>,
    path: &Path,
    sender: &mpsc::SyncSender<Event>,
    wake: &mpsc::Receiver<()>,
    loopback_only: bool,
) {
    let mut listener: Option<(TcpListener, SocketAddr)> = None;
    let mut bind_error: Option<String> = None;
    let mut checked: Option<Instant> = None;
    let mut handled_pairing = 0;
    let mut busy_until = Instant::now();
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
                let context = Context {
                    shared,
                    path,
                    sender,
                    address: *address,
                };
                if let Err(error) = serve(&mut stream, &context) {
                    eprintln!("Luma web access: {error}");
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

struct Context<'a> {
    shared: &'a Mutex<Shared>,
    path: &'a Path,
    sender: &'a mpsc::SyncSender<Event>,
    address: SocketAddr,
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
        if route == "/api/settings" {
            let shared = context.shared.lock().unwrap();
            let Some(device) = device(&request, &shared.devices) else {
                drop(shared);
                return json(
                    stream,
                    "401 Unauthorized",
                    &serde_json::json!({ "error": "Pair this device first.", "pair": true }),
                );
            };
            let mut body = listing(&shared.settings);
            body["device"] = serde_json::json!(device.name);
            drop(shared);
            return json(stream, "200 OK", &body);
        }
        let (kind, body): (&str, &[u8]) = match route.as_str() {
            "/" => (
                "text/html; charset=utf-8",
                include_bytes!("../assets/web-access/index.html"),
            ),
            // The tile editor's sheet carries the shared companion look.
            "/app.css" => (
                "text/css; charset=utf-8",
                include_bytes!("../assets/tile-editor/app.css"),
            ),
            "/web.css" => (
                "text/css; charset=utf-8",
                include_bytes!("../assets/web-access/web.css"),
            ),
            "/app.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/web-access/app.js"),
            ),
            "/luma-mark.svg" => ("image/svg+xml", include_bytes!("../assets/app-icon.svg")),
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
        "/api/forget" => forget(stream, context, &request),
        _ => error_response(stream, "404 Not Found", "Not found"),
    }
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
    let saved = save_devices(context.path, &shared.devices);
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
    let mut reply = listing(&shared.settings);
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
    let saved = save_devices(context.path, &shared.devices);
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
