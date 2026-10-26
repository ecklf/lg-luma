use luma_widgets::fetch::{Fetch, FetchError};
use luma_widgets::manifest::Manifest;
use luma_widgets::refresh::{self, Cache, Context, WidgetError};
use luma_widgets::sandbox::Job;
use serde_json::{Map, Value, json};
use std::{cell::RefCell, collections::BTreeMap};

/// A request as the fake saw it: its URL and headers.
type Asked = (String, Vec<(String, String)>);

/// Replies by URL; every request is recorded with its headers.
#[derive(Default)]
struct Fake {
    replies: BTreeMap<String, Result<Value, FetchError>>,
    asked: RefCell<Vec<Asked>>,
}

impl Fetch for Fake {
    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        _limit: usize,
    ) -> Result<Value, FetchError> {
        self.asked.borrow_mut().push((url.into(), headers.to_vec()));
        self.replies
            .get(url)
            .cloned()
            .unwrap_or(Err(FetchError::Unavailable("not found".into())))
    }
}

fn manifest(sources: Value, extra_settings: Value) -> Manifest {
    let mut settings = vec![
        json!({ "id": "games", "type": "choices", "label": { "en": "Games" },
                "options": [ { "value": "csgo", "label": { "en": "CS" } },
                             { "value": "valorant", "label": { "en": "Valorant" } },
                             { "value": "dota2", "label": { "en": "Dota" } } ],
                "default": ["csgo"] }),
        json!({ "id": "key", "type": "secret", "label": { "en": "Key" },
                "send_to": ["https://api.example.com"] }),
    ];
    settings.extend(extra_settings.as_array().cloned().unwrap_or_default());
    let value = json!({
        "schema": 1, "id": "demo", "version": "1.0.0", "luma": "2.9.0",
        "name": { "en": "Demo" }, "description": { "en": "A demo." }, "icon": "tv",
        "origins": ["https://feed.example.com", "https://api.example.com"],
        "size": { "default": [6, 2], "min": [4, 1], "max": [12, 3] },
        "card": "ticker",
        "strings": { "title": { "en": "Prices", "ru": "Цены" } },
        "settings": settings,
        "library": "def first_value: .[keys[0]];",
        "sources": sources
    });
    Manifest::parse(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn shared_and_live() -> Manifest {
    manifest(
        json!([
            { "id": "shared", "when": { "no_secret": "key" },
              "requests": [ { "id": "feed", "for_each": "games",
                              "url": "https://feed.example.com/{item}.json" } ],
              "transform": "{title: $t.title, items: [.feed | to_entries[] | {id: .key, card: {label: .key, value: .value.price}}]}" },
            { "id": "live", "when": { "secret": "key" },
              "refresh": { "every": 300, "min": 30 },
              "requests": [
                  { "id": "list", "for_each": "games", "max_age": 0,
                    "url": "https://api.example.com/{item}/list",
                    "headers": { "Authorization": "Bearer {secret.key}" } },
                  { "id": "teams", "for_each": "games", "max_age": 600, "required": false,
                    "vars": { "ids": ".list[$item] | map(tostring) | join(\",\") | select(. != \"\")" },
                    "url": "https://api.example.com/{item}/teams?ids={var.ids}",
                    "headers": { "Authorization": "Bearer {secret.key}" } } ],
              "transform": "{refresh: 5, items: [{id: \"x\", card: {label: $source, value: (.teams | tostring)}}], seen: [$settings.key, ($settings | keys)]}" }
        ]),
        json!([]),
    )
}

fn transform(job: Job) -> Result<Value, luma_widgets::Error> {
    luma_widgets::jq::run(&job.program, job.input, &job.vars)
}

struct Run<'a> {
    manifest: &'a Manifest,
    settings: Value,
    secrets: BTreeMap<String, String>,
}

impl Run<'_> {
    fn go(
        &self,
        fake: &Fake,
        cache: &mut Cache,
        jobs: &mut Vec<Job>,
    ) -> Result<refresh::Refreshed, WidgetError> {
        let settings = self
            .manifest
            .settings_values(self.settings.as_object().unwrap_or(&Map::new()));
        let cx = Context {
            manifest: self.manifest,
            settings: &settings,
            secrets: &self.secrets,
            lang: "ru",
            now: 1793021218,
        };
        refresh::run(&cx, fake, cache, &mut |job: Job| {
            jobs.push(job.clone());
            transform(job)
        })
    }
}

