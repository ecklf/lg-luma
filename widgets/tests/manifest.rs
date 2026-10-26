use luma_widgets::manifest::{Access, CardKind, Manifest, SettingKind, Version, When};
use serde_json::{Value, json};

fn esports() -> Value {
    json!({
        "schema": 1,
        "id": "esports",
        "version": "1.0.0",
        "luma": "2.9.0",
        "name": { "en": "Esports", "ru": "Киберспорт" },
        "description": { "en": "Live and upcoming matches." },
        "authors": ["eluz1ve"],
        "icon": "gaming",
        "origins": [
            "https://eluz1ve.github.io",
            "https://api.pandascore.co",
            "https://cdn-api.pandascore.co"
        ],
        "size": { "default": [6, 2], "min": [6, 2], "max": [12, 3] },
        "instances": 4,
        "card": "scoreboard",
        "strings": { "map": { "en": "Map {number}", "ru": "Карта {number}" } },
        "settings": [
            { "id": "games", "type": "choices", "label": { "en": "Disciplines" },
              "options": [
                  { "value": "csgo", "label": { "en": "Counter-Strike 2" } },
                  { "value": "dota2", "label": { "en": "Dota 2" } },
                  { "value": "valorant", "label": { "en": "Valorant" } }
              ],
              "default": ["csgo"] },
            { "id": "major_only", "type": "toggle", "label": { "en": "Major events only" }, "default": true },
            { "id": "key", "type": "secret", "label": { "en": "PandaScore key" },
              "hint": { "en": "Free at pandascore.co." },
              "send_to": ["https://api.pandascore.co"], "pattern": "^[A-Za-z0-9._-]{16,128}$" }
        ],
        "library": "def shown($item): .running[$item] // [];",
        "sources": [
            { "id": "shared", "when": { "no_secret": "key" },
              "refresh": { "every": 300, "min": 300 },
              "requests": [
                  { "id": "feed", "for_each": "games",
                    "url": "https://eluz1ve.github.io/luma-feeds/esports/{item}.json" }
              ],
              "transform": "{items: []}" },
            { "id": "live", "when": { "secret": "key" },
              "refresh": { "every": 300, "min": 30 },
              "requests": [
                  { "id": "running", "for_each": "games",
                    "url": "https://api.pandascore.co/{item}/matches/running?per_page=25",
                    "headers": { "Authorization": "Bearer {secret.key}" } },
                  { "id": "teams", "for_each": "games", "max_age": 1800, "required": false,
                    "vars": { "ids": "[shown($item)[].opponents[]?.opponent.id] | map(tostring) | join(\",\")" },
                    "url": "https://api.pandascore.co/{item}/teams?filter%5Bid%5D={var.ids}&per_page=100",
                    "headers": { "Authorization": "Bearer {secret.key}" } }
              ],
              "transform": "{items: []}" }
        ]
    })
}

fn parse(value: &Value) -> Result<Manifest, String> {
    Manifest::parse(&serde_json::to_vec(value).unwrap()).map_err(|e| e.to_string())
}

/// The change is refused with an error mentioning `reason`.
fn refused(change: impl FnOnce(&mut Value), reason: &str) {
    let mut value = esports();
    change(&mut value);
    let error = parse(&value).expect_err(reason);
    assert!(
        error.contains(reason),
        "expected \"{reason}\", got \"{error}\""
    );
}

#[test]
fn an_esports_shaped_manifest_parses() {
    let manifest = parse(&esports()).unwrap();
    assert_eq!(manifest.id, "esports");
    assert_eq!(manifest.version, Version(1, 0, 0));
    assert!(manifest.luma <= Version(2, 9, 0));
    assert_eq!(manifest.name.get("ru"), "Киберспорт");
    assert_eq!(manifest.name.get("de"), "Esports");
    assert_eq!(manifest.card, CardKind::Scoreboard);
    assert_eq!(manifest.instances, 4);
    assert_eq!(manifest.size.default, [6, 2]);
    assert_eq!(manifest.origins.len(), 3);
    assert!(matches!(
        manifest.settings[2].kind,
        SettingKind::Secret { .. }
    ));
    assert_eq!(manifest.sources[0].when, When::NoSecret("key".into()));
    assert_eq!(manifest.sources[1].when, When::Secret("key".into()));
    let teams = &manifest.sources[1].requests[1];
    assert_eq!(teams.max_age, Some(1800));
    assert!(!teams.required);
    assert_eq!(teams.limit, 1024 * 1024);
    assert!(manifest.sources[1].requests[0].required);
    assert_eq!(manifest.sources[1].refresh.min, 30);
}

