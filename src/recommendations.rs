//! TMDB "trending" feed for the Recommendations widget. Network/TLS and JSON parsing happen on a
//! worker thread (see `recommendations-service`-style usage in `main`), never on the SDL thread.
//! Uses a TMDB v3 API key (the `?api_key=` query parameter), matching the key format Free My TV uses.
use crate::model::RecType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single trending title shown as a poster card.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct Recommendation {
    /// TMDB id, used to fetch details (genre, seasons/runtime).
    pub id: i64,
    /// Display title (movie `title` or show `name`).
    pub title: String,
    /// TMDB poster path, e.g. `/abc123.jpg`; combine with the image base to get a URL.
    pub poster_path: String,
    /// `true` for a TV show, `false` for a movie (used for the badge and, later, deep-linking).
    pub is_show: bool,
    /// Release/first-air year (from the trending feed), empty when unknown.
    pub year: String,
    /// Primary genre name (from the detail request), empty until detailed.
    pub genre: String,
    /// Seasons ("2 seasons") for shows or runtime ("1h 30m") for movies; empty until detailed.
    pub detail: String,
    /// True when it carries TMDB's Animation genre (id 16) — i.e. cartoons/anime.
    pub is_anime: bool,
    /// Streaming service display names where it's available (flatrate), best first, e.g. ["Disney+"].
    /// The app to launch is resolved against the TV's installed apps at click time.
    pub services: Vec<String>,
    /// The installed app id to launch, resolved at fetch time against the TV's apps (Free My TV's
    /// approach). Empty when no installed app provides it — such items are dropped from the feed.
    pub app_id: String,
    /// Display name of the resolved installed service (e.g. "Netflix"), for the details line.
    pub service_name: String,
    /// Whether the detail request has already filled genre/detail.
    pub detailed: bool,
}

/// TMDB's Animation genre id (covers cartoons and anime, in both movies and shows).
const ANIMATION_GENRE: i64 = 16;

/// A TV app as the worker needs it: just the id and title used to match streaming services.
/// (The full `model::App` isn't available on the worker thread, so we pass this lean copy.)
#[derive(Clone, Debug)]
pub struct InstalledApp {
    pub id: String,
    pub title: String,
}

/// Maps a TMDB provider name (lowercased substring match) to a display name and the webOS app id
/// A streaming service: how its TMDB provider name is detected, its display name, and substrings
/// to match against installed apps (by id or title) so we launch whatever the TV actually has.
/// Mirrors Free My TV's SERVICES table.
pub struct StreamingService {
    /// Lowercased substring found in the TMDB provider_name.
    pub tmdb: &'static str,
    /// Display name shown in the details line.
    pub name: &'static str,
    /// Lowercased substrings to match an installed app's id or title.
    pub app_match: &'static [&'static str],
}

pub const SERVICES: &[StreamingService] = &[
    StreamingService {
        tmdb: "netflix",
        name: "Netflix",
        app_match: &["netflix"],
    },
    StreamingService {
        tmdb: "disney",
        name: "Disney+",
        app_match: &["disney"],
    },
    StreamingService {
        tmdb: "star+",
        name: "Star+",
        app_match: &["disney", "star"],
    },
    StreamingService {
        tmdb: "amazon prime",
        name: "Prime Video",
        app_match: &["amazon", "prime"],
    },
    StreamingService {
        tmdb: "prime video",
        name: "Prime Video",
        app_match: &["amazon", "prime"],
    },
    StreamingService {
        tmdb: "globoplay",
        name: "Globoplay",
        app_match: &["globoplay", "globo"],
    },
    StreamingService {
        tmdb: "max",
        name: "Max",
        app_match: &["wbd", "hbo", ".max", "max"],
    },
    StreamingService {
        tmdb: "apple tv",
        name: "Apple TV",
        app_match: &["apple", "appletv"],
    },
    StreamingService {
        tmdb: "paramount",
        name: "Paramount+",
        app_match: &["paramount", "cbs"],
    },
    StreamingService {
        tmdb: "crunchyroll",
        name: "Crunchyroll",
        app_match: &["crunchyroll"],
    },
    StreamingService {
        tmdb: "telecine",
        name: "Telecine",
        app_match: &["telecine"],
    },
    StreamingService {
        tmdb: "youtube",
        name: "YouTube",
        app_match: &["youtube"],
    },
];

