//! Optional, key-free price widgets. All HTTP stays on a bounded worker queue.
use crate::{
    i18n::Language,
    model::{HomeWidget, Preferences},
    state::Page,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Read,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub const CURRENCIES: [&str; 7] = ["USD", "EUR", "GBP", "JPY", "CAD", "AUD", "CHF"];
const REFRESH: i64 = 300;
const LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub symbol: String,
    pub currency: String,
    pub position: crate::model::WidgetPosition,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            symbol: String::new(),
            currency: "USD".into(),
            position: crate::model::WidgetPosition { column: 0, row: 2 },
        }
    }
}
impl Settings {
    pub fn sanitize(&mut self, kind: HomeWidget) {
        self.symbol = normalize_symbol(kind, &self.symbol).unwrap_or_default();
        if !CURRENCIES.contains(&self.currency.as_str()) {
            self.currency = "USD".into();
        }
    }
}
pub fn deserialize_settings<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Settings, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).unwrap_or_default())
}
pub fn normalize_symbol(kind: HomeWidget, value: &str) -> Option<String> {
    let kind = kind.kind();
    let value = value.trim().to_ascii_uppercase();
    let limit = if kind == HomeWidget::Crypto { 12 } else { 32 };
    // Yahoo index tickers have one leading caret (for example, ^GSPC).
    let ticker = if kind == HomeWidget::Stocks {
        value.strip_prefix('^').unwrap_or(&value)
    } else {
        &value
    };
    (!value.is_empty()
        && value.len() <= limit
        && ticker
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && ticker.bytes().all(|c| {
            c.is_ascii_alphanumeric() || (kind == HomeWidget::Stocks && matches!(c, b'.' | b'-'))
        }))
    .then_some(value)
}
pub fn index(kind: HomeWidget) -> usize {
    match kind {
        HomeWidget::ExtraCrypto(i) => 2 + usize::from(i) * 2,
        HomeWidget::ExtraStocks(i) => 3 + usize::from(i) * 2,
        _ => usize::from(kind == HomeWidget::Stocks),
    }
}
pub fn settings(preferences: &Preferences, kind: HomeWidget) -> &Settings {
    if let HomeWidget::ExtraCrypto(i) = kind {
        return &preferences.extra_crypto[usize::from(i)];
    }
    if let HomeWidget::ExtraStocks(i) = kind {
        return &preferences.extra_stocks[usize::from(i)];
    }
    if kind == HomeWidget::Crypto {
        &preferences.crypto
    } else {
        &preferences.stocks
    }
}
pub fn settings_mut(preferences: &mut Preferences, kind: HomeWidget) -> &mut Settings {
    if let HomeWidget::ExtraCrypto(i) = kind {
        return &mut preferences.extra_crypto[usize::from(i)];
    }
    if let HomeWidget::ExtraStocks(i) = kind {
        return &mut preferences.extra_stocks[usize::from(i)];
    }
    if kind == HomeWidget::Crypto {
        &mut preferences.crypto
    } else {
        &mut preferences.stocks
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    kind: HomeWidget,
    symbol: String,
    currency: String,
}
impl Key {
    fn new(kind: HomeWidget, config: &Settings) -> Option<Self> {
        if kind.kind() == HomeWidget::Crypto && !CURRENCIES.contains(&config.currency.as_str()) {
            return None;
        }
        Some(Self {
            kind,
            symbol: normalize_symbol(kind, &config.symbol)?,
            currency: if kind.kind() == HomeWidget::Crypto {
                config.currency.clone()
            } else {
                String::new()
            },
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Quote {
    pub price: f64,
    pub currency: String,
    pub timestamp: i64,
    pub change_percent: Option<f64>,
}
pub fn daily_change(current: f64, previous: f64) -> Option<f64> {
    if !price(current) || !price(previous) {
        return None;
    }
    let change = (current / previous - 1.) * 100.;
    change.is_finite().then_some(change)
}
pub fn relative_age(language: Language, timestamp: i64, now: i64) -> String {
    let seconds = now.saturating_sub(timestamp).max(0);
    let (key, count) = match seconds {
        0..60 => return language.t("marketJustNow").into(),
        60..3600 => ("marketMinutesAgo", seconds / 60),
        3600..86400 => ("marketHoursAgo", seconds / 3600),
        _ => ("marketDaysAgo", seconds / 86400),
    };
    language.format(key, &[("count", &count.to_string())])
}
fn price(value: f64) -> bool {
    value.is_finite() && value > 0. && value <= 1e12
}
pub fn parse_quote(
    kind: HomeWidget,
    symbol: &str,
    currency: &str,
    value: &Value,
    now: i64,
) -> Option<Quote> {
    let kind = kind.kind();
    let (amount, currency, timestamp) = if kind == HomeWidget::Crypto {
        let data = &value["data"];
        if data["currency"].as_str()? != currency {
            return None;
        }
        if data
            .get("base")
            .and_then(Value::as_str)
            .is_some_and(|base| base != symbol)
        {
            return None;
        }
        (
            data["amount"].as_str()?.parse::<f64>().ok()?,
            currency.to_owned(),
            now,
        )
    } else {
        if !value["chart"]["error"].is_null() {
            return None;
        }
        let meta = &value["chart"]["result"].as_array()?.first()?["meta"];
        if meta["symbol"].as_str()? != symbol
            || !matches!(meta["instrumentType"].as_str()?, "EQUITY" | "ETF" | "INDEX")
        {
            return None;
        }
        (
            meta["regularMarketPrice"].as_f64()?,
            meta["currency"].as_str()?.to_owned(),
            meta["regularMarketTime"].as_i64()?,
        )
    };
    if !price(amount)
        || currency.len() != 3
        || !currency.bytes().all(|c| c.is_ascii_alphabetic())
        || timestamp <= 0
        || timestamp > now + 600
    {
        return None;
    }
    Some(Quote {
        price: amount,
        currency,
        timestamp,
        change_percent: if kind == HomeWidget::Stocks {
            let meta = &value["chart"]["result"][0]["meta"];
            meta["chartPreviousClose"]
                .as_f64()
                .or_else(|| meta["previousClose"].as_f64())
                .and_then(|previous| daily_change(amount, previous))
        } else {
            None
        },
    })
}
pub fn format_price(quote: &Quote) -> String {
    if quote.price < 0.000001 {
        format!("{:.4e} {}", quote.price, quote.currency)
    } else if quote.price < 1. {
        format!("{:.6} {}", quote.price, quote.currency)
    } else {
        format!("{:.2} {}", quote.price, quote.currency)
    }
}

#[derive(Default)]
pub struct Slot {
    key: Option<Key>,
    generation: u64,
    pending: bool,
    next: i64,
    pub quote: Option<Quote>,
    pub failed: bool,
}
pub struct Markets {
    pub slots: Vec<Slot>,
}
impl Default for Markets {
    fn default() -> Self {
        Self {
            slots: (0..50).map(|_| Slot::default()).collect(),
        }
    }
}
struct Job {
    key: Key,
    generation: u64,
}
struct Reply {
    job: Job,
    quote: Option<Quote>,
}
pub struct Service {
    generations: Arc<Vec<AtomicU64>>,
    sender: Option<mpsc::SyncSender<Job>>,
    receiver: mpsc::Receiver<Reply>,
}

impl Service {
    pub fn new(test_server: Option<&str>, enabled: bool) -> Result<Self, String> {
        let origin = if let Some(server) = test_server {
            let uri: ureq::http::Uri = server.parse().map_err(|_| "Invalid market test server")?;
            let address: SocketAddr = uri
                .authority()
                .ok_or("Missing market test address")?
                .as_str()
                .parse()
                .map_err(|_| "Market tests need an IP-literal loopback address")?;
            if uri.scheme_str() != Some("http")
                || !address.ip().is_loopback()
                || address.port() == 0
                || uri.path() != "/"
                || uri.query().is_some()
            {
                return Err("Market tests need an HTTP loopback origin".into());
            }
            Some(format!("http://{address}"))
        } else {
            None
        };
        let (sender, jobs) = mpsc::sync_channel::<Job>(50);
        let (results, receiver) = mpsc::sync_channel(50);
        let generations = Arc::new((0..50).map(|_| AtomicU64::new(0)).collect::<Vec<_>>());
        if enabled || origin.is_some() {
            let active = generations.clone();
            thread::Builder::new()
                .name("market-quotes".into())
                .spawn(move || {
                    let agent = ureq::Agent::config_builder()
                        .timeout_global(Some(Duration::from_secs(8)))
                        .max_redirects(0)
                        .proxy(None)
                        .build()
                        .new_agent();
                    while let Ok(job) = jobs.recv() {
                        if active[index(job.key.kind)].load(Ordering::SeqCst) != job.generation {
                            continue;
                        }
                        let quote = fetch(&agent, origin.as_deref(), &job.key);
                        if results.send(Reply { job, quote }).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(Self {
                generations,
                sender: Some(sender),
                receiver,
            })
        } else {
            Ok(Self {
                generations,
                sender: None,
                receiver,
            })
        }
    }
}
fn fetch(agent: &ureq::Agent, origin: Option<&str>, key: &Key) -> Option<Quote> {
    let started = Instant::now();
    let endpoint = if key.kind.kind() == HomeWidget::Crypto {
        format!(
            "{}/v2/prices/{}-{}/spot",
            origin.unwrap_or("https://api.coinbase.com"),
            key.symbol,
            key.currency
        )
    } else {
        format!(
            "{}/v8/finance/chart/{}?interval=1d&range=1d",
            origin.unwrap_or("https://query1.finance.yahoo.com"),
            key.symbol.replace('^', "%5E")
        )
    };
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut quote = parse_quote(
        key.kind,
        &key.symbol,
        &key.currency,
        &request_json(agent, &endpoint, Duration::from_secs(8))?,
        now,
    )?;
    if key.kind.kind() == HomeWidget::Crypto
        && let Some(previous_day) = time::OffsetDateTime::from_unix_timestamp(now)
            .ok()?
            .date()
            .previous_day()
        && let Some(remaining) = Duration::from_secs(8)
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
        && let Some(history) =
            request_json(agent, &format!("{endpoint}?date={previous_day}"), remaining)
        && let Some(previous) = parse_quote(key.kind, &key.symbol, &key.currency, &history, now)
    {
        quote.change_percent = daily_change(quote.price, previous.price);
    }
    Some(quote)
}
fn request_json(agent: &ureq::Agent, endpoint: &str, timeout: Duration) -> Option<Value> {
    let mut response = agent
        .get(endpoint)
        .header("User-Agent", "LumaNative")
        .header("Accept", "application/json")
        .config()
        .timeout_global(Some(timeout))
        .build()
        .call()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > LIMIT {
        return None;
    }
    serde_json::from_slice::<Value>(&bytes).ok()
}
impl Markets {
    pub fn tick(
        &mut self,
        service: &Service,
        preferences: &Preferences,
        page: Page,
        visible: bool,
        now: i64,
    ) -> bool {
        let mut changed = false;
        for kind in preferences
            .widgets()
            .into_iter()
            .filter(|kind| matches!(kind.kind(), HomeWidget::Crypto | HomeWidget::Stocks))
        {
            let config = settings(preferences, kind);
            let key = config.enabled.then(|| Key::new(kind, config)).flatten();
            let slot = &mut self.slots[index(kind)];
            if slot.key != key {
                slot.key = key;
                slot.generation = slot.generation.wrapping_add(1);
                slot.pending = false;
                slot.next = 0;
                slot.quote = None;
                slot.failed = false;
                changed = true;
            }
            service.generations[index(kind)].store(slot.generation, Ordering::SeqCst);
            if visible
                && page == Page::Home
                && !slot.pending
                && now >= slot.next
                && let (Some(key), Some(sender)) = (&slot.key, &service.sender)
                && sender
                    .try_send(Job {
                        key: key.clone(),
                        generation: slot.generation,
                    })
                    .is_ok()
            {
                slot.pending = true;
                changed = true;
            }
        }
        while let Ok(reply) = service.receiver.try_recv() {
            let slot = &mut self.slots[index(reply.job.key.kind)];
            if slot.key.as_ref() != Some(&reply.job.key) || slot.generation != reply.job.generation
            {
                continue;
            }
            slot.pending = false;
            slot.next = now + REFRESH;
            slot.failed = reply.quote.is_none();
            if reply.quote.is_some() {
                slot.quote = reply.quote;
            }
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn relative_ages_use_compact_units_and_clamp_clock_skew() {
        for (seconds, expected) in [
            (0, "Just now"),
            (59, "Just now"),
            (60, "1m ago"),
            (3599, "59m ago"),
            (3600, "1h ago"),
            (86399, "23h ago"),
            (86400, "1d ago"),
            (172800, "2d ago"),
        ] {
            assert_eq!(relative_age(Language::En, 1000, 1000 + seconds), expected);
        }
        assert_eq!(relative_age(Language::En, 1100, 1000), "Just now");
        for language in Language::ALL {
            assert_eq!(
                relative_age(language, 1000, 1060),
                language.format("marketMinutesAgo", &[("count", "1")])
            );
        }
    }

    #[test]
    fn daily_changes_use_a_valid_baseline_and_missing_baselines_stay_unknown() {
        assert!((daily_change(104., 100.).unwrap() - 4.).abs() < 1e-10);
        assert!((daily_change(98., 100.).unwrap() + 2.).abs() < 1e-10);
        assert_eq!(daily_change(100., 100.), Some(0.));
        for baseline in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(daily_change(100., baseline).is_none());
        }
        let mut stock = json!({"chart":{"error":null,"result":[{"meta":{"symbol":"AAPL","instrumentType":"EQUITY","currency":"USD","regularMarketPrice":104.,"regularMarketTime":900,"chartPreviousClose":100.}}]}});
        assert!(
            (parse_quote(HomeWidget::Stocks, "AAPL", "", &stock, 1000)
                .unwrap()
                .change_percent
                .unwrap()
                - 4.)
                .abs()
                < 1e-10
        );
        stock["chart"]["result"][0]["meta"]["chartPreviousClose"] = json!(0);
        assert!(
            parse_quote(HomeWidget::Stocks, "AAPL", "", &stock, 1000)
                .unwrap()
                .change_percent
                .is_none()
        );
    }
    #[test]
    fn scheduling_is_optional_visible_only_throttled_and_rejects_stale_results() {
        let (sender, jobs) = mpsc::sync_channel(2);
        let (results, receiver) = mpsc::sync_channel(2);
        let service = Service {
            sender: Some(sender),
            receiver,
            generations: Arc::new((0..50).map(|_| AtomicU64::new(0)).collect()),
        };
        let mut preferences = Preferences::default();
        let mut markets = Markets::default();
        markets.tick(&service, &preferences, Page::Home, true, 1000);
        assert!(jobs.try_recv().is_err());
        preferences.crypto.enabled = true;
        preferences.crypto.symbol = "BTC".into();
        markets.tick(&service, &preferences, Page::Settings, true, 1000);
        markets.tick(&service, &preferences, Page::Home, false, 1000);
        assert!(jobs.try_recv().is_err());
        markets.tick(&service, &preferences, Page::Home, true, 1000);
        let old = jobs.try_recv().unwrap();
        markets.tick(&service, &preferences, Page::Home, true, 1001);
        assert!(
            jobs.try_recv().is_err(),
            "Only one in-flight job per widget"
        );
        preferences.crypto.symbol = "ETH".into();
        markets.tick(&service, &preferences, Page::Home, true, 1002);
        let current = jobs.try_recv().unwrap();
        results
            .send(Reply {
                job: old,
                quote: Some(Quote {
                    change_percent: None,
                    price: 999.,
                    currency: "USD".into(),
                    timestamp: 1000,
                }),
            })
            .unwrap();
        markets.tick(&service, &preferences, Page::Home, true, 1002);
        assert!(markets.slots[0].quote.is_none());
        results
            .send(Reply {
                job: current,
                quote: Some(Quote {
                    change_percent: None,
                    price: 123.,
                    currency: "USD".into(),
                    timestamp: 1002,
                }),
            })
            .unwrap();
        markets.tick(&service, &preferences, Page::Home, true, 1003);
        assert_eq!(markets.slots[0].quote.as_ref().unwrap().price, 123.);
        markets.tick(&service, &preferences, Page::Home, true, 1302);
        assert!(jobs.try_recv().is_err());
        markets.tick(&service, &preferences, Page::Home, true, 1303);
        let retry = jobs.try_recv().unwrap();
        results
            .send(Reply {
                job: retry,
                quote: None,
            })
            .unwrap();
        markets.tick(&service, &preferences, Page::Home, true, 1304);
        assert!(markets.slots[0].failed);
        assert_eq!(markets.slots[0].quote.as_ref().unwrap().price, 123.);
        preferences.crypto.enabled = false;
        markets.tick(&service, &preferences, Page::Home, true, 1605);
        assert!(jobs.try_recv().is_err());
        assert!(markets.slots[0].quote.is_none());
    }

    #[test]
    fn tiny_prices_do_not_round_to_zero_and_stock_time_and_units_are_validated() {
        assert!(
            format_price(&Quote {
                change_percent: None,
                price: 0.00000001,
                currency: "USD".into(),
                timestamp: 1000
            })
            .contains("e-")
        );
        for (currency, timestamp) in [("", 900), ("US/D", 900), ("USD", 0), ("USD", 2000)] {
            let value = json!({"chart":{"error":null,"result":[{"meta":{"symbol":"AAPL","instrumentType":"EQUITY","currency":currency,"regularMarketPrice":123.4,"regularMarketTime":timestamp}}]}});
            assert!(parse_quote(HomeWidget::Stocks, "AAPL", "", &value, 1000).is_none());
        }
    }
    #[test]
    fn index_symbols_are_validated_and_index_quotes_are_accepted() {
        for kind in [HomeWidget::Stocks, HomeWidget::ExtraStocks(0)] {
            assert_eq!(normalize_symbol(kind, " ^gspc ").as_deref(), Some("^GSPC"));
            for invalid in ["^", "^^GSPC", "GS^PC", "^../GSPC", "^GSPC?x=1"] {
                assert!(normalize_symbol(kind, invalid).is_none());
            }
        }
        assert!(normalize_symbol(HomeWidget::Crypto, "^GSPC").is_none());
        let mut config = Settings {
            symbol: "^gspc".into(),
            ..Default::default()
        };
        config.sanitize(HomeWidget::Stocks);
        assert_eq!(config.symbol, "^GSPC");
        let mut value = json!({"chart":{"error":null,"result":[{"meta":{"symbol":"^GSPC","instrumentType":"INDEX","currency":"USD","regularMarketPrice":5100.,"regularMarketTime":900,"chartPreviousClose":5000.}}]}});
        let quote = parse_quote(HomeWidget::Stocks, "^GSPC", "", &value, 1000).unwrap();
        assert_eq!(quote.price, 5100.);
        assert!((quote.change_percent.unwrap() - 2.).abs() < 1e-10);
        assert!(parse_quote(HomeWidget::Stocks, "^DJI", "", &value, 1000).is_none());
        value["chart"]["result"][0]["meta"]["instrumentType"] = json!("FUTURE");
        assert!(parse_quote(HomeWidget::Stocks, "^GSPC", "", &value, 1000).is_none());
    }

    #[test]
    fn symbols_cannot_change_the_request_path_or_origin() {
        for symbol in [
            "../BTC",
            "BTC/USD",
            "AAPL?x=1",
            "https://evil",
            "AAPL\n",
            "",
        ] {
            if symbol == "AAPL\n" {
                continue;
            }
            assert!(normalize_symbol(HomeWidget::Stocks, symbol).is_none());
        }
        assert_eq!(
            normalize_symbol(HomeWidget::Stocks, " brk-b ").as_deref(),
            Some("BRK-B")
        );
        assert_eq!(
            normalize_symbol(HomeWidget::Crypto, "btc").as_deref(),
            Some("BTC")
        );
    }
    #[test]
    fn quotes_validate_identity_prices_currency_and_quote_time() {
        let crypto = json!({"data":{"amount":"12.345", "currency":"USD", "base":"BTC"}});
        assert!(parse_quote(HomeWidget::Crypto, "BTC", "USD", &crypto, 1000).is_some());
        assert!(parse_quote(HomeWidget::Crypto, "ETH", "USD", &crypto, 1000).is_none());
        for amount in ["NaN", "inf", "-1", "0", "1e20"] {
            assert!(
                parse_quote(
                    HomeWidget::Crypto,
                    "BTC",
                    "USD",
                    &json!({"data":{"amount":amount,"currency":"USD"}}),
                    1000
                )
                .is_none()
            );
        }
        let stock = json!({"chart":{"error":null,"result":[{"meta":{"symbol":"AAPL","instrumentType":"EQUITY","currency":"USD","regularMarketPrice":123.4,"regularMarketTime":900}}]}});
        assert!(parse_quote(HomeWidget::Stocks, "AAPL", "", &stock, 1000).is_some());
        assert!(parse_quote(HomeWidget::Stocks, "MSFT", "", &stock, 1000).is_none());
    }
    #[test]
    fn mock_origins_are_loopback_only() {
        for origin in [
            "https://example.com",
            "http://localhost:1234",
            "http://127.0.0.1:1234/path",
            "http://127.0.0.1:1234/?x=1",
        ] {
            assert!(Service::new(Some(origin), false).is_err());
        }
        assert!(Service::new(Some("http://127.0.0.1:1234"), false).is_ok());
    }
}