#[test]
fn without_a_key_the_shared_source_fans_out_over_the_choices() {
    let manifest = shared_and_live();
    let mut fake = Fake::default();
    fake.replies.insert(
        "https://feed.example.com/csgo.json".into(),
        Ok(json!({ "price": 1 })),
    );
    fake.replies.insert(
        "https://feed.example.com/valorant.json".into(),
        Ok(json!({ "price": 2 })),
    );
    let run = Run {
        manifest: &manifest,
        settings: json!({ "games": ["csgo", "valorant"] }),
        secrets: BTreeMap::new(),
    };
    let refreshed = run.go(&fake, &mut Cache::default(), &mut vec![]).unwrap();
    assert_eq!(refreshed.source, "shared");
    assert_eq!(refreshed.document.items.len(), 2);
    assert_eq!(
        refreshed.document.title,
        Some(luma_widgets::document::Text::Plain("Цены".into()))
    );
    assert_eq!(refreshed.refresh, 300);
    let asked: Vec<String> = fake
        .asked
        .borrow()
        .iter()
        .map(|(url, _)| url.clone())
        .collect();
    assert_eq!(
        asked,
        [
            "https://feed.example.com/csgo.json",
            "https://feed.example.com/valorant.json"
        ]
    );
}

#[test]
fn with_a_key_the_chain_runs_and_the_key_stays_in_headers() {
    let manifest = shared_and_live();
    let mut fake = Fake::default();
    fake.replies.insert(
        "https://api.example.com/csgo/list".into(),
        Ok(json!([3, 4])),
    );
    fake.replies.insert(
        "https://api.example.com/csgo/teams?ids=3%2C4".into(),
        Ok(json!(["NAVI"])),
    );
    let secrets = BTreeMap::from([("key".to_owned(), "s3cr3t-value".to_owned())]);
    let run = Run {
        manifest: &manifest,
        settings: json!({}),
        secrets,
    };
    let mut jobs = vec![];
    let refreshed = run.go(&fake, &mut Cache::default(), &mut jobs).unwrap();
    assert_eq!(refreshed.source, "live");
    // Clamped to the source's minimum.
    assert_eq!(refreshed.refresh, 30);
    let asked = fake.asked.borrow();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[1].0, "https://api.example.com/csgo/teams?ids=3%2C4");
    assert!(asked.iter().all(|(url, headers)| {
        !url.contains("s3cr3t")
            && headers.contains(&("Authorization".into(), "Bearer s3cr3t-value".into()))
    }));
    for job in &jobs {
        let text = serde_json::to_string(job).unwrap();
        assert!(
            !text.contains("s3cr3t"),
            "the secret reached a transform: {text}"
        );
    }
    let luma_widgets::document::Card::Ticker(card) = &refreshed.document.items[0].card else {
        panic!("not a ticker")
    };
    assert_eq!(
        card.label,
        luma_widgets::document::Text::Plain("live".into())
    );
}

#[test]
fn a_refused_key_and_failing_optional_steps() {
    let manifest = shared_and_live();
    let secrets = BTreeMap::from([("key".to_owned(), "k".to_owned())]);
    let run = Run {
        manifest: &manifest,
        settings: json!({}),
        secrets,
    };
    let mut fake = Fake::default();
    fake.replies.insert(
        "https://api.example.com/csgo/list".into(),
        Err(FetchError::Refused),
    );
    assert_eq!(
        run.go(&fake, &mut Cache::default(), &mut vec![])
            .unwrap_err(),
        WidgetError::KeyRefused {
            secret: "key".into()
        }
    );
    // The teams step fails, and the transform gets null for it.
    fake.replies
        .insert("https://api.example.com/csgo/list".into(), Ok(json!([3])));
    let refreshed = run.go(&fake, &mut Cache::default(), &mut vec![]).unwrap();
    let luma_widgets::document::Card::Ticker(card) = &refreshed.document.items[0].card else {
        panic!("not a ticker")
    };
    assert_eq!(
        card.value,
        luma_widgets::document::Text::Plain("{\"csgo\":null}".into())
    );
    fake.replies.insert(
        "https://api.example.com/csgo/list".into(),
        Err(FetchError::Busy),
    );
    assert_eq!(
        run.go(&fake, &mut Cache::default(), &mut vec![])
            .unwrap_err(),
        WidgetError::Busy
    );
}