/// The service whose TMDB name matches `provider_name`, if any.
pub fn service_for(provider_name: &str) -> Option<&'static StreamingService> {
    let n = provider_name.to_lowercase();
    if n.contains(" channel") {
        return None; // "X Amazon Channel" is sold inside another app
    }
    SERVICES.iter().find(|s| {
        if s.tmdb == "max" {
            // avoid matching "cinemax" etc.; word-ish match
            n == "max" || n.contains("hbo max") || n.contains(" max")
        } else {
            n.contains(s.tmdb)
        }
    })
}

/// Installed apps that share a brand substring with a streaming service but are NOT the player
/// (voice assistants, stores, companions). Matching these would launch the wrong app — e.g.
/// "amazon" must resolve to Prime Video (id `amazon`), never `amazon.alexa.view` (Alexa).
const NOT_PLAYERS: &[&str] = &["alexa", "assistant", "store", "companion", ".demo", "setup"];

/// The installed app that provides `service`, matched by id or title (Free My TV's `appFor`).
pub fn app_for<'a>(
    service: &StreamingService,
    installed: &'a [InstalledApp],
) -> Option<&'a InstalledApp> {
    installed.iter().find(|app| {
        let id = app.id.to_lowercase();
        let title = app.title.to_lowercase();
        // Never resolve to a non-player app that merely shares a brand word (e.g. Alexa).
        if NOT_PLAYERS
            .iter()
            .any(|bad| id.contains(bad) || title.contains(bad))
        {
            return false;
        }
        service
            .app_match
            .iter()
            .any(|m| id.contains(m) || title.contains(m))
    })
}

/// True when some installed app provides this service (so its titles are worth showing).
pub fn usable(service: &StreamingService, installed: &[InstalledApp]) -> bool {
    app_for(service, installed).is_some()
}

impl Recommendation {
    /// Full poster URL at the given width bucket (e.g. "w342"), using TMDB's image CDN.
    pub fn poster_url(&self, width: &str) -> String {
        format!("https://image.tmdb.org/t/p/{width}{}", self.poster_path)
    }

    /// The on-disk cache path for this poster under `cache_dir` (empty poster_path -> None).
    pub fn poster_file(&self, cache_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let name = self.poster_path.trim_start_matches('/').replace('/', "_");
        if name.is_empty() {
            None
        } else {
            Some(cache_dir.join(name))
        }
    }
}

/// Parse a TMDB trending response (`{"results":[...]}`) into recommendations.
/// `as_show` labels the batch (TMDB's movie and tv endpoints are queried separately).
pub fn parse_results(value: &Value, as_show: bool) -> Vec<Recommendation> {
    let Some(results) = value.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in results {
        let poster_path = item
            .get("poster_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if poster_path.is_empty() {
            continue; // no artwork, no card
        }
        // Movies carry `title`, shows carry `name`.
        let title = item
            .get("title")
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect::<String>();
        if title.is_empty() {
            continue;
        }
        let id = item.get("id").and_then(Value::as_i64).unwrap_or(0);
        // Year from release_date (movie) or first_air_date (show): "YYYY-..." -> "YYYY".
        let year = item
            .get("release_date")
            .or_else(|| item.get("first_air_date"))
            .and_then(Value::as_str)
            .map(|d| d.chars().take(4).collect::<String>())
            .filter(|y| y.len() == 4)
            .unwrap_or_default();
        // Anime/cartoon detection from the trending genre_ids.
        let is_anime = item
            .get("genre_ids")
            .and_then(Value::as_array)
            .map(|ids| ids.iter().any(|g| g.as_i64() == Some(ANIMATION_GENRE)))
            .unwrap_or(false);
        // Quality signal for sorting "the best": rating gated by a minimum vote count.
        let votes = item.get("vote_count").and_then(Value::as_i64).unwrap_or(0);
        let rating = item
            .get("vote_average")
            .and_then(Value::as_f64)
            .unwrap_or(0.);
        let score = if votes >= 200 { rating } else { 0. };
        out.push((
            score,
            Recommendation {
                id,
                title,
                poster_path,
                is_show: as_show,
                year,
                is_anime,
                ..Default::default()
            },
        ));
    }
    // Best first: higher rating (with enough votes) leads; trending order breaks ties.
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    out.into_iter().map(|(_, rec)| rec).collect()
}

const LIMIT: usize = 512 * 1024;

/// TMDB client. One per worker; cloning shares the underlying ureq agent config cheaply.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    base: String,
}

