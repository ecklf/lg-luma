use luma_widgets::catalog::{self, KeyPair};
use luma_widgets::tree;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("luma-widgets-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, value: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value).unwrap();
}

/// A widget folder: a manifest naming its jq files, fixtures and expected output.
fn widget(root: &Path) -> PathBuf {
    let dir = root.join("demo");
    let manifest = json!({
        "schema": 1, "id": "demo", "version": "1.0.0", "luma": "2.9.0",
        "name": { "en": "Demo" }, "description": { "en": "A demo." }, "icon": "tv",
        "origins": ["https://feed.example.com"],
        "size": { "default": [6, 2], "min": [4, 1], "max": [12, 3] },
        "card": "ticker",
        "settings": [ { "id": "coins", "type": "choices", "label": { "en": "Coins" },
                        "options": [ { "value": "btc", "label": { "en": "BTC" } },
                                     { "value": "eth", "label": { "en": "ETH" } } ],
                        "default": ["btc"] } ],
        "library": { "file": "library.jq" },
        "sources": [ { "id": "main",
                       "requests": [ { "id": "price", "for_each": "coins",
                                       "url": "https://feed.example.com/{item}.json" } ],
                       "transform": { "file": "main.jq" } } ]
    });
    write(
        &dir.join("manifest.json"),
        &serde_json::to_string_pretty(&manifest).unwrap(),
    );
    write(
        &dir.join("library.jq"),
        "def card: {label: .key, value: .value.price};",
    );
    write(
        &dir.join("main.jq"),
        "{items: [.price | to_entries[] | {id: .key, card: card}]}",
    );
    write(
        &dir.join("fixtures/main/context.json"),
        r#"{"settings": {"coins": ["btc", "eth"]}, "lang": "en", "now": 1793021218}"#,
    );
    write(
        &dir.join("fixtures/main/index.json"),
        r#"{"https://feed.example.com/btc.json": "btc.json", "https://feed.example.com/eth.json": "eth.json"}"#,
    );
    write(&dir.join("fixtures/main/btc.json"), r#"{"price": 67120}"#);
    write(&dir.join("fixtures/main/eth.json"), r#"{"price": 2400}"#);
    write(
        &dir.join("expected/main.json"),
        r#"{"items": [{"id": "btc", "card": {"label": "btc", "value": 67120}},
                      {"id": "eth", "card": {"label": "eth", "value": 2400}}]}"#,
    );
    dir
}

fn in_process(job: luma_widgets::sandbox::Job) -> Result<Value, luma_widgets::Error> {
    luma_widgets::jq::run(&job.program, job.input, &job.vars)
}

#[test]
fn a_widget_folder_loads_with_its_files_inlined() {
    let root = scratch("load");
    let dir = widget(&root);
    let (manifest, bytes) = tree::load(&dir).unwrap();
    assert_eq!(
        manifest.library,
        "def card: {label: .key, value: .value.price};"
    );
    assert!(manifest.sources[0].transform.starts_with("{items:"));
    let packed: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        packed["sources"][0]["transform"],
        json!(manifest.sources[0].transform)
    );
}

#[test]
fn fixtures_run_and_match_the_expected_output() {
    let root = scratch("fixtures");
    let dir = widget(&root);
    let output = tree::run_fixture(&dir, "main", &mut in_process).unwrap();
    assert_eq!(output["items"][1]["card"]["value"], json!(2400));
    assert_eq!(tree::test(&dir, &mut in_process).unwrap(), ["main"]);
    write(&dir.join("fixtures/main/eth.json"), r#"{"price": 1}"#);
    let error = tree::test(&dir, &mut in_process).unwrap_err();
    assert!(error.0.contains("main"), "{error}");
}

#[test]
fn a_request_without_a_fixture_fails_the_run() {
    let root = scratch("missing");
    let dir = widget(&root);
    std::fs::write(
        dir.join("fixtures/main/index.json"),
        r#"{"https://feed.example.com/btc.json": "btc.json"}"#,
    )
    .unwrap();
    let error = tree::run_fixture(&dir, "main", &mut in_process).unwrap_err();
    assert!(error.0.contains("eth.json"), "{error}");
}

#[test]
fn packing_and_signing_give_a_catalog_luma_accepts() {
    let root = scratch("pack");
    widget(&root);
    std::fs::create_dir_all(root.join("_template")).unwrap();
    let out = root.join("out");
    let index = tree::pack(&root, &out, 42, "2026-10-26T12:00:00Z").unwrap();
    assert_eq!(index.widgets.len(), 1);
    assert_eq!(index.widgets[0].url, "manifests/demo-1.0.0.json");
    let keys = KeyPair::from_seed([3; 32]);
    tree::sign(&out, &keys).unwrap();
    let bytes = std::fs::read(out.join("index.json")).unwrap();
    let signature = std::fs::read_to_string(out.join("index.json.sig")).unwrap();
    let verified = catalog::verify(&bytes, &signature, &[keys.public()], 0).unwrap();
    let manifest = std::fs::read(out.join(&verified.widgets[0].url)).unwrap();
    assert_eq!(
        catalog::check_manifest(&verified.widgets[0], &manifest)
            .unwrap()
            .id,
        "demo"
    );
}
