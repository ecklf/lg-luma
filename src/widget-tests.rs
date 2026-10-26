use crate::home_grid::{Block, BlockKind};
use crate::model::{Preferences, demo_inventory};
use crate::state::{AddChoice, Effect, State};
use crate::widget_store::{CatalogState, Store};
use crate::widget_worker::{CatalogError, WidgetRequest, WidgetResponse};
use luma_widgets::catalog::{self, Index};
use luma_widgets::manifest::Manifest;
use luma_widgets::refresh::{Refreshed, WidgetError};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn manifest_json(version: &str, origins: &[&str]) -> Value {
    json!({
        "schema": 1, "id": "scores", "version": version, "luma": "2.8.0",
        "name": { "en": "Scores", "ru": "Счёт" }, "description": { "en": "Scores of games." },
        "icon": "gaming", "origins": origins,
        "size": { "default": [8, 2], "min": [6, 2], "max": [12, 3] },
        "instances": 2, "card": "scoreboard",
        "settings": [
            { "id": "league", "type": "choice", "label": { "en": "League" },
              "options": [ { "value": "a", "label": { "en": "A" } }, { "value": "b", "label": { "en": "B" } } ],
              "default": "a" },
            { "id": "key", "type": "secret", "label": { "en": "API key" },
              "send_to": [origins[0]] } ],
        "sources": [ { "id": "main", "refresh": { "every": 120, "min": 60 },
                       "requests": [ { "id": "list", "url": format!("{}/{{settings.league}}.json", origins[0]) } ],
                       "transform": "{items: []}" } ]
    })
}

