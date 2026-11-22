//! Phase-two Home Chooser integration: privileged reads through the launcher's
//! real app-named handle. Mutations keep their existing confirmation semantics.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const SERVICE: &str = "luna://org.webosbrew.homechooser.service";
static ENABLED: AtomicBool = AtomicBool::new(false);
static INFO: Mutex<Option<(Instant, Result<Info, String>)>> = Mutex::new(None);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Release);
}

#[derive(Clone, Debug)]
struct Info {
    version: u64,
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

fn info(reply: Value) -> Result<Info, String> {
    let reply = checked(reply)?;
    let version = reply
        .get("apiVersion")
        .and_then(Value::as_u64)
        .filter(|n| *n >= 1)
        .ok_or("Home Chooser API is too old")?;
    if reply.get("root").and_then(Value::as_bool) != Some(true) {
        return Err("Open Home Chooser to finish root setup".into());
    }
    if reply.get("permitted").and_then(Value::as_bool) != Some(true) {
        return Err("Reinstall Luma after Home Chooser permission setup".into());
    }
    // Reads do not require isSelectedHome. Never infer mutation authorization
    // from a cached status reply or silently promote a pending uninstall.
    Ok(Info { version })
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

fn read(method: &str, payload: Value, level: u64) -> Result<Value, String> {
    // Serialize probes across artwork/platform workers and cache negative
    // detection briefly, so a missing/refused service is not queried per icon.
    let status = {
        let mut cache = INFO.lock().map_err(|_| "Home Chooser status lock failed")?;
        if cache
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() >= Duration::from_secs(30))
        {
            let result = transport("getInfo", &json!({})).and_then(info);
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

fn icons_with(
    id: &str,
    mut read: impl FnMut(Value) -> Result<Value, String>,
) -> Result<Vec<Vec<u8>>, String> {
    if !crate::model::valid_app_id(id) {
        return Err("Invalid Home Chooser icon app ID".into());
    }
    // Request both candidates in one batch, retaining the existing large-first
    // preference and the artwork worker's bounded, version-keyed cache.
    let items = [
        json!({"id": id, "size": "largeIcon"}),
        json!({"id": id, "size": "icon"}),
    ];
    let icons = parse_icons(&read(json!({"items": items}))?, items.len())?;
    let mut result = Vec::new();
    for (index, mut icon) in icons.into_iter().enumerate() {
        if icon.get("errorCode").and_then(Value::as_str) == Some("overBudget") {
            icon = parse_icons(&read(json!({"items": [items[index]]}))?, 1)?.remove(0);
        }
        match icon.get("errorCode").and_then(Value::as_str) {
            Some("noIcon" | "notFound" | "iconTooLarge") => continue,
            Some(code) => return Err(format!("Home Chooser icon: {code}")),
            None => result.push(icon_bytes(&icon)?),
        }
    }
    Ok(result)
}

pub fn icons(id: &str) -> Option<Vec<Vec<u8>>> {
    optional(|| icons_with(id, |payload| read("getIcons", payload, 2)))
}

#[cfg(test)]
#[path = "home-chooser-tests.rs"]
mod tests;
