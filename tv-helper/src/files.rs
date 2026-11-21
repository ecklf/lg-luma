use crate::{
    APP, ROOTS, Result,
    system::{self, System},
    valid_app_id,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

fn resolve(directory: &Path, icon: &str) -> Option<PathBuf> {
    if icon.is_empty() || icon.len() > 4096 || icon.chars().any(char::is_control) {
        return None;
    }
    let icon = icon.strip_prefix("file://").unwrap_or(icon);
    if icon.contains("://") {
        return None;
    }
    let path = Path::new(icon);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    let path = if path.is_absolute() {
        path.into()
    } else {
        directory.join(path)
    };
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    (ROOTS.iter().any(|root| path.starts_with(root))
        && matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp"))
    .then_some(path)
}
#[derive(Deserialize)]
pub struct IconSource {
    pub id: String,
    pub directory: Option<String>,
    pub large_icon: Option<String>,
    pub icon: Option<String>,
}
pub fn read_icons(system: &impl System, source: &IconSource) -> Result<Value> {
    if !valid_app_id(&source.id) {
        return Err("Invalid application ID".into());
    }
    let mut directories: Vec<_> = ROOTS
        .iter()
        .map(|root| Path::new(root).join(&source.id))
        .collect();
    if let Some(directory) = &source.directory {
        let p = PathBuf::from(directory);
        if ROOTS.iter().any(|root| p.parent() == Some(Path::new(root)))
            && !p.components().any(|c| matches!(c, Component::ParentDir))
        {
            directories.retain(|old| old != &p);
            directories.insert(0, p);
        }
    }
    let mut remaining = 512 * 1024;
    let mut seen = HashSet::new();
    let mut icons = vec![];
    for directory in directories {
        let manifest = system::read(system, directory.join("appinfo.json"), 64 * 1024)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .filter(|m| m["id"] == source.id)
            .unwrap_or(Value::Null);
        for icon in [
            source.large_icon.as_deref(),
            manifest["largeIcon"].as_str(),
            source.icon.as_deref(),
            manifest["icon"].as_str(),
            Some("icon.png"),
        ]
        .into_iter()
        .flatten()
        {
            let Some(path) = resolve(&directory, icon) else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            if let Ok(bytes) = system::read(system, &path, remaining) {
                if bytes.is_empty() {
                    continue;
                }
                remaining -= bytes.len();
                let alphabet = b"0123456789abcdef";
                let mut hex = String::with_capacity(bytes.len() * 2);
                for byte in bytes {
                    hex.push(alphabet[(byte >> 4) as usize] as char);
                    hex.push(alphabet[(byte & 15) as usize] as char);
                }
                icons.push(hex);
            }
        }
    }
    Ok(json!({"returnValue":true,"icons":icons}))
}
fn installed(system: &impl System, directory: &Path) -> Result<Value> {
    if !ROOTS[..3]
        .iter()
        .any(|root| directory == Path::new(root).join(APP))
    {
        return Err("Invalid Luma installation directory".into());
    }
    let app = system::json(system, directory.join("appinfo.json"))?;
    if app["id"] != APP || app["type"] != "native" || app["main"] != "luma" {
        return Err("Requires an installed native Luma package".into());
    }
    Ok(app)
}
/// Stage the package's static launch screen without consulting user preferences.
pub(crate) fn stage_splash(system: &impl System, directory: &Path) -> Result<()> {
    installed(system, directory)?;
    let bytes = system::read(system, directory.join("splash.png"), 32 * 1024 * 1024)?;
    system::write(system, "/var/lib/luma-home/app/splash.png", &bytes, 0o644)
}
