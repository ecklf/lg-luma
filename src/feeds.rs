//! Data for Home widgets, fetched on a worker thread: Binance 24 h mini tickers for crypto prices,
//! TVmaze season premieres and their posters. As for weather: fixed HTTPS origins, no proxy or
//! redirects, bounded replies and timeouts; nothing runs on the SDL thread.
use crate::app_artwork::Artwork;
use serde_json::Value;
use std::{
    io::Read,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

/// Coins shown left to right; a wider widget shows more of them.
pub const COINS: [&str; 6] = ["BTC", "ETH", "SOL", "BNB", "XRP", "DOGE"];
const PRICE_HOSTS: [&str; 2] = ["https://api.binance.com", "https://api.binance.us"];
const SCHEDULES: [&str; 2] = [
    "https://api.tvmaze.com/schedule/web",
    "https://api.tvmaze.com/schedule",
];
const POSTER_ORIGIN: &str = "https://static.tvmaze.com/";
const JSON_LIMIT: usize = 1024 * 1024;
const IMAGE_LIMIT: usize = 2 * 1024 * 1024;
/// Popular shows only: TVmaze's weight is its 0–100 popularity.
const MIN_WEIGHT: u64 = 95;
const PREMIERES: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub coin: &'static str,
    /// In US dollars (USDT pairs).
    pub last: f64,
    /// Percent over 24 hours.
    pub change: f64,
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Premiere {
    pub title: String,
    pub season: u64,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub poster: Option<String>,
}

/// What Home's widgets show; filled from `FeedResponse`s on the SDL thread.
#[derive(Default)]
pub struct FeedData {
    pub quotes: Vec<Quote>,
    pub premieres: Vec<Premiere>,
    pub posters: std::collections::HashMap<String, Arc<Artwork>>,
    pub crypto_failed: bool,
    pub premieres_failed: bool,
    /// Store widgets' logos, photos and posters by URL.
    pub images: std::collections::HashMap<String, Arc<Artwork>>,
    /// Store widget image URLs already asked for, loaded or not: a failed one is not retried.
    pub requested: std::collections::HashSet<String>,
    /// Store widgets may want images not asked for yet.
    pub images_due: bool,
}

/// A loaded logo's backdrop when it has no colour of its own.
pub const NO_ACCENT: [u8; 3] = [16, 16, 20];

pub enum FeedRequest {
    Crypto,
    Premieres,
    Poster(String),
    /// A store widget's image; the widget's origins were checked already.
    WidgetImage {
        url: String,
        poster: bool,
    },
}

pub enum FeedResponse {
    Crypto(Result<Vec<Quote>, String>),
    Premieres(Result<Vec<Premiere>, String>),
    Poster(String, Option<Arc<Artwork>>),
    WidgetImage(String, Option<Arc<Artwork>>),
}

pub struct Feeds {
    sender: mpsc::Sender<FeedRequest>,
    pub receiver: mpsc::Receiver<FeedResponse>,
}

impl Feeds {
    pub fn new() -> Self {
        let (sender, requests) = mpsc::channel();
        let (responses, receiver) = mpsc::channel();
        thread::spawn(move || {
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .max_redirects(0)
                .proxy(None)
                .build()
                .new_agent();
            while let Ok(request) = requests.recv() {
                let response = match request {
                    FeedRequest::Crypto => FeedResponse::Crypto(quotes(&agent)),
                    FeedRequest::Premieres => FeedResponse::Premieres(premieres(&agent)),
                    FeedRequest::Poster(url) => {
                        let image = poster(&agent, &url);
                        FeedResponse::Poster(url, image)
                    }
                    FeedRequest::WidgetImage { url, poster } => {
                        let image = widget_image(&agent, &url, poster);
                        FeedResponse::WidgetImage(url, image)
                    }
                };
                if responses.send(response).is_err() {
                    break;
                }
            }
        });
        Self { sender, receiver }
    }

    pub fn send(&self, request: FeedRequest) {
        let _ = self.sender.send(request);
    }
}

fn get(agent: &ureq::Agent, url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut response = agent
        .get(url)
        .header("User-Agent", "LumaNative")
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{url}: {e}"))?;
    if bytes.len() > limit {
        return Err(format!("{url}: reply too large"));
    }
    Ok(bytes)
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<Value, String> {
    serde_json::from_slice(&get(agent, url, JSON_LIMIT)?).map_err(|e| format!("{url}: {e}"))
}

fn quotes(agent: &ureq::Agent) -> Result<Vec<Quote>, String> {
    let symbols = COINS
        .iter()
        .map(|coin| format!("%22{coin}USDT%22"))
        .collect::<Vec<_>>()
        .join(",");
    let mut error = String::new();
    // Binance.com refuses some regions; Binance.US answers the same call.
    for host in PRICE_HOSTS {
        let url = format!("{host}/api/v3/ticker/24hr?type=MINI&symbols=%5B{symbols}%5D");
        match get_json(agent, &url).and_then(|body| parse_quotes(&body)) {
            Ok(quotes) if !quotes.is_empty() => return Ok(quotes),
            Ok(_) => error = format!("{host}: no prices"),
            Err(e) => error = e,
        }
    }
    Err(error)
}

/// Mini tickers in `COINS` order; pairs Binance does not list are left out.
pub fn parse_quotes(body: &Value) -> Result<Vec<Quote>, String> {
    let items = body
        .as_array()
        .ok_or("Binance did not return a ticker list")?;
    let number = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<f64>().ok())
    };
    Ok(COINS
        .iter()
        .filter_map(|coin| {
            let symbol = format!("{coin}USDT");
            let item = items
                .iter()
                .find(|item| item.get("symbol").and_then(Value::as_str) == Some(&symbol))?;
            let (open, last) = (number(item, "openPrice")?, number(item, "lastPrice")?);
            Some(Quote {
                coin,
                last,
                change: if open > 0. {
                    (last - open) / open * 100.
                } else {
                    0.
                },
                low: number(item, "lowPrice")?,
                high: number(item, "highPrice")?,
            })
        })
        .collect())
}

