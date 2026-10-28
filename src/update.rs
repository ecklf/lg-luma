//! Read-only release discovery. Installation stays with Homebrew Channel.
use serde::{Deserialize, Serialize};
use std::{io::Read, sync::mpsc, thread, time::Duration};

const FEED: &str = "https://github.com/ecklf/lg-luma/releases/latest/download/repo.json";
const LIMIT: usize = 256 * 1024;
pub const HOMEBREW_CHANNEL: &str = "org.webosbrew.hbchannel";

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
                    Err("Public release checks are disabled for scripted/simulated previews".into())
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
fn version(value: &str) -> Option<[u64; 3]> {
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