fn manifest(version: &str, origins: &[&str]) -> (Manifest, Vec<u8>) {
    let bytes = serde_json::to_vec(&manifest_json(version, origins)).unwrap();
    (Manifest::parse(&bytes).unwrap(), bytes)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("luma-widget-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("preferences.json")
}

/// A state with the inventory loaded and the store kept in `prefs`'s folder.
fn ready(prefs: &std::path::Path) -> State {
    let mut state = State::new(Preferences::default());
    state.set_inventory(demo_inventory(), None);
    state.widgets.store = Store::open(prefs, &state.preferences.widgets.installed);
    state
}

/// `scores` installed and one block of it on Home, in slot 0.
fn installed(prefs: &std::path::Path) -> State {
    let mut state = ready(prefs);
    let (manifest, bytes) = manifest("1.0.0", &["https://api.example.com"]);
    state.widget_installed(manifest, &bytes).unwrap();
    state.add_widget("scores").unwrap();
    state
}

fn refreshed(document: Value) -> Refreshed {
    Refreshed {
        document: luma_widgets::document::Document::parse(
            &document,
            luma_widgets::manifest::CardKind::Scoreboard,
            &[],
        )
        .unwrap(),
        source: "main".into(),
        refresh: 120,
        input: json!({}),
    }
}

#[test]
fn a_store_widget_block_and_its_slot_round_trip_through_preferences() {
    let kind = BlockKind::Widget(3);
    assert_eq!(serde_json::to_value(kind).unwrap(), json!({ "widget": 3 }));
    let mut preferences = Preferences::default();
    preferences.home_layout.push(Block {
        kind,
        x: 0,
        y: 6,
        w: 8,
        h: 2,
    });
    preferences.widgets.slots.insert(
        3,
        crate::model::WidgetSlot {
            widget: "scores".into(),
            settings: json!({ "league": "b" }).as_object().unwrap().clone(),
        },
    );
    let text = serde_json::to_string(&preferences).unwrap();
    let back: Preferences = serde_json::from_str(&text).unwrap();
    assert!(back.home_layout.iter().any(|b| b.kind == kind));
    assert_eq!(back.widgets.slots[&3].settings["league"], json!("b"));
    // A broken widgets entry costs only the widgets, not the preferences.
    let mut value: Value = serde_json::from_str(&text).unwrap();
    value["widgets"] = json!("oops");
    let back: Preferences = serde_json::from_value(value).unwrap();
    assert!(back.widgets.slots.is_empty());
    assert!(back.home_layout.iter().any(|b| b.kind == kind));
}

#[test]
fn installing_writes_the_manifest_and_adding_takes_a_slot_with_its_size() {
    let prefs = scratch("install");
    let mut state = installed(&prefs);
    let file = prefs.parent().unwrap().join("widgets/scores.json");
    assert!(file.exists());
    assert_eq!(
        state.preferences.widgets.installed["scores"].version,
        "1.0.0"
    );
    let block = state
        .home_blocks()
        .into_iter()
        .find(|b| b.kind == BlockKind::Widget(0))
        .unwrap();
    assert_eq!((block.w, block.h), (8, 2));
    assert_eq!(state.preferences.widgets.slots[&0].widget, "scores");
    // Reopening finds it again.
    let store = Store::open(&prefs, &state.preferences.widgets.installed);
    assert_eq!(
        store.manifest("scores").unwrap().version.to_string(),
        "1.0.0"
    );
    // A second instance, then no more.
    assert!(
        state
            .add_choices()
            .contains(&AddChoice::Widget("scores".into()))
    );
    state.add_widget("scores").unwrap();
    assert!(
        !state
            .add_choices()
            .contains(&AddChoice::Widget("scores".into()))
    );
    assert_eq!(state.add_choices().last(), Some(&AddChoice::Store));
}

#[test]
fn removing_a_widget_takes_everything_of_it() {
    let prefs = scratch("remove");
    let mut state = installed(&prefs);
    state
        .set_widget_secret(0, "key", Some("abc123".into()))
        .unwrap();
    let effects = state.remove_widget("scores");
    assert!(effects.contains(&Effect::Save));
    assert!(
        state
            .home_blocks()
            .iter()
            .all(|b| b.kind != BlockKind::Widget(0))
    );
    assert!(state.preferences.widgets.slots.is_empty());
    assert!(state.preferences.widgets.installed.is_empty());
    assert!(!prefs.parent().unwrap().join("widgets/scores.json").exists());
    assert_eq!(state.widgets.store.secret("scores", "key"), None);
}

#[test]
fn secrets_live_in_their_own_private_file() {
    let prefs = scratch("secrets");
    let mut state = installed(&prefs);
    state
        .set_widget_secret(0, "key", Some("abc123".into()))
        .unwrap();
    let file = prefs.parent().unwrap().join("secrets.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(std::fs::read_to_string(&file).unwrap().contains("abc123"));
    let saved = serde_json::to_string(&state.preferences).unwrap();
    assert!(!saved.contains("abc123"));
    let store = Store::open(&prefs, &state.preferences.widgets.installed);
    assert_eq!(store.secret("scores", "key"), Some("abc123"));
}

#[test]
fn a_shown_widget_is_refreshed_on_its_interval_and_old_results_are_dropped() {
    let prefs = scratch("refresh");
    let mut state = installed(&prefs);
    let now = Instant::now();
    let requests = state.widget_requests(now, true);
    let job = requests
        .iter()
        .find_map(|r| match r {
            WidgetRequest::Refresh(job) => Some(job.clone()),
            _ => None,
        })
        .expect("a refresh for the new block");
    assert_eq!(job.slot, 0);
    assert_eq!(job.settings["league"], json!("a"));
    // In flight: nothing more until it answers.
    assert!(
        state
            .widget_requests(now, true)
            .iter()
            .all(|r| !matches!(r, WidgetRequest::Refresh(_)))
    );
    // A setting changes meanwhile; the old answer is dropped.
    state.set_widget_setting(0, "league", json!("b"));
    state.widget_response(WidgetResponse::Document {
        slot: 0,
        generation: job.generation,
        result: Ok(refreshed(
            json!({ "items": [ { "id": "1", "card": { "sides": [] } } ] }),
        )),
    });
    assert!(
        state
            .widgets
            .slot(0)
            .and_then(|s| s.document.as_ref())
            .is_none()
    );
    let job = state
        .widget_requests(now, true)
        .into_iter()
        .find_map(|r| match r {
            WidgetRequest::Refresh(job) => Some(job),
            _ => None,
        })
        .expect("a refresh with the new setting");
    assert_eq!(job.settings["league"], json!("b"));
    state.widget_response(WidgetResponse::Document {
        slot: 0,
        generation: job.generation,
        result: Ok(refreshed(
            json!({ "items": [ { "id": "1", "card": { "sides": [] } } ] }),
        )),
    });
    assert_eq!(
        state
            .widgets
            .slot(0)
            .unwrap()
            .document
            .as_ref()
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        state
            .widget_requests(now + Duration::from_secs(60), true)
            .iter()
            .all(|r| !matches!(r, WidgetRequest::Refresh(_)))
    );
    assert!(
        state
            .widget_requests(now + Duration::from_secs(121), true)
            .iter()
            .any(|r| matches!(r, WidgetRequest::Refresh(_)))
    );
    // Not while Home is hidden.
    let mut hidden = installed(&scratch("hidden"));
    assert!(
        hidden
            .widget_requests(now, false)
            .iter()
            .all(|r| !matches!(r, WidgetRequest::Refresh(_)))
    );
}

#[test]
fn failures_back_off_and_keep_the_last_document() {
    let prefs = scratch("backoff");
    let mut state = installed(&prefs);
    let now = Instant::now();
    let generation = |state: &mut State, at| {
        state
            .widget_requests(at, true)
            .into_iter()
            .find_map(|r| match r {
                WidgetRequest::Refresh(job) => Some(job.generation),
                _ => None,
            })
    };
    let first = generation(&mut state, now).unwrap();
    state.widget_response(WidgetResponse::Document {
        slot: 0,
        generation: first,
        result: Ok(refreshed(json!({ "items": [ { "id": "1", "card": {} } ] }))),
    });
    let next = generation(&mut state, now + Duration::from_secs(121)).unwrap();
    state.widget_response(WidgetResponse::Document {
        slot: 0,
        generation: next,
        result: Err(WidgetError::Unavailable("offline".into())),
    });
    let slot = state.widgets.slot(0).unwrap();
    assert!(slot.document.is_some(), "the last document stays");
    assert!(generation(&mut state, now + Duration::from_secs(121 + 59)).is_none());
    assert!(generation(&mut state, now + Duration::from_secs(121 + 61)).is_some());
}

#[test]
fn a_new_secret_refreshes_at_once_with_it() {
    let prefs = scratch("secret-refresh");
    let mut state = installed(&prefs);
    let now = Instant::now();
    let _ = state.widget_requests(now, true);
    state
        .set_widget_secret(0, "key", Some("abc123".into()))
        .unwrap();
    let requests = state.widget_requests(now, true);
    assert!(requests.contains(&WidgetRequest::Forget("scores".into())));
    let job = requests
        .into_iter()
        .find_map(|r| match r {
            WidgetRequest::Refresh(job) => Some(job),
            _ => None,
        })
        .unwrap();
    assert_eq!(job.secrets["key"], "abc123");
}

fn signed_index(entries: Vec<luma_widgets::catalog::Entry>, sequence: u64) -> Index {
    Index {
        schema: 1,
        sequence,
        published: "2026-10-26T12:00:00Z".into(),
        widgets: entries,
    }
}

#[test]
fn updates_that_need_no_new_access_install_themselves() {
    let prefs = scratch("updates");
    let mut state = installed(&prefs);
    let (newer, bytes) = manifest("1.1.0", &["https://api.example.com"]);
    let entry = catalog::entry(&newer, &bytes, "manifests/scores-1.1.0.json");
    state.widget_response(WidgetResponse::Catalog(Ok(signed_index(
        vec![entry.clone()],
        7,
    ))));
    assert_eq!(state.preferences.widgets.catalog_sequence, 7);
    let requests = state.widget_requests(Instant::now(), true);
    assert!(requests.contains(&WidgetRequest::Install(entry)));

    let mut state = installed(&scratch("widening"));
    let (wider, bytes) = manifest(
        "1.1.0",
        &["https://api.example.com", "https://more.example.com"],
    );
    let entry = catalog::entry(&wider, &bytes, "manifests/scores-1.1.0.json");
    state.widget_response(WidgetResponse::Catalog(Ok(signed_index(
        vec![entry.clone()],
        8,
    ))));
    let requests = state.widget_requests(Instant::now(), true);
    assert!(!requests.contains(&WidgetRequest::Install(entry)));
    assert!(state.widget_needs_consent("scores"));
}

#[test]
fn an_unverified_catalog_offers_nothing() {
    let prefs = scratch("unverified");
    let mut state = ready(&prefs);
    state.widget_response(WidgetResponse::Catalog(Err(CatalogError::Unverified(
        "bad signature".into(),
    ))));
    assert!(matches!(state.widgets.catalog, CatalogState::Unverified));
    assert!(state.store_entries().is_empty());
}

fn scene(state: &State) -> crate::view::Scene {
    let assets = crate::view::Assets {
        root: PathBuf::new(),
        version: "test".into(),
        metrics: std::collections::BTreeMap::new(),
        artwork: crate::app_artwork::Service::new(false),
    };
    let navigation = crate::view::navigation(&assets, state.preferences.language);
    crate::view::scene_with_pill(
        state,
        &assets,
        navigation.items[state.page_index()].x,
        navigation.items[state.page_index()].w,
        time::OffsetDateTime::parse(
            "2026-10-26T19:30:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap(),
    )
}

fn texts(state: &State) -> Vec<String> {
    scene(state)
        .draw
        .into_iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(text, ..) => Some(text),
            _ => None,
        })
        .collect()
}

/// Shows a document in slot 0 as the worker would answer it.
fn show(state: &mut State, document: Value) {
    let generation = state
        .widget_requests(Instant::now(), true)
        .into_iter()
        .find_map(|r| match r {
            WidgetRequest::Refresh(job) => Some(job.generation),
            _ => None,
        })
        .unwrap_or_else(|| state.widgets.slot(0).unwrap().generation);
    state.widget_response(WidgetResponse::Document {
        slot: 0,
        generation,
        result: Ok(refreshed(document)),
    });
}

fn matches(ids: &[&str]) -> Value {
    json!({
        "title": "League A",
        "updated": 1793043043,
        "items": ids.iter().enumerate().map(|(n, id)| json!({
            "id": id,
            "card": {
                "live": n == 0,
                "status": { "start": "2026-10-26T21:00:00Z" },
                "caption": format!("Match {id}"),
                "sides": [ { "name": format!("Home {id}"), "short": "H", "score": "1", "lead": true },
                           { "name": format!("Away {id}"), "short": "A", "score": "0", "lead": false } ],
                "progress": if n == 0 {
                    json!({ "label": "Map 2 · Mirage", "value": "7:5", "steps": ["done", "live", "next"] })
                } else {
                    Value::Null
                },
                "footer": "BO3 · Playoffs"
            },
            "dialog": {
                "header": { "title": format!("Match {id}"), "chips": [ { "text": "BO3" } ] },
                "hero": { "live": n == 0, "sides": [ { "name": "Home", "score": "1", "lead": true }, { "name": "Away", "score": "0" } ],
                          "lines": [ "Map 2 · Mirage", { "as_of": 1793043043 } ] },
                "tiles": { "title": "Maps", "slots": 3, "items": [
                    { "number": "01", "title": "Inferno", "state": "done", "value": "13:11", "result": { "name": "H", "text": "H · 48 min" } },
                    { "number": "02", "title": "Mirage", "state": "live", "result": { "text": { "elapsed": "2026-10-26T19:11:00Z" } } } ] },
                "lists": [ { "title": "Home lineup", "rows": [ { "title": "s1mple", "subtitle": "Oleksandr", "meta": "UA" } ],
                             "extra": [ { "label": "Bench", "text": "b1" } ] },
                           { "title": "Away lineup", "empty": "Lineup not announced" } ],
                "footer": { "label": "Streams", "text": "EN Twitch", "credit": "Maps: Liquipedia", "note": "A note." }
            }
        })).collect::<Vec<_>>()
    })
}

#[test]
fn a_store_row_draws_its_cards_and_steps_through_them() {
    let mut state = installed(&scratch("row"));
    show(&mut state, matches(&["a", "b", "c", "d"]));
    let scene = scene(&state);
    let cards: Vec<_> = scene
        .controls
        .iter()
        .filter(|c| matches!(c.target, crate::state::Target::WidgetItem(0, _)))
        .collect();
    assert!(cards.len() >= 2, "one control per visible card");
    assert!((cards[0].rect.w - crate::design::WIDE_W).abs() < 0.5);
    let text = texts(&state);
    assert!(text.iter().any(|t| t == "Scores · League A"), "{text:?}");
    assert!(text.iter().any(|t| t.starts_with("Data as of")));
    assert!(text.iter().any(|t| t == "Home a"));
    assert!(text.iter().any(|t| t == "7:5"));
    assert!(text.iter().any(|t| t == "BO3 · Playoffs"));
    state.focus = crate::state::Target::WidgetItem(0, 0);
    state.direction(crate::state::Direction::Right, &scene.controls);
    assert_eq!(state.focus, crate::state::Target::WidgetItem(0, 1));
    assert_eq!(
        state.row_of(&state.focus).map(|(_, i, n)| (i, n)),
        Some((1, 4))
    );
}

#[test]
fn ok_opens_an_items_dialog_that_follows_the_item() {
    let mut state = installed(&scratch("dialog"));
    show(&mut state, matches(&["a", "b", "c"]));
    state.focus = crate::state::Target::WidgetItem(0, 1);
    state.activate();
    assert!(
        matches!(&state.overlay, Some(crate::state::Overlay::WidgetDialog { id, .. }) if id == "b")
    );
    let text = texts(&state);
    for expected in [
        "Match b",
        "Inferno",
        "13:11",
        "H · 48 min",
        "s1mple",
        "BENCH",
        "b1",
        "Lineup not announced",
        "Maps: Liquipedia",
        "A note.",
    ] {
        assert!(text.iter().any(|t| t == expected), "{expected} in {text:?}");
    }
    assert!(
        text.iter().any(|t| t.ends_with("min") && t != "H · 48 min"),
        "elapsed minutes"
    );
    // The item moves to the front: the dialog follows it.
    let generation = |state: &State| state.widgets.slot(0).unwrap().generation;
    let answer = |state: &mut State, ids: &[&str]| {
        let generation = generation(state);
        state.widgets.slots.get_mut(&0).unwrap().in_flight = true;
        state.widget_response(WidgetResponse::Document {
            slot: 0,
            generation,
            result: Ok(refreshed(matches(ids))),
        });
    };
    answer(&mut state, &["b", "c"]);
    assert!(matches!(&state.overlay,
        Some(crate::state::Overlay::WidgetDialog { id, restore: crate::state::Target::WidgetItem(0, 0), .. }) if id == "b"));
    // It is gone: the dialog closes.
    answer(&mut state, &["c"]);
    assert!(state.overlay.is_none());
}

#[test]
fn a_row_without_items_says_why() {
    let mut state = installed(&scratch("ghost"));
    assert!(texts(&state).iter().any(|t| t == "Loading…"));
    let generation = state.widgets.slot(0).unwrap().generation;
    let _ = state.widget_requests(Instant::now(), true);
    state.widget_response(WidgetResponse::Document {
        slot: 0,
        generation,
        result: Err(WidgetError::KeyRefused {
            secret: "key".into(),
        }),
    });
    assert!(
        texts(&state)
            .iter()
            .any(|t| t == "API key was not accepted. Check it in the widget's settings.")
    );
    state.set_widget_secret(0, "key", None).unwrap();
    let generation = state.widgets.slot(0).unwrap().generation;
    let _ = state.widget_requests(Instant::now(), true);
    state.widget_response(WidgetResponse::Document {
        slot: 0,
        generation,
        result: Err(WidgetError::NoSource {
            secret: "key".into(),
        }),
    });
    assert!(
        texts(&state)
            .iter()
            .any(|t| t == "Enter your API key in the widget's settings.")
    );
    show(
        &mut state,
        json!({ "empty": "No games today", "items": [] }),
    );
    assert!(texts(&state).iter().any(|t| t == "No games today"));
}

#[test]
fn media_and_ticker_rows_use_their_card_widths() {
    let mut state = installed(&scratch("templates"));
    let mut manifest = manifest_json("1.0.0", &["https://api.example.com"]);
    manifest["card"] = json!("ticker");
    manifest["id"] = json!("prices");
    let bytes = serde_json::to_vec(&manifest).unwrap();
    state
        .widget_installed(Manifest::parse(&bytes).unwrap(), &bytes)
        .unwrap();
    let slot = state.add_widget("prices").unwrap();
    let generation = state.widgets.slot(slot).unwrap().generation;
    let _ = state.widget_requests(Instant::now(), true);
    let document = luma_widgets::document::Document::parse(
        &json!({ "items": [ { "id": "btc", "card": { "label": "BTC", "value": "67 120", "change": { "text": "+2.4%", "tone": "up" } } } ] }),
        luma_widgets::manifest::CardKind::Ticker,
        &[],
    )
    .unwrap();
    state.widget_response(WidgetResponse::Document {
        slot,
        generation,
        result: Ok(Refreshed {
            document,
            source: "main".into(),
            refresh: 120,
            input: json!({}),
        }),
    });
    let scene = scene(&state);
    let tile = scene
        .controls
        .iter()
        .find(|c| c.target == crate::state::Target::WidgetItem(slot, 0))
        .expect("the ticker card");
    assert!((tile.rect.w - crate::design::TILE_W).abs() < 0.5);
    let text = texts(&state);
    assert!(text.iter().any(|t| t == "67 120") && text.iter().any(|t| t == "+2.4%"));
}

fn catalog_with(state: &mut State, manifests: &[(Manifest, Vec<u8>)]) {
    let entries = manifests
        .iter()
        .map(|(m, bytes)| {
            catalog::entry(m, bytes, &format!("manifests/{}-{}.json", m.id, m.version))
        })
        .collect();
    state.widget_response(WidgetResponse::Catalog(Ok(signed_index(entries, 3))));
}

#[test]
fn the_store_opens_from_add_widget_and_installs_onto_home() {
    let mut state = ready(&scratch("store"));
    state.focus = crate::state::Target::AddBlock;
    state.activate();
    let last = state.add_choices().len() - 1;
    assert!(texts(&state).iter().any(|t| t == "Widget store…"));
    state.focus = crate::state::Target::AddKind(last);
    state.activate();
    assert!(matches!(
        state.overlay,
        Some(crate::state::Overlay::Store { .. })
    ));
    assert!(
        state
            .widget_requests(Instant::now(), false)
            .iter()
            .any(|r| matches!(r, WidgetRequest::Catalog { .. }))
    );
    assert!(texts(&state).iter().any(|t| t == "Loading the catalog…"));
    let (manifest, bytes) = manifest("1.0.0", &["https://api.example.com"]);
    catalog_with(&mut state, &[(manifest.clone(), bytes.clone())]);
    let text = texts(&state);
    assert!(text.iter().any(|t| t == "Scores") && text.iter().any(|t| t == "Scores of games."));
    state.focus = crate::state::Target::StoreEntry(0);
    state.activate();
    assert!(
        matches!(&state.overlay, Some(crate::state::Overlay::StoreEntry { id, .. }) if id == "scores")
    );
    let text = texts(&state);
    for expected in [
        "CONNECTS TO",
        "api.example.com",
        "Your API key is sent only to api.example.com.",
        "Install and add to Home",
    ] {
        assert!(text.iter().any(|t| t == expected), "{expected} in {text:?}");
    }
    assert_eq!(state.focus, crate::state::Target::StoreAction(0));
    state.activate();
    let entry = state.store_entries()[0].clone();
    assert!(
        state
            .widget_requests(Instant::now(), false)
            .contains(&WidgetRequest::Install(entry.clone()))
    );
    let effects = state.widget_response(WidgetResponse::Installed(
        Box::new(entry),
        Ok(Box::new((manifest, bytes))),
    ));
    assert!(effects.contains(&Effect::Save));
    assert!(state.overlay.is_none());
    assert_eq!(
        state.focus,
        crate::state::Target::Block(BlockKind::Widget(0))
    );
    // Back on its page, it is installed: Add once more, or Remove.
    assert_eq!(
        state.store_actions("scores"),
        [
            crate::widget_state::StoreAction::Add,
            crate::widget_state::StoreAction::Remove
        ]
    );
}

#[test]
fn back_from_a_widgets_page_returns_to_the_list() {
    let mut state = ready(&scratch("store-back"));
    state.open_store(crate::state::Target::AddBlock);
    let (manifest, bytes) = manifest("1.0.0", &["https://api.example.com"]);
    catalog_with(&mut state, &[(manifest, bytes)]);
    state.focus = crate::state::Target::StoreEntry(0);
    state.activate();
    state.back();
    assert!(matches!(
        state.overlay,
        Some(crate::state::Overlay::Store { .. })
    ));
    assert_eq!(state.focus, crate::state::Target::StoreEntry(0));
    state.back();
    assert!(state.overlay.is_none());
    assert_eq!(state.focus, crate::state::Target::AddBlock);
}

#[test]
fn a_widgets_settings_page_follows_its_manifest() {
    let mut state = installed(&scratch("settings"));
    state.open(2);
    state.open_widget_settings(0);
    use crate::widget_state::WidgetRow;
    assert_eq!(
        state.widget_setting_rows(0),
        [
            WidgetRow::Setting(0),
            WidgetRow::SecretType(1),
            WidgetRow::SecretPhone(1)
        ]
    );
    let text = texts(&state);
    for expected in ["League", "A", "API key", "Not set", "Enter from your phone"] {
        assert!(text.iter().any(|t| t == expected), "{expected} in {text:?}");
    }
    // A choice opens its options; picking one saves it and returns.
    state.focus = crate::state::Target::WidgetSetting(0);
    state.activate();
    assert_eq!(
        state.subpage,
        Some(crate::state::Subpage::WidgetOptions(0, 0))
    );
    state.focus = crate::state::Target::Choice(1);
    let effects = state.activate();
    assert!(effects.contains(&Effect::Save));
    assert_eq!(
        state.preferences.widgets.slots[&0].settings["league"],
        json!("b")
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Widget(0)));
    assert_eq!(state.focus, crate::state::Target::WidgetSetting(0));
}