/// Dates from three days ago to ten days ahead, `YYYY-MM-DD`.
pub fn schedule_days(today: time::Date) -> Vec<String> {
    (-3..=10)
        .filter_map(|offset| today.checked_add(time::Duration::days(offset)))
        .map(|day| {
            format!(
                "{:04}-{:02}-{:02}",
                day.year(),
                day.month() as u8,
                day.day()
            )
        })
        .collect()
}

fn premieres(agent: &ureq::Agent) -> Result<Vec<Premiere>, String> {
    let today = time::OffsetDateTime::now_utc().date();
    let mut days = Vec::new();
    let mut failures = 0;
    for day in schedule_days(today) {
        for schedule in SCHEDULES {
            let url = if schedule.ends_with("/web") {
                format!("{schedule}?date={day}")
            } else {
                format!("{schedule}?country=US&date={day}")
            };
            match get_json(agent, &url) {
                Ok(body) => days.push(body),
                Err(error) => {
                    failures += 1;
                    eprintln!("Luma premieres: {error}");
                }
            }
        }
    }
    if days.is_empty() {
        return Err(format!("TVmaze unavailable ({failures} requests failed)"));
    }
    Ok(parse_premieres(&days))
}

/// First episodes of popular scripted or animated shows, one per show, by air date.
pub fn parse_premieres(days: &[Value]) -> Vec<Premiere> {
    let mut found: Vec<(u64, u64, Premiere)> = Vec::new();
    for episode in days.iter().filter_map(Value::as_array).flatten() {
        let show = episode
            .pointer("/_embedded/show")
            .or_else(|| episode.get("show"));
        let Some(show) = show else { continue };
        let kind = show.get("type").and_then(Value::as_str).unwrap_or("");
        let weight = show.get("weight").and_then(Value::as_u64).unwrap_or(0);
        if episode.get("number").and_then(Value::as_u64) != Some(1)
            || weight < MIN_WEIGHT
            || !matches!(kind, "Scripted" | "Animation")
        {
            continue;
        }
        let id = show.get("id").and_then(Value::as_u64).unwrap_or(0);
        if found.iter().any(|(other, ..)| *other == id) {
            continue;
        }
        let (Some(title), Some(date)) = (
            show.get("name").and_then(Value::as_str),
            episode.get("airdate").and_then(Value::as_str),
        ) else {
            continue;
        };
        let poster = show
            .pointer("/image/medium")
            .and_then(Value::as_str)
            .filter(|url| poster_allowed(url))
            .map(str::to_owned);
        found.push((
            id,
            weight,
            Premiere {
                title: title.chars().take(80).collect(),
                season: episode.get("season").and_then(Value::as_u64).unwrap_or(1),
                date: date.chars().take(10).collect(),
                poster,
            },
        ));
    }
    found.sort_by(|a, b| a.2.date.cmp(&b.2.date).then(b.1.cmp(&a.1)));
    found
        .into_iter()
        .take(PREMIERES)
        .map(|(.., premiere)| premiere)
        .collect()
}

pub fn poster_allowed(url: &str) -> bool {
    url.starts_with(POSTER_ORIGIN) && !url.contains("..") && url.len() < 512
}

fn poster(agent: &ureq::Agent, url: &str) -> Option<Arc<Artwork>> {
    if !poster_allowed(url) {
        return None;
    }
    let bytes = get(agent, url, IMAGE_LIMIT)
        .map_err(|error| eprintln!("Luma poster: {error}"))
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let pixels = image.thumbnail(300, 450).into_rgba8();
    Some(Arc::new(Artwork {
        key: format!("poster:{url}"),
        pixels,
        backdrop: [[16, 16, 20]; 2],
    }))
}

/// A store widget's image over HTTPS, decoded and shrunk here, off the SDL thread: a poster up
/// to 300×450, a logo or photo up to 160×160 carrying its own colour.
fn widget_image(agent: &ureq::Agent, url: &str, poster: bool) -> Option<Arc<Artwork>> {
    if !url.starts_with("https://") {
        return None;
    }
    let bytes = get(agent, url, IMAGE_LIMIT)
        .map_err(|error| eprintln!("Luma widget image: {error}"))
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let (w, h) = if poster { (300, 450) } else { (160, 160) };
    let pixels = image.thumbnail(w, h).into_rgba8();
    // A logo's own colour tints the dialog around it.
    let accent = crate::app_artwork::accent(&pixels);
    Some(Arc::new(Artwork {
        key: format!("image:{url}"),
        pixels,
        backdrop: accent.map_or([NO_ACCENT; 2], |accent| [accent; 2]),
    }))
}

#[cfg(test)]
#[path = "feeds-tests.rs"]
mod tests;
