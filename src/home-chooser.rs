//! Phase-two Home Chooser integration: privileged reads through the launcher's
//! real app-named handle. Mutations keep their existing confirmation semantics.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const SERVICE: &str = "luna://org.webosbrew.homechooser.service";
static ENABLED: AtomicBool = AtomicBool::new(false);
static INFO: Mutex<Option<(Instant, Result<Info, String>)>> = Mutex::new(None);
static STATUS: Mutex<Option<Status>> = Mutex::new(None);
static INVALIDATE: AtomicBool = AtomicBool::new(false);
static CHANGED: AtomicBool = AtomicBool::new(false);
static FEEDS: Mutex<Option<HashMap<&'static str, Feed>>> = Mutex::new(None);
pub type IconCandidates = Vec<Vec<u8>>;
pub type IconBatch = Vec<Result<IconCandidates, String>>;

#[derive(Default)]
struct Feed {
    snapshot: Option<Value>,
    ended: bool,
}

impl Feed {
    fn apply(&mut self, reply: Value, field: &str, limit: usize) -> Result<bool, String> {
        if self.ended {
            return Ok(false);
        }
        let valid = checked(reply).and_then(|reply| {
            if reply.get("subscribed").and_then(Value::as_bool) != Some(true) {
                return Err("Home Chooser did not accept subscription".into());
            }
            let values = array(&reply, field, limit)?;
            if values.as_array().unwrap().iter().any(|v| !v.is_object()) {
                return Err("Invalid Home Chooser subscription entry".into());
            }
            Ok(reply)
        });
        match valid {
            Ok(reply) => {
                let changed = self.snapshot.as_ref() != Some(&reply);
                self.snapshot = Some(reply);
                Ok(changed)
            }
            Err(error) => {
                self.snapshot = None;
                self.ended = true;
                Err(error)
            }
        }
    }
}

pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::AcqRel)
}

fn feed_field(method: &str) -> Option<(&'static str, &'static str, usize)> {
    match method {
        "listApps" => Some(("listApps", "apps", 1000)),
        "listInputs" => Some(("listInputs", "inputs", 128)),
        _ => None,
    }
}

fn feed_reply(method: &'static str, reply: Value) {
    let Some((_, field, limit)) = feed_field(method) else {
        return;
    };
    if let Ok(mut feeds) = FEEDS.lock() {
        let Some(feed) = feeds.as_mut().and_then(|feeds| feeds.get_mut(method)) else {
            return;
        };
        match feed.apply(reply, field, limit) {
            Ok(changed) => {
                if changed {
                    CHANGED.store(true, Ordering::Release);
                }
            }
            Err(error) => {
                eprintln!(
                    "Luma Home Chooser subscription {method}: {error}; reverting to one-shot refreshes"
                );
                CHANGED.store(true, Ordering::Release);
            }
        }
    }
}

fn subscribe(method: &'static str) -> Result<(), String> {
    let uri = format!("{SERVICE}/{method}");
    #[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
    return crate::luna::native_app::subscribe(&uri, move |reply| feed_reply(method, reply));
    #[cfg(not(all(feature = "webos", target_os = "linux", target_arch = "arm")))]
    {
        let _ = uri;
        Err("Home Chooser subscriptions require native webOS".into())
    }
}

fn feed_snapshot(method: &str, version: u64) -> Option<Value> {
    let (method, _, _) = feed_field(method)?;
    if version < 2 {
        return None;
    }
    let start = {
        let mut feeds = FEEDS.lock().ok()?;
        let feeds = feeds.get_or_insert_with(HashMap::new);
        if let Some(feed) = feeds.get(method) {
            return feed.snapshot.clone();
        }
        feeds.insert(method, Feed::default());
        true
    };
    if start && let Err(error) = subscribe(method) {
        feed_reply(method, json!({"returnValue": false, "errorText": error}));
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Unavailable,
    UpdateRequired,
    SetupRequired,
    PermissionRequired,
    Ready { selected: bool },
}

impl Status {
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Unavailable => "homeChooserUnavailable",
            Self::UpdateRequired => "homeChooserUpdate",
            Self::SetupRequired => "homeChooserSetup",
            Self::PermissionRequired => "homeChooserPermission",
            Self::Ready { selected: false } => "homeChooserSelect",
            Self::Ready { selected: true } => "homeChooserReady",
        }
    }
}

pub fn take_status() -> Option<Status> {
    STATUS.try_lock().ok()?.take()
}

pub fn invalidate() {
    INVALIDATE.store(true, Ordering::Release);
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Release);
}

#[derive(Clone, Debug)]
struct Info {
    version: u64,
    selected: bool,
}

fn checked(reply: Value) -> Result<Value, String> {
    if reply.get("returnValue").and_then(Value::as_bool) != Some(true) {
        return Err(format!(
            "{}: {}",
            reply.get("errorCode").unwrap_or(&Value::Null),
            reply
                .get("errorText")
                .and_then(Value::as_str)
                .unwrap_or("call refused")
                .chars()
                .take(240)
                .collect::<String>()
        ));
    }
    Ok(reply)
}

