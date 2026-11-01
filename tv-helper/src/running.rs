//! Running-card operations for the already-root Home, without a JS service startup.
use crate::{ROOTS, Result, accepted, system::System, valid_app_id};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, sync::Mutex, time::Duration};

#[derive(Default)]
pub struct Usage {
    used: BTreeMap<String, String>,
}

impl Usage {
    pub fn observe(&mut self, event: &Value) {
        let Some(id) = event["appId"].as_str().filter(|id| valid_app_id(id)) else {
            return;
        };
        let status = event["status"].as_str().unwrap_or_default();
        if matches!(status, "stop" | "stopped" | "closed") || event["backgroundStatus"] == "preload"
        {
            self.used.remove(id);
        } else if matches!(status, "foreground" | "relaunching")
            && event["reason"] != "preload"
            && let Some(pid) = event["processId"]
                .as_str()
                .filter(|s| s.parse::<u32>().is_ok())
        {
            self.used.insert(id.into(), pid.into());
        }
    }

    fn visible(&self, id: &str, pid: &str, preloaded: bool) -> bool {
        !preloaded || self.used.get(id).is_some_and(|used| used == pid)
    }
}

static USAGE: Mutex<Usage> = Mutex::new(Usage {
    used: BTreeMap::new(),
});

pub fn observe_usage(event: &Value) {
    if let Ok(mut usage) = USAGE.lock() {
        usage.observe(event);
    }
}

fn preloaded(system: &impl System, pid: &str) -> bool {
    fs::read(system.path(Path::new(&format!("/proc/{pid}/cmdline"))))
        .unwrap_or_default()
        .split(|b| *b == 0)
        .filter_map(|arg| serde_json::from_slice::<Value>(arg).ok())
        .any(|arg| arg["reason"] == "preload" || arg.get("preload").is_some())
}

fn call(system: &impl System, method: &str, payload: Value) -> Result<Value> {
    let output = system.run(
        "/usr/bin/luna-send",
        &[
            "-n".into(),
            "1".into(),
            "-w".into(),
            "4000".into(),
            format!("luna://com.webos.applicationManager/{method}"),
            payload.to_string(),
        ],
        Duration::from_secs(5),
        1024 * 1024,
    )?;
    if !output.success {
        return Err("Running-app request failed".into());
    }
    accepted(serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?)
}

pub fn eligible(row: &Value) -> bool {
    let Some(id) = row["id"].as_str() else {
        return false;
    };
    valid_app_id(id)
        && row["defaultWindowType"] == "card"
        && !matches!(
            id,
            "com.webos.app.home" | "com.webos.app.livetv" | "com.ecklf.luma" | "airplay"
        )
        && !id.starts_with("com.webos.app.hdmi")
        && !id.starts_with("com.luma.")
        && row["processid"]
            .as_str()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|pid| pid > 1)
}

fn rows(reply: &Value) -> Result<&Vec<Value>> {
    reply["running"]
        .as_array()
        .ok_or_else(|| "Invalid running-app reply".into())
}

fn identity(system: &impl System, pid: u32) -> Option<String> {
    let stat = fs::read_to_string(system.path(Path::new(&format!("/proc/{pid}/stat")))).ok()?;
    let fields = stat.rsplit_once(") ")?.1;
    if matches!(fields.split_whitespace().next(), Some("Z" | "X")) {
        return None;
    }
    // comm may contain spaces or parentheses; fields after its final ')' start at field 3.
    fields.split_whitespace().nth(19).map(str::to_owned)
}

