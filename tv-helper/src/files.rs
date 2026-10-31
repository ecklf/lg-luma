use crate::{
    APP, ROOTS, Result,
    system::{self, System},
    valid_app_id, valid_background,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
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
pub fn sync_splash(system: &impl System, directory: &Path, background: &str) -> Result<Value> {
    installed(system, directory)?;
    if !valid_background(background) {
        return Err("Invalid splash background".into());
    }
    let image = system::read(
        system,
        directory.join(format!("splash/{background}.png")),
        32 * 1024 * 1024,
    )?;
    replace(system, &directory.join("splash.png"), &image)?;
    let config_path = Path::new("/var/lib/luma-home/state.json");
    if system::exists(system, config_path) {
        let config = system::json(system, config_path)?;
        if config["enabled"] == true
            && config["method"] != "executable"
            && config["appDir"].as_str() == directory.to_str()
        {
            replace(
                system,
                Path::new("/var/lib/luma-home/app/splash.png"),
                &image,
            )?;
        }
    }
    Ok(json!({"returnValue":true,"background":background}))
}
fn replace(system: &impl System, path: &Path, image: &[u8]) -> Result<()> {
    if system::read(system, path, 32 * 1024 * 1024).is_ok_and(|bytes| bytes == image) {
        return Ok(());
    }
    system::write(system, path, image, 0o644)
}
/// Select the same app-owned storage precedence as native preferences; never follow a storage
/// directory symlink while running as root. Older native builds also saved splash_background.
pub(crate) fn stage_splash(system: &impl System, directory: &Path) -> Result<()> {
    let mut candidates = vec![
        format!("/media/developer/{APP}/preferences.json"),
        format!("/media/developer/temp/{APP}/preferences.json"),
        format!("/media/internal/.{APP}/preferences.json"),
    ];
    if directory.starts_with("/media/cryptofs") {
        candidates.rotate_right(1);
    }
    let mut ranked = vec![];
    for (index, path) in candidates.into_iter().enumerate() {
        let parent = Path::new(&path).parent().unwrap();
        if let Ok(meta) = fs::symlink_metadata(system.path(parent))
            && meta.is_dir()
            && system::exists(system, &path)
        {
            #[cfg(unix)]
            let root_owned = {
                use std::os::unix::fs::MetadataExt;
                meta.uid() == 0
            };
            #[cfg(not(unix))]
            let root_owned = false;
            ranked.push((root_owned, index, path));
        }
    }
    ranked.sort_by_key(|(root, index, _)| (*root, *index));
    let mut candidates: Vec<_> = ranked
        .into_iter()
        .map(|(_, _, path)| PathBuf::from(path))
        .collect();
    candidates.push(directory.join("preferences.json"));
    let background = candidates
        .iter()
        .find_map(|path| {
            system::json(system, path).ok().and_then(|value| {
                value
                    .get("splash_background")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| value.get("background").and_then(Value::as_str))
                    .filter(|s| valid_background(s))
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "dunes".into());
    let path = directory.join(format!("splash/{background}.png"));
    let path = if system::exists(system, &path) {
        path
    } else {
        directory.join("splash.png")
    };
    let bytes = system::read(system, path, 32 * 1024 * 1024)?;
    system::write(system, "/var/lib/luma-home/app/splash.png", &bytes, 0o644)
}
