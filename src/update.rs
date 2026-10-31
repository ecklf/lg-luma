pub mod discovery {
    use serde::{Deserialize, Serialize};
    use std::{io::Read, sync::mpsc, thread, time::Duration};

    const FEED: &str = "https://github.com/ecklf/lg-luma/releases/latest/download/repo.json";
    const LIMIT: usize = 256 * 1024;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Status {
        #[default]
        Unchecked,
        Checking,
        Checked,
        Failed,
    }

    pub fn public_checks_enabled(scripted: bool, simulated: bool) -> bool {
        !scripted && !simulated
    }

    pub struct Service {
        pub receiver: mpsc::Receiver<Result<Option<String>, String>>,
        sender: mpsc::Sender<()>,
    }

    impl Service {
        /// Interactive desktop and TV runs share the feed; scripted/simulated previews stay offline.
        pub fn new(enabled: bool) -> Self {
            let (sender, receiver) = mpsc::channel();
            let (command_tx, command_rx) = mpsc::channel();
            thread::spawn(move || {
                let agent = ureq::Agent::config_builder()
                    .timeout_global(Some(Duration::from_secs(10)))
                    .max_redirects(5)
                    .https_only(true)
                    .proxy(None)
                    .build()
                    .new_agent();
                while command_rx.recv().is_ok() {
                    let result = if enabled {
                        fetch(&agent, FEED, env!("CARGO_PKG_VERSION"))
                    } else {
                        Err(
                            "Public release checks are disabled for scripted/simulated previews"
                                .into(),
                        )
                    };
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            });
            let service = Self {
                receiver,
                sender: command_tx,
            };
            if enabled {
                service.check();
            }
            service
        }

        pub fn check(&self) {
            let _ = self.sender.send(());
        }
    }

    fn fetch(agent: &ureq::Agent, url: &str, current: &str) -> Result<Option<String>, String> {
        let mut response = agent
            .get(url)
            .header("Accept", "application/json")
            .header("User-Agent", "LumaNative")
            .call()
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > LIMIT {
            return Err("Release feed is too large".into());
        }
        newer_version(&bytes, current)
    }

    #[derive(Deserialize)]
    struct Feed {
        packages: Vec<Package>,
    }
    #[derive(Deserialize)]
    struct Package {
        id: String,
        manifest: Manifest,
    }
    #[derive(Deserialize)]
    struct Manifest {
        id: String,
        version: String,
    }

    /// Shipping releases use strict three-part numeric versions, never lexical comparison.
    pub(super) fn version(value: &str) -> Option<[u64; 3]> {
        if value.len() > 32 {
            return None;
        }
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        let mut result = [0; 3];
        for (index, part) in parts.into_iter().enumerate() {
            if part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            result[index] = part.parse().ok()?;
        }
        Some(result)
    }

    pub fn newer_version(bytes: &[u8], current: &str) -> Result<Option<String>, String> {
        let current = version(current).ok_or("Invalid installed version")?;
        let feed: Feed = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let mut matching = feed
            .packages
            .into_iter()
            .filter(|package| package.id == "com.ecklf.luma");
        let package = matching
            .next()
            .ok_or("Luma is missing from the release feed")?;
        if matching.next().is_some() || package.manifest.id != "com.ecklf.luma" {
            return Err("Ambiguous Luma release metadata".into());
        }
        let latest = version(&package.manifest.version).ok_or("Invalid release version")?;
        Ok((latest > current).then_some(package.manifest.version))
    }

    pub fn validate_preview(latest: &str, current: &str) -> Result<(), String> {
        let current = version(current).ok_or("Invalid installed version")?;
        let latest = version(latest).ok_or("Update preview requires a version such as 99.0.0")?;
        if latest < current {
            return Err(
                "Update preview version must match or be newer than the installed version".into(),
            );
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn interactive_checks_are_live_but_scripts_and_simulations_stay_offline() {
            assert!(public_checks_enabled(false, false));
            assert!(!public_checks_enabled(true, false));
            assert!(!public_checks_enabled(false, true));
            assert!(!public_checks_enabled(true, true));
        }
        #[test]
        fn preview_requires_a_valid_current_or_newer_version() {
            assert!(validate_preview("99.0.0", "2.13.0").is_ok());
            assert!(validate_preview("2.13.0", "2.13.0").is_ok());
            for value in ["", "garbage", "2.12.0", "3.0.0-beta", "https://example.com"] {
                assert!(validate_preview(value, "2.13.0").is_err());
            }
        }
        fn feed(value: &str) -> Vec<u8> {
            serde_json::to_vec(&serde_json::json!({"packages":[{"id":"com.ecklf.luma","manifest":{"id":"com.ecklf.luma","version":value}}]})).unwrap()
        }
        #[test]
        fn compares_numeric_versions_and_never_offers_equal_or_older_releases() {
            for (current, latest, newer) in [
                ("2.9.0", "2.10.0", true),
                ("2.13.0", "2.13.1", true),
                ("2.13.0", "3.0.0", true),
                ("2.13.0", "2.13.0", false),
                ("3.0.0", "2.99.0", false),
            ] {
                assert_eq!(
                    newer_version(&feed(latest), current).unwrap(),
                    newer.then(|| latest.to_owned())
                );
            }
        }
        #[test]
        fn rejects_invalid_prerelease_and_foreign_metadata() {
            for value in [
                "",
                "2.14",
                "02.14.0",
                "2.14.0-beta.1",
                "v2.14.0",
                "2.14.0\n",
                "2.14.18446744073709551616",
                "https://example.com",
            ] {
                assert!(newer_version(&feed(value), "2.13.0").is_err());
            }
            assert!(newer_version(br#"{"packages":[]}"#, "2.13.0").is_err());
            assert!(newer_version(br#"{"packages":[{"id":"com.ecklf.luma","manifest":{"id":"other","version":"9.0.0"}}]}"#, "2.13.0").is_err());
            assert!(newer_version(b"not json", "2.13.0").is_err());
            let package: serde_json::Value = serde_json::from_slice(&feed("9.0.0")).unwrap();
            let entry = &package["packages"][0];
            let duplicates =
                serde_json::to_vec(&serde_json::json!({"packages":[entry, entry]})).unwrap();
            assert!(newer_version(&duplicates, "2.13.0").is_err());
        }
        #[test]
        fn disabled_service_never_contacts_the_network() {
            let service = Service::new(false);
            assert!(service.receiver.try_recv().is_err());
            service.check();
            assert!(
                service
                    .receiver
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .is_err()
            );
        }
        #[test]
        fn local_http_checks_bound_responses_and_report_errors() {
            use std::{io::Write, net::TcpListener};
            for (status, body, valid) in [
                ("200 OK", feed("9.0.0"), true),
                ("503 Unavailable", vec![], false),
                ("200 OK", vec![b' '; LIMIT + 1], false),
                ("200 OK", b"invalid".to_vec(), false),
            ] {
                let server = TcpListener::bind("127.0.0.1:0").unwrap();
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || {
                    let (mut socket, _) = server.accept().unwrap();
                    let mut request = [0; 2048];
                    let _ = socket.read(&mut request);
                    let _ = write!(
                        socket,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(&body);
                });
                let agent = ureq::Agent::config_builder()
                    .timeout_global(Some(Duration::from_secs(2)))
                    .proxy(None)
                    .build()
                    .new_agent();
                assert_eq!(
                    fetch(&agent, &format!("http://{address}/repo.json"), "2.13.0").is_ok(),
                    valid
                );
                worker.join().unwrap();
            }
        }
    }
}
pub use discovery::{Service, Status, public_checks_enabled, validate_preview};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
    time::Instant,
};

pub const FEED: &str = "https://github.com/ecklf/lg-luma/releases/latest/download/repo.json";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Only IPKs from this project's own release assets are installed.
const RELEASES: &str = "https://github.com/ecklf/lg-luma/releases/download/";
/// A fresh directory per attempt; never truncate a predictable path in shared /tmp.
fn download_directory() -> Result<(String, PathBuf), String> {
    use ring::rand::SecureRandom;
    use std::os::unix::fs::DirBuilderExt;
    let mut random = [0u8; 16];
    ring::rand::SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| "No secure randomness")?;
    let attempt: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let directory = std::env::temp_dir().join(format!("luma-update-{attempt}"));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    Ok((attempt, directory))
}

#[derive(serde::Deserialize, Debug)]
pub struct InstallResult {
    pub attempt: String,
    pub state: String,
    pub message: String,
}

fn terminal(result: &InstallResult) -> Option<Result<(), String>> {
    match result.state.as_str() {
        "success" => Some(Ok(())),
        "failed" => Some(Err(result.message.clone())),
        _ => None,
    }
}

fn wait_for_install(attempt: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(330);
    loop {
        if let Ok(Some(result)) = crate::platform::update_result_for(Some(attempt))
            && result.attempt == attempt
            && let Some(done) = terminal(&result)
        {
            return done;
        }
        if Instant::now() >= deadline {
            return Err(
                "The update did not finish in time. Check /var/lib/luma-update before retrying."
                    .into(),
            );
        }
        thread::sleep(Duration::from_secs(2));
    }
}
const FEED_LIMIT: u64 = 256 * 1024;
const MAX_IPK: u64 = 96 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum InstallStatus {
    #[default]
    Idle,
    Downloading(u8),
    Installing(String),
    Failed(String),
}

pub fn newer(current: &str, candidate: &str) -> bool {
    matches!((discovery::version(current), discovery::version(candidate)), (Some(a), Some(b)) if b > a)
}

/// The Luma package of a Homebrew Channel feed, checked before anything is downloaded.
pub fn parse_feed(feed: &Value) -> Result<Release, String> {
    let mut packages = feed["packages"]
        .as_array()
        .ok_or("The release feed has no packages")?
        .iter()
        .filter(|package| package["id"] == "com.ecklf.luma");
    let manifest = packages
        .next()
        .map(|package| &package["manifest"])
        .ok_or("The release feed has no Luma package")?;
    if packages.next().is_some() || manifest["id"] != "com.ecklf.luma" {
        return Err("Ambiguous Luma release metadata".into());
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
    if discovery::version(&release.version).is_none() {
        return Err("The release feed has no valid version".into());
    }
    let file = release.url.strip_prefix(RELEASES).unwrap_or_default();
    if !file.ends_with("_arm.ipk") || file.contains(['?', '#', '\\']) || file.contains("..") {
        return Err("The release feed points outside Luma's releases".into());
    }
    if release.sha256.len() != 64 || !release.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("The release feed has no valid checksum".into());
    }
    if release.size == 0 || release.size > MAX_IPK {
        return Err("The release feed has no valid package size".into());
    }
    Ok(release)
}

pub enum Request {
    Recover,
    Download(String),
}

pub enum Response {
    Progress(u8),
    Finished(Result<(), String>),
    /// Downloaded, verified and handed to the installer: the version being installed.
    Installing(String),
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        // GitHub answers release downloads with a redirect to its asset storage.
        .max_redirects(5)
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(180)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .proxy(None)
        .build()
        .new_agent()
}

fn check(agent: &ureq::Agent) -> Result<Option<Release>, String> {
    let mut response = agent
        .get(FEED)
        .header("User-Agent", "LumaNative")
        .call()
        .map_err(|_| "GitHub is unavailable. Try again shortly.")?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(FEED_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The release feed could not be read")?;
    if bytes.len() as u64 > FEED_LIMIT {
        return Err("The release feed is too large".into());
    }
    let feed: Value =
        serde_json::from_slice(&bytes).map_err(|_| "The release feed could not be read")?;
    let release = parse_feed(&feed)?;
    Ok(newer(CURRENT, &release.version).then_some(release))
}

fn download(
    agent: &ureq::Agent,
    release: &Release,
    progress: &Sender<Response>,
    destination: &Path,
) -> Result<(), String> {
    let mut response = agent
        .get(&release.url)
        .header("User-Agent", "LumaNative")
        .call()
        .map_err(|_| "The update could not be downloaded")?;
    let mut reader = response.body_mut().as_reader().take(release.size + 1);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let (mut buffer, mut total, mut reported) = (vec![0; 64 * 1024], 0u64, 0u8);
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "The download was interrupted")?;
        if count == 0 {
            break;
        }
        total += count as u64;
        digest.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        let percent = (total * 100 / release.size).min(100) as u8;
        if percent >= reported + 5 {
            reported = percent;
            let _ = progress.send(Response::Progress(percent));
        }
    }
    file.sync_all().map_err(|e| e.to_string())?;
    let hex: String = digest
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if total != release.size || hex != release.sha256 {
        return Err("The download does not match the release checksum".into());
    }
    Ok(())
}

pub struct Worker {
    requests: Sender<Request>,
    pub receiver: Receiver<Response>,
}

impl Worker {
    pub fn new(enabled: bool) -> Self {
        let (requests, incoming) = mpsc::channel::<Request>();
        let (responses, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("luma-update".into())
            .spawn(move || {
                let agent = agent();
                for request in incoming {
                    let response = match request {
                        Request::Recover if enabled => {
                            // A package install can close Luma. Surface a persisted failure on
                            // its next launch, or continue watching the still-running attempt.
                            match crate::platform::update_result_for(None) {
                                Ok(Some(result)) => {
                                    if let Some(done) = terminal(&result) {
                                        if done.is_ok() {
                                            continue;
                                        }
                                        Response::Finished(done)
                                    } else {
                                        let _ = responses.send(Response::Installing(String::new()));
                                        Response::Finished(wait_for_install(&result.attempt))
                                    }
                                }
                                _ => continue,
                            }
                        }
                        Request::Download(version) if enabled => {
                            let result = (|| {
                                // Re-read authenticated release metadata at install time, but
                                // never silently install a version the user was not offered.
                                let release =
                                    check(&agent)?.ok_or("No newer release is available")?;
                                if release.version != version {
                                    return Err(
                                        "The release changed. Check for updates again.".into()
                                    );
                                }
                                let (attempt, directory) = download_directory()?;
                                let ipk = directory.join("package.ipk");
                                if let Err(error) = download(&agent, &release, &responses, &ipk) {
                                    let _ = fs::remove_dir_all(&directory);
                                    return Err(error);
                                }
                                // A timed-out exec may already have started the unit. Never
                                // unlink its input on that ambiguous handoff; observe the result.
                                let handoff =
                                    crate::platform::start_update(&release, &attempt, &ipk);
                                if let Err(error) = &handoff {
                                    eprintln!("Update handoff: {error}");
                                }
                                let _ =
                                    responses.send(Response::Installing(release.version.clone()));
                                let done = wait_for_install(&attempt);
                                // The root helper has its own verified copy. On timeout leave
                                // the source intact in case a delayed handoff is still reading it.
                                if done.as_ref().err().is_none_or(|e| !e.contains("in time")) {
                                    let _ = fs::remove_dir_all(&directory);
                                }
                                done
                            })();
                            Response::Finished(result)
                        }
                        Request::Recover => continue,
                        Request::Download(_) => Response::Finished(Err(
                            "Installation is only available in interactive TV runs".into(),
                        )),
                    };
                    if responses.send(response).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn the update worker");
        let _ = requests.send(Request::Recover);
        Self { requests, receiver }
    }

    pub fn request(&self, request: Request) {
        let _ = self.requests.send(request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn feed(version: &str, url: &str, sha: &str, size: u64) -> Value {
        json!({"packages": [{"id": "com.ecklf.luma", "manifest": {
            "id": "com.ecklf.luma", "version": version, "ipkUrl": url, "ipkHash": {"sha256": sha}, "ipkSize": size
        }}]})
    }

    const URL: &str =
        "https://github.com/ecklf/lg-luma/releases/download/v9.0.0/com.ecklf.luma_9.0.0_arm.ipk";

    #[test]
    fn attempt_paths_are_private_unique_and_results_are_terminal_only_when_finished() {
        use std::os::unix::fs::PermissionsExt;
        let (a, first) = download_directory().unwrap();
        let (b, second) = download_directory().unwrap();
        assert_ne!(a, b);
        assert_eq!(
            fs::metadata(&first).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for state in ["starting", "installing"] {
            assert!(
                terminal(&InstallResult {
                    attempt: a.clone(),
                    state: state.into(),
                    message: "".into()
                })
                .is_none()
            );
        }
        let failed = InstallResult {
            attempt: a,
            state: "failed".into(),
            message: "disk full".into(),
        };
        assert_eq!(terminal(&failed), Some(Err("disk full".into())));
        fs::remove_dir(first).unwrap();
        fs::remove_dir(second).unwrap();
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(newer("2.8.4", "2.8.10"));
        assert!(newer("2.8.4", "3.0.0"));
        assert!(!newer("2.8.4", "2.8.4"));
        assert!(!newer("2.8.4", "2.7.99"));
        assert!(!newer("2.8.4", "2.9"));
        assert!(!newer("2.8.4", "2.9.0-beta"));
        assert!(!newer("2.8.4", "02.9.0"));
        assert!(!newer("2.8.4", "+2.9.0"));
    }

    #[test]
    fn disabled_installer_rejects_downloads_without_network_or_platform_calls() {
        let worker = Worker::new(false);
        assert!(worker.receiver.try_recv().is_err());
        worker.request(Request::Download("99.0.0".into()));
        assert!(matches!(
            worker
                .receiver
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Response::Finished(Err(_))
        ));
    }

    #[test]
    fn installer_rejects_foreign_and_ambiguous_manifests() {
        let entry = feed("9.0.0", URL, &"a".repeat(64), 1000);
        let package = &entry["packages"][0];
        assert!(parse_feed(&json!({"packages": [package, package]})).is_err());
        let mut foreign = entry;
        foreign["packages"][0]["manifest"]["id"] = json!("other.app");
        assert!(parse_feed(&foreign).is_err());
    }

    #[test]
    fn only_luma_release_assets_with_a_checksum_are_accepted() {
        let sha = "a".repeat(64);
        let release = parse_feed(&feed("9.0.0", URL, &sha.to_uppercase(), 1000)).unwrap();
        assert_eq!(release.version, "9.0.0");
        assert_eq!(release.sha256, sha);
        for bad in [
            feed(
                "9.0.0",
                "https://example.com/com.ecklf.luma_9.0.0_arm.ipk",
                &sha,
                1000,
            ),
            feed(
                "9.0.0",
                "https://github.com/ecklf/lg-luma/releases/download/../x_arm.ipk",
                &sha,
                1000,
            ),
            feed("9.0.0", URL, "abc", 1000),
            feed("9.0.0", URL, &sha, 0),
            feed("9.0.0", URL, &sha, MAX_IPK + 1),
            feed("nine", URL, &sha, 1000),
            json!({"packages": []}),
        ] {
            assert!(parse_feed(&bad).is_err(), "{bad}");
        }
    }
}