#[test]
fn a_var_without_a_value_skips_its_request() {
    let manifest = shared_and_live();
    let secrets = BTreeMap::from([("key".to_owned(), "k".to_owned())]);
    let run = Run {
        manifest: &manifest,
        settings: json!({}),
        secrets,
    };
    let mut fake = Fake::default();
    fake.replies
        .insert("https://api.example.com/csgo/list".into(), Ok(json!([])));
    run.go(&fake, &mut Cache::default(), &mut vec![]).unwrap();
    assert_eq!(fake.asked.borrow().len(), 1);
}

#[test]
fn replies_are_reused_within_max_age() {
    let manifest = shared_and_live();
    let secrets = BTreeMap::from([("key".to_owned(), "k".to_owned())]);
    let run = Run {
        manifest: &manifest,
        settings: json!({}),
        secrets,
    };
    let mut fake = Fake::default();
    fake.replies
        .insert("https://api.example.com/csgo/list".into(), Ok(json!([3])));
    fake.replies.insert(
        "https://api.example.com/csgo/teams?ids=3".into(),
        Ok(json!(["NAVI"])),
    );
    let mut cache = Cache::default();
    run.go(&fake, &mut cache, &mut vec![]).unwrap();
    run.go(&fake, &mut cache, &mut vec![]).unwrap();
    let asked: Vec<String> = fake
        .asked
        .borrow()
        .iter()
        .map(|(url, _)| url.clone())
        .collect();
    // The list (max_age 0) twice, the teams (max_age 600) once.
    assert_eq!(
        asked,
        [
            "https://api.example.com/csgo/list",
            "https://api.example.com/csgo/teams?ids=3",
            "https://api.example.com/csgo/list"
        ]
    );
}

#[test]
fn settings_are_encoded_into_urls() {
    let manifest = manifest(
        json!([ { "id": "main", "requests": [
            { "id": "search", "url": "https://feed.example.com/q?games={settings.games}&city={settings.city}" } ],
            "transform": "{items: []}" } ]),
        json!([ { "id": "city", "type": "text", "label": { "en": "City" }, "default": "" } ]),
    );
    let mut fake = Fake::default();
    fake.replies.insert(
        "https://feed.example.com/q?games=csgo%2Cdota2&city=a%20b%26c%2Fd".into(),
        Ok(json!({})),
    );
    let run = Run {
        manifest: &manifest,
        settings: json!({ "games": ["dota2", "csgo"], "city": "a b&c/d" }),
        secrets: BTreeMap::new(),
    };
    run.go(&fake, &mut Cache::default(), &mut vec![]).unwrap();
    assert_eq!(fake.asked.borrow().len(), 1);
}

#[test]
fn too_many_requests_are_refused_before_any_is_sent() {
    let steps: Vec<Value> = (0..8)
        .map(|n| json!({ "id": format!("s{n}"), "for_each": "many", "url": "https://feed.example.com/{item}.json" }))
        .collect();
    let options: Vec<Value> = (0..5)
        .map(|n| json!({ "value": format!("o{n}"), "label": { "en": "O" } }))
        .collect();
    let manifest = manifest(
        json!([ { "id": "main", "requests": steps, "transform": "{items: []}" } ]),
        json!([ { "id": "many", "type": "choices", "label": { "en": "Many" }, "options": options,
                  "default": ["o0", "o1", "o2", "o3", "o4"] } ]),
    );
    let fake = Fake::default();
    // 8 steps × 5 values = 40 requests, over the 32 a refresh may make.
    let run = Run {
        manifest: &manifest,
        settings: json!({}),
        secrets: BTreeMap::new(),
    };
    let error = run
        .go(&fake, &mut Cache::default(), &mut vec![])
        .unwrap_err();
    assert_eq!(error, WidgetError::Unavailable("too many requests".into()));
    assert!(fake.asked.borrow().is_empty());
}

#[test]
fn no_source_applies_without_its_secret() {
    let manifest = manifest(
        json!([ { "id": "live", "when": { "secret": "key" }, "requests": [], "transform": "{items: []}" } ]),
        json!([]),
    );
    let run = Run {
        manifest: &manifest,
        settings: json!({}),
        secrets: BTreeMap::new(),
    };
    assert_eq!(
        run.go(&Fake::default(), &mut Cache::default(), &mut vec![])
            .unwrap_err(),
        WidgetError::NoSource {
            secret: "key".into()
        }
    );
}