fn inspect(reply: Value) -> (Result<Info, String>, Status) {
    let reply = match checked(reply) {
        Ok(reply) => reply,
        Err(error) => return (Err(error), Status::Unavailable),
    };
    let version = reply
        .get("apiVersion")
        .and_then(Value::as_u64)
        .filter(|n| *n >= 1);
    let Some(version) = version else {
        return (
            Err("Home Chooser API is too old".into()),
            Status::UpdateRequired,
        );
    };
    if reply.get("root").and_then(Value::as_bool) != Some(true) {
        return (
            Err("Open Home Chooser to finish root setup".into()),
            Status::SetupRequired,
        );
    }
    if reply.get("permitted").and_then(Value::as_bool) != Some(true) {
        return (
            Err("Reinstall Luma after Home Chooser permission setup".into()),
            Status::PermissionRequired,
        );
    }
    // Reads do not require isSelectedHome. Never infer mutation authorization
    // from a cached status reply or silently promote a pending uninstall.
    let Some(selected) = reply.get("isSelectedHome").and_then(Value::as_bool) else {
        return (
            Err("Invalid Home Chooser selection status".into()),
            Status::Unavailable,
        );
    };
    (Ok(Info { version, selected }), Status::Ready { selected })
}

#[cfg(test)]
fn info(reply: Value) -> Result<Info, String> {
    inspect(reply).0
}

fn probe() -> Result<Info, String> {
    let (result, status) = match transport("getInfo", &json!({})) {
        Ok(reply) => inspect(reply),
        Err(error) => (Err(error), Status::Unavailable),
    };
    if let Ok(mut queued) = STATUS.lock() {
        *queued = Some(status);
    }
    result
}

pub fn refresh_status() {
    if ENABLED.load(Ordering::Acquire) {
        invalidate();
        let _ = read_info();
    }
}

/// Called only after an explicit owner action. Home Chooser owns confirmation.
pub fn request_payload() -> Value {
    refresh_status();
    let request = INFO
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .as_ref()
                .and_then(|(_, result)| result.as_ref().ok())
                .map(|info| !info.selected)
        })
        .unwrap_or(false);
    chooser_launch(request)
}

fn chooser_launch(request: bool) -> Value {
    if request {
        json!({"id": "org.webosbrew.homechooser", "params": {"request": crate::platform::APP_ID}})
    } else {
        json!({"id": "org.webosbrew.homechooser"})
    }
}

fn transport(method: &str, payload: &Value) -> Result<Value, String> {
    let uri = format!("{SERVICE}/{method}");
    #[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
    return crate::luna::native_app::call(&uri, payload);
    #[cfg(not(all(feature = "webos", target_os = "linux", target_arch = "arm")))]
    {
        let _ = (uri, payload);
        Err("Home Chooser is only available on native webOS builds".into())
    }
}

fn read_info() -> Result<Info, String> {
    // Serialize probes across artwork/platform workers and cache negative
    // detection briefly, so a missing/refused service is not queried per icon.
    let status = {
        let mut cache = INFO.lock().map_err(|_| "Home Chooser status lock failed")?;
        if INVALIDATE.swap(false, Ordering::AcqRel)
            || cache
                .as_ref()
                .is_none_or(|(at, _)| at.elapsed() >= Duration::from_secs(30))
        {
            let result = probe();
            if let Err(error) = &result {
                eprintln!("Luma Home Chooser: {error}; using Homebrew fallback");
            }
            *cache = Some((Instant::now(), result));
        }
        cache
            .as_ref()
            .ok_or("Missing Home Chooser status")?
            .1
            .clone()?
    };
    Ok(status)
}

fn read(method: &str, payload: Value, level: u64) -> Result<Value, String> {
    let status = read_info()?;
    if let Some(snapshot) = feed_snapshot(method, status.version) {
        return Ok(snapshot);
    }
    request(&status, method, payload, level, transport)
}

fn request(
    info: &Info,
    method: &str,
    payload: Value,
    level: u64,
    call: impl FnOnce(&str, &Value) -> Result<Value, String>,
) -> Result<Value, String> {
    if info.version < level {
        return Err(format!("Home Chooser {method} requires API level {level}"));
    }
    checked(call(method, &payload)?)
}

fn optional<T>(read: impl FnOnce() -> Result<T, String>) -> Option<T> {
    if !ENABLED.load(Ordering::Acquire) {
        return None;
    }
    match read() {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!("Luma Home Chooser read: {error}; using Homebrew fallback");
            None
        }
    }
}

fn array(reply: &Value, field: &str, limit: usize) -> Result<Value, String> {
    reply
        .get(field)
        .and_then(Value::as_array)
        .filter(|a| a.len() <= limit)
        .cloned()
        .map(Value::Array)
        .ok_or_else(|| format!("Invalid Home Chooser {field}"))
}

pub fn apps() -> Option<Value> {
    optional(|| array(&read("listApps", json!({}), 1)?, "apps", 1000))
}

