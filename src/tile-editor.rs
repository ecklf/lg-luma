//! Shortcut tiles edited from the owner's phone or computer instead of the TV keyboard. While the
//! TV shows the QR dialog this worker serves a small page and owns the tile list; every accepted
//! change goes back to the launcher, which saves it like an edit made in Settings.
use crate::companion::{
    Request, Session, error_response, listen, local_peer, random_token, read_exact,
    redirect_to_page, response, session,
};
use crate::model::{
    CustomTile, TILE_ICONS, TILE_LIMIT, TileMethod, clean_input_name, new_tile_id, valid_tile_url,
};
use serde::Deserialize;
use std::{
    io::BufReader,
    net::TcpStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_BODY: usize = 4096;
const SESSION_TIME: Duration = Duration::from_secs(600);
// A page load is a handful of requests; editing a dozen tiles stays well inside this.
const SESSION_REQUEST_LIMIT: usize = 256;
const REQUEST_TIME: Duration = Duration::from_secs(20);
const STOP: u64 = u64::MAX;

pub enum Event {
    Session(u64, Result<Session, String>),
    /// The complete tile list after an accepted change.
    Tiles(u64, Vec<CustomTile>),
    Ended(u64),
}

#[derive(Default)]
struct Snapshot {
    tiles: Vec<CustomTile>,
    revision: u64,
    acknowledged: u64,
}

pub struct Service {
    desired: Arc<AtomicU64>,
    /// Accepted edits remain authoritative until the SDL thread has consumed them.
    tiles: Arc<Mutex<Snapshot>>,
    wake: mpsc::Sender<()>,
    pub receiver: mpsc::Receiver<Event>,
    active: bool,
    generation: u64,
}

impl Service {
    pub fn new(loopback_only: bool) -> Result<Self, String> {
        let desired = Arc::new(AtomicU64::new(0));
        let tiles = Arc::new(Mutex::new(Snapshot::default()));
        let worker_desired = desired.clone();
        let worker_tiles = tiles.clone();
        let (sender, receiver) = mpsc::sync_channel(4);
        let (wake, updates) = mpsc::channel();
        thread::Builder::new()
            .name("luma-tile-editor".into())
            .spawn(move || {
                worker(
                    &worker_desired,
                    &worker_tiles,
                    &sender,
                    &updates,
                    loopback_only,
                )
            })
            .map_err(|e| format!("Could not start tile editor worker: {e}"))?;
        Ok(Self {
            desired,
            tiles,
            wake,
            receiver,
            active: false,
            generation: 0,
        })
    }

    pub fn set_active(&mut self, active: bool, tiles: &[CustomTile]) {
        if self.active != active {
            self.active = active;
            self.generation += 1;
            // Serialize session rotation with commit: an old request either commits before
            // this snapshot or sees its generation revoked, never overwriting a new session.
            let mut current = self.tiles.lock().unwrap();
            if active && current.revision == current.acknowledged {
                current.tiles = tiles.to_vec();
            }
            self.desired
                .store(if active { self.generation } else { 0 }, Ordering::Release);
            drop(current);
            let _ = self.wake.send(());
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn acknowledge(&self, revision: u64) {
        let mut current = self.tiles.lock().unwrap();
        current.acknowledged = current.acknowledged.max(revision.min(current.revision));
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.desired.store(STOP, Ordering::Release);
        let _ = self.wake.send(());
    }
}

/// A tile as the page submits it. An empty id adds a new tile.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Edit {
    id: String,
    title: String,
    url: String,
    method: TileMethod,
    icon: String,
}

impl Edit {
    /// The tile this edit describes, validated before saving to launcher preferences.
    fn tile(&self) -> Result<CustomTile, String> {
        let title = clean_input_name(&self.title);
        if title.is_empty() {
            return Err("Add a name for this shortcut.".into());
        }
        let url = self.url.trim().to_owned();
        if !valid_tile_url(&url) {
            return Err("Enter a valid http:// or https:// address with a host name.".into());
        }
        if !self.icon.is_empty() && !TILE_ICONS.iter().any(|(icon, _)| *icon == self.icon) {
            return Err("Choose one of the icons shown.".into());
        }
        Ok(CustomTile {
            id: self.id.clone(),
            title,
            url,
            method: self.method,
            icon: self.icon.clone(),
        })
    }
}

/// Adds or replaces a tile and returns its id.
fn save(tiles: &mut Vec<CustomTile>, edit: &Edit) -> Result<String, String> {
    let mut tile = edit.tile()?;
    if tile.id.is_empty() {
        if tiles.len() >= TILE_LIMIT {
            return Err("No more shortcuts can be added.".into());
        }
        tile.id = new_tile_id(tiles, &tile.title);
        tiles.push(tile.clone());
        return Ok(tile.id);
    }
    let existing = tiles
        .iter_mut()
        .find(|existing| existing.id == tile.id)
        .ok_or("This shortcut was removed. Reload the page.")?;
    *existing = tile.clone();
    Ok(tile.id)
}

fn remove(tiles: &mut Vec<CustomTile>, id: &str) -> Result<(), String> {
    let index = tiles
        .iter()
        .position(|tile| tile.id == id)
        .ok_or("This shortcut was already removed.")?;
    tiles.remove(index);
    Ok(())
}

fn icon_svg(icon: &str) -> &'static str {
    match icon {
        "weather-moon" => include_str!("../assets/ui-icons/weather-moon.svg"),
        "weather-sun" => include_str!("../assets/ui-icons/weather-sun.svg"),
        "weather-cloud" => include_str!("../assets/ui-icons/weather-cloud.svg"),
        "tv" => include_str!("../assets/ui-icons/tv.svg"),
        "power" => include_str!("../assets/ui-icons/power.svg"),
        "globe" => include_str!("../assets/ui-icons/globe.svg"),
        "clock" => include_str!("../assets/ui-icons/clock.svg"),
        "coffee" => include_str!("../assets/ui-icons/coffee.svg"),
        "gaming" => include_str!("../assets/ui-icons/gaming.svg"),
        "heart" => include_str!("../assets/ui-icons/heart.svg"),
        "location" => include_str!("../assets/ui-icons/location.svg"),
        "thermometer" => include_str!("../assets/ui-icons/thermometer.svg"),
        "sparkles" => include_str!("../assets/ui-icons/sparkles.svg"),
        "alert" => include_str!("../assets/ui-icons/alert.svg"),
        "eye" => include_str!("../assets/ui-icons/eye.svg"),
        _ => "",
    }
}

/// The page's whole view of the TV: the tiles, the icons it may choose and the limit.
fn listing(tiles: &[CustomTile]) -> serde_json::Value {
    let language = crate::i18n::Language::En;
    serde_json::json!({
        "tiles": tiles,
        "icons": TILE_ICONS
            .iter()
            .map(|(icon, name)| serde_json::json!({
                "id": icon,
                "name": language.t(name),
                "svg": icon_svg(icon),
            }))
            .collect::<Vec<_>>(),
        "limit": TILE_LIMIT,
    })
}

fn worker(
    desired: &AtomicU64,
    shared: &Mutex<Snapshot>,
    sender: &mpsc::SyncSender<Event>,
    wake: &mpsc::Receiver<()>,
    loopback_only: bool,
) {
    let agent = crate::webhook::agent();
    let mut handled = 0;
    loop {
        let generation = desired.load(Ordering::Acquire);
        if generation == STOP {
            break;
        }
        if generation == 0 || generation == handled {
            if wake.recv().is_err() {
                break;
            }
            continue;
        }
        handled = generation;
        let mut tiles = shared.lock().unwrap().tiles.clone();
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
                    let host = address.to_string();
                    let commit = |tiles: &[CustomTile]| {
                        let revision = {
                            let mut current = shared.lock().unwrap();
                            if !valid() {
                                return Err("Editing session ended".into());
                            }
                            current.tiles = tiles.to_vec();
                            current.revision += 1;
                            current.revision
                        };
                        // Queue the accepted edit before replying, without holding the lock
                        // while waiting for SDL to drain the bounded event channel.
                        sender
                            .send(Event::Tiles(revision, tiles.to_vec()))
                            .map_err(|_| "Launcher stopped".to_owned())
                    };
                    if let Err(error) = serve(
                        &mut stream,
                        &host,
                        &token,
                        &mut tiles,
                        &agent,
                        valid,
                        commit,
                    ) {
                        eprintln!("Luma tile editor: {error}");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25))
                }
                Err(error) => {
                    eprintln!("Luma tile editor server: {error}");
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

fn json(stream: &mut TcpStream, status: &str, body: &serde_json::Value) -> Result<(), String> {
    response(
        stream,
        status,
        "application/json; charset=utf-8",
        body.to_string().as_bytes(),
    )
}

/// Serves one request; changes are published by `commit` before a success reply.
/// `Ok(true)` means the local tile list changed, including if the reply was lost.
fn serve(
    stream: &mut TcpStream,
    host: &str,
    token: &str,
    tiles: &mut Vec<CustomTile>,
    agent: &ureq::Agent,
    valid: impl Fn() -> bool,
    commit: impl FnOnce(&[CustomTile]) -> Result<(), String>,
) -> Result<bool, String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + REQUEST_TIME;
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
            error_response(stream, "400 Bad Request", &error)?;
            return Ok(false);
        }
    };
    if request.foreign(host) {
        error_response(stream, "403 Forbidden", "Open the address shown on your TV")?;
        return Ok(false);
    }
    if !valid() {
        return Err("Editing session ended".into());
    }
    if request.method == "GET" && request.length == 0 && request.target == format!("/{token}") {
        redirect_to_page(stream, token)?;
        return Ok(false);
    }
    let Some(route) = request.target.strip_prefix(&format!("/{token}/")) else {
        error_response(stream, "404 Not Found", "Editing session not found")?;
        return Ok(false);
    };
    if request.method == "GET" && request.length == 0 {
        if route == "tiles.json" {
            json(stream, "200 OK", &listing(tiles))?;
            return Ok(false);
        }
        let (kind, body): (&str, &[u8]) = match route {
            "" => (
                "text/html; charset=utf-8",
                include_bytes!("../assets/tile-editor/index.html"),
            ),
            "app.css" => (
                "text/css; charset=utf-8",
                include_bytes!("../assets/tile-editor/app.css"),
            ),
            "app.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/tile-editor/app.js"),
            ),
            "tiles.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../assets/tile-editor/tiles.js"),
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
        || !matches!(route, "save" | "remove" | "send")
        || request.headers.get("x-luma-token").map(String::as_str) != Some(token)
    {
        error_response(stream, "403 Forbidden", "Invalid request")?;
        return Ok(false);
    }
    if request
        .headers
        .get("content-type")
        .is_none_or(|value| value.split(';').next() != Some("application/json"))
    {
        error_response(stream, "415 Unsupported Media Type", "Send JSON")?;
        return Ok(false);
    }
    let edit = match read_edit(&mut reader, &request, &valid, deadline) {
        Ok(edit) => edit,
        Err(error) => {
            drop(reader);
            error_response(stream, "400 Bad Request", &error)?;
            return Ok(false);
        }
    };
    drop(reader);
    let result = match route {
        "send" => {
            // Sent from the TV, so the test takes the same network path as a tile press.
            let result = edit
                .tile()
                .and_then(|tile| crate::webhook::send(agent, &tile));
            return match result {
                Ok(()) => json(stream, "200 OK", &serde_json::json!({})).map(|()| false),
                Err(error) => error_response(stream, "502 Bad Gateway", &error).map(|()| false),
            };
        }
        "save" => {
            let mut next = tiles.clone();
            save(&mut next, &edit).map(|id| (next, Some(id)))
        }
        _ => {
            let mut next = tiles.clone();
            remove(&mut next, &edit.id).map(|()| (next, None))
        }
    };
    match result {
        Ok((next, id)) => {
            if !valid() {
                return Err("Editing session ended".into());
            }
            commit(&next)?;
            *tiles = next;
            let mut body = listing(tiles);
            body["id"] = serde_json::json!(id);
            // The change is applied even if the phone disconnects before receiving its reply.
            let _ = json(stream, "200 OK", &body);
            Ok(true)
        }
        Err(error) => {
            error_response(stream, "422 Unprocessable Content", &error)?;
            Ok(false)
        }
    }
}

fn read_edit(
    reader: &mut impl std::io::Read,
    request: &Request,
    valid: &impl Fn() -> bool,
    deadline: Instant,
) -> Result<Edit, String> {
    let mut body = vec![0; request.length];
    read_exact(reader, &mut body, valid, deadline)?;
    serde_json::from_slice(&body).map_err(|_| "Invalid shortcut".into())
}

#[cfg(test)]
#[path = "tile-editor-tests.rs"]
mod tests;