#[test]
fn ids_and_origins_must_be_plain() {
    refused(|m| m["id"] = json!("Esports"), "id \"Esports\"");
    refused(|m| m["id"] = json!("e"), "id \"e\"");
    refused(
        |m| m["origins"][0] = json!("https://eluz1ve.github.io/path"),
        "https://host[:port]",
    );
    refused(
        |m| m["origins"][0] = json!("http://eluz1ve.github.io"),
        "https://host[:port]",
    );
    refused(
        |m| m["origins"][0] = json!("https://10.0.0.2"),
        "https://host[:port]",
    );
    refused(
        |m| m["origins"][0] = json!("https://localhost"),
        "https://host[:port]",
    );
    refused(
        |m| m["origins"][0] = json!("https://*.github.io"),
        "https://host[:port]",
    );
    refused(
        |m| {
            m["origins"] = json!(
                (0..9)
                    .map(|n| format!("https://a{n}.example.com"))
                    .collect::<Vec<_>>()
            )
        },
        "1–8 distinct origins",
    );
}

#[test]
fn url_templates_keep_their_origin_and_secrets_out() {
    refused(
        |m| m["sources"][0]["requests"][0]["url"] = json!("https://{item}.github.io/x.json"),
        "origin must be literal",
    );
    refused(
        |m| m["sources"][0]["requests"][0]["url"] = json!("https://example.com/x.json"),
        "not on one of the widget's origins",
    );
    refused(
        |m| {
            m["sources"][1]["requests"][0]["url"] =
                json!("https://api.pandascore.co/x?token={secret.key}")
        },
        "forbidden placeholder {secret.key}",
    );
    refused(
        |m| {
            m["sources"][1]["requests"][0]["url"] = json!("https://eluz1ve.github.io/x/{item}.json")
        },
        "outside its send_to",
    );
    refused(
        |m| m["settings"][2]["send_to"] = json!(["https://example.com"]),
        "send_to must list",
    );
    refused(
        |m| m["sources"][0]["requests"][0]["url"] = json!("https://eluz1ve.github.io/{var.nope}"),
        "{var.nope}",
    );
}

#[test]
fn structure_mistakes_are_caught() {
    refused(|m| m["colour"] = json!("red"), "unknown field `colour`");
    refused(
        |m| m["settings"][0]["colour"] = json!("red"),
        "unknown field `colour`",
    );
    refused(
        |m| m["sources"][0]["requests"][0]["for_each"] = json!("major_only"),
        "for_each must name a choices setting",
    );
    refused(
        |m| {
            let step = m["sources"][1]["requests"][0].clone();
            m["sources"][1]["requests"][1] = step;
        },
        "step ids must be unique",
    );
    refused(|m| m["schema"] = json!(2), "schema 2");
    refused(|m| m["name"] = json!({ "ru": "Киберспорт" }), "English");
    refused(|m| m["card"] = json!("chart"), "unknown variant `chart`");
    refused(
        |m| m["sources"][0]["transform"] = json!({ "file": "shared.jq" }),
        "luma-widget pack",
    );
    refused(
        |m| m["sources"][0]["refresh"] = json!({ "every": 10, "min": 10 }),
        "30 ≤ refresh.min",
    );
    refused(|m| m["size"]["min"] = json!([2, 1]), "4–12 cells wide");
    refused(
        |m| m["sources"][0]["when"] = json!({ "secret": "games" }),
        "\"games\" is not a secret",
    );
}

#[test]
fn saved_settings_fall_back_to_defaults() {
    let manifest = parse(&esports()).unwrap();
    let saved = json!({ "games": ["valorant", "lol", "csgo"], "major_only": "yes", "key": "x" });
    let values = manifest.settings_values(saved.as_object().unwrap());
    assert_eq!(values["games"], json!(["csgo", "valorant"]));
    assert_eq!(values["major_only"], json!(true));
    assert!(!values.contains_key("key"));
    let values = manifest.settings_values(json!({ "games": ["lol"] }).as_object().unwrap());
    assert_eq!(values["games"], json!(["csgo"]));
}

#[test]
fn access_widens_only_when_it_grows() {
    let old = parse(&esports()).unwrap().access();
    let mut more = esports();
    more["origins"]
        .as_array_mut()
        .unwrap()
        .push(json!("https://static.example.com"));
    assert!(old.widens(&parse(&more).unwrap().access()));
    let mut fewer = esports();
    fewer["origins"].as_array_mut().unwrap().pop();
    let fewer = parse(&fewer).unwrap().access();
    assert!(!old.widens(&fewer));
    assert!(fewer.widens(&old));
    assert_eq!(
        old.secret_destinations,
        ["https://api.pandascore.co".to_owned()].into()
    );
    let _: &Access = &old;
}
