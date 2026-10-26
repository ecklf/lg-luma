use luma_widgets::document::{Card, Document, StepState, Text, Tone};
use luma_widgets::manifest::{CardKind, Origin};
use serde_json::{Value, json};

fn origins() -> Vec<Origin> {
    vec![Origin::parse("https://cdn-api.pandascore.co").unwrap()]
}

fn parse(value: Value) -> Document {
    Document::parse(&value, CardKind::Scoreboard, &origins()).unwrap()
}

fn scoreboard(id: &str) -> Value {
    json!({
        "id": id,
        "card": {
            "live": true,
            "status": { "start": "2026-10-26T18:00:00Z" },
            "logo": "https://cdn-api.pandascore.co/league.png",
            "caption": "PGL Masters",
            "sides": [
                { "name": "9z", "short": "9z", "image": "https://cdn-api.pandascore.co/9z.png", "score": 1, "lead": true },
                null
            ],
            "progress": { "label": "Map 3 · Mirage", "value": "7:5", "steps": ["done", "done", "live", "bogus"] }
        },
        "dialog": {
            "header": { "title": "PGL Masters", "chips": [ { "text": "BO3" }, { "text": "$1 000 000", "tone": "gold" } ] },
            "hero": { "live": true, "sides": [ { "name": "9z", "caption": "AR", "score": "1", "lead": true }, null ],
                      "lines": [ "Map 3", { "as_of": 1793021218 }, "third" ] },
            "tiles": { "title": "Maps", "slots": 9,
                       "items": [ { "number": "01", "title": "Inferno", "state": "done", "value": "13:11",
                                    "result": { "name": "9z", "text": { "elapsed": "2026-10-26T13:02:50Z" } } } ] },
            "lists": [ { "title": "9z lineup", "rows": [ { "title": "meyern", "subtitle": "Ignacio Meyer", "meta": "AR" } ],
                         "extra": [ { "label": "Bench", "text": "dgt" } ] } ],
            "footer": { "label": "Streams", "text": "EN Twitch", "credit": "Maps: Liquipedia", "note": "…" }
        }
    })
}

#[test]
fn a_scoreboard_item_parses_into_template_fields() {
    let doc = parse(
        json!({ "title": "Counter-Strike 2", "updated": 1793021218, "refresh": 30,
                            "items": [scoreboard("1")] }),
    );
    assert_eq!(doc.title, Some(Text::Plain("Counter-Strike 2".into())));
    assert_eq!(doc.updated, Some(1793021218));
    assert_eq!(doc.refresh, Some(30));
    let item = &doc.items[0];
    let Card::Scoreboard(card) = &item.card else {
        panic!("not a scoreboard")
    };
    assert!(card.live);
    assert_eq!(
        card.status,
        Some(Text::Start("2026-10-26T18:00:00Z".into()))
    );
    let side = card.sides[0].as_ref().unwrap();
    assert_eq!(side.score.as_deref(), Some("1"));
    assert!(side.lead);
    assert!(card.sides[1].is_none());
    let progress = card.progress.as_ref().unwrap();
    assert_eq!(
        progress.steps,
        [
            StepState::Done,
            StepState::Done,
            StepState::Live,
            StepState::Next
        ]
    );
    let dialog = item.dialog.as_ref().unwrap();
    let header = dialog.header.as_ref().unwrap();
    assert_eq!(header.chips[1].tone, Tone::Gold);
    let hero = dialog.hero.as_ref().unwrap();
    assert_eq!(
        hero.lines,
        [Text::Plain("Map 3".into()), Text::AsOf(1793021218)]
    );
    let tiles = dialog.tiles.as_ref().unwrap();
    assert_eq!(tiles.slots, 7);
    assert_eq!(
        tiles.items[0].result.as_ref().unwrap().text,
        Some(Text::Elapsed("2026-10-26T13:02:50Z".into()))
    );
    assert_eq!(dialog.lists[0].extra[0].text, Text::Plain("dgt".into()));
    assert!(dialog.footer.is_some());
}

#[test]
fn documents_are_capped_and_cleaned() {
    let items: Vec<Value> = (0..60).map(|n| scoreboard(&n.to_string())).collect();
    let long = "x".repeat(300);
    let doc = parse(json!({ "title": format!("a\u{7}b{long}"), "items": items }));
    assert_eq!(doc.items.len(), 50);
    let Some(Text::Plain(title)) = doc.title else {
        panic!("no title")
    };
    assert_eq!(title.chars().count(), 200);
    assert!(title.starts_with("ab"));
}

#[test]
fn foreign_images_and_unknown_fields_are_dropped() {
    let mut item = scoreboard("1");
    item["card"]["logo"] = json!("https://evil.example.com/league.png");
    item["card"]["colour"] = json!("red");
    let doc = parse(json!({ "items": [item] }));
    let Card::Scoreboard(card) = &doc.items[0].card else {
        panic!("not a scoreboard")
    };
    assert_eq!(card.logo, None);
    assert_eq!(
        card.sides[0].as_ref().unwrap().image.as_deref(),
        Some("https://cdn-api.pandascore.co/9z.png")
    );
}

#[test]
fn items_need_a_unique_id_and_a_card() {
    let doc = parse(
        json!({ "items": [scoreboard("1"), scoreboard("1"), { "id": "2" }, { "card": {} }] }),
    );
    assert_eq!(doc.items.len(), 1);
    assert!(Document::parse(&json!([1, 2]), CardKind::Scoreboard, &origins()).is_err());
}

#[test]
fn media_and_ticker_cards_parse() {
    let doc = Document::parse(
        &json!({ "items": [ { "id": "a", "card": { "image": "https://cdn-api.pandascore.co/p.jpg", "title": "Andor", "subtitle": "Season 2" } } ] }),
        CardKind::Media,
        &origins(),
    )
    .unwrap();
    assert!(
        matches!(&doc.items[0].card, Card::Media(media) if media.title == Text::Plain("Andor".into()))
    );
    let doc = Document::parse(
        &json!({ "items": [ { "id": "btc", "card": { "label": "BTC", "value": 67120, "change": { "text": "+2.4%", "tone": "up" } } } ] }),
        CardKind::Ticker,
        &origins(),
    )
    .unwrap();
    let Card::Ticker(ticker) = &doc.items[0].card else {
        panic!("not a ticker")
    };
    assert_eq!(ticker.value, Text::Plain("67120".into()));
    assert_eq!(ticker.change.as_ref().unwrap().tone, Tone::Up);
}
