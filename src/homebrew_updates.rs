//! Opt-in automatic updates of installed Homebrew apps (Settings → System). Luma compares the
//! apps Homebrew Channel installed with Homebrew Channel's own repository, downloads a newer
//! package with its repository checksum and hands it to `luma-helper app-update`, which checks it
//! again as root and installs it only while the app is closed. Luma and Homebrew Channel update
//! themselves and are never touched here.
use crate::update::{self, Release};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
#[path = "homebrew-updates-tests.rs"]
mod tests;

pub const REPO: &str = "https://repo.webosbrew.org/api/apps.json";
const REPO_LIMIT: u64 = 4 * 1024 * 1024;
/// Where Homebrew Channel installs apps.
pub const APPS: &str = luma_tv_helper::app_update::APPS;
/// Leaves the TV alone right after a boot, then checks twice a day.
const FIRST_CHECK: Duration = Duration::from_secs(10 * 60);
const INTERVAL: Duration = Duration::from_secs(12 * 3600);

/// A newer package of an installed Homebrew app.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub id: String,
    pub title: String,
    pub release: Release,
}

fn valid_package(release: &Release) -> bool {
    let file = release.url.rsplit('/').next().unwrap_or_default();
    release.url.starts_with("https://")
        && file.ends_with(".ipk")
        && !release.url.contains(['?', '#', '\\', ' '])
        && !release.url.contains("..")
        && luma_tv_helper::app_update::valid_version(&release.version)
        && release.sha256.len() == 64
        && release.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        && release.size > 0
        && release.size <= update::MAX_IPK
}

/// The repository's packages by app id, with title. Packages with incomplete or unsafe
/// metadata are skipped rather than failing the whole check.
pub fn parse_repo(feed: &Value) -> Result<BTreeMap<String, (String, Release)>, String> {
    let packages = feed["packages"]
        .as_array()
        .ok_or("The Homebrew repository has no packages")?;
    let mut found = BTreeMap::new();
    for package in packages {
        let manifest = &package["manifest"];
        let Some(id) = package["id"].as_str() else {
            continue;
        };
        if manifest["id"] != id || !luma_tv_helper::valid_app_id(id) {
            continue;
        }
        let text = |key: &str| manifest[key].as_str().unwrap_or_default().to_owned();
        let release = Release {
            version: text("version"),
            url: text("ipkUrl"),
            sha256: manifest["ipkHash"]["sha256"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase(),
            size: manifest["ipkSize"].as_u64().unwrap_or_default(),
        };
        if valid_package(&release) {
            let title = package["title"].as_str().unwrap_or(id).to_owned();
            found.insert(id.to_owned(), (title, release));
        }
    }
    Ok(found)
}

/// Installed Homebrew apps with a newer repository version, never Luma or Homebrew Channel.
pub fn due(
    installed: &BTreeMap<String, String>,
    repo: &BTreeMap<String, (String, Release)>,
) -> Vec<Update> {
    installed
        .iter()
        .filter(|(id, _)| !luma_tv_helper::app_update::EXCLUDED.contains(&id.as_str()))
        .filter_map(|(id, version)| {
            let (title, release) = repo.get(id)?;
            update::newer(version, &release.version).then(|| Update {
                id: id.clone(),
                title: title.clone(),
                release: release.clone(),
            })
        })
        .collect()
}

/// Homebrew apps and their versions, read from their installed manifests.
pub fn installed(root: &Path) -> BTreeMap<String, String> {
    let Ok(entries) = fs::read_dir(root) else {
        return BTreeMap::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let manifest: Value = serde_json::from_slice(
                &fs::read(entry.path().join("appinfo.json"))
                    .ok()
                    .filter(|bytes| bytes.len() < 64 * 1024)?,
            )
            .ok()?;
            let id = manifest["id"].as_str()?;
            (entry.file_name().to_str() == Some(id)).then(|| {
                (
                    id.to_owned(),
                    manifest["version"].as_str().unwrap_or_default().to_owned(),
                )
            })
        })
        .collect()
}

fn fetch_repo(agent: &ureq::Agent) -> Result<BTreeMap<String, (String, Release)>, String> {
    let mut response = agent
        .get(REPO)
        .header("User-Agent", "LumaNative")
        .call()
        .map_err(|_| "The Homebrew repository is unavailable")?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(REPO_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The Homebrew repository could not be read")?;
    if bytes.len() as u64 > REPO_LIMIT {
        return Err("The Homebrew repository list is too large".into());
    }
    parse_repo(&serde_json::from_slice(&bytes).map_err(|e| e.to_string())?)
}

pub enum Event {
    /// An app was updated; its title.
    Updated(String),
}

pub struct Worker {
    enabled: Arc<AtomicBool>,
    pub receiver: Receiver<Event>,
}

impl Worker {
    /// Starts the background checks; `active` is false outside TV runs, where nothing is checked.
    pub fn new(active: bool) -> Self {
        let enabled = Arc::new(AtomicBool::new(false));
        let (events, receiver) = mpsc::channel();
        if active {
            let flag = enabled.clone();
            if let Err(error) = thread::Builder::new()
                .name("luma-homebrew-updates".into())
                .spawn(move || run(&flag, &events))
            {
                eprintln!("Homebrew updates: {error}");
            }
        }
        Self { enabled, receiver }
    }

    /// Follows the setting; the first check waits until ten minutes after Luma started.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

fn run(enabled: &AtomicBool, events: &Sender<Event>) {
    let agent = update::agent();
    let mut next = Instant::now() + FIRST_CHECK;
    loop {
        thread::sleep(Duration::from_secs(60));
        // While the setting is off nothing is scheduled: turning it on checks within a minute.
        if Instant::now() < next || !enabled.load(Ordering::Relaxed) {
            continue;
        }
        next = Instant::now() + INTERVAL;
        match fetch_repo(&agent) {
            Ok(repo) => {
                for update in due(&installed(Path::new(APPS)), &repo) {
                    match install(&agent, &update) {
                        Ok(()) => {
                            eprintln!("Homebrew updates: {} {}", update.id, update.release.version);
                            if events.send(Event::Updated(update.title)).is_err() {
                                return;
                            }
                        }
                        // A running app is retried at the next check.
                        Err(error) => eprintln!("Homebrew updates: {}: {error}", update.id),
                    }
                }
            }
            Err(error) => eprintln!("Homebrew updates: {error}"),
        }
    }
}

fn install(agent: &ureq::Agent, update: &Update) -> Result<(), String> {
    let (_, directory) = update::download_directory()?;
    let ipk = directory.join("package.ipk");
    let result = update::download(agent, &update.release, |_| {}, &ipk).and_then(|()| {
        crate::platform::update_app(
            &update.id,
            &update.release.version,
            &update.release.sha256,
            &ipk,
        )
    });
    let _ = fs::remove_dir_all(&directory);
    result
}
