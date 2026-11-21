//! Installs an update of an installed Homebrew app that Luma downloaded from Homebrew Channel's
//! repository (opt-in setting). Everything Luma checked is checked again here as root: the app is
//! a Homebrew installation, it is not running, and the package matches its repository checksum.
use crate::{
    APP, Result, accepted,
    system::{self, System},
    valid_app_id,
};
use serde_json::{Value, json};
use std::{io::Read, path::Path};

/// Homebrew Channel installs apps here, as developer-mode packages.
pub const APPS: &str = "/media/developer/apps/usr/palm/applications";
/// Updated by their own mechanisms: Luma's self-update and Homebrew Channel's updater.
pub const EXCLUDED: [&str; 2] = [APP, "org.webosbrew.hbchannel"];
const WORK: &str = "/var/lib/luma-update/apps";
const MAX_IPK: u64 = 96 * 1024 * 1024;

pub fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn valid_sha256(sha: &str) -> bool {
    sha.len() == 64
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The version of an installed Homebrew app, or an error if it is not one.
pub fn installed_version(system: &impl System, id: &str) -> Result<String> {
    let manifest = system::json(system, format!("{APPS}/{id}/appinfo.json"))?;
    if manifest["id"] != id {
        return Err("Not an installed Homebrew app".into());
    }
    manifest["version"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Installed app has no version".into())
}

fn running(system: &impl System, id: &str) -> Result<bool> {
    let reply = accepted(system::luna(
        system,
        "com.webos.applicationManager",
        "running",
        &json!({}),
    )?)?;
    Ok(reply["running"]
        .as_array()
        .is_some_and(|apps| apps.iter().any(|app| app["id"] == id)))
}

fn sha256(system: &impl System, path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(system.path(path)).map_err(|e| e.to_string())?;
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_IPK {
            return Err("Package too large".into());
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Installs `ipk` as `version` of the Homebrew app `id`. The package is first copied into a
/// root-owned directory, so nothing can change it between the checksum check and LG's installer.
pub fn install(
    system: &impl System,
    id: &str,
    version: &str,
    sha: &str,
    ipk: &Path,
) -> Result<Value> {
    system::root(system)?;
    if !valid_app_id(id) || EXCLUDED.contains(&id) {
        return Err("This app is not updated by Luma".into());
    }
    if !valid_version(version) || !valid_sha256(sha) || !ipk.is_absolute() {
        return Err("Invalid update request".into());
    }
    let current = installed_version(system, id)?;
    if current == version {
        return Ok(json!({"returnValue": true, "version": version, "changed": false}));
    }
    if running(system, id)? {
        return Err("The app is running; the update waits until it is closed".into());
    }
    system::mkdir(system, "/var/lib/luma-update")?;
    system::mkdir(system, WORK)?;
    let copy = Path::new(WORK).join(format!("{id}.ipk"));
    let result = (|| {
        let bytes = system::read(system, ipk, MAX_IPK as usize)?;
        system::write(system, &copy, &bytes, 0o600)?;
        if sha256(system, &copy)? != sha {
            return Err("The package does not match its repository checksum".into());
        }
        // Checked once more right before installing: the user may have opened the app meanwhile.
        if running(system, id)? {
            return Err("The app is running; the update waits until it is closed".into());
        }
        accepted(system::luna(
            system,
            "com.webos.appInstallService",
            "dev/install",
            &json!({"id": id, "ipkUrl": copy.to_string_lossy(), "subscribe": false}),
        )?)?;
        // LG's installer answers before it finishes; the installed manifest is the proof.
        for _ in 0..240 {
            if installed_version(system, id).is_ok_and(|v| v == version) {
                return Ok(());
            }
            system.pause();
        }
        Err(format!("{id} was not updated to {version} in time"))
    })();
    let _ = system::remove(system, &copy);
    result?;
    Ok(json!({"returnValue": true, "version": version, "changed": true}))
}
