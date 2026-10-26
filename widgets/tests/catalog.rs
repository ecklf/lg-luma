use luma_widgets::catalog::{self, Index, KeyPair};
use serde_json::{Value, json};

fn manifest() -> Value {
    json!({
        "schema": 1, "id": "demo", "version": "1.2.0", "luma": "2.9.0",
        "name": { "en": "Demo" }, "description": { "en": "A demo." },
        "authors": ["eluz1ve"], "icon": "tv",
        "origins": ["https://example.com", "https://api.example.com"],
        "size": { "default": [6, 2], "min": [4, 1], "max": [12, 3] },
        "card": "media",
        "settings": [ { "id": "key", "type": "secret", "label": { "en": "Key" },
                        "send_to": ["https://api.example.com"] } ],
        "sources": [ { "id": "main", "requests": [ { "id": "list", "url": "https://example.com/list.json" } ],
                       "transform": "{items: []}" } ]
    })
}

fn keys() -> KeyPair {
    KeyPair::from_seed([7; 32])
}

fn index(sequence: u64, bytes: &[u8]) -> Vec<u8> {
    let manifest = luma_widgets::manifest::Manifest::parse(bytes).unwrap();
    let entry = catalog::entry(&manifest, bytes, "manifests/demo-1.2.0.json");
    serde_json::to_vec_pretty(&Index {
        schema: 1,
        sequence,
        published: "2026-10-26T12:00:00Z".into(),
        widgets: vec![entry],
    })
    .unwrap()
}

#[test]
fn a_signed_index_verifies_with_the_compiled_key() {
    let bytes = serde_json::to_vec(&manifest()).unwrap();
    let index = index(5, &bytes);
    let signature = keys().sign(&index);
    let verified = catalog::verify(&index, &signature, &[keys().public()], 5).unwrap();
    assert_eq!(verified.sequence, 5);
    assert_eq!(verified.widgets[0].id, "demo");
    assert_eq!(verified.widgets[0].size, bytes.len());
    // A second key is accepted too, for rotation.
    let other = KeyPair::from_seed([8; 32]);
    assert!(catalog::verify(&index, &signature, &[other.public(), keys().public()], 0).is_ok());
}

#[test]
fn tampered_unknown_or_replayed_indexes_are_refused() {
    let bytes = serde_json::to_vec(&manifest()).unwrap();
    let index = index(5, &bytes);
    let signature = keys().sign(&index);
    let mut tampered = index.clone();
    let at = tampered.len() / 2;
    tampered[at] ^= 1;
    assert!(catalog::verify(&tampered, &signature, &[keys().public()], 0).is_err());
    let other = KeyPair::from_seed([8; 32]);
    assert!(catalog::verify(&index, &signature, &[other.public()], 0).is_err());
    let error = catalog::verify(&index, &signature, &[keys().public()], 6).unwrap_err();
    assert!(error.0.contains("older"), "{error}");
    assert!(catalog::verify(&index, "not base64!", &[keys().public()], 0).is_err());
}

#[test]
fn a_manifest_must_match_its_entry() {
    let bytes = serde_json::to_vec(&manifest()).unwrap();
    let index: Index = serde_json::from_slice(&index(1, &bytes)).unwrap();
    let entry = &index.widgets[0];
    assert_eq!(catalog::check_manifest(entry, &bytes).unwrap().id, "demo");
    let mut changed = bytes.clone();
    changed.push(b' ');
    assert!(catalog::check_manifest(entry, &changed).is_err());
    // Same bytes as the entry claims, but the manifest reaches further than the entry says.
    let mut wider = manifest();
    wider["origins"]
        .as_array_mut()
        .unwrap()
        .push(json!("https://more.example.com"));
    let wider_bytes = serde_json::to_vec(&wider).unwrap();
    let mut entry = entry.clone();
    entry.size = wider_bytes.len();
    entry.sha256 = catalog::sha256(&wider_bytes);
    let error = catalog::check_manifest(&entry, &wider_bytes).unwrap_err();
    assert!(error.0.contains("access"), "{error}");
}

#[test]
fn every_program_of_a_manifest_must_compile() {
    let mut bad = manifest();
    bad["sources"][0]["transform"] = json!("now");
    let manifest =
        luma_widgets::manifest::Manifest::parse(&serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(catalog::check_programs(&manifest).is_err());
    let mut bad_var = self::manifest();
    bad_var["sources"][0]["requests"][0]["vars"] = json!({ "x": "input" });
    bad_var["sources"][0]["requests"][0]["url"] = json!("https://example.com/{var.x}.json");
    let manifest =
        luma_widgets::manifest::Manifest::parse(&serde_json::to_vec(&bad_var).unwrap()).unwrap();
    assert!(catalog::check_programs(&manifest).is_err());
}
