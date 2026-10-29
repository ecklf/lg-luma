use super::*;
use serde_json::json;

fn symbols(list: &[&str]) -> Vec<String> {
    list.iter().map(|symbol| (*symbol).into()).collect()
}
fn quote(symbol: &str) -> Quote {
    Quote {
        symbol: symbol.into(),
        price: 1.,
        change: 0.,
    }
}

#[test]
fn tickers_give_prices_and_changes_for_every_requested_coin() {
    let reply = json!([
        {"symbol": "ETHUSDT", "lastPrice": "2600.50", "openPrice": "2700.00"},
        {"symbol": "BTCUSDT", "lastPrice": "67420.12", "openPrice": "66600.00"},
    ]);
    let quotes = parse_tickers(&reply, &symbols(&["BTC", "ETH"])).unwrap();
    assert_eq!(quotes[0].symbol, "BTC");
    assert_eq!(quotes[0].price, 67420.12);
    assert!((quotes[0].change - 1.2314).abs() < 0.001);
    assert!((quotes[1].change + 3.685).abs() < 0.001);

    // A missing coin, a non-numeric price or a non-array reply is unreadable.
    assert!(parse_tickers(&reply, &symbols(&["BTC", "SOL"])).is_err());
    let bad = json!([{"symbol": "BTCUSDT", "lastPrice": "lots", "openPrice": "1"}]);
    assert!(parse_tickers(&bad, &symbols(&["BTC"])).is_err());
    let zero = json!([{"symbol": "BTCUSDT", "lastPrice": "1", "openPrice": "0"}]);
    assert!(parse_tickers(&zero, &symbols(&["BTC"])).is_err());
    assert!(parse_tickers(&json!({"code": -1121}), &symbols(&["BTC"])).is_err());
}

#[test]
fn prices_and_changes_are_formatted_for_cards() {
    assert_eq!(format_price(67420.123), "$67,420.12");
    assert_eq!(format_price(1234567.), "$1,234,567.00");
    assert_eq!(format_price(2.5), "$2.50");
    assert_eq!(format_price(0.12345), "$0.1235");
    assert_eq!(format_change(1.2349), "+1.23%");
    assert_eq!(format_change(-0.851), "−0.85%");
    assert_eq!(format_change(-0.001), "+0.00%");
}

#[test]
fn coin_preferences_keep_known_coins_in_settings_order() {
    #[derive(serde::Deserialize)]
    struct Holder {
        #[serde(deserialize_with = "deserialize_coins")]
        coins: Vec<String>,
    }
    let parse = |value| serde_json::from_value::<Holder>(value).unwrap().coins;
    assert_eq!(
        parse(json!({"coins": ["DOGE", "BTC", "FAKE", "BTC", 7]})),
        symbols(&["BTC", "DOGE"])
    );
    assert_eq!(
        parse(json!({"coins": ["TON", "DOGE", "ADA", "XRP", "BNB"]})),
        symbols(&["BNB", "XRP", "ADA", "DOGE"])
    );
    assert_eq!(parse(json!({"coins": "BTC"})), default_coins());
    assert!(parse(json!({"coins": []})).is_empty());
}

#[test]
fn prices_are_fetched_only_while_home_shows_the_widget() {
    let coins = symbols(&["BTC", "ETH"]);
    let mut crypto = Crypto::default();
    assert!(crypto.sync(false, &coins, true, 1000).0.is_none());
    assert!(crypto.sync(true, &coins, false, 1000).0.is_none());
    assert!(crypto.sync(true, &[], true, 1000).0.is_none());
    let (Some(command), _) = crypto.sync(true, &coins, true, 1000) else {
        panic!("Home with the widget should fetch");
    };
    assert_eq!(command.symbols, coins);
    assert!(crypto.loading);
    // Only one request at a time.
    assert!(crypto.sync(true, &coins, true, 1001).0.is_none());
    assert!(crypto.accept(command.id, Ok(vec![quote("BTC"), quote("ETH")]), 1002));
    assert!(!crypto.loading);
    assert!(
        crypto
            .sync(true, &coins, true, 1002 + REFRESH_SECONDS - 1)
            .0
            .is_none()
    );
    assert!(
        crypto
            .sync(true, &coins, true, 1002 + REFRESH_SECONDS)
            .0
            .is_some()
    );
}

#[test]
fn failures_retry_later_and_old_prices_are_dropped() {
    let coins = symbols(&["BTC"]);
    let mut crypto = Crypto::default();
    let (Some(first), _) = crypto.sync(true, &coins, true, 1000) else {
        panic!();
    };
    crypto.accept(first.id, Ok(vec![quote("BTC")]), 1000);
    let (Some(second), _) = crypto.sync(true, &coins, true, 1060) else {
        panic!();
    };
    crypto.accept(second.id, Err("offline".into()), 1060);
    assert!(crypto.failed);
    assert_eq!(crypto.quotes.len(), 1, "recent prices stay after a failure");
    // A failure waits for the retry instead of asking again at once.
    assert!(crypto.sync(true, &coins, true, 1061).0.is_none());
    // Ten minutes after the last answer the prices are no longer shown.
    crypto.sync(true, &coins, true, 1000 + 10 * 60 + 1);
    assert!(crypto.quotes.is_empty());
}

#[test]
fn choosing_other_coins_cancels_the_old_request_and_fetches_at_once() {
    let mut crypto = Crypto::default();
    let (Some(old), _) = crypto.sync(true, &symbols(&["BTC"]), true, 1000) else {
        panic!();
    };
    let (Some(new), _) = crypto.sync(true, &symbols(&["BTC", "ETH"]), true, 1001) else {
        panic!("new coins should be fetched now");
    };
    assert!(!crypto.accept(old.id, Ok(vec![quote("BTC")]), 1002));
    assert!(crypto.accept(new.id, Ok(vec![quote("BTC"), quote("ETH")]), 1002));
    assert_eq!(crypto.quotes.len(), 2);
}
