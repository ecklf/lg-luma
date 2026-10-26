use super::*;
use serde_json::json;

#[test]
fn quotes_carry_price_change_and_range_in_coin_order() {
    let body = json!([
        {"symbol": "ETHUSDT", "openPrice": "2400.0", "highPrice": "2450.0", "lowPrice": "2280.0",
         "lastPrice": "2300.0"},
        {"symbol": "BTCUSDT", "openPrice": "80000.00", "highPrice": "81000.00",
         "lowPrice": "79000.00", "lastPrice": "80800.00"},
        {"symbol": "XYZUSDT", "openPrice": "1", "highPrice": "1", "lowPrice": "1", "lastPrice": "1"}
    ]);
    let quotes = parse_quotes(&body).unwrap();
    assert_eq!(
        quotes.iter().map(|q| q.coin).collect::<Vec<_>>(),
        ["BTC", "ETH"]
    );
    assert!((quotes[0].change - 1.0).abs() < 1e-9);
    assert!((quotes[1].change - (-100. / 24.)).abs() < 1e-9);
    assert_eq!((quotes[0].low, quotes[0].high), (79000., 81000.));
    assert!(parse_quotes(&json!({"code": 0})).is_err());
}

fn episode(name: &str, season: u32, number: u32, date: &str, kind: &str, weight: u32) -> Value {
    json!({
        "season": season, "number": number, "airdate": date,
        "_embedded": {"show": {
            "id": weight * 1000 + season, "name": name, "type": kind, "weight": weight,
            "image": {"medium": format!("https://static.tvmaze.com/{name}.jpg")}
        }}
    })
}

#[test]
fn premieres_keep_first_episodes_of_popular_scripted_shows_by_date() {
    let web = json!([
        episode("Lupin", 4, 1, "2026-10-23", "Scripted", 100),
        episode("Lupin", 4, 2, "2026-10-23", "Scripted", 100),
        episode("Love Island", 8, 1, "2026-10-22", "Reality", 99),
        episode("Niche", 2, 1, "2026-10-22", "Scripted", 60),
    ]);
    // The US schedule embeds the show under "show" instead of "_embedded".
    let us = json!([
        {"season": 2, "number": 1, "airdate": "2026-10-21",
         "show": {"id": 7, "name": "Stick", "type": "Animation", "weight": 99, "image": null}},
        episode("Lupin", 4, 1, "2026-10-23", "Scripted", 100),
    ]);
    let premieres = parse_premieres(&[web, us]);
    assert_eq!(
        premieres
            .iter()
            .map(|p| (p.title.as_str(), p.season, p.date.as_str()))
            .collect::<Vec<_>>(),
        [("Stick", 2, "2026-10-21"), ("Lupin", 4, "2026-10-23")]
    );
    assert_eq!(premieres[0].poster, None);
    assert_eq!(
        premieres[1].poster.as_deref(),
        Some("https://static.tvmaze.com/Lupin.jpg")
    );
}

#[test]
fn posters_only_come_from_the_tvmaze_image_origin() {
    assert!(poster_allowed(
        "https://static.tvmaze.com/uploads/images/medium_portrait/1/2.jpg"
    ));
    assert!(!poster_allowed("http://static.tvmaze.com/a.jpg"));
    assert!(!poster_allowed(
        "https://static.tvmaze.com.evil.example/a.jpg"
    ));
    assert!(!poster_allowed("https://example.com/a.jpg"));
}

#[test]
fn schedule_days_run_from_three_days_ago_to_ten_ahead() {
    let today = time::Date::from_calendar_date(2026, time::Month::October, 25).unwrap();
    let days = schedule_days(today);
    assert_eq!(days.len(), 14);
    assert_eq!(days.first().map(String::as_str), Some("2026-10-22"));
    assert_eq!(days.last().map(String::as_str), Some("2026-11-04"));
}