#[test]
fn a_checklist_keeps_at_least_its_minimum() {
    let mut value = manifest_json("1.0.0", &["https://api.example.com"]);
    value["id"] = json!("multi");
    value["settings"][0] = json!({ "id": "leagues", "type": "choices", "label": { "en": "Leagues" },
        "options": [ { "value": "a", "label": { "en": "A" } }, { "value": "b", "label": { "en": "B" } } ],
        "default": ["a"] });
    value["sources"][0]["requests"][0]["url"] =
        json!("https://api.example.com/{settings.leagues}.json");
    let bytes = serde_json::to_vec(&value).unwrap();
    let mut state = ready(&scratch("checklist"));
    state
        .widget_installed(Manifest::parse(&bytes).unwrap(), &bytes)
        .unwrap();
    let slot = state.add_widget("multi").unwrap();
    state.open(2);
    state.open_widget_settings(slot);
    state.focus = crate::state::Target::WidgetSetting(0);
    state.activate();
    state.focus = crate::state::Target::Choice(0);
    assert!(state.activate().is_empty(), "the last one stays");
    state.focus = crate::state::Target::Choice(1);
    state.activate();
    assert_eq!(state.slot_settings(slot)["leagues"], json!(["a", "b"]));
    state.focus = crate::state::Target::Choice(0);
    state.activate();
    assert_eq!(state.slot_settings(slot)["leagues"], json!(["b"]));
    assert_eq!(
        state.subpage,
        Some(crate::state::Subpage::WidgetOptions(slot, 0)),
        "a checklist stays open"
    );
}