fn input_devices(reply: &Value) -> Result<Value, String> {
    let raw = array(reply, "inputs", 128)?;
    // Adapt Home Chooser's normalized fields to the existing EIM parser. Keep
    // unknown signal/connection absent, not fabricated as false. Only canonical
    // HDMI targets are used by that parser, just like the legacy inventory.
    let mut devices = Vec::new();
    for input in raw.as_array().unwrap() {
        if !input.is_object() {
            return Err("Invalid Home Chooser input entry".into());
        }
        let mut device = input.clone();
        device["appId"] = input["id"].clone();
        device["id"] = input["inputId"].clone();
        if let Some(signal) = input.get("signal") {
            device["hdmiSignalExist"] = signal.clone();
        }
        devices.push(device);
    }
    Ok(Value::Array(devices))
}

pub fn inputs() -> Option<Value> {
    optional(|| input_devices(&read("listInputs", json!({}), 2)?))
}

fn icon_bytes(icon: &Value) -> Result<Vec<u8>, String> {
    let length = icon
        .get("byteLength")
        .and_then(Value::as_u64)
        .filter(|n| *n <= crate::app_artwork::MAX_FILE as u64)
        .ok_or("Home Chooser icon exceeds Luma's size limit")?;
    let mime = icon
        .get("mimeType")
        .and_then(Value::as_str)
        .filter(|mime| matches!(*mime, "image/png" | "image/jpeg" | "image/webp"))
        .ok_or("Unsupported Home Chooser icon MIME type")?;
    let prefix = format!("data:{mime};base64,");
    let data = icon
        .get("dataUrl")
        .and_then(Value::as_str)
        .and_then(|s| s.strip_prefix(&prefix))
        .filter(|s| s.len() <= crate::app_artwork::MAX_FILE.div_ceil(3) * 4)
        .ok_or("Invalid Home Chooser icon data URL")?;
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| "Invalid Home Chooser icon base64")?;
    if bytes.len() as u64 != length {
        return Err("Home Chooser icon length mismatch".into());
    }
    Ok(bytes)
}

fn parse_icons(reply: &Value, count: usize) -> Result<Vec<Value>, String> {
    let icons = array(reply, "icons", count)?;
    let icons = icons.as_array().unwrap();
    if icons.len() != count {
        return Err("Incomplete Home Chooser icon batch".into());
    }
    Ok(icons.clone())
}

fn icons_batch_with(
    ids: &[&str],
    mut read: impl FnMut(Value) -> Result<Value, String>,
) -> Result<IconBatch, String> {
    if ids.iter().any(|id| !crate::model::valid_app_id(id)) {
        return Err("Invalid Home Chooser icon app ID".into());
    }
    let mut result = Vec::new();
    for chunk in ids.chunks(32) {
        let mut pending: Vec<_> = chunk
            .iter()
            .flat_map(|id| {
                [
                    json!({"id": id, "size": "largeIcon"}),
                    json!({"id": id, "size": "icon"}),
                ]
            })
            .enumerate()
            .collect();
        let mut decoded: Vec<Result<Option<Vec<u8>>, String>> = vec![Ok(None); pending.len()];
        while !pending.is_empty() {
            let items: Vec<_> = pending.iter().map(|(_, item)| item.clone()).collect();
            let icons = parse_icons(&read(json!({"items": items}))?, pending.len())?;
            let count = pending.len();
            let mut deferred = Vec::new();
            for ((slot, item), icon) in pending.into_iter().zip(icons) {
                decoded[slot] = match icon.get("errorCode").and_then(Value::as_str) {
                    Some("overBudget") => {
                        deferred.push((slot, item));
                        continue;
                    }
                    Some("noIcon" | "notFound" | "iconTooLarge") => Ok(None),
                    Some(code) => {
                        eprintln!("Luma Home Chooser icon {:?}: {code}", item);
                        Err(format!("Home Chooser icon: {code}"))
                    }
                    None => icon_bytes(&icon).map(Some),
                };
            }
            if deferred.len() == count {
                return Err("Home Chooser icon budget made no progress".into());
            }
            pending = deferred;
        }
        for pair in decoded.chunks(2) {
            let bytes: Vec<_> = pair
                .iter()
                .filter_map(|icon| icon.as_ref().ok()?.clone())
                .collect();
            if bytes.is_empty()
                && let Some(error) = pair.iter().find_map(|icon| icon.as_ref().err())
            {
                result.push(Err(error.clone()));
            } else {
                result.push(Ok(bytes));
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
fn icons_with(
    id: &str,
    read: impl FnMut(Value) -> Result<Value, String>,
) -> Result<Vec<Vec<u8>>, String> {
    icons_batch_with(&[id], read)?.remove(0)
}

pub fn icons_batch(ids: &[&str]) -> Option<IconBatch> {
    optional(|| icons_batch_with(ids, |payload| read("getIcons", payload, 2)))
}

#[cfg(test)]
#[path = "home-chooser-tests.rs"]
mod tests;