impl Client {
    pub fn new(test_server: Option<&str>) -> Self {
        let base = match test_server {
            Some(server) => server.trim_end_matches('/').to_string(),
            None => "https://api.themoviedb.org/3".to_string(),
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(8)))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent();
        Self { agent, base }
    }

    fn request(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        use std::io::Read;
        let url = format!("{}{path}", self.base);
        let mut request = self
            .agent
            .get(&url)
            .header("Accept", "application/json")
            .header("User-Agent", "LumaNative");
        for (key, value) in query {
            request = request.query(key, value);
        }
        let mut response = request
            .call()
            .map_err(|_| "TMDB is unavailable. Check the API key and network.".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "TMDB returned status {}",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The TMDB response could not be read".to_string())?;
        if bytes.len() > LIMIT {
            return Err("The TMDB response is too large".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "TMDB returned unreadable data".into())
    }

    /// The TMDB provider ids for the streaming apps installed on this TV, in the region. Mirrors
    /// Free My TV's `fetchList` first step: ask TMDB which providers exist, keep those we have an
    /// app for. Used to filter `discover` so the feed only holds watchable titles.
    fn installed_provider_ids(
        &self,
        key: &str,
        language: &str,
        region: &str,
        installed: &[InstalledApp],
    ) -> Vec<i64> {
        let Ok(value) = self.request(
            "/watch/providers/movie",
            &[
                ("api_key", key),
                ("language", language),
                ("watch_region", region),
            ],
        ) else {
            return Vec::new();
        };
        let Some(results) = value.get("results").and_then(Value::as_array) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        for provider in results {
            let matched = provider
                .get("provider_name")
                .and_then(Value::as_str)
                .and_then(service_for)
                .is_some_and(|service| usable(service, installed));
            if matched
                && let Some(id) = provider.get("provider_id").and_then(Value::as_i64)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        ids
    }

    /// The recommendations list: Free My TV's blend of what's trending this week and recent
    /// releases, kept only when an installed streaming app provides it. `trending/week` candidates
    /// are confirmed by their details; `discover` is pre-filtered by the installed providers.
    /// Movies and shows are interleaved so a `Both` row mixes them. Returns at most `limit` items.
    #[allow(clippy::too_many_arguments)]
    pub fn trending(
        &self,
        key: &str,
        kind: RecType,
        language: &str,
        region: &str,
        installed: &[InstalledApp],
        labels: &DetailLabels,
        page: u32,
        limit: usize,
    ) -> Result<Vec<Recommendation>, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("Add a TMDB API key in Settings to see recommendations.".into());
        }
        if installed.is_empty() {
            return Err("No streaming apps are installed to recommend from.".into());
        }
        // TMDB pages are 1-based; the Refresh card bumps this to pull a fresh batch.
        let page_str = page.max(1).to_string();

        let want_movies = matches!(kind, RecType::Movies | RecType::Both);
        let want_shows = matches!(kind, RecType::Shows | RecType::Both);

        // Pre-filter discover by the TV's providers (empty = none matched on TMDB).
        let provider_ids = self.installed_provider_ids(key, language, region, installed);

        // "Hot": trending this week, kept only when its details resolve an installed app.
        let hot = |me: &Self, as_show: bool| -> Vec<Recommendation> {
            let path = if as_show {
                "/trending/tv/week"
            } else {
                "/trending/movie/week"
            };
            let Ok(value) = me.request(
                path,
                &[
                    ("api_key", key),
                    ("language", language),
                    ("page", page_str.as_str()),
                ],
            ) else {
                return Vec::new();
            };
            let mut candidates = parse_results(&value, as_show);
            candidates.truncate(16);
            candidates
                .into_iter()
                .filter_map(|mut item| {
                    me.detail(&mut item, key, language, region, installed, labels);
                    (!item.app_id.is_empty()).then_some(item)
                })
                .collect()
        };

        // "Fresh": recent releases per installed provider, so every service is represented (a
        // single mixed query ranked by popularity would always be dominated by the big services,
        // hiding e.g. Crunchyroll). We fetch each provider's top titles, then interleave them.
        let fresh = |me: &Self, as_show: bool| -> Vec<Recommendation> {
            if provider_ids.is_empty() {
                return Vec::new();
            }
            let (path, votes): (&str, &str) = if as_show {
                ("/discover/tv", "20")
            } else {
                ("/discover/movie", "30")
            };
            // One list per provider, each already a popularity-ranked page for THAT service.
            let mut per_provider: Vec<Vec<Recommendation>> = Vec::new();
            for id in &provider_ids {
                let provider = id.to_string();
                let query: Vec<(&str, &str)> = vec![
                    ("api_key", key),
                    ("language", language),
                    ("watch_region", region),
                    ("with_watch_monetization_types", "flatrate"),
                    ("include_adult", "false"),
                    ("sort_by", "popularity.desc"),
                    ("vote_count.gte", votes),
                    ("with_watch_providers", provider.as_str()),
                    ("page", page_str.as_str()),
                ];
                if let Ok(value) = me.request(path, &query) {
                    let mut list = parse_results(&value, as_show);
                    list.truncate(6); // cap per service so no single one floods the row
                    if !list.is_empty() {
                        per_provider.push(list);
                    }
                }
            }
            // Round-robin across services: first pick of each, then second pick of each, …
            let mut out = Vec::new();
            let mut i = 0;
            loop {
                let mut added = false;
                for list in &per_provider {
                    if let Some(item) = list.get(i) {
                        out.push(item.clone());
                        added = true;
                    }
                }
                if !added {
                    break;
                }
                i += 1;
            }
            out
        };

        let (hot_movies, hot_shows) = (
            if want_movies {
                hot(self, false)
            } else {
                Vec::new()
            },
            if want_shows {
                hot(self, true)
            } else {
                Vec::new()
            },
        );
        let (fresh_movies, fresh_shows) = (
            if want_movies {
                fresh(self, false)
            } else {
                Vec::new()
            },
            if want_shows {
                fresh(self, true)
            } else {
                Vec::new()
            },
        );

        // Trending first (already watchable), then recent releases; movies/shows interleaved,
        // de-duplicated by TMDB id+kind so a title doesn't appear twice.
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut push_unique = |item: Recommendation, out: &mut Vec<Recommendation>| {
            if seen.insert((item.id, item.is_show)) {
                out.push(item);
            }
        };
        for item in interleave(hot_movies, hot_shows) {
            push_unique(item, &mut out);
        }
        for item in interleave(fresh_movies, fresh_shows) {
            push_unique(item, &mut out);
        }
        out.truncate(limit);
        Ok(out)
    }

    /// Fill `genre`, `detail` (seasons/runtime), the `services` list, and resolve the installed
    /// `app_id`/`service_name` for one item. Mirrors Free My TV's `details`: the first flatrate
    /// provider that maps to an installed app wins, so clicking the card always opens something.
    #[allow(clippy::too_many_arguments)]
    fn detail(
        &self,
        item: &mut Recommendation,
        key: &str,
        language: &str,
        region: &str,
        installed: &[InstalledApp],
        labels: &DetailLabels,
    ) {
        let path = if item.is_show {
            format!("/tv/{}", item.id)
        } else {
            format!("/movie/{}", item.id)
        };
        let Ok(value) = self.request(
            &path,
            &[
                ("api_key", key),
                ("language", language),
                ("append_to_response", "watch/providers"),
            ],
        ) else {
            return;
        };
        // Primary genre: prefer our localized override by TMDB genre id (fixes the composite TV
        // genres TMDB leaves in English), falling back to the name TMDB already localized.
        if let Some(first) = value
            .get("genres")
            .and_then(Value::as_array)
            .and_then(|g| g.first())
        {
            let id = first.get("id").and_then(Value::as_i64).unwrap_or(0);
            if let Some(localized) = labels.genres.get(&id) {
                item.genre = localized.trim().chars().take(40).collect();
            } else if let Some(name) = first.get("name").and_then(Value::as_str) {
                item.genre = name.trim().chars().take(40).collect();
            }
        }
        // Streaming provider for the region (flatrate = subscription), mapped to an installed app.
        // The first provider we both recognize AND have an app for resolves the launch target.
        if let Some(flatrate) = value
            .get("watch/providers")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.get(region))
            .and_then(|c| c.get("flatrate"))
            .and_then(Value::as_array)
        {
            for provider in flatrate {
                let Some(service) = provider
                    .get("provider_name")
                    .and_then(Value::as_str)
                    .and_then(service_for)
                else {
                    continue;
                };
                let name = service.name.to_string();
                if !item.services.contains(&name) {
                    item.services.push(name);
                }
                // Resolve the installed app the first time a provider maps to one.
                if item.app_id.is_empty()
                    && let Some(app) = app_for(service, installed)
                {
                    item.app_id = app.id.clone();
                    item.service_name = service.name.to_string();
                }
            }
        }
        // Seasons (show) or runtime (movie).
        if item.is_show {
            if let Some(n) = value
                .get("number_of_seasons")
                .and_then(Value::as_i64)
                .filter(|n| *n > 0)
            {
                let label = if n == 1 {
                    &labels.season
                } else {
                    &labels.seasons
                };
                item.detail = format!("{n} {label}");
            }
        } else if let Some(minutes) = value
            .get("runtime")
            .and_then(Value::as_i64)
            .filter(|m| *m > 0)
        {
            let (h, m) = (minutes / 60, minutes % 60);
            item.detail = if h > 0 {
                format!("{h}h {m}min")
            } else {
                format!("{m}min")
            };
        }
        item.detailed = true;
    }
}

