//! PlxNative widgets: Continue watching and Recently added from the owner's Plex Media Server,
//! opened in PlxNative (`com.beb.plxnative`) through its launch parameters
//! `{"plex":{"server","ratingKey","action"}}`.
//!
//! Nothing here runs until one of the two widgets is on Home: no worker threads, no account or
//! cache file reads, no requests and no poster downloads. Removing both widgets stops the workers
//! and releases what was loaded.
//!
//! plex.tv is a fixed HTTPS origin. Server addresses come only from the account's plex.tv
//! resource list, and must be IP literals or `*.plex.direct` names. Plain HTTP is accepted only
//! for connections plex.tv marks as local, to a private IPv4 address. Requests never follow
//! redirects or use a proxy, and replies are bounded. The account token is kept in its own file
//! beside the preferences (mode 0600), never in preferences, diagnostics or logs, and is sent only
//! as a header.
use crate::{
    model::{HomeWidget, Preferences, WidgetPosition},
    state::{Overlay, Page, State, Subpage, Target},
};
use image::{ImageReader, Limits, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fmt,
    io::{Cursor, Read},
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub const APP_ID: &str = "com.beb.plxnative";
pub const APP_NAME: &str = "PlxNative";
/// Where the owner types the code, on a phone or computer.
pub const LINK_PAGE: &str = "plex.tv/link";
const LINK_URL: &str = "https://plex.tv/link";
const PLEX_TV: &str = "https://plex.tv";
const PRODUCT: &str = "Luma";
pub const MAX_ITEMS: usize = 20;
/// Lists younger than this are not fetched again.
const FRESH: i64 = 600;
/// Back from another app with lists older than this: fetch again (something was probably watched).
const RETURN: i64 = 60;
/// Older saved lists are not shown at all.
const SHOWN: i64 = 7 * 24 * 3600;
/// Waits after failed fetches in a row.
const RETRY: [i64; 3] = [120, 600, 1800];
/// plex.tv codes last 15 minutes; a new one is asked for a little before.
const PIN_LIFE: i64 = 14 * 60;
const PIN_POLL: i64 = 2;
const JSON_LIMIT: usize = 1024 * 1024;
const IMAGE_LIMIT: usize = 2 * 1024 * 1024;
const ACCOUNT_LIMIT: u64 = 64 * 1024;
const CACHE_LIMIT: u64 = 256 * 1024;
pub const POSTER_WIDTH: u32 = 200;
pub const POSTER_HEIGHT: u32 = 300;
const POSTERS_IN_FLIGHT: usize = 3;

// Home row geometry: a half-width 3×2 row is 864×280 logical pixels, a wide 6×2 row 1752×280.
pub const ROW_HEADING: f32 = 30.;
pub const POSTER_TOP: f32 = 38.;
pub const POSTER_W: f32 = 125.;
pub const POSTER_H: f32 = 188.;
pub const POSTER_GAP: f32 = 20.;
const HALF_WIDTH: f32 = 864.;
const WIDE_WIDTH: f32 = 1752.;

/// Posters that fit side by side in a row of this width.
pub fn visible_in(width: f32) -> usize {
    (((width + POSTER_GAP) / (POSTER_W + POSTER_GAP)) as usize).max(1)
}

/// Posters a list's row shows at once: 6 at half width, 12 wide.
pub fn visible_posters(preferences: &Preferences, list: List) -> usize {
    visible_in(if placement(preferences, list).wide {
        WIDE_WIDTH
    } else {
        HALF_WIDTH
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Resume or start playback.
    #[default]
    Play,
    /// Show the item's details page.
    Open,
}

impl Action {
    pub const ALL: [Self; 2] = [Self::Play, Self::Open];
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Play => "plexActionPlay",
            Self::Open => "plexActionOpen",
        }
    }
    fn param(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Open => "open",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Placement {
    pub enabled: bool,
    pub position: WidgetPosition,
    /// The whole width (6×2) instead of half (3×2).
    pub wide: bool,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            enabled: false,
            position: WidgetPosition { column: 0, row: 2 },
            wide: false,
        }
    }
}

/// What preferences keep: where the widgets sit, their size and what OK does. No account data.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub continue_watching: Placement,
    pub recently_added: Placement,
    pub action: Action,
}

impl Default for Settings {
    /// Both half-width rows side by side below Greeting and Weather.
    fn default() -> Self {
        Self {
            continue_watching: Placement::default(),
            recently_added: Placement {
                position: WidgetPosition { column: 3, row: 2 },
                ..Placement::default()
            },
            action: Action::default(),
        }
    }
}

pub fn deserialize_settings<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Settings, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).unwrap_or_default())
}

/// The two lists, one per widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum List {
    Continue,
    Recent,
}

impl List {
    pub const ALL: [Self; 2] = [Self::Continue, Self::Recent];
    pub fn of(widget: HomeWidget) -> Option<Self> {
        match widget {
            HomeWidget::ContinueWatching => Some(Self::Continue),
            HomeWidget::RecentlyAdded => Some(Self::Recent),
            _ => None,
        }
    }
    pub fn widget(self) -> HomeWidget {
        match self {
            Self::Continue => HomeWidget::ContinueWatching,
            Self::Recent => HomeWidget::RecentlyAdded,
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn empty_key(self) -> &'static str {
        match self {
            Self::Continue => "plexNothingToContinue",
            Self::Recent => "plexNothingNew",
        }
    }
}

pub fn placement(preferences: &Preferences, list: List) -> &Placement {
    match list {
        List::Continue => &preferences.plex.continue_watching,
        List::Recent => &preferences.plex.recently_added,
    }
}

pub fn placement_mut(preferences: &mut Preferences, list: List) -> &mut Placement {
    match list {
        List::Continue => &mut preferences.plex.continue_watching,
        List::Recent => &mut preferences.plex.recently_added,
    }
}

/// The lists whose widget is on Home, in the order they are fetched.
pub fn enabled_lists(preferences: &Preferences) -> Vec<List> {
    List::ALL
        .into_iter()
        .filter(|list| placement(preferences, *list).enabled)
        .collect()
}

// ---- account (its own file, never preferences) ----

#[derive(Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Account {
    /// Random per installation; plex.tv lists it under the account's devices.
    pub client_id: String,
    pub token: String,
    pub name: String,
    pub server: Option<Server>,
}

/// The chosen server, with the connection that answered and its own token.
#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub uri: String,
    pub token: String,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("signed_in", &self.signed_in())
            .field("server", &self.server)
            .finish()
    }
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

impl Account {
    pub fn signed_in(&self) -> bool {
        valid_token(&self.token)
    }
    pub fn ready(&self) -> bool {
        self.signed_in() && self.server.is_some()
    }
    fn sanitize(mut self) -> Self {
        if !valid_client_id(&self.client_id) {
            self.client_id.clear();
        }
        if !valid_token(&self.token) {
            self.token.clear();
            self.server = None;
        }
        self.name = clean(&self.name, 80);
        if let Some(server) = &mut self.server {
            server.name = clean(&server.name, 80);
        }
        if self.server.as_ref().is_some_and(|server| {
            !valid_server_id(&server.id)
                || !valid_token(&server.token)
                || checked_connection(&server.uri, true).as_deref() != Some(server.uri.as_str())
        }) {
            self.server = None;
        }
        self
    }
}

