//! Crypto prices for Home: Binance 24-hour tickers against USDT, fetched only while Home shows
//! the widget. Requests and scheduling mirror weather: one job in flight, generation IDs, and
//! stale data dropped rather than shown as current.
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The coins Settings lists first, as (symbol, name); any other Binance coin is found by search.
pub const COINS: [(&str, &str); 8] = [
    ("BTC", "Bitcoin"),
    ("ETH", "Ethereum"),
    ("SOL", "Solana"),
    ("BNB", "BNB"),
    ("XRP", "XRP"),
    ("ADA", "Cardano"),
    ("DOGE", "Dogecoin"),
    ("TON", "Toncoin"),
];
/// Home has room for four cards in a row.
pub const MAX_COINS: usize = 4;
pub const REFRESH_SECONDS: i64 = 60;
const RETRY_SECONDS: i64 = 60;
/// Older prices are dropped instead of being shown as current.
const MAX_AGE: i64 = 10 * 60;

/// Opt-in switches: only `true` turns them on, and anything malformed leaves them off.
pub fn deserialize_flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(Value::deserialize(deserializer)? == Value::Bool(true))
}

pub fn default_coins() -> Vec<String> {
    ["BTC", "ETH", "SOL"].map(String::from).to_vec()
}

/// A Binance base asset as Luma keeps it: 2–10 capital letters or digits.
pub fn valid_symbol(symbol: &str) -> bool {
    (2..=10).contains(&symbol.len())
        && symbol
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// The coin's name for the listed coins, else its symbol.
pub fn name(symbol: &str) -> &str {
    COINS
        .iter()
        .find(|(coin, _)| *coin == symbol)
        .map_or(symbol, |(_, name)| name)
}

/// Puts chosen coins in display order: the listed ones in their order, then the others in the
/// order they were added.
pub fn order(coins: &mut [String]) {
    coins.sort_by_key(|coin| {
        COINS
            .iter()
            .position(|(symbol, _)| symbol == coin)
            .unwrap_or(COINS.len())
    });
}

/// Valid symbols only, without repeats, at most `MAX_COINS`, in display order; anything
/// malformed keeps the defaults rather than failing the preferences.
pub fn deserialize_coins<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    let Value::Array(values) = Value::deserialize(deserializer)? else {
        return Ok(default_coins());
    };
    let mut chosen: Vec<String> = Vec::new();
    for symbol in values.iter().filter_map(Value::as_str) {
        if valid_symbol(symbol) && !chosen.iter().any(|coin| coin == symbol) {
            chosen.push(symbol.to_owned());
        }
    }
    order(&mut chosen);
    chosen.truncate(MAX_COINS);
    Ok(chosen)
}

/// The base assets Binance trades against USDT, from a `ticker/price` reply, sorted.
pub fn parse_catalog(value: &Value) -> Result<Vec<String>, String> {
    let tickers = value
        .as_array()
        .ok_or("The price service returned unreadable data.")?;
    let mut symbols: Vec<String> = tickers
        .iter()
        .filter_map(|ticker| ticker["symbol"].as_str()?.strip_suffix("USDT"))
        .filter(|symbol| valid_symbol(symbol))
        .map(String::from)
        .collect();
    symbols.sort();
    symbols.dedup();
    if symbols.is_empty() {
        return Err("The price service listed no coins.".into());
    }
    Ok(symbols)
}

/// Search results shown at once.
pub const MATCHES: usize = 6;