/// Localized labels for seasons (singular/plural); runtime uses h/min literally.
/// `genres` maps a TMDB genre id to a localized name, used to override the composite TV genres
/// that TMDB leaves in English (e.g. "Sci-Fi & Fantasy") so they read correctly in every language.
#[derive(Clone, Default)]
pub struct DetailLabels {
    pub season: String,
    pub seasons: String,
    /// TMDB genre id -> localized display name (filled from the UI's i18n table).
    pub genres: std::collections::HashMap<i64, String>,
}

/// Alternate two lists (a, b, a, b, …), appending any remainder, so a mixed row isn't all movies first.
fn interleave(a: Vec<Recommendation>, b: Vec<Recommendation>) -> Vec<Recommendation> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut ai, mut bi) = (a.into_iter(), b.into_iter());
    loop {
        match (ai.next(), bi.next()) {
            (Some(x), Some(y)) => {
                out.push(x);
                out.push(y);
            }
            (Some(x), None) => out.push(x),
            (None, Some(y)) => out.push(y),
            (None, None) => break,
        }
    }
    out
}

/// The cached TMDB list plus when it was fetched, persisted so the widget shows something at once.
/// Mirrors Free My TV: the full list is fetched at most once per interval, and the visible row
/// rotates through that cached list on each interval tick — the API is not hit on every rotation.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Feed {
    pub items: Vec<Recommendation>,
    /// Unix seconds of the last successful fetch.
    pub fetched_at: i64,
    /// The type the cached list was fetched for (a change forces a refetch).
    pub kind: Option<RecType>,
    /// Manual page advance added by the Refresh card (see `visible`).
    #[serde(default)]
    pub extra_shift: usize,
}