pub fn account_path(preferences: &Path) -> PathBuf {
    preferences.with_extension("plex.json")
}

pub fn cache_path(preferences: &Path) -> PathBuf {
    preferences.with_extension("plex-shelves.json")
}

fn read_bounded(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

pub fn load_account(path: &Path) -> Account {
    read_bounded(path, ACCOUNT_LIMIT)
        .and_then(|bytes| serde_json::from_slice::<Account>(&bytes).ok())
        .unwrap_or_default()
        .sanitize()
}

fn save_account(path: &Path, account: &Account) {
    let result = serde_json::to_vec(account)
        .map_err(|e| e.to_string())
        .and_then(|bytes| crate::storage::write_atomic(path, &bytes));
    if let Err(error) = result {
        eprintln!("Luma PlxNative: could not save the sign-in: {error}");
    }
}

pub fn load_cache(path: &Path) -> Option<Shelves> {
    read_bounded(path, CACHE_LIMIT)
        .and_then(|bytes| serde_json::from_slice::<Shelves>(&bytes).ok())
        .map(Shelves::sanitize)
}

fn save_cache(path: &Path, shelves: &Shelves) {
    let result = serde_json::to_vec(shelves)
        .map_err(|e| e.to_string())
        .and_then(|bytes| crate::storage::write_atomic(path, &bytes));
    if let Err(error) = result {
        eprintln!("Luma PlxNative: could not save the lists: {error}");
    }
}

pub fn new_client_id() -> Option<String> {
    let mut bytes = [0u8; 12];
    getrandom::getrandom(&mut bytes).ok()?;
    Some(format!(
        "luma-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

// ---- pure protocol ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub id: u64,
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub uri: String,
    pub local: bool,
    pub relay: bool,
}

/// A server on the account, as plex.tv lists it.
#[derive(Clone, PartialEq)]
pub struct ServerChoice {
    pub id: String,
    pub name: String,
    pub token: String,
    pub owned: bool,
    pub connections: Vec<Connection>,
}

impl fmt::Debug for ServerChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerChoice")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("owned", &self.owned)
            .field("connections", &self.connections)
            .finish()
    }
}

/// One entry of a list. For an episode `title` is the show, `subtitle` "S2 · E5" and `thumb` the
/// show's poster; for a film `subtitle` is the year.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Item {
    /// The ratingKey, digits only.
    pub key: String,
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    /// A server path starting with `/`, or empty.
    pub thumb: String,
    /// 0..1, how far it was watched.
    pub progress: f32,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            key: String::new(),
            kind: String::new(),
            title: String::new(),
            subtitle: String::new(),
            thumb: String::new(),
            progress: 0.,
        }
    }
}

/// What one fetch read, for one server. Also the saved copy.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Shelves {
    /// The server (machineIdentifier) the lists came from.
    pub origin: String,
    /// Unix seconds.
    pub at: i64,
    pub continue_watching: Option<Vec<Item>>,
    pub recently_added: Option<Vec<Item>>,
}

impl Shelves {
    pub fn list(&self, list: List) -> Option<&[Item]> {
        match list {
            List::Continue => self.continue_watching.as_deref(),
            List::Recent => self.recently_added.as_deref(),
        }
    }
    fn set(&mut self, list: List, items: Vec<Item>) {
        match list {
            List::Continue => self.continue_watching = Some(items),
            List::Recent => self.recently_added = Some(items),
        }
    }
    fn sanitize(mut self) -> Self {
        for items in [&mut self.continue_watching, &mut self.recently_added]
            .into_iter()
            .flatten()
        {
            items.retain(|item| valid_rating_key(&item.key));
            items.truncate(MAX_ITEMS);
            for item in items.iter_mut() {
                item.kind = clean(&item.kind, 16);
                item.title = clean(&item.title, 200);
                item.subtitle = clean(&item.subtitle, 200);
                if !valid_thumb(&item.thumb) {
                    item.thumb.clear();
                }
                item.progress = if item.progress.is_finite() {
                    item.progress.clamp(0., 1.)
                } else {
                    0.
                };
            }
        }
        self
    }
    /// Usable for this server: same origin and not older than a week.
    fn usable(&self, origin: &str, now: i64) -> bool {
        self.origin == origin && self.at <= now + 600 && now - self.at < SHOWN
    }
}

fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

fn number(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0. && *n < 1e15)
                .map(|n| n as u64)
        })
        .or_else(|| {
            value
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 15 && s.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0)
}

pub fn valid_token(token: &str) -> bool {
    (8..=128).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn valid_client_id(id: &str) -> bool {
    (8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_server_id(id: &str) -> bool {
    (8..=80).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_rating_key(key: &str) -> bool {
    (1..=12).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_digit())
}

fn valid_thumb(thumb: &str) -> bool {
    thumb.starts_with('/')
        && thumb.len() <= 500
        && thumb
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'?' | b'#' | b'%' | b'\\'))
}

/// The headers plex.tv and a server want. The token is only ever a header.
fn headers(client: &str, token: Option<&str>) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Accept", "application/json".to_owned()),
        ("X-Plex-Product", PRODUCT.to_owned()),
        ("X-Plex-Version", env!("CARGO_PKG_VERSION").to_owned()),
        ("X-Plex-Client-Identifier", client.to_owned()),
        ("X-Plex-Platform", "webOS".to_owned()),
        ("X-Plex-Device", "LG TV".to_owned()),
        ("X-Plex-Device-Name", PRODUCT.to_owned()),
    ];
    if let Some(token) = token {
        headers.push(("X-Plex-Token", token.to_owned()));
    }
    headers
}

pub fn parse_pin(value: &Value) -> Option<Pin> {
    let id = number(&value["id"]);
    let code = text(&value["code"]);
    (id > 0 && (4..=12).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| Pin {
            id,
            code: code.to_ascii_uppercase(),
        })
}

pub fn parse_pin_token(value: &Value) -> Option<String> {
    let token = text(&value["authToken"]);
    valid_token(&token).then_some(token)
}

pub fn parse_account_name(value: &Value) -> String {
    let name = ["friendlyName", "username", "title"]
        .into_iter()
        .map(|key| text(&value[key]))
        .find(|name| !name.trim().is_empty())
        .unwrap_or_default();
    clean(&name, 80)
}

/// The server address to use for a connection, or None when it is not one Luma talks to:
/// http(s), no credentials, path or query, a host that is an IP literal or a `*.plex.direct`
/// name, and plain http only to a private IPv4 address of a local connection.
pub fn checked_connection(uri: &str, local: bool) -> Option<String> {
    let base = checked_base(uri)?;
    if base.starts_with("http://") {
        let parsed: ureq::http::Uri = base.parse().ok()?;
        let host = parsed.host()?;
        let ip: IpAddr = host.parse().ok()?;
        let private = match ip {
            IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
            IpAddr::V6(ip) => ip.is_loopback(),
        };
        if !local || !private {
            return None;
        }
    }
    Some(base)
}

