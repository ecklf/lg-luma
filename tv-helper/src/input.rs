//! Optional TV-input helper compatible with PR #124; never implicitly registers an input.
use crate::{
    APP, Result, accepted,
    system::{self, System},
};
use serde_json::{Value, json};
use std::path::Path;
fn eim(system: &impl System, method: &str, payload: Value) -> Result<Value> {
    accepted(system::luna(
        system,
        "com.webos.service.eim",
        method,
        &payload,
    )?)
}
fn listed(system: &impl System) -> Result<bool> {
    let reply = eim(system, "getAllInputStatus", json!({}))?;
    Ok(reply["devices"]
        .as_array()
        .ok_or("Invalid input inventory")?
        .iter()
        .any(|d| d["appId"] == APP))
}
pub fn configure(system: &impl System, command: &str, directory: &Path) -> Result<Value> {
    system::root(system)?;
    match command {
        "status" => return Ok(json!({"returnValue":true,"registered":listed(system)?})),
        "enable" => {
            let app = system::json(system, directory.join("appinfo.json"))?;
            if app["id"] != APP || app["supportGIP"] != true {
                return Err("Install the Luma package with supportGIP first".into());
            }
            if listed(system)? {
                return Ok(json!({"returnValue":true,"registered":true}));
            }
            let result=eim(system,"addDevice",json!({"appId":APP,"pigImage":"","mvpdIcon":"","label":"Luma","description":"Luma Home"})).and_then(|_|if listed(system)?{Ok(())}else{Err("This firmware did not list Luma as an input".into())});
            if let Err(error) = result {
                return match eim(system, "deleteDevice", json!({"appId":APP})) {
                    Ok(_) => Err(format!("{error}; registration removed")),
                    Err(cleanup) => {
                        Err(format!("{error}; cleanup failed: {cleanup}; retry disable"))
                    }
                };
            }
        }
        "disable" => {
            eim(system, "deleteDevice", json!({"appId":APP}))?;
            if listed(system)? {
                return Err("Luma is still registered as an input".into());
            }
        }
        _ => return Err("Unknown TV-input command".into()),
    }
    Ok(json!({"returnValue":true,"registered":command=="enable"}))
}
