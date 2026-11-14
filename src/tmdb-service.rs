//! Off-thread TMDB discover/trending + backdrop downloads. Never blocks the SDL thread.
use crate::tmdb::{self, Catalog, Command, DiskCache, Response, Show};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const JSON_LIMIT: usize = 512 * 1024;
const IMAGE_LIMIT: usize = 4 * 1024 * 1024;
const API: &str = "https://api.themoviedb.org/3";
const IMG: &str = "https://image.tmdb.org/t/p/w1280";

pub struct Client {
    agent: ureq::Agent,
}

impl Client {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent();
        Self { agent }
    }

    fn get_json(&self, url: &str) -> Result<Value, String> {
        let mut response = self
            .agent
            .get(url)
            .header("Accept", "application/json")
            .header("User-Agent", "LumaNative")
            .call()
            .map_err(|error| format!("TMDB request failed: {error}"))?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(format!("TMDB HTTP {}", response.status()));
        }
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(JSON_LIMIT as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|error| format!("TMDB read failed: {error}"))?;
        if body.len() > JSON_LIMIT {
            return Err("TMDB reply too large".into());
        }
        serde_json::from_slice(&body).map_err(|error| format!("TMDB JSON: {error}"))
    }

    fn download(&self, url: &str, dest: &Path) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", "LumaNative")
            .call()
            .map_err(|error| format!("TMDB image failed: {error}"))?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(format!("TMDB image HTTP {}", response.status()));
        }
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(IMAGE_LIMIT as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|error| format!("TMDB image read: {error}"))?;
        if body.len() > IMAGE_LIMIT || body.is_empty() {
            return Err("TMDB image too large or empty".into());
        }
        let rgb = image::load_from_memory(&body)
            .map_err(|error| format!("TMDB decode: {error}"))?
            .to_rgb8();
        image::save_buffer(
            dest,
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("TMDB encode: {error}"))?;
        Ok(())
    }

    fn fetch_shows(&self, command: &Command, cache_dir: &Path) -> Result<Vec<Show>, String> {
        let Command::Refresh {
            catalog,
            key,
            region,
            language,
            ..
        } = command;
        let key = tmdb::sanitize_key(key);
        if key.len() < 16 {
            return Err("TMDB API key required".into());
        }
        let raw = match catalog {
            Catalog::Providers(ids) => {
                let mut last = Err("No TMDB providers".into());
                for id in *ids {
                    let url = format!(
                        "{API}/discover/tv?api_key={key}&watch_region={region}&language={language}&with_watch_providers={id}&sort_by=popularity.desc&vote_count.gte=40&include_adult=false"
                    );
                    match self.get_json(&url) {
                        Ok(value) => {
                            let shows = tmdb::parse_shows(&value);
                            if !shows.is_empty() {
                                last = Ok(shows);
                                break;
                            }
                            last = Ok(shows);
                        }
                        Err(error) => last = Err(error),
                    }
                }
                last?
            }
            Catalog::Trending => {
                let url = format!("{API}/trending/all/week?api_key={key}&language={language}");
                tmdb::parse_shows(&self.get_json(&url)?)
            }
        };
        let mut shows = raw;
        for show in &mut shows {
            let file = cache_dir.join(format!("backdrop-{}.jpg", show.id));
            if !file.is_file() {
                let url = format!("{IMG}{}", show.backdrop);
                if let Err(error) = self.download(&url, &file) {
                    eprintln!("Luma TMDB backdrop {}: {error}", show.id);
                    continue;
                }
            }
            if file.is_file() {
                show.backdrop_path = Some(file.to_string_lossy().into_owned());
            }
        }
        shows.retain(|show| show.backdrop_path.is_some());
        Ok(shows)
    }
}

pub fn cache_dir(preferences_path: &Path) -> PathBuf {
    preferences_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("tmdb")
}

pub fn cache_file(preferences_path: &Path) -> PathBuf {
    cache_dir(preferences_path).join("cache.json")
}

pub fn load_cache(path: &Path) -> DiskCache {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_cache(path: &Path, cache: &DiskCache) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(cache) {
        let _ = fs::write(path, bytes);
    }
}

pub struct Service {
    tx: Mutex<Option<mpsc::Sender<(u64, Command)>>>,
    rx: Mutex<mpsc::Receiver<Response>>,
    generation: AtomicU64,
    _worker: JoinHandle<()>,
}

impl Service {
    pub fn new(client: Client, preferences_path: PathBuf) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<(u64, Command)>();
        let (res_tx, res_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("luma-tmdb".into())
            .spawn(move || {
                let images = cache_dir(&preferences_path);
                let disk = cache_file(&preferences_path);
                let mut cache = load_cache(&disk);
                while let Ok((generation, command)) = cmd_rx.recv() {
                    let Command::Refresh {
                        app_id,
                        catalog,
                        region,
                        language,
                        ..
                    } = &command;
                    let context = format!(
                        "{region}|{language}|{}",
                        match catalog {
                            Catalog::Providers(ids) => ids
                                .iter()
                                .map(|id| id.to_string())
                                .collect::<Vec<_>>()
                                .join("|"),
                            Catalog::Trending => "trending".into(),
                        }
                    );
                    let now = time::OffsetDateTime::now_utc().unix_timestamp();
                    if cache.context == context
                        && now.saturating_sub(cache.ts) < tmdb::CACHE_TTL_SECS
                        && let Some(mut shows) = cache.items.get(app_id).cloned()
                        && !shows.is_empty()
                    {
                        for show in &mut shows {
                            let file = images.join(format!("backdrop-{}.jpg", show.id));
                            if file.is_file() {
                                show.backdrop_path = Some(file.to_string_lossy().into_owned());
                            }
                        }
                        shows.retain(|show| show.backdrop_path.is_some());
                        if !shows.is_empty() {
                            let _ = res_tx.send(Response {
                                generation,
                                app_id: app_id.clone(),
                                result: Ok(shows),
                            });
                            continue;
                        }
                    }
                    let result = client.fetch_shows(&command, &images);
                    if let Ok(shows) = &result {
                        if cache.context != context {
                            cache = DiskCache {
                                context: context.clone(),
                                ts: now,
                                items: HashMap::new(),
                            };
                        }
                        cache.ts = now;
                        cache.items.insert(app_id.clone(), shows.clone());
                        save_cache(&disk, &cache);
                    }
                    let _ = res_tx.send(Response {
                        generation,
                        app_id: app_id.clone(),
                        result,
                    });
                }
            })
            .expect("TMDB worker");
        Self {
            tx: Mutex::new(Some(cmd_tx)),
            rx: Mutex::new(res_rx),
            generation: AtomicU64::new(0),
            _worker: worker,
        }
    }

    pub fn send(&self, command: Command) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(guard) = self.tx.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.send((generation, command));
        }
        generation
    }

    pub fn try_recv(&self) -> Option<Response> {
        self.rx.lock().ok()?.try_recv().ok()
    }
}

use std::collections::HashMap;

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