#[test]
fn a_secret_typed_or_sent_from_a_phone_goes_to_the_secrets_file() {
    let prefs = scratch("secret-entry");
    let mut state = installed(&prefs);
    state.open(2);
    state.open_widget_settings(0);
    state.focus = crate::state::Target::WidgetSetting(1);
    state.activate();
    assert!(state.text_input_active());
    state.text("typed-key-1");
    state.finish_text_input();
    assert_eq!(
        state.widgets.store.secret("scores", "key"),
        Some("typed-key-1")
    );
    assert!(
        !serde_json::to_string(&state.preferences)
            .unwrap()
            .contains("typed-key-1")
    );
    assert!(texts(&state).iter().any(|t| t == "•••• ey-1"));
    assert!(
        state
            .widget_setting_rows(0)
            .contains(&crate::widget_state::WidgetRow::SecretForget(1))
    );
    // From the phone.
    state.focus = crate::state::Target::WidgetSetting(2);
    assert_eq!(state.activate(), [Effect::PhoneKeyStart]);
    assert!(
        state
            .phone_widget_secret()
            .is_some_and(|(label, ..)| label == "API key")
    );
    state.phone_key_received("phone-key-2".into());
    assert_eq!(
        state.widgets.store.secret("scores", "key"),
        Some("phone-key-2")
    );
    assert_eq!(state.focus, crate::state::Target::WidgetSetting(2));
    // Forget it.
    state.focus = crate::state::Target::WidgetSetting(3);
    state.activate();
    assert_eq!(state.widgets.store.secret("scores", "key"), None);
}

#[test]
fn a_secret_given_at_run_time_is_used_but_never_saved() {
    let prefs = scratch("runtime-secret");
    let mut state = installed(&prefs);
    state
        .widgets
        .store
        .set_runtime_secret("scores", "key", "from-the-environment".into());
    assert_eq!(
        state.widgets.store.secret("scores", "key"),
        Some("from-the-environment")
    );
    let job = state
        .widget_requests(Instant::now(), true)
        .into_iter()
        .find_map(|r| match r {
            WidgetRequest::Refresh(job) => Some(job),
            _ => None,
        })
        .unwrap();
    assert_eq!(job.secrets["key"], "from-the-environment");
    // Saving another secret does not write it either.
    state
        .set_widget_secret(0, "key", Some("typed".into()))
        .unwrap();
    let file =
        std::fs::read_to_string(prefs.parent().unwrap().join("secrets.json")).unwrap_or_default();
    assert!(!file.contains("from-the-environment"));
    assert_eq!(
        crate::widget_store::runtime_secret_variable("my-widget", "api_key"),
        "LUMA_WIDGET_SECRET_MY_WIDGET_API_KEY"
    );
}
