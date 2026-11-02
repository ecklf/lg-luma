//! The CS2 Score worker: one HTTP job at a time off the SDL thread, newest request wins.
use crate::app_artwork::Artwork;
use crate::cs2::{Command, Feed, LOGO_ORIGIN, parse_feed};
use std::{
    io::Read,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const LIMIT: usize = 256 * 1024;
const LOGO_LIMIT: usize = 1024 * 1024;
/// Map screenshots stand behind text, so they keep more width than a logo.
const BACKDROP_WIDTH: u32 = 640;
const FEED: &str = "https://eluz1ve.github.io/luma-feeds/cs2/matches.json";

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    url: String,
}
impl Client {
    pub fn new(test_server: Option<&str>) -> Result<Self, String> {
        let url = if let Some(server) = test_server {
            let uri: ureq::http::Uri = server
                .parse()
                .map_err(|e| format!("Invalid CS2 test server: {e}"))?;
            let address: SocketAddr = uri
                .authority()
                .ok_or("CS2 test server needs a loopback address and port")?
                .as_str()
                .parse()
                .map_err(|_| "CS2 tests require an IP-literal loopback address and port")?;
            if uri.scheme_str() != Some("http")
                || !address.ip().is_loopback()
                || address.port() == 0
                || uri.path() != "/"
                || uri.query().is_some()
            {
                return Err("CS2 test server must be an HTTP loopback origin".into());
            }
            format!("http://{address}/luma-feeds/cs2/matches.json")
        } else {
            FEED.into()
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent();
        Ok(Self { agent, url })
    }
    /// A logo or a map's screenshot from Liquipedia's commons, decoded and scaled on this
    /// worker.
    pub fn image(&self, url: &str, backdrop: bool) -> Option<Arc<Artwork>> {
        if !url.starts_with(LOGO_ORIGIN) {
            return None;
        }
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", "LumaNative")
            .call()
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(LOGO_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > LOGO_LIMIT {
            return None;
        }
        let mut artwork = if backdrop {
            decode_backdrop(&bytes)?
        } else {
            crate::app_artwork::decode(&bytes)?
        };
        artwork.key = format!("cs2-image:{url}");
        Some(Arc::new(artwork))
    }
    pub fn feed(&self) -> Result<Feed, String> {
        let mut response = self
            .agent
            .get(&self.url)
            .header("Accept", "application/json")
            .header("User-Agent", "LumaNative")
            .call()
            .map_err(|_| "Scores are unavailable.")?;
        if !response.status().is_success() {
            return Err("Scores are unavailable.".into());
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The score feed could not be read")?;
        if bytes.len() > LIMIT {
            return Err("The score feed is too large".into());
        }
        parse_feed(
            &serde_json::from_slice(&bytes)
                .map_err(|_| "The score feed returned unreadable data")?,
        )
    }
}

/// A screenshot within the same limits as app artwork, scaled to `BACKDROP_WIDTH`.
fn decode_backdrop(bytes: &[u8]) -> Option<Artwork> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let pixels = if image.width() > BACKDROP_WIDTH {
        image.thumbnail(BACKDROP_WIDTH, BACKDROP_WIDTH).into_rgba8()
    } else {
        image.into_rgba8()
    };
    Some(Artwork {
        key: String::new(),
        pixels,
        backdrop: [[0; 3]; 2],
    })
}

pub enum Response {
    Feed {
        id: u64,
        result: Result<Feed, String>,
    },
    Logo {
        url: String,
        image: Option<Arc<Artwork>>,
    },
}

pub struct Service {
    sender: Option<mpsc::Sender<Command>>,
    logos: Option<mpsc::Sender<(String, bool)>>,
    generation: Arc<AtomicU64>,
    pub receiver: mpsc::Receiver<Response>,
}
impl Service {
    pub fn new(client: Client) -> Self {
        let (sender, jobs) = mpsc::channel::<Command>();
        let (responses, receiver) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let current = generation.clone();
        // Logos come one at a time on their own worker, so a slow one never delays scores.
        let (logos, wanted) = mpsc::channel::<(String, bool)>();
        let logo_client = client.clone();
        let logo_responses = responses.clone();
        thread::spawn(move || {
            for (url, backdrop) in wanted {
                let image = logo_client.image(&url, backdrop);
                if logo_responses.send(Response::Logo { url, image }).is_err() {
                    break;
                }
            }
        });
        thread::spawn(move || {
            while let Ok(mut job) = jobs.recv() {
                // Only the newest waiting request is worth answering.
                while let Ok(newer) = jobs.try_recv() {
                    job = newer;
                }
                if current.load(Ordering::SeqCst) != job.id {
                    continue;
                }
                let result = client.feed();
                if current.load(Ordering::SeqCst) == job.id
                    && responses
                        .send(Response::Feed { id: job.id, result })
                        .is_err()
                {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            logos: Some(logos),
            generation,
            receiver,
        }
    }
    /// Requests older than `id` are dropped before or after they run.
    pub fn generation(&self, id: u64) {
        self.generation.store(id, Ordering::SeqCst);
    }
    pub fn image(&self, url: String, backdrop: bool) {
        if let Some(logos) = &self.logos {
            let _ = logos.send((url, backdrop));
        }
    }
    pub fn send(&self, command: Command) {
        self.generation(command.id);
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.generation.store(u64::MAX, Ordering::SeqCst);
        self.sender.take();
        self.logos.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_origins_are_loopback_only() {
        assert!(Client::new(Some("http://127.0.0.1:1234")).is_ok());
        assert!(Client::new(Some("http://[::1]:1234")).is_ok());
        for invalid in [
            "http://example.com:1234",
            "https://127.0.0.1:1234",
            "http://127.0.0.1:0",
            "http://127.0.0.1:1234/path",
            "http://127.0.0.1:1234?query=yes",
        ] {
            assert!(Client::new(Some(invalid)).is_err(), "{invalid}");
        }
    }
}
