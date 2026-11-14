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
            // Keep discovers snappy: long timeouts made Stremio appear broken while
            // a sequential multi-provider chain waited on dead addresses.
            .timeout_global(Some(Duration::from_secs(5)))
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

    fn discover_provider(
        &self,
        key: &str,
        region: &str,
        language: &str,
        provider: u32,
    ) -> Result<Vec<Show>, String> {
        // `with_watch_monetization_types` is required for reliable provider filtering.
        let url = format!(
            "{API}/discover/tv?api_key={key}&watch_region={region}&language={language}&with_watch_providers={provider}&with_watch_monetization_types=flatrate&sort_by=popularity.desc&vote_count.gte=40&include_adult=false"
        );
        let shows = tmdb::parse_shows(&self.get_json(&url)?);
        if shows.is_empty() {
            return Err(format!("TMDB provider {provider} returned no shows"));
        }
        Ok(shows)
    }

    fn discover_trending(&self, key: &str, language: &str) -> Result<Vec<Show>, String> {
        let url = format!("{API}/trending/tv/week?api_key={key}&language={language}");
        let shows = tmdb::parse_shows(&self.get_json(&url)?);
        if shows.is_empty() {
            return Err("TMDB trending returned no shows".into());
        }
        Ok(shows)
    }

    fn discover_all_platforms(
        &self,
        key: &str,
        region: &str,
        language: &str,
    ) -> Result<Vec<Show>, String> {
        // Prefer one trending reply so aggregators are not blocked on a long
        // sequential discover chain (and on provider ids that are empty in-region).
        match self.discover_trending(key, language) {
            Ok(shows) => Ok(shows),
            Err(trending_error) => {
                let mut picked = Vec::new();
                let mut seen = std::collections::HashSet::new();
                let mut last_error = Some(trending_error);
                for provider in tmdb::ALL_PLATFORM_PROVIDERS {
                    if picked.len() >= tmdb::MAX_SHOWS {
                        break;
                    }
                    match self.discover_provider(key, region, language, *provider) {
                        Ok(shows) => {
                            if let Some(show) = shows.into_iter().find(|show| seen.insert(show.id))
                            {
                                picked.push(show);
                            }
                        }
                        Err(error) => last_error = Some(error),
                    }
                }
                if picked.is_empty() {
                    return Err(last_error.unwrap_or_else(|| "No TMDB providers".into()));
                }
                Ok(picked)
            }
        }
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
        eprintln!(
            "Luma TMDB: fetching {} ({region}/{language})",
            match catalog {
                Catalog::Providers(ids) => format!("providers {ids:?}"),
                Catalog::AllPlatforms => "trending/all-platforms".into(),
            }
        );
        let raw = match catalog {
            Catalog::Providers(ids) => {
                let mut last = Err("No TMDB providers".into());
                for id in *ids {
                    match self.discover_provider(&key, region, language, *id) {
                        Ok(shows) => {
                            last = Ok(shows);
                            break;
                        }
                        Err(error) => last = Err(error),
                    }
                }
                last?
            }
            Catalog::AllPlatforms => self.discover_all_platforms(&key, region, language)?,
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
        if shows.is_empty() {
            return Err("TMDB produced no usable backdrops".into());
        }
        eprintln!("Luma TMDB: ready {} show(s)", shows.len());
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

/// `backdrop_path` is skipped in JSON; reattach local files after load so the
/// shelf can reveal art instead of the brand-gray fallback.
pub fn hydrate_shows(shows: &mut [Show], images: &Path) {
    for show in shows {
        let file = images.join(format!("backdrop-{}.jpg", show.id));
        if file.is_file() {
            show.backdrop_path = Some(file.to_string_lossy().into_owned());
        } else {
            show.backdrop_path = None;
        }
    }
}

pub fn hydrate_cache(cache: &mut DiskCache, images: &Path) {
    for shows in cache.items.values_mut() {
        hydrate_shows(shows, images);
        shows.retain(|show| show.backdrop_path.is_some());
    }
    cache.items.retain(|_, shows| !shows.is_empty());
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
                            Catalog::AllPlatforms => "trending-week".into(),
                        }
                    );
                    let now = time::OffsetDateTime::now_utc().unix_timestamp();
                    if cache.context == context
                        && now.saturating_sub(cache.ts) < tmdb::CACHE_TTL_SECS
                        && let Some(mut shows) = cache.items.get(app_id).cloned()
                        && !shows.is_empty()
                    {
                        hydrate_shows(&mut shows, &images);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmdb::Show;

    #[test]
    fn hydrate_reattaches_existing_backdrop_files() {
        let dir = std::env::temp_dir().join(format!("luma-tmdb-hydrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("backdrop-42.jpg");
        fs::write(&file, b"jpeg").unwrap();
        let mut shows = vec![
            Show {
                id: 42,
                name: "Silo".into(),
                overview: String::new(),
                backdrop: "/a.jpg".into(),
                backdrop_path: None,
            },
            Show {
                id: 7,
                name: "Missing".into(),
                overview: String::new(),
                backdrop: "/b.jpg".into(),
                backdrop_path: Some("stale".into()),
            },
        ];
        hydrate_shows(&mut shows, &dir);
        assert_eq!(
            shows[0].backdrop_path.as_deref(),
            Some(file.to_str().unwrap())
        );
        assert!(shows[1].backdrop_path.is_none());
        let mut cache = DiskCache {
            context: "ES|ca-ES|trending".into(),
            ts: 1,
            items: HashMap::from([("tv.stremioorigin.app".into(), shows)]),
        };
        hydrate_cache(&mut cache, &dir);
        let kept = &cache.items["tv.stremioorigin.app"];
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, 42);
        let _ = fs::remove_dir_all(&dir);
    }
}
