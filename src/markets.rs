//! Optional, key-free price widgets. All HTTP stays on a bounded worker queue.
use crate::{
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
    time::Duration,
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
    let value = value.trim().to_ascii_uppercase();
    let limit = if kind == HomeWidget::Crypto { 12 } else { 32 };
    (!value.is_empty()
        && value.len() <= limit
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|c| {
            c.is_ascii_alphanumeric() || (kind == HomeWidget::Stocks && matches!(c, b'.' | b'-'))
        }))
    .then_some(value)
}
pub fn index(kind: HomeWidget) -> usize {
    usize::from(kind == HomeWidget::Stocks)
}
pub fn settings(preferences: &Preferences, kind: HomeWidget) -> &Settings {
    if kind == HomeWidget::Crypto {
        &preferences.crypto
    } else {
        &preferences.stocks
    }
}
pub fn settings_mut(preferences: &mut Preferences, kind: HomeWidget) -> &mut Settings {
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
        if kind == HomeWidget::Crypto && !CURRENCIES.contains(&config.currency.as_str()) {
            return None;
        }
        Some(Self {
            kind,
            symbol: normalize_symbol(kind, &config.symbol)?,
            currency: if kind == HomeWidget::Crypto {
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
            || !matches!(meta["instrumentType"].as_str()?, "EQUITY" | "ETF")
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
#[derive(Default)]
pub struct Markets {
    pub slots: [Slot; 2],
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
    generations: Arc<[AtomicU64; 2]>,
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
        let (sender, jobs) = mpsc::sync_channel::<Job>(2);
        let (results, receiver) = mpsc::sync_channel(2);
        let generations = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
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
    let endpoint = if key.kind == HomeWidget::Crypto {
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
            key.symbol
        )
    };
    let mut response = agent
        .get(&endpoint)
        .header("User-Agent", "LumaNative")
        .header("Accept", "application/json")
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
    parse_quote(
        key.kind,
        &key.symbol,
        &key.currency,
        &serde_json::from_slice::<Value>(&bytes).ok()?,
        time::OffsetDateTime::now_utc().unix_timestamp(),
    )
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
        for kind in [HomeWidget::Crypto, HomeWidget::Stocks] {
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
    fn scheduling_is_optional_visible_only_throttled_and_rejects_stale_results() {
        let (sender, jobs) = mpsc::sync_channel(2);
        let (results, receiver) = mpsc::sync_channel(2);
        let service = Service {
            sender: Some(sender),
            receiver,
            generations: Arc::new([AtomicU64::new(0), AtomicU64::new(0)]),
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