/// The coin search on Settings › Crypto: what was typed and Binance's list of coins, loaded once
/// when the search is first opened.
#[derive(Default)]
pub struct Search {
    pub query: String,
    pub catalog: Vec<String>,
    pub loading: bool,
    pub failed: bool,
    /// Set when the search opens without a list; the worker fetches it.
    pub wanted: bool,
}
impl Search {
    /// Symbols starting with the query first, then those containing it, alphabetically.
    pub fn matches(&self) -> Vec<&str> {
        let query = self.query.trim().to_ascii_uppercase();
        if query.is_empty() {
            return Vec::new();
        }
        let starts = self.catalog.iter().filter(|s| s.starts_with(&query));
        let contains = self
            .catalog
            .iter()
            .filter(|s| !s.starts_with(&query) && s.contains(&query));
        starts
            .chain(contains)
            .take(MATCHES)
            .map(String::as_str)
            .collect()
    }
    /// Takes the typed text: letters and digits only, upper case, at most ten.
    pub fn edit(&mut self, query: &str) {
        self.query = query
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .take(10)
            .collect();
    }
    pub fn accept(&mut self, result: Result<Vec<String>, String>) {
        self.loading = false;
        match result {
            Ok(catalog) => {
                self.catalog = catalog;
                self.failed = false;
            }
            Err(_) => self.failed = true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Quote {
    pub symbol: String,
    /// In US dollars (USDT pairs).
    pub price: f64,
    /// Percent over the last 24 hours.
    pub change: f64,
}

/// Reads a `ticker/24hr` reply for the requested symbols; a reply missing any of them is
/// unreadable, so the cards never mix fresh and missing prices.
pub fn parse_tickers(value: &Value, symbols: &[String]) -> Result<Vec<Quote>, String> {
    let unreadable = || "The price service returned unreadable data.".to_owned();
    let tickers = value.as_array().ok_or_else(unreadable)?;
    let number = |ticker: &Value, key: &str| {
        ticker[key]
            .as_str()
            .and_then(|text| text.parse::<f64>().ok())
            .filter(|n| n.is_finite() && *n > 0.)
    };
    symbols
        .iter()
        .map(|symbol| {
            let pair = format!("{symbol}USDT");
            let ticker = tickers
                .iter()
                .find(|ticker| ticker["symbol"].as_str() == Some(pair.as_str()))
                .ok_or_else(unreadable)?;
            let price = number(ticker, "lastPrice").ok_or_else(unreadable)?;
            let open = number(ticker, "openPrice").ok_or_else(unreadable)?;
            Ok(Quote {
                symbol: symbol.clone(),
                price,
                change: (price - open) / open * 100.,
            })
        })
        .collect()
}

/// US dollars with thousands separators; small prices keep four significant digits.
pub fn format_price(price: f64) -> String {
    let decimals = if price >= 1. {
        2
    } else if price >= 0.01 {
        4
    } else {
        // 0.00001042 has four zeros after the point, then its four digits.
        ((-price.log10()).floor() as usize + 4).min(12)
    };
    let text = format!("{price:.decimals$}");
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("${grouped}.{fraction}")
}

/// The 24-hour change with its sign, such as `+1.24%` or `−0.85%`.
pub fn format_change(change: f64) -> String {
    // Adding zero turns a rounded -0.0 into 0.0, so it does not print as "+-0.00%".
    let rounded = (change * 100.).round() / 100. + 0.;
    if rounded < 0. {
        format!("−{:.2}%", -rounded)
    } else {
        format!("+{rounded:.2}%")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub id: u64,
    pub symbols: Vec<String>,
}

#[derive(Default, Serialize)]
pub struct Crypto {
    #[serde(skip)]
    pub search: Search,
    pub quotes: Vec<Quote>,
    pub updated_at: i64,
    pub loading: bool,
    pub failed: bool,
    /// The coins the prices are for.
    #[serde(skip)]
    symbols: Vec<String>,
    #[serde(skip)]
    id: u64,
    #[serde(skip)]
    pending: Option<u64>,
    #[serde(skip)]
    next_fetch: i64,
}
impl Crypto {
    pub fn fetching(&self) -> bool {
        self.pending.is_some()
    }
    pub fn generation(&self) -> u64 {
        self.id
    }
    /// Starts a fetch when Home shows the widget and its prices are due, and drops prices that
    /// are too old or for other coins. Returns the request to send and whether anything changed.
    pub fn sync(
        &mut self,
        enabled: bool,
        symbols: &[String],
        home_visible: bool,
        now: i64,
    ) -> (Option<Command>, bool) {
        let before = (
            self.loading,
            self.failed,
            self.updated_at,
            self.quotes.len(),
        );
        if self.symbols != symbols {
            // Different coins: drop the old prices and any request for them, and fetch now.
            self.symbols = symbols.to_vec();
            self.quotes.clear();
            self.failed = false;
            self.next_fetch = 0;
            if self.pending.take().is_some() {
                self.id += 1;
            }
        }
        if now - self.updated_at > MAX_AGE {
            self.quotes.clear();
        }
        let mut command = None;
        if !enabled || !home_visible || symbols.is_empty() {
            if self.pending.take().is_some() {
                self.id += 1;
            }
            self.loading = false;
        } else if self.pending.is_none() && now >= self.next_fetch {
            self.id += 1;
            self.pending = Some(self.id);
            self.loading = self.quotes.is_empty();
            command = Some(Command {
                id: self.id,
                symbols: symbols.to_vec(),
            });
        }
        let changed = before
            != (
                self.loading,
                self.failed,
                self.updated_at,
                self.quotes.len(),
            );
        (command, changed)
    }
    /// Takes the reply to the request in flight; replies to older requests are ignored.
    pub fn accept(&mut self, id: u64, result: Result<Vec<Quote>, String>, now: i64) -> bool {
        if self.pending != Some(id) {
            return false;
        }
        self.pending = None;
        self.loading = false;
        match result {
            Ok(quotes) => {
                self.quotes = quotes;
                self.updated_at = now;
                self.failed = false;
                self.next_fetch = now + REFRESH_SECONDS;
            }
            Err(_) => {
                self.failed = true;
                self.next_fetch = now + RETRY_SECONDS;
            }
        }
        true
    }
}

#[cfg(test)]
#[path = "crypto-tests.rs"]
mod tests;