impl Feed {
    /// True when the cache should be refetched: empty, older than `interval_seconds`, or a different type.
    pub fn is_stale(&self, now: i64, interval_seconds: i64, kind: RecType) -> bool {
        self.items.is_empty()
            || self.kind != Some(kind)
            || now < self.fetched_at
            || now - self.fetched_at >= interval_seconds
    }

    /// The `count` recommendations to show now, rotating through the cached list by the interval tick,
    /// so the selection changes every `interval_seconds` without any network call (Free My TV's approach).
    pub fn visible(&self, now: i64, interval_seconds: i64, count: usize) -> Vec<Recommendation> {
        let len = self.items.len();
        if len == 0 || count == 0 {
            return Vec::new();
        }
        let interval = interval_seconds.max(1);
        let tick = now.div_euclid(interval).max(0) as usize;
        let take = count.min(len);
        // The temporal rotation advances by whole pages of `take` as time ticks.
        // `extra_shift` (the Refresh button) advances by a fixed odd stride so the shown set
        // actually changes even when the view requests the whole pool (take == len): a stride of
        // len would be a no-op mod len, so use a stride coprime-ish to len.
        let refresh_stride = (len / 3).max(1);
        let start = (tick
            .wrapping_mul(take)
            .wrapping_add(self.extra_shift.wrapping_mul(refresh_stride)))
            % len;
        (0..take)
            .map(|i| self.items[(start + i) % len].clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Worker: fetches the list and downloads+decodes posters off the SDL thread.
// Mirrors weather-service's shape (one Service, a receiver the UI drains per frame),
// but a single latest-wins job: a new request supersedes an in-flight one.
// ---------------------------------------------------------------------------
use std::path::Path;
use std::path::PathBuf;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread;

const POSTER_WIDTH: &str = "w342"; // TMDB bucket: small enough for the TV, sharp as a card
const POSTER_LIMIT: usize = 2 * 1024 * 1024;
/// Max posters fetched per refresh (keeps a row reasonable and bounds network/memory).
/// Also the upper bound for the rotating window the UI shows (see `view`/`state`/`accept`).
pub const MAX_ITEMS: usize = 24;

pub struct Request {
    pub id: u64,
    pub key: String,
    pub kind: RecType,
    pub language: String,
    pub region: String,
    /// Directory where downloaded posters are cached on disk.
    pub cache_dir: PathBuf,
    /// Localized "season"/"seasons" labels for show detail lines.
    pub labels: DetailLabels,
    /// The TV's installed apps (id/title), used to filter and resolve launchable titles.
    pub installed: Vec<InstalledApp>,
    /// Leave cartoons/anime (TMDB's Animation genre) out of the feed.
    pub hide_animation: bool,
    /// TMDB page to fetch (1-based), bumped by the Refresh card for a fresh batch of titles.
    pub page: u32,
}

pub enum Response {
    /// The whole refresh finished: the cached list and the posters that decoded.
    Loaded {
        id: u64,
        result: Result<(Vec<Recommendation>, usize), String>,
    },
}

#[derive(Default)]
struct Pending {
    job: Option<Request>,
    closed: bool,
}

pub struct Service {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    generation: Arc<AtomicU64>,
    pub receiver: mpsc::Receiver<Response>,
}

impl Service {
    pub fn new(client: Client) -> Self {
        let (sender, receiver) = mpsc::channel();
        let pending: Arc<(Mutex<Pending>, Condvar)> =
            Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let worker_pending = pending.clone();
        let worker_generation = generation.clone();
        thread::spawn(move || {
            loop {
                let job = {
                    let (mutex, ready) = &*worker_pending;
                    let mut guard = mutex.lock().unwrap();
                    while guard.job.is_none() && !guard.closed {
                        guard = ready.wait(guard).unwrap();
                    }
                    if guard.closed {
                        break;
                    }
                    guard.job.take().unwrap()
                };
                // Skip stale jobs (a newer request already arrived).
                if worker_generation.load(Ordering::SeqCst) != job.id {
                    continue;
                }
                let result = load(&client, &job, &worker_generation);
                if worker_generation.load(Ordering::SeqCst) == job.id
                    && sender
                        .send(Response::Loaded { id: job.id, result })
                        .is_err()
                {
                    break;
                }
            }
        });
        Self {
            pending,
            generation,
            receiver,
        }
    }

    /// Request a refresh; the id lets the UI ignore responses older than the latest ask.
    pub fn send(&self, request: Request) {
        self.generation.store(request.id, Ordering::SeqCst);
        let (mutex, ready) = &*self.pending;
        mutex.lock().unwrap().job = Some(request);
        ready.notify_one();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let (mutex, ready) = &*self.pending;
        let mut guard = mutex.lock().unwrap();
        guard.closed = true;
        guard.job = None;
        ready.notify_one();
    }
}

/// Fetch the recommendations list, fill any missing details, drop titles with no installed app,
/// then download each poster. Bails early if superseded. Mirrors Free My TV: every kept title is
/// watchable on an app the TV has, so clicking a card always opens something.
fn load(
    client: &Client,
    job: &Request,
    generation: &AtomicU64,
) -> Result<(Vec<Recommendation>, usize), String> {
    let items = client.trending(
        &job.key,
        job.kind,
        &job.language,
        &job.region,
        &job.installed,
        &job.labels,
        job.page,
        MAX_ITEMS,
    )?;
    let mut kept = Vec::new();
    let mut saved = 0usize;
    for mut item in items {
        if generation.load(Ordering::SeqCst) != job.id {
            break; // a newer request arrived; stop wasting bandwidth
        }
        // "Fresh" (discover) items aren't detailed yet; "hot" (trending) ones already are.
        if !item.detailed {
            client.detail(
                &mut item,
                &job.key,
                &job.language,
                &job.region,
                &job.installed,
                &job.labels,
            );
        }
        // Only keep titles an installed app can actually play.
        if item.app_id.is_empty() {
            continue;
        }
        // Optionally leave cartoons/anime out.
        if job.hide_animation && item.is_anime {
            continue;
        }
        if download_poster(client, &item, &job.cache_dir).is_some() {
            saved += 1;
        }
        kept.push(item);
    }
    Ok((kept, saved))
}

/// Download one poster from the TMDB image CDN and save it under `cache_dir`.
/// Returns the saved path (the renderer decodes and caches it as a texture when drawing).
fn download_poster(client: &Client, item: &Recommendation, cache_dir: &Path) -> Option<PathBuf> {
    use std::io::Read;
    // A stable, path-safe filename from the TMDB poster path (e.g. "/abc.jpg" -> "abc.jpg").
    let file_name = item.poster_path.trim_start_matches('/').replace('/', "_");
    if file_name.is_empty() {
        return None;
    }
    let path = cache_dir.join(&file_name);
    // Reuse a cached file if it's already here.
    if path.is_file() {
        return Some(path);
    }
    let url = item.poster_url(POSTER_WIDTH);
    let mut response = client.agent.get(&url).call().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(POSTER_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() || bytes.len() > POSTER_LIMIT {
        return None;
    }
    // Sanity check it's a decodable image before trusting it on disk.
    if image::guess_format(&bytes).is_err() {
        return None;
    }
    std::fs::create_dir_all(cache_dir).ok()?;
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

// ---------------------------------------------------------------------------
// UI-side state (lives in State; testable, holds no GL textures). Tracks the cached
// feed and decides when to ask the worker for a refresh, mirroring Free My TV's timing.
// ---------------------------------------------------------------------------

/// The widget's logical state on the UI thread.
#[derive(Default)]
pub struct Recommendations {
    /// Cached list + when it was fetched (persisted via storage, like the weather cache).
    pub feed: Feed,
    /// Monotonic request id so the worker's responses can be matched to the latest ask.
    pub request_id: u64,
    /// True while a refresh is in flight (avoids piling up requests).
    pub loading: bool,
    /// Last error text from the worker, shown in place of the row when there are no items.
    pub error: Option<String>,
    /// Focused card index within the visible row.
    pub focus: usize,
    /// Index of the first card currently shown (left edge of the visible window). The row only
    /// slides when focus would leave this window, so going back moves the cursor with a still row
    /// until it reaches the first visible card (Netflix-style).
    pub window_start: usize,
    /// Where posters are cached on disk (set by the tick; used by the view to find files).
    pub cache_dir: PathBuf,
    /// Number of items in the feed (= index of the refresh card), for carousel navigation bounds.
    pub visible_len: usize,
    /// TMDB page to fetch (1-based). The Refresh card bumps this and forces a refetch, so each
    /// refresh pulls a different batch of titles from the API instead of reshuffling the same list.
    pub page: u32,
}

impl Recommendations {
    /// Decide whether to ask the worker to refresh. Returns a `Request` to send, or None.
    /// `now` is Unix seconds; `count` is how many cards the row shows.
    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        prefs: &crate::model::Preferences,
        language: &str,
        region: &str,
        cache_dir: &Path,
        labels: &DetailLabels,
        installed: &[InstalledApp],
        now: i64,
    ) -> Option<Request> {
        if !prefs.show_recommendations || prefs.tmdb_key.trim().is_empty() || self.loading {
            return None;
        }
        if installed.is_empty() {
            return None; // nothing to recommend from yet (apps not loaded)
        }
        let interval = prefs.rec_interval.seconds();
        if !self.feed.is_stale(now, interval, prefs.rec_type) {
            return None;
        }
        self.request_id += 1;
        self.loading = true;
        Some(Request {
            id: self.request_id,
            key: prefs.tmdb_key.trim().to_string(),
            kind: prefs.rec_type,
            language: language.to_string(),
            region: region.to_string(),
            cache_dir: cache_dir.to_path_buf(),
            labels: labels.clone(),
            installed: installed.to_vec(),
            hide_animation: prefs.rec_hide_animation,
            page: self.page.max(1),
        })
    }

    /// Apply a finished refresh (newest request only). Returns the posters to upload as textures.
    pub fn accept(
        &mut self,
        id: u64,
        result: Result<(Vec<Recommendation>, usize), String>,
        now: i64,
        kind: crate::model::RecType,
    ) -> bool {
        if id != self.request_id {
            return false; // a newer request superseded this one
        }
        self.loading = false;
        match result {
            Ok((items, _saved)) => {
                self.visible_len = items.len().min(MAX_ITEMS);
                self.feed = Feed {
                    items,
                    fetched_at: now,
                    kind: Some(kind),
                    extra_shift: 0,
                };
                self.error = None;
                self.focus = 0;
                self.window_start = 0;
                true
            }
            Err(message) => {
                self.error = Some(message);
                false
            }
        }
    }

    /// The recommendations to show right now (rotated by the interval tick).
    pub fn visible(&self, now: i64, interval_seconds: i64, count: usize) -> Vec<Recommendation> {
        self.feed.visible(now, interval_seconds, count)
    }

    /// Load a previously saved feed from disk so the widget shows titles immediately at startup,
    /// without a network call, until the interval expires or the user hits Refresh.
    pub fn load_cache(&mut self, path: &Path) {
        if let Some(cache) = load_feed_cache(path) {
            self.visible_len = cache.feed.items.len().min(MAX_ITEMS);
            self.feed = cache.feed;
            self.page = cache.page.max(1);
        }
    }

    /// Persist the current feed (and page) to disk after a successful refresh.
    pub fn save_cache(&self, path: &Path) {
        let cache = FeedCache {
            feed: self.feed.clone(),
            page: self.page.max(1),
        };
        let _ = save_feed_cache(path, &cache);
    }
}

/// The feed plus the TMDB page it was fetched for, persisted together so a restart restores both.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct FeedCache {
    pub feed: Feed,
    #[serde(default)]
    pub page: u32,
}

/// Where the feed cache lives, next to the preferences file (mirrors the weather cache).
pub fn cache_path(preferences: &Path) -> PathBuf {
    preferences.with_extension("recommendations.json")
}

/// Read the saved feed cache, bounded like the weather cache. None on any problem.
pub fn load_feed_cache(path: &Path) -> Option<FeedCache> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(512 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 512 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// Atomically write the feed cache to disk.
pub fn save_feed_cache(path: &Path, cache: &FeedCache) -> Result<(), String> {
    crate::storage::write_atomic(path, &serde_json::to_vec(cache).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod feed_tests {
    use super::*;
    use crate::model::RecType;

    fn rec(n: usize) -> Recommendation {
        Recommendation {
            title: format!("t{n}"),
            poster_path: format!("/p{n}.jpg"),
            ..Default::default()
        }
    }

    #[test]
    fn stale_when_empty_old_or_type_changed() {
        let feed = Feed {
            items: vec![rec(1)],
            fetched_at: 1000,
            kind: Some(RecType::Both),
            extra_shift: 0,
        };
        assert!(
            !feed.is_stale(1000 + 3599, 3600, RecType::Both),
            "fresh dentro do intervalo"
        );
        assert!(
            feed.is_stale(1000 + 3600, 3600, RecType::Both),
            "stale no intervalo"
        );
        assert!(
            feed.is_stale(1500, 3600, RecType::Movies),
            "stale se mudou o tipo"
        );
        let empty = Feed::default();
        assert!(empty.is_stale(0, 3600, RecType::Both), "vazio sempre stale");
    }

    #[test]
    fn visible_rotates_by_tick_without_repeating_within_a_window() {
        let feed = Feed {
            items: (0..10).map(rec).collect(),
            fetched_at: 0,
            kind: Some(RecType::Both),
            extra_shift: 0,
        };
        let a = feed.visible(0, 3600, 3);
        let b = feed.visible(3600, 3600, 3); // proximo tick
        assert_eq!(a.len(), 3);
        assert_ne!(a, b, "a selecao muda a cada intervalo");
        // dentro do mesmo tick, estavel
        assert_eq!(feed.visible(100, 3600, 3), feed.visible(3599, 3600, 3));
    }

    #[test]
    fn refresh_extra_shift_changes_the_set_even_for_the_whole_pool() {
        // Regressão: antes, pedir o pool inteiro (take == len) zerava o deslocamento do refresh
        // (x * len % len == 0), então o botão Atualizar não trocava nada.
        let base = Feed {
            items: (0..24).map(rec).collect(),
            fetched_at: 0,
            kind: Some(RecType::Both),
            extra_shift: 0,
        };
        let shifted = Feed {
            extra_shift: 1,
            ..Feed {
                items: (0..24).map(rec).collect(),
                fetched_at: 0,
                kind: Some(RecType::Both),
                extra_shift: 0,
            }
        };
        // Mesmo pedindo todos os 24, a ordem (primeiros itens) muda com o refresh.
        let a = base.visible(0, 3600, 24);
        let b = shifted.visible(0, 3600, 24);
        assert_eq!(a.len(), 24);
        assert_eq!(b.len(), 24);
        assert_ne!(a[0], b[0], "o refresh precisa trocar o conjunto visível");
    }
}
