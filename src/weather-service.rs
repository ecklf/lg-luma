//! Dedicated latest-job HTTP workers and an ordered cache writer. No request,
//! DNS/TLS work or cache write blocks the SDL thread.
use crate::i18n::Language;
use crate::weather::{Cache, Command, Location, Reading, parse_locations, parse_reading};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const LIMIT: usize = 256 * 1024;

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    geocoding: String,
    forecast: String,
}
impl Client {
    pub fn new(test_server: Option<&str>) -> Result<Self, String> {
        let (geocoding, forecast) = if let Some(server) = test_server {
            let uri: ureq::http::Uri = server
                .parse()
                .map_err(|e| format!("Invalid weather test server: {e}"))?;
            let address: SocketAddr = uri
                .authority()
                .ok_or("Weather test server needs a loopback address and port")?
                .as_str()
                .parse()
                .map_err(|_| "Weather tests require an IP-literal loopback address and port")?;
            if uri.scheme_str() != Some("http")
                || !address.ip().is_loopback()
                || address.port() == 0
                || uri.path() != "/"
                || uri.query().is_some()
            {
                return Err("Weather test server must be an HTTP loopback origin".into());
            }
            (
                format!("http://{address}/v1/search"),
                format!("http://{address}/v1/forecast"),
            )
        } else {
            (
                "https://geocoding-api.open-meteo.com/v1/search".into(),
                "https://api.open-meteo.com/v1/forecast".into(),
            )
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent();
        Ok(Self {
            agent,
            geocoding,
            forecast,
        })
    }
    fn request(&self, endpoint: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        let mut request = self
            .agent
            .get(endpoint)
            .header("Accept", "application/json")
            .header("User-Agent", "LumaNative");
        for (key, value) in query {
            request = request.query(key, value);
        }
        let mut response = request
            .call()
            .map_err(|_| "Open-Meteo is unavailable. Try again shortly.")?;
        if !response.status().is_success() {
            return Err("Open-Meteo is unavailable. Try again shortly.".into());
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The weather response could not be read")?;
        if bytes.len() > LIMIT {
            return Err("The weather response is too large".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "The weather service returned unreadable data".into())
    }
    pub fn search(&self, query: &str, language: Language) -> Result<Vec<Location>, String> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Err("Enter at least two characters.".into());
        }
        parse_locations(&self.request(
            &self.geocoding,
            &[
                ("name", query),
                ("count", "6"),
                ("language", language.code()),
                ("format", "json"),
            ],
        )?)
    }
    pub fn forecast(&self, location: &Location) -> Result<Reading, String> {
        let latitude = location.latitude.to_string();
        let longitude = location.longitude.to_string();
        parse_reading(&self.request(
            &self.forecast,
            &[
                ("latitude", &latitude),
                ("longitude", &longitude),
                ("current", "temperature_2m,is_day,weather_code"),
                ("temperature_unit", "celsius"),
                ("timezone", "auto"),
            ],
        )?)
    }
}

pub enum Response {
    Search {
        id: u64,
        result: Result<Vec<Location>, String>,
    },
    Forecast {
        id: u64,
        key: String,
        result: Result<Reading, String>,
    },
}
#[derive(Default)]
struct Pending {
    job: Option<Command>,
    closed: bool,
}
#[derive(Default)]
struct Queue {
    pending: Mutex<Pending>,
    ready: Condvar,
    generation: AtomicU64,
}
impl Queue {
    fn submit(&self, id: u64, job: Command) {
        self.generation.store(id, Ordering::SeqCst);
        self.pending.lock().unwrap().job = Some(job);
        self.ready.notify_one();
    }
    fn close(&self) {
        self.generation.store(u64::MAX, Ordering::SeqCst);
        let mut pending = self.pending.lock().unwrap();
        pending.closed = true;
        pending.job = None;
        self.ready.notify_one();
    }
}
pub struct Service {
    search: Arc<Queue>,
    forecast: Arc<Queue>,
    pub receiver: mpsc::Receiver<Response>,
    cache: Option<mpsc::Sender<Option<Cache>>>,
    cache_worker: Option<JoinHandle<()>>,
}
impl Service {
    pub fn new(client: Client, path: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel();
        let spawn = |client: Client, sender: mpsc::Sender<Response>| {
            let queue = Arc::new(Queue::default());
            let worker = queue.clone();
            thread::spawn(move || {
                loop {
                    let job = {
                        let mut pending = worker.pending.lock().unwrap();
                        while pending.job.is_none() && !pending.closed {
                            pending = worker.ready.wait(pending).unwrap();
                        }
                        if pending.closed {
                            break;
                        }
                        pending.job.take().unwrap()
                    };
                    let id = match &job {
                        Command::Search { id, .. } | Command::Forecast { id, .. } => *id,
                        _ => continue,
                    };
                    if worker.generation.load(Ordering::SeqCst) != id {
                        continue;
                    }
                    let response = match job {
                        Command::Search {
                            id,
                            query,
                            language,
                        } => Response::Search {
                            id,
                            result: client.search(&query, language),
                        },
                        Command::Forecast { id, location } => Response::Forecast {
                            id,
                            key: location.key(),
                            result: client.forecast(&location),
                        },
                        _ => continue,
                    };
                    if worker.generation.load(Ordering::SeqCst) == id
                        && sender.send(response).is_err()
                    {
                        break;
                    }
                }
            });
            queue
        };
        let search = spawn(client.clone(), sender.clone());
        let forecast = spawn(client, sender);
        let (cache, writes) = mpsc::channel::<Option<Cache>>();
        let cache_worker = thread::spawn(move || {
            for cache in writes {
                if let Some(cache) = cache {
                    let _ = save_cache(&path, &cache);
                } else {
                    let _ = fs::remove_file(&path);
                }
            }
        });
        Self {
            search,
            forecast,
            receiver,
            cache: Some(cache),
            cache_worker: Some(cache_worker),
        }
    }
    pub fn generations(&self, (search, forecast): (u64, u64)) {
        self.search.generation.store(search, Ordering::SeqCst);
        self.forecast.generation.store(forecast, Ordering::SeqCst);
    }
    pub fn send(&self, command: Command) {
        match command {
            job @ Command::Search { id, .. } => self.search.submit(id, job),
            job @ Command::Forecast { id, .. } => self.forecast.submit(id, job),
            Command::Persist(cache) => {
                if let Some(sender) = &self.cache {
                    let _ = sender.send(Some(cache));
                }
            }
            Command::ClearCache => {
                if let Some(sender) = &self.cache {
                    let _ = sender.send(None);
                }
            }
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.search.close();
        self.forecast.close();
        self.cache.take();
        if let Some(worker) = self.cache_worker.take() {
            let _ = worker.join();
        }
    }
}
pub fn cache_path(preferences: &Path) -> PathBuf {
    preferences.with_extension("weather.json")
}
pub fn load_cache(path: &Path) -> Option<Cache> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 64 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
pub(crate) fn save_cache(path: &Path, cache: &Cache) -> Result<(), String> {
    crate::storage::write_atomic(path, &serde_json::to_vec(cache).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_origins_are_loopback_only_and_never_follow_redirects() {
        assert!(Client::new(Some("http://127.0.0.1:1234")).is_ok());
        assert!(Client::new(Some("http://[::1]:1234")).is_ok());
        for invalid in [
            "http://example.com:1234",
            "https://127.0.0.1:1234",
            "http://127.0.0.1:0",
            "http://127.0.0.1:1234/path",
            "http://user@127.0.0.1:1234",
            "http://127.0.0.1:1234?query=yes",
        ] {
            assert!(Client::new(Some(invalid)).is_err(), "{invalid}");
        }
    }
    #[test]
    fn cache_writes_are_ordered_and_flushed_before_shutdown() {
        let directory =
            std::env::temp_dir().join(format!("luma-weather-cache-{}", std::process::id()));
        let path = directory.join("preferences.weather.json");
        let cache = Cache {
            key: "48.2085,16.3721".into(),
            updated_at: 1000,
            reading: Reading {
                temperature: 14.2,
                code: 2,
                is_day: true,
            },
        };
        {
            let service = Service::new(
                Client::new(Some("http://127.0.0.1:1234")).unwrap(),
                path.clone(),
            );
            service.send(Command::Persist(cache.clone()));
            service.send(Command::ClearCache);
            service.send(Command::Persist(cache.clone()));
        }
        assert_eq!(load_cache(&path), Some(cache));
        {
            let service = Service::new(
                Client::new(Some("http://127.0.0.1:1234")).unwrap(),
                path.clone(),
            );
            service.send(Command::ClearCache);
        }
        assert!(!path.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