/// The scheme and authority of a server address, normalized, when its shape is acceptable.
fn checked_base(uri: &str) -> Option<String> {
    let uri = uri.trim().trim_end_matches('/');
    if uri.len() > 200 {
        return None;
    }
    let parsed: ureq::http::Uri = uri.parse().ok()?;
    let scheme = parsed.scheme_str()?;
    let authority = parsed.authority()?;
    if !matches!(scheme, "http" | "https")
        || authority.as_str().contains('@')
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
    {
        return None;
    }
    let host = parsed.host()?;
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let ip_literal = bare.parse::<IpAddr>().is_ok();
    let plex_direct = host.len() > ".plex.direct".len()
        && host.to_ascii_lowercase().ends_with(".plex.direct")
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'));
    if !ip_literal && !plex_direct {
        return None;
    }
    Some(format!(
        "{scheme}://{}",
        authority.as_str().to_ascii_lowercase()
    ))
}

/// The order connections are tried in: the home network first (plain http to its address before
/// the plex.direct name), then over the internet, the relay last.
pub fn order_connections(connections: &mut [Connection]) {
    let rank = |c: &Connection| {
        if c.relay {
            3
        } else if c.local {
            u8::from(!c.uri.starts_with("http://"))
        } else {
            2
        }
    };
    connections.sort_by_key(rank);
}

/// The servers on the account (its own first), each with its connections in the order to try.
pub fn parse_servers(value: &Value) -> Vec<ServerChoice> {
    let mut servers = Vec::new();
    for resource in value.as_array().into_iter().flatten() {
        if !text(&resource["provides"])
            .split(',')
            .any(|part| part.trim() == "server")
        {
            continue;
        }
        let id = text(&resource["clientIdentifier"]);
        let token = text(&resource["accessToken"]);
        if !valid_server_id(&id) || !valid_token(&token) {
            continue;
        }
        let mut connections: Vec<Connection> = Vec::new();
        for connection in resource["connections"].as_array().into_iter().flatten() {
            let local = connection["local"] == Value::Bool(true);
            let relay = connection["relay"] == Value::Bool(true);
            if let Some(uri) = checked_connection(&text(&connection["uri"]), local && !relay)
                && !connections.iter().any(|c| c.uri == uri)
            {
                connections.push(Connection { uri, local, relay });
            }
            // A local connection also answers on plain http at its address (no certificate).
            let address = text(&connection["address"]);
            let port = number(&connection["port"]);
            if local
                && !relay
                && address.parse::<std::net::Ipv4Addr>().is_ok()
                && (1..65536).contains(&port)
                && let Some(uri) = checked_connection(&format!("http://{address}:{port}"), true)
                && !connections.iter().any(|c| c.uri == uri)
            {
                connections.push(Connection {
                    uri,
                    local: true,
                    relay: false,
                });
            }
        }
        if connections.is_empty() || servers.len() >= 20 {
            continue;
        }
        order_connections(&mut connections);
        connections.truncate(8);
        let name = clean(&text(&resource["name"]), 80);
        servers.push(ServerChoice {
            name: if name.is_empty() { id.clone() } else { name },
            id,
            token,
            owned: resource["owned"] == Value::Bool(true),
            connections,
        });
    }
    servers.sort_by_key(|server| !server.owned);
    servers
}

pub fn parse_identity(value: &Value) -> String {
    text(&value["MediaContainer"]["machineIdentifier"])
}

fn continue_url(base: &str) -> String {
    format!(
        "{base}/hubs/continueWatching?count={MAX_ITEMS}&X-Plex-Container-Start=0&X-Plex-Container-Size={MAX_ITEMS}"
    )
}

fn on_deck_url(base: &str) -> String {
    format!("{base}/library/onDeck?X-Plex-Container-Start=0&X-Plex-Container-Size={MAX_ITEMS}")
}