pub fn list(system: &impl System) -> Result<Value> {
    crate::system::root(system)?;
    let reply = call(system, "running", json!({}))?;
    let entries: Vec<Value> = rows(&reply)?
        .iter()
        .rev()
        .filter(|r| eligible(r))
        .filter(|r| {
            r["processid"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .and_then(|pid| identity(system, pid))
                .is_some()
        })
        .filter(|r| {
            let id = r["id"].as_str().unwrap();
            let pid = r["processid"].as_str().unwrap();
            USAGE
                .lock()
                .map(|usage| usage.visible(id, pid, preloaded(system, pid)))
                .unwrap_or(false)
        })
        .map(|row| {
            let id = row["id"].as_str().unwrap();
            let title = ROOTS
                .iter()
                .find_map(|root| {
                    let bytes =
                        fs::read(system.path(Path::new(&format!("{root}/{id}/appinfo.json"))))
                            .ok()?;
                    let manifest: Value = serde_json::from_slice(&bytes).ok()?;
                    manifest["title"].as_str().map(str::to_owned)
                })
                .unwrap_or_else(|| id.into());
            json!({"appId":id,"title":title})
        })
        .collect();
    Ok(json!({"returnValue":true,"ready":true,"recentsAppList":entries}))
}

pub fn close(system: &impl System, id: &str) -> Result<Value> {
    crate::system::root(system)?;
    if !valid_app_id(id) {
        return Err("Invalid application identifier".into());
    }
    let reply = call(system, "running", json!({}))?;
    let running = rows(&reply)?;
    // A card can outlive its process between displaying it and pressing Up.
    // Closing an already-absent application is a successful no-op.
    if !running.iter().any(|r| r["id"] == id) {
        return Ok(json!({"returnValue":true}));
    }
    let row = running
        .iter()
        .find(|r| r["id"] == id && eligible(r))
        .ok_or("Application is not in the closable running list")?;
    let pid = row["processid"]
        .as_str()
        .unwrap()
        .parse::<u32>()
        .map_err(|e| e.to_string())?;
    let shared = running
        .iter()
        .any(|r| r["id"] != id && r["processid"] == row["processid"]);
    let before = identity(system, pid);
    call(system, "closeByAppId", json!({"id":id}))?;
    if let Ok(mut usage) = USAGE.lock() {
        usage.used.remove(id);
    }
    if !shared && before.is_some() && identity(system, pid) == before {
        system.terminate(pid)?;
        // Native apps exit asynchronously. Bound the reconciliation window so
        // the immediate refresh doesn't resurrect a process we just closed.
        for _ in 0..30 {
            if identity(system, pid) != before {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(json!({"returnValue":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preloaded_apps_only_appear_after_real_use_and_not_after_restart() {
        let mut usage = Usage::default();
        let id = "youtube.leanback.v4";
        assert!(!usage.visible(id, "123", true));
        usage.observe(
            &json!({"appId":id,"processId":"123","status":"relaunching","reason":"preload"}),
        );
        assert!(!usage.visible(id, "123", true));
        usage.observe(&json!({"appId":id,"processId":"123","status":"foreground"}));
        assert!(usage.visible(id, "123", true));
        assert!(!usage.visible(id, "124", true));
        usage.observe(&json!({"appId":id,"processId":"123","status":"background","backgroundStatus":"normal"}));
        assert!(usage.visible(id, "123", true));
        usage.observe(&json!({"appId":id,"processId":"123","status":"closed"}));
        assert!(!usage.visible(id, "123", true));
        assert!(usage.visible("other.tv", "555", false));
    }
    #[test]
    fn running_cards_exclude_system_inputs_and_invalid_pids() {
        let mut row = json!({"id":"example.tv","defaultWindowType":"card","processid":"123"});
        assert!(eligible(&row));
        for id in [
            "com.webos.app.home",
            "com.webos.app.hdmi3",
            "com.webos.app.livetv",
            "com.ecklf.luma",
            "com.luma.native",
            "airplay",
            "bad;id",
        ] {
            row["id"] = json!(id);
            assert!(!eligible(&row), "{id}");
        }
        row["id"] = json!("example.tv");
        for pid in ["0", "1", "-5", "x"] {
            row["processid"] = json!(pid);
            assert!(!eligible(&row));
        }
        row["processid"] = json!("123");
        row["defaultWindowType"] = json!("overlay");
        assert!(!eligible(&row));
    }
}