fn recent_url(base: &str) -> String {
    format!(
        "{base}/library/recentlyAdded?X-Plex-Container-Start=0&X-Plex-Container-Size={MAX_ITEMS}"
    )
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// A poster scaled by the server to the drawn size, so the TV never decodes a full-size picture.
pub fn poster_url(base: &str, thumb: &str) -> Option<String> {
    valid_thumb(thumb).then(|| {
        format!(
            "{base}/photo/:/transcode?width={POSTER_WIDTH}&height={POSTER_HEIGHT}&minSize=1&upscale=1&url={}",
            encode(thumb)
        )
    })
}

/// Films and TV only; music, photos and clips are left out.
fn item_of(value: &Value) -> Option<Item> {
    let kind = text(&value["type"]);
    let key = text(&value["ratingKey"]);
    if !matches!(kind.as_str(), "movie" | "episode" | "season" | "show") || !valid_rating_key(&key)
    {
        return None;
    }
    let duration = number(&value["duration"]);
    let offset = number(&value["viewOffset"]);
    let progress = if duration > 0 && offset > 0 {
        (offset as f64 / duration as f64).min(1.) as f32
    } else {
        0.
    };
    let first = |keys: &[&str]| {
        keys.iter()
            .map(|key| text(&value[*key]))
            .find(|value| !value.is_empty())
            .unwrap_or_default()
    };
    let index = |key: &str| match number(&value[key]) {
        0 => "?".to_owned(),
        n => n.to_string(),
    };
    let (title, subtitle, thumb, progress) = match kind.as_str() {
        "episode" => (
            first(&["grandparentTitle", "title"]),
            format!("S{} · E{}", index("parentIndex"), index("index")),
            first(&["grandparentThumb", "parentThumb", "thumb"]),
            progress,
        ),
        "season" => (
            first(&["parentTitle", "title"]),
            text(&value["title"]),
            first(&["thumb", "parentThumb"]),
            0.,
        ),
        _ => (
            text(&value["title"]),
            match number(&value["year"]) {
                0 => String::new(),
                year => year.to_string(),
            },
            text(&value["thumb"]),
            progress,
        ),
    };
    Some(Item {
        key,
        kind,
        title: clean(&title, 200),
        subtitle: clean(&subtitle, 200),
        thumb: if valid_thumb(&thumb) {
            thumb
        } else {
            String::new()
        },
        progress,
    })
}

/// The items of a hub or library reply (`Metadata` directly, or the first hub's `Metadata`).
pub fn parse_items(value: &Value) -> Vec<Item> {
    let container = &value["MediaContainer"];
    let mut metadata = container["Metadata"].as_array();
    if metadata.is_none_or(Vec::is_empty) {
        metadata = container["Hub"]
            .as_array()
            .and_then(|hubs| hubs.first())
            .and_then(|hub| hub["Metadata"].as_array());
    }
    let mut items: Vec<Item> = Vec::new();
    for item in metadata.into_iter().flatten().filter_map(item_of) {
        if items.len() >= MAX_ITEMS {
            break;
        }
        if !items.iter().any(|known| known.key == item.key) {
            items.push(item);
        }
    }
    items
}

fn is_container(value: &Value) -> bool {
    value["MediaContainer"].is_object()
}

/// A season or a show has nothing to resume, so it always opens its page.
pub fn action_for(item: &Item, wanted: Action) -> Action {
    if matches!(item.kind.as_str(), "season" | "show") {
        Action::Open
    } else {
        wanted
    }
}

/// PlxNative's launch parameters.
pub fn launch_params(item: &Item, server: &str, wanted: Action) -> Value {
    json!({"plex": {"server": server, "ratingKey": item.key, "action": action_for(item, wanted).param()}})
}

/// A QR code for the link page, in the companion dialog's format.
pub fn link_qr() -> Option<(i32, Vec<bool>)> {
    let qr = qrcodegen::QrCode::encode_text(LINK_URL, qrcodegen::QrCodeEcc::High).ok()?;
    let size = qr.size();
    Some((
        size,
        (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .map(|(x, y)| qr.get_module(x, y))
            .collect(),
    ))
}

// ---- worker ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No answer: network down, server off, timed out or refused.
    NoAnswer,
    /// An HTTP status; 401 means the sign-in is no longer valid.
    Status(u16),
    /// An answer that is not what was asked for.
    Invalid,
}

enum Job {
    Pin {
        client: String,
    },
    CheckPin {
        client: String,
        id: u64,
    },
    SignedIn {
        client: String,
        token: String,
    },
    Servers {
        client: String,
        token: String,
    },
    Connect {
        client: String,
        server: ServerChoice,
    },
    Shelves {
        generation: u64,
        client: String,
        server: Server,
        lists: Vec<List>,
        cache: Option<PathBuf>,
    },
    Recover {
        generation: u64,
        client: String,
        token: String,
        server: Server,
    },
}

enum Reply {
    Pin(Result<Pin, Failure>),
    CheckPin {
        id: u64,
        token: Result<Option<String>, Failure>,
    },
    SignedIn {
        token: String,
        name: String,
        servers: Result<Vec<ServerChoice>, Failure>,
    },
    Servers(Result<Vec<ServerChoice>, Failure>),
    Connected {
        id: String,
        result: Result<Server, Failure>,
    },
    Shelves {
        generation: u64,
        result: Result<Shelves, Failure>,
    },
    Recovered {
        generation: u64,
        result: Result<Server, Failure>,
    },
}

struct PosterJob {
    generation: u64,
    key: String,
    url: String,
    client: String,
    token: String,
}

struct PosterReply {
    generation: u64,
    key: String,
    image: Option<Arc<PosterImage>>,
}

pub struct PosterImage {
    pub key: String,
    pub pixels: RgbaImage,
}

/// Where plex.tv is: the real origin, or a loopback mock for desktop tests.
#[derive(Clone, Debug)]
pub struct Origin(Option<String>);

impl Origin {
    pub fn public() -> Self {
        Self(None)
    }
    /// `http://127.0.0.1:PORT` only; rejected on TV by the caller.
    pub fn test(server: &str) -> Result<Self, String> {
        let uri: ureq::http::Uri = server
            .parse()
            .map_err(|_| "Invalid PlxNative test server")?;
        let address: std::net::SocketAddr = uri
            .authority()
            .ok_or("Missing PlxNative test address")?
            .as_str()
            .parse()
            .map_err(|_| "PlxNative tests need an IP-literal loopback address")?;
        if uri.scheme_str() != Some("http")
            || !address.ip().is_loopback()
            || address.port() == 0
            || !matches!(uri.path(), "" | "/")
            || uri.query().is_some()
        {
            return Err("PlxNative tests need an HTTP loopback origin".into());
        }
        Ok(Self(Some(format!("http://{address}"))))
    }
    fn plex_tv(&self) -> &str {
        self.0.as_deref().unwrap_or(PLEX_TV)
    }
}

pub struct Service {
    jobs: mpsc::SyncSender<Job>,
    replies: mpsc::Receiver<Reply>,
    posters: mpsc::SyncSender<PosterJob>,
    poster_replies: mpsc::Receiver<PosterReply>,
    generation: Arc<AtomicU64>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .proxy(None)
        .build()
        .new_agent()
}

impl Service {
    /// Starts the two workers: one for plex.tv and the server's lists, one for posters, so a slow
    /// poster never holds up a sign-in or a refresh.
    pub fn start(origin: Origin) -> Result<Self, String> {
        let (jobs, job_receiver) = mpsc::sync_channel::<Job>(8);
        let (reply_sender, replies) = mpsc::sync_channel(8);
        let (posters, poster_receiver) = mpsc::sync_channel::<PosterJob>(POSTERS_IN_FLIGHT);
        let (poster_sender, poster_replies) = mpsc::sync_channel(POSTERS_IN_FLIGHT);
        let generation = Arc::new(AtomicU64::new(0));
        let active = generation.clone();
        thread::Builder::new()
            .name("plex".into())
            .spawn(move || {
                let agent = agent();
                while let Ok(job) = job_receiver.recv() {
                    let reply = run(&agent, &origin, job, &active);
                    if let Some(reply) = reply
                        && reply_sender.send(reply).is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        let active = generation.clone();
        thread::Builder::new()
            .name("plex-posters".into())
            .spawn(move || {
                let agent = agent();
                while let Ok(job) = poster_receiver.recv() {
                    let image = (active.load(Ordering::SeqCst) == job.generation)
                        .then(|| fetch_poster(&agent, &job))
                        .flatten();
                    let reply = PosterReply {
                        generation: job.generation,
                        key: job.key,
                        image,
                    };
                    if poster_sender.send(reply).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            jobs,
            replies,
            posters,
            poster_replies,
            generation,
        })
    }
}

fn request(
    agent: &ureq::Agent,
    post: bool,
    url: &str,
    headers: &[(&'static str, String)],
    timeout: Duration,
    limit: usize,
) -> Result<Vec<u8>, Failure> {
    let result = if post {
        let mut builder = agent.post(url);
        for (name, value) in headers {
            builder = builder.header(*name, value.as_str());
        }
        builder
            .config()
            .timeout_global(Some(timeout))
            .build()
            .send_empty()
    } else {
        let mut builder = agent.get(url);
        for (name, value) in headers {
            builder = builder.header(*name, value.as_str());
        }
        builder
            .config()
            .timeout_global(Some(timeout))
            .build()
            .call()
    };
    let mut response = match result {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => return Err(Failure::Status(code)),
        Err(_) => return Err(Failure::NoAnswer),
    };
    if !response.status().is_success() {
        return Err(Failure::Status(response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::NoAnswer)?;
    if bytes.len() > limit {
        return Err(Failure::Invalid);
    }
    Ok(bytes)
}

fn request_json(
    agent: &ureq::Agent,
    post: bool,
    url: &str,
    headers: &[(&'static str, String)],
    timeout: Duration,
) -> Result<Value, Failure> {
    let bytes = request(agent, post, url, headers, timeout, JSON_LIMIT)?;
    serde_json::from_slice(&bytes).map_err(|_| Failure::Invalid)
}

fn fetch_servers(
    agent: &ureq::Agent,
    origin: &Origin,
    client: &str,
    token: &str,
) -> Result<Vec<ServerChoice>, Failure> {
    request_json(
        agent,
        false,
        &format!(
            "{}/api/v2/resources?includeHttps=1&includeRelay=1",
            origin.plex_tv()
        ),
        &headers(client, Some(token)),
        Duration::from_secs(15),
    )
    .map(|value| parse_servers(&value))
}

/// The first connection, in order, that answers as this server.
fn connect(agent: &ureq::Agent, client: &str, server: &ServerChoice) -> Result<Server, Failure> {
    let mut last = Failure::NoAnswer;
    for connection in &server.connections {
        let timeout = Duration::from_secs(if connection.local { 3 } else { 8 });
        match request_json(
            agent,
            false,
            &format!("{}/identity", connection.uri),
            &headers(client, None),
            timeout,
        ) {
            Ok(value) => {
                let id = parse_identity(&value);
                if !id.is_empty() && id != server.id {
                    last = Failure::Invalid;
                    continue;
                }
                return Ok(Server {
                    id: server.id.clone(),
                    name: server.name.clone(),
                    uri: connection.uri.clone(),
                    token: server.token.clone(),
                });
            }
            Err(failure) => last = failure,
        }
    }
    Err(last)
}

fn fetch_list(
    agent: &ureq::Agent,
    server: &Server,
    headers: &[(&'static str, String)],
    list: List,
) -> Result<Vec<Item>, Failure> {
    let timeout = Duration::from_secs(10);
    match list {
        List::Recent => request_json(agent, false, &recent_url(&server.uri), headers, timeout)
            .map(|value| parse_items(&value)),
        // The hub the Plex apps show; a server too old for it has On Deck instead.
        List::Continue => {
            match request_json(agent, false, &continue_url(&server.uri), headers, timeout) {
                Ok(value) if is_container(&value) => Ok(parse_items(&value)),
                Ok(_) | Err(Failure::Status(400 | 404)) => {
                    request_json(agent, false, &on_deck_url(&server.uri), headers, timeout)
                        .map(|value| parse_items(&value))
                }
                Err(failure) => Err(failure),
            }
        }
    }
}

fn run(agent: &ureq::Agent, origin: &Origin, job: Job, active: &AtomicU64) -> Option<Reply> {
    let plex_tv = origin.plex_tv();
    Some(match job {
        Job::Pin { client } => Reply::Pin(
            request_json(
                agent,
                true,
                &format!("{plex_tv}/api/v2/pins?strong=false"),
                &headers(&client, None),
                Duration::from_secs(10),
            )
            .and_then(|value| parse_pin(&value).ok_or(Failure::Invalid)),
        ),
        Job::CheckPin { client, id } => Reply::CheckPin {
            id,
            token: request_json(
                agent,
                false,
                &format!("{plex_tv}/api/v2/pins/{id}"),
                &headers(&client, None),
                Duration::from_secs(10),
            )
            .map(|value| parse_pin_token(&value)),
        },
        Job::SignedIn { client, token } => {
            let name = request_json(
                agent,
                false,
                &format!("{plex_tv}/api/v2/user"),
                &headers(&client, Some(&token)),
                Duration::from_secs(10),
            )
            .map(|value| parse_account_name(&value))
            .unwrap_or_default();
            let servers = fetch_servers(agent, origin, &client, &token);
            Reply::SignedIn {
                token,
                name,
                servers,
            }
        }
        Job::Servers { client, token } => {
            Reply::Servers(fetch_servers(agent, origin, &client, &token))
        }
        Job::Connect { client, server } => Reply::Connected {
            id: server.id.clone(),
            result: connect(agent, &client, &server),
        },
        Job::Shelves {
            generation,
            client,
            server,
            lists,
            cache,
        } => {
            if active.load(Ordering::SeqCst) != generation {
                return None;
            }
            let headers = headers(&client, Some(&server.token));
            let mut shelves = Shelves {
                origin: server.id.clone(),
                at: time::OffsetDateTime::now_utc().unix_timestamp(),
                ..Default::default()
            };
            let mut result = Ok(());
            for list in lists {
                match fetch_list(agent, &server, &headers, list) {
                    Ok(items) => shelves.set(list, items),
                    Err(failure) => {
                        result = Err(failure);
                        break;
                    }
                }
            }
            if result.is_ok()
                && let Some(cache) = cache
            {
                save_cache(&cache, &shelves);
            }
            Reply::Shelves {
                generation,
                result: result.map(|()| shelves),
            }
        }
        Job::Recover {
            generation,
            client,
            token,
            server,
        } => {
            // The server did not answer: its addresses change with the home network or a new
            // router, so ask plex.tv for them again.
            let result = fetch_servers(agent, origin, &client, &token).and_then(|servers| {
                let found = servers
                    .into_iter()
                    .find(|choice| choice.id == server.id)
                    .ok_or(Failure::Invalid)?;
                connect(agent, &client, &found)
            });
            Reply::Recovered { generation, result }
        }
    })
}

fn fetch_poster(agent: &ureq::Agent, job: &PosterJob) -> Option<Arc<PosterImage>> {
    let bytes = request(
        agent,
        false,
        &job.url,
        &headers(&job.client, Some(&job.token)),
        Duration::from_secs(10),
        IMAGE_LIMIT,
    )
    .ok()?;
    Some(Arc::new(PosterImage {
        key: job.key.clone(),
        pixels: decode_poster(&bytes)?,
    }))
}

/// Decodes a poster and crops it to 2:3 at the drawn resolution.
pub fn decode_poster(bytes: &[u8]) -> Option<RgbaImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(3072);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    // Cover: scale so both sides fill the poster, then crop the middle.
    let scale = (POSTER_WIDTH as f32 / width as f32).max(POSTER_HEIGHT as f32 / height as f32);
    let scaled_w = ((width as f32 * scale).round() as u32).max(POSTER_WIDTH);
    let scaled_h = ((height as f32 * scale).round() as u32).max(POSTER_HEIGHT);
    let scaled = image::imageops::resize(
        &image,
        scaled_w,
        scaled_h,
        image::imageops::FilterType::Triangle,
    );
    Some(
        image::imageops::crop_imm(
            &scaled,
            (scaled_w - POSTER_WIDTH) / 2,
            (scaled_h - POSTER_HEIGHT) / 2,
            POSTER_WIDTH,
            POSTER_HEIGHT,
        )
        .to_image(),
    )
}

// ---- runtime state ----

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Ready,
    Failed,
    /// The sign-in is no longer valid; nothing is fetched until the owner signs in again.
    Expired,
}

#[derive(Clone, Debug, Default)]
pub struct SignIn {
    pub pin: Option<Pin>,
    pub failed: bool,
    started: i64,
    next_check: i64,
    checking: bool,
    requesting: bool,
}

#[derive(Clone, Debug, Default)]
pub enum Servers {
    #[default]
    Unknown,
    Loading,
    Loaded(Vec<ServerChoice>),
    Failed,
}

pub enum Poster {
    Loading,
    Ready(Arc<PosterImage>),
    /// Failed during this refresh; tried again after the next successful refresh.
    Failed(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    SignIn,
    CancelSignIn,
    SignOut,
    LoadServers,
    ChooseServer(usize),
}

#[derive(Default)]
pub struct Plex {
    paths: Option<(PathBuf, PathBuf)>,
    loaded: bool,
    pub account: Account,
    shelves: Option<Shelves>,
    pub status: Status,
    pub sign_in: Option<SignIn>,
    pub servers: Servers,
    /// The server a connection is being looked for, after it was chosen.
    pub connecting: Option<String>,
    pub posters: HashMap<String, Poster>,
    /// Remembered poster per row, and the first poster shown.
    pub selected: [usize; 2],
    pub first: [usize; 2],
    pending: Vec<Command>,
    generation: u64,
    signature: String,
    next: i64,
    failures: usize,
    recovered: bool,
    in_flight: bool,
    hidden: bool,
    rev: u64,
    posters_in_flight: usize,
}

impl fmt::Debug for Plex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Plex")
            .field("loaded", &self.loaded)
            .field("account", &self.account)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Plex {
    /// Where the sign-in and the saved lists live. Nothing is read until a widget is on Home.
    pub fn set_paths(&mut self, preferences: &Path) {
        self.paths = Some((account_path(preferences), cache_path(preferences)));
    }

    pub fn request(&mut self, command: Command) {
        self.pending.push(command);
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }

    /// Work is still on its way: for scripted captures that wait for lists and posters.
    pub fn busy(&self, preferences: &Preferences) -> bool {
        let lists = enabled_lists(preferences);
        if lists.is_empty() {
            return false;
        }
        let posters_missing = self.account.server.as_ref().is_some_and(|server| {
            lists.iter().any(|list| {
                self.items(*list).iter().any(|item| {
                    valid_thumb(&item.thumb)
                        && !self
                            .posters
                            .contains_key(&poster_key(&server.id, &item.thumb))
                })
            })
        });
        !self.loaded
            || self.in_flight
            || self.posters_in_flight > 0
            || posters_missing
            || self.connecting.is_some()
            || matches!(self.servers, Servers::Loading)
            || (self.account.ready() && self.status == Status::Idle)
            || self
                .sign_in
                .as_ref()
                .is_some_and(|sign_in| sign_in.pin.is_none() && !sign_in.failed)
    }

    /// The items to show for a list: only from the chosen server and not older than a week.
    pub fn items(&self, list: List) -> &[Item] {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        match (&self.shelves, &self.account.server) {
            (Some(shelves), Some(server)) if shelves.usable(&server.id, now) => {
                shelves.list(list).unwrap_or_default()
            }
            _ => &[],
        }
    }

    pub fn has_list(&self, list: List) -> bool {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        matches!((&self.shelves, &self.account.server),
            (Some(shelves), Some(server)) if shelves.usable(&server.id, now) && shelves.list(list).is_some())
    }

    pub fn poster(&self, item: &Item) -> Option<&Arc<PosterImage>> {
        let server = self.account.server.as_ref()?;
        match self.posters.get(&poster_key(&server.id, &item.thumb))? {
            Poster::Ready(image) => Some(image),
            _ => None,
        }
    }

    /// Selects a poster in a row and keeps it inside the window of `visible` posters.
    pub fn select(&mut self, list: List, index: usize, visible: usize) {
        let count = self.items(list).len();
        let index = index.min(count.saturating_sub(1));
        let row = list.index();
        self.selected[row] = index;
        if index < self.first[row] {
            self.first[row] = index;
        } else if index >= self.first[row] + visible {
            self.first[row] = index + 1 - visible;
        }
        self.first[row] = self.first[row].min(count.saturating_sub(visible));
    }

    fn load(&mut self, now: i64) {
        self.loaded = true;
        if let Some((account, cache)) = &self.paths {
            self.account = load_account(account);
            self.shelves = load_cache(cache).filter(|shelves| {
                self.account
                    .server
                    .as_ref()
                    .is_some_and(|server| shelves.usable(&server.id, now))
            });
        }
    }

    fn unload(&mut self) {
        let paths = self.paths.take();
        *self = Self {
            paths,
            ..Default::default()
        };
    }

    fn save_account(&self) {
        if let Some((path, _)) = &self.paths {
            save_account(path, &self.account);
        }
    }

    fn retry_later(&mut self, now: i64) {
        self.failures += 1;
        self.next = now + RETRY[(self.failures - 1).min(RETRY.len() - 1)];
        self.status = Status::Failed;
    }
}

fn poster_key(server: &str, thumb: &str) -> String {
    format!("plex:{server}:{thumb}")
}

/// Things the tick asks the rest of the state to do, applied after the Plex borrow ends.
#[derive(Default)]
struct Outcome {
    changed: bool,
    notices: Vec<&'static str>,
    open_servers: bool,
    close_servers: bool,
}

/// Runs once per event-loop iteration. Starts the workers on first need, sends due work and
/// applies replies. `network` is None for scripted or preview runs, which make no requests.
pub fn tick(
    state: &mut State,
    service: &mut Option<Service>,
    network: Option<&Origin>,
    visible: bool,
    now: i64,
) -> bool {
    let lists = enabled_lists(&state.preferences);
    let windows = List::ALL.map(|list| visible_posters(&state.preferences, list));
    let home = visible && state.page == Page::Home;
    let plex = &mut state.plex;
    if lists.is_empty() {
        // No widget on Home: stop the workers and forget what was loaded.
        *service = None;
        if plex.loaded {
            plex.unload();
            return true;
        }
        return false;
    }
    let mut out = Outcome::default();
    if !plex.loaded {
        plex.load(now);
        out.changed = true;
    }
    if service.is_none() {
        let Some(origin) = network else {
            plex.pending.clear();
            return out.changed;
        };
        match Service::start(origin.clone()) {
            Ok(started) => *service = Some(started),
            Err(error) => {
                eprintln!("Luma PlxNative: could not start: {error}");
                return out.changed;
            }
        }
    }
    let Some(service) = service.as_ref() else {
        return out.changed;
    };
    for command in std::mem::take(&mut plex.pending) {
        command_step(plex, service, command, now, &mut out);
    }
    sign_in_step(plex, service, visible, now);
    refresh_step(plex, service, &lists, home, visible, now, &mut out);
    for reply in service.replies.try_iter().collect::<Vec<_>>() {
        reply_step(plex, service, reply, now, windows, &mut out);
    }
    for reply in service.poster_replies.try_iter().collect::<Vec<_>>() {
        plex.posters_in_flight = plex.posters_in_flight.saturating_sub(1);
        out.changed = true;
        if reply.generation != plex.generation {
            plex.posters.remove(&reply.key);
            continue;
        }
        plex.posters.insert(
            reply.key,
            reply.image.map_or(Poster::Failed(plex.rev), Poster::Ready),
        );
    }
    if home {
        poster_step(plex, service, &lists);
    }
    finish(state, out)
}

fn finish(state: &mut State, out: Outcome) -> bool {
    let mut changed = out.changed;
    for key in out.notices {
        state.notify(state.preferences.language.t(key));
        changed = true;
    }
    if matches!(state.overlay, Some(Overlay::PlexSignIn)) && state.plex.sign_in.is_none() {
        state.overlay = None;
        if state.page == Page::Settings {
            state.focus = Target::PlexAccount;
        }
        changed = true;
    }
    if out.open_servers
        && state.page == Page::Settings
        && let Some(Subpage::Plex(widget)) = state.subpage
    {
        state.subpage = Some(Subpage::PlexServers(widget));
        state.focus = Target::Choice(0);
        state.scroll = 0.;
        changed = true;
    }
    if out.close_servers
        && let Some(Subpage::PlexServers(widget)) = state.subpage
    {
        state.subpage = Some(Subpage::Plex(widget));
        state.focus = Target::PlexServer;
        changed = true;
    }
    if changed {
        state.reconcile_plex_focus();
    }
    changed
}

fn command_step(plex: &mut Plex, service: &Service, command: Command, now: i64, out: &mut Outcome) {
    out.changed = true;
    match command {
        Command::SignIn => {
            if plex.account.client_id.is_empty() {
                let Some(id) = new_client_id() else {
                    out.notices.push("plexSignInFailed");
                    return;
                };
                plex.account.client_id = id;
                plex.save_account();
            }
            let mut sign_in = SignIn {
                started: now,
                ..Default::default()
            };
            sign_in.requesting = service
                .jobs
                .try_send(Job::Pin {
                    client: plex.account.client_id.clone(),
                })
                .is_ok();
            sign_in.failed = !sign_in.requesting;
            plex.sign_in = Some(sign_in);
        }
        Command::CancelSignIn => plex.sign_in = None,
        Command::SignOut => {
            plex.account = Account {
                client_id: std::mem::take(&mut plex.account.client_id),
                ..Default::default()
            };
            plex.save_account();
            if let Some((_, cache)) = &plex.paths {
                let _ = std::fs::remove_file(cache);
            }
            plex.shelves = None;
            plex.posters.clear();
            plex.servers = Servers::Unknown;
            plex.status = Status::Idle;
            plex.selected = [0; 2];
            plex.first = [0; 2];
        }
        Command::LoadServers => {
            if plex.account.signed_in()
                && service
                    .jobs
                    .try_send(Job::Servers {
                        client: plex.account.client_id.clone(),
                        token: plex.account.token.clone(),
                    })
                    .is_ok()
            {
                plex.servers = Servers::Loading;
            }
        }
        Command::ChooseServer(index) => {
            let Servers::Loaded(servers) = &plex.servers else {
                return;
            };
            let Some(server) = servers.get(index) else {
                return;
            };
            if plex
                .account
                .server
                .as_ref()
                .is_some_and(|chosen| chosen.id == server.id)
            {
                out.close_servers = true;
                return;
            }
            if service
                .jobs
                .try_send(Job::Connect {
                    client: plex.account.client_id.clone(),
                    server: server.clone(),
                })
                .is_ok()
            {
                plex.connecting = Some(server.id.clone());
            }
        }
    }
}

fn sign_in_step(plex: &mut Plex, service: &Service, visible: bool, now: i64) {
    let client = plex.account.client_id.clone();
    let Some(sign_in) = &mut plex.sign_in else {
        return;
    };
    if !visible || sign_in.checking || sign_in.requesting || sign_in.failed {
        return;
    }
    let Some(pin) = &sign_in.pin else {
        return;
    };
    if now - sign_in.started >= PIN_LIFE {
        // The code is about to expire: show a new one.
        sign_in.pin = None;
        sign_in.started = now;
        sign_in.requesting = service.jobs.try_send(Job::Pin { client }).is_ok();
        return;
    }
    if now >= sign_in.next_check {
        sign_in.checking = service
            .jobs
            .try_send(Job::CheckPin { client, id: pin.id })
            .is_ok();
    }
}

fn refresh_step(
    plex: &mut Plex,
    service: &Service,
    lists: &[List],
    home: bool,
    visible: bool,
    now: i64,
    out: &mut Outcome,
) {
    let signature = match (&plex.account.server, plex.account.signed_in()) {
        (Some(server), true) => format!(
            "{}@{}|{}|{:?}",
            server.id, server.uri, plex.account.token, lists
        ),
        _ => String::new(),
    };
    if signature != plex.signature {
        plex.signature = signature;
        plex.generation = plex.generation.wrapping_add(1);
        service.generation.store(plex.generation, Ordering::SeqCst);
        plex.in_flight = false;
        plex.next = now;
        plex.failures = 0;
        plex.recovered = false;
        plex.status = Status::Idle;
        let origin = plex.account.server.as_ref().map(|server| server.id.clone());
        if plex
            .shelves
            .as_ref()
            .is_some_and(|shelves| Some(&shelves.origin) != origin.as_ref())
        {
            plex.shelves = None;
        }
        plex.posters.clear();
        plex.posters_in_flight = 0;
        out.changed = true;
    }
    // Fetch again on return from another app when the lists are older than a minute.
    if !visible {
        plex.hidden = true;
    } else if std::mem::take(&mut plex.hidden)
        && plex
            .shelves
            .as_ref()
            .is_none_or(|shelves| now - shelves.at >= RETURN)
        && plex.status != Status::Expired
    {
        plex.next = plex.next.min(now);
    }
    // Saved lists younger than ten minutes are not fetched again at start.
    if plex.status == Status::Idle
        && let Some(shelves) = &plex.shelves
        && lists.iter().all(|list| shelves.list(*list).is_some())
        && now - shelves.at < FRESH
        && plex.next <= now
        && plex.failures == 0
    {
        plex.next = shelves.at + FRESH;
        plex.status = Status::Ready;
    }
    if !home
        || !plex.account.ready()
        || plex.status == Status::Expired
        || plex.in_flight
        || now < plex.next
    {
        return;
    }
    let Some(server) = plex.account.server.clone() else {
        return;
    };
    if service
        .jobs
        .try_send(Job::Shelves {
            generation: plex.generation,
            client: plex.account.client_id.clone(),
            server,
            lists: lists.to_vec(),
            cache: plex.paths.as_ref().map(|(_, cache)| cache.clone()),
        })
        .is_ok()
    {
        plex.in_flight = true;
        if plex.status != Status::Ready {
            plex.status = Status::Loading;
        }
        out.changed = true;
    }
}

fn reply_step(
    plex: &mut Plex,
    service: &Service,
    reply: Reply,
    now: i64,
    windows: [usize; 2],
    out: &mut Outcome,
) {
    out.changed = true;
    match reply {
        Reply::Pin(result) => {
            let Some(sign_in) = &mut plex.sign_in else {
                return;
            };
            sign_in.requesting = false;
            match result {
                Ok(pin) => {
                    sign_in.pin = Some(pin);
                    sign_in.started = now;
                    sign_in.next_check = now + PIN_POLL;
                }
                Err(_) => sign_in.failed = true,
            }
        }
        Reply::CheckPin { id, token } => {
            let Some(sign_in) = &mut plex.sign_in else {
                return;
            };
            if sign_in.pin.as_ref().is_none_or(|pin| pin.id != id) {
                return;
            }
            sign_in.checking = false;
            sign_in.next_check = now + PIN_POLL;
            if let Ok(Some(token)) = token {
                plex.sign_in = None;
                plex.account.token = token.clone();
                plex.account.name.clear();
                plex.account.server = None;
                plex.save_account();
                plex.status = Status::Idle;
                if service
                    .jobs
                    .try_send(Job::SignedIn {
                        client: plex.account.client_id.clone(),
                        token,
                    })
                    .is_ok()
                {
                    plex.servers = Servers::Loading;
                }
            }
        }
        Reply::SignedIn {
            token,
            name,
            servers,
        } => {
            if token != plex.account.token {
                return;
            }
            plex.account.name = name;
            plex.save_account();
            match servers {
                Ok(servers) if servers.is_empty() => {
                    plex.servers = Servers::Loaded(servers);
                    out.notices.push("plexNoServers");
                }
                Ok(servers) => {
                    let single = servers.len() == 1;
                    plex.servers = Servers::Loaded(servers);
                    if single {
                        command_step(plex, service, Command::ChooseServer(0), now, out);
                    } else {
                        out.open_servers = true;
                    }
                }
                Err(_) => plex.servers = Servers::Failed,
            }
        }
        Reply::Servers(result) => {
            plex.servers = match result {
                Ok(servers) => Servers::Loaded(servers),
                Err(Failure::Status(401)) => {
                    plex.status = Status::Expired;
                    Servers::Failed
                }
                Err(_) => Servers::Failed,
            };
        }
        Reply::Connected { id, result } => {
            if plex.connecting.as_deref() != Some(id.as_str()) {
                return;
            }
            plex.connecting = None;
            match result {
                Ok(server) => {
                    plex.account.server = Some(server);
                    plex.save_account();
                    plex.shelves = None;
                    out.close_servers = true;
                }
                Err(_) => out.notices.push("plexServerUnreachable"),
            }
        }
        Reply::Shelves { generation, result } => {
            if generation != plex.generation {
                return;
            }
            match result {
                Ok(shelves) => {
                    plex.in_flight = false;
                    plex.shelves = Some(shelves);
                    plex.status = Status::Ready;
                    plex.failures = 0;
                    plex.recovered = false;
                    plex.next = now + FRESH;
                    plex.rev = plex.rev.wrapping_add(1);
                    let server = plex.account.server.as_ref().map(|s| s.id.clone());
                    let keep: Vec<String> = List::ALL
                        .into_iter()
                        .flat_map(|list| plex.items(list).to_vec())
                        .filter_map(|item| server.as_ref().map(|id| poster_key(id, &item.thumb)))
                        .collect();
                    plex.posters.retain(|key, poster| {
                        keep.contains(key) && !matches!(poster, Poster::Loading)
                    });
                    plex.posters_in_flight = 0;
                    for list in List::ALL {
                        let selected = plex.selected[list.index()];
                        plex.select(list, selected, windows[list.index()]);
                    }
                }
                Err(Failure::Status(401)) => {
                    plex.in_flight = false;
                    plex.status = Status::Expired;
                    out.notices.push("plexSignInExpired");
                }
                Err(Failure::NoAnswer) if !plex.recovered => {
                    plex.recovered = true;
                    let Some(server) = plex.account.server.clone() else {
                        plex.in_flight = false;
                        return;
                    };
                    if service
                        .jobs
                        .try_send(Job::Recover {
                            generation,
                            client: plex.account.client_id.clone(),
                            token: plex.account.token.clone(),
                            server,
                        })
                        .is_err()
                    {
                        plex.in_flight = false;
                        plex.retry_later(now);
                    }
                }
                Err(_) => {
                    plex.in_flight = false;
                    plex.retry_later(now);
                }
            }
        }
        Reply::Recovered { generation, result } => {
            if generation != plex.generation {
                return;
            }
            plex.in_flight = false;
            match result {
                Ok(server)
                    if plex.account.server.as_ref().is_some_and(|known| {
                        known.uri != server.uri || known.token != server.token
                    }) =>
                {
                    // A new address: the changed signature fetches again at once.
                    plex.account.server = Some(server);
                    plex.save_account();
                }
                Err(Failure::Status(401)) => {
                    plex.status = Status::Expired;
                    out.notices.push("plexSignInExpired");
                }
                _ => plex.retry_later(now),
            }
        }
    }
}

fn poster_step(plex: &mut Plex, service: &Service, lists: &[List]) {
    let Some(server) = plex.account.server.clone() else {
        return;
    };
    let mut wanted = Vec::new();
    for list in lists {
        let row = list.index();
        // The visible window first, then the rest of the row.
        let items = plex.items(*list);
        let start = plex.first[row].min(items.len());
        for item in items[start..].iter().chain(items[..start].iter()) {
            if let Some(url) = poster_url(&server.uri, &item.thumb) {
                wanted.push((poster_key(&server.id, &item.thumb), url));
            }
        }
    }
    for (key, url) in wanted {
        if plex.posters_in_flight >= POSTERS_IN_FLIGHT {
            break;
        }
        match plex.posters.get(&key) {
            None => {}
            Some(Poster::Failed(rev)) if *rev != plex.rev => {}
            Some(_) => continue,
        }
        if service
            .posters
            .try_send(PosterJob {
                generation: plex.generation,
                key: key.clone(),
                url,
                client: plex.account.client_id.clone(),
                token: server.token.clone(),
            })
            .is_err()
        {
            break;
        }
        plex.posters.insert(key, Poster::Loading);
        plex.posters_in_flight += 1;
    }
}

#[cfg(test)]
impl Service {
    /// A service without workers, for scheduling tests.
    fn detached() -> (Self, mpsc::Receiver<Job>, mpsc::SyncSender<Reply>) {
        let (jobs, job_receiver) = mpsc::sync_channel(8);
        let (reply_sender, replies) = mpsc::sync_channel(8);
        let (posters, _poster_receiver) = mpsc::sync_channel(POSTERS_IN_FLIGHT);
        let (_poster_sender, poster_replies) = mpsc::sync_channel(POSTERS_IN_FLIGHT);
        (
            Self {
                jobs,
                replies,
                posters,
                poster_replies,
                generation: Arc::new(AtomicU64::new(0)),
            },
            job_receiver,
            reply_sender,
        )
    }
}

#[cfg(test)]
#[path = "plex-tests.rs"]
mod tests;
