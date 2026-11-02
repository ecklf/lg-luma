use super::*;
use serde_json::json;

const SHA: &str = "74d9788be634b57fed0591e7e82c9e3d8856dc46d14d29998a481724299d8824";

fn package(id: &str, version: &str, url: &str, sha: &str, size: u64) -> Value {
    json!({"id": id, "title": format!("{id} title"), "manifestUrl": "https://example.com/m.json",
        "manifest": {"id": id, "version": version, "ipkUrl": url, "ipkHash": {"sha256": sha}, "ipkSize": size}})
}

fn moonlight(version: &str) -> Value {
    package(
        "com.limelight.webos",
        version,
        &format!(
            "https://github.com/mariotaku/moonlight-tv/releases/download/v{version}/com.limelight.webos_{version}_arm.ipk"
        ),
        SHA,
        1_508_072,
    )
}

#[test]
fn the_repository_yields_only_packages_with_complete_safe_metadata() {
    let feed = json!({"packages": [
        moonlight("1.6.37"),
        // Not HTTPS, a path escape, a wrong checksum, no size, too large, an invalid version,
        // and a manifest for another app: all skipped, not fatal.
        package("a.http", "1.0.0", "http://example.com/a.ipk", SHA, 10),
        package("b.escape", "1.0.0", "https://example.com/../b.ipk", SHA, 10),
        package("c.sha", "1.0.0", "https://example.com/c.ipk", "abc", 10),
        package("d.size", "1.0.0", "https://example.com/d.ipk", SHA, 0),
        package("e.large", "1.0.0", "https://example.com/e.ipk", SHA, 200 * 1024 * 1024),
        package("f.version", "1.0-beta", "https://example.com/f.ipk", SHA, 10),
        package("g.zip", "1.0.0", "https://example.com/g.zip", SHA, 10),
        {"id": "h.other", "manifest": {"id": "i.other"}},
    ]});
    let repo = parse_repo(&feed).unwrap();
    assert_eq!(repo.keys().collect::<Vec<_>>(), ["com.limelight.webos"]);
    let (title, release) = &repo["com.limelight.webos"];
    assert_eq!(title, "com.limelight.webos title");
    assert_eq!(release.version, "1.6.37");
    assert_eq!(release.sha256, SHA);
    assert!(parse_repo(&json!({"nothing": []})).is_err());
}

#[test]
fn only_newer_versions_of_installed_homebrew_apps_are_due_never_luma_or_homebrew_channel() {
    let repo = parse_repo(&json!({"packages": [
        moonlight("1.6.37"),
        package("youtube.leanback.v4", "0.5.3", "https://example.com/y.ipk", SHA, 10),
        package("com.ecklf.luma", "9.0.0", "https://example.com/l.ipk", SHA, 10),
        package("org.webosbrew.hbchannel", "9.0.0", "https://example.com/h.ipk", SHA, 10),
        package("not.installed", "1.0.0", "https://example.com/n.ipk", SHA, 10),
    ]}))
    .unwrap();
    let installed = BTreeMap::from([
        ("com.limelight.webos".to_string(), "1.6.36".to_string()),
        ("youtube.leanback.v4".to_string(), "0.5.3".to_string()),
        ("com.ecklf.luma".to_string(), "2.16.0".to_string()),
        ("org.webosbrew.hbchannel".to_string(), "0.7.3".to_string()),
        ("local.only".to_string(), "1.0.0".to_string()),
    ]);
    let due = due(&installed, &repo);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "com.limelight.webos");
    assert_eq!(due[0].release.version, "1.6.37");
    // A repository that is behind the installed version never downgrades.
    let older = BTreeMap::from([("com.limelight.webos".to_string(), "1.7.0".to_string())]);
    assert!(self::due(&older, &repo).is_empty());
}

#[test]
fn installed_apps_are_read_from_their_own_manifests() {
    let root = std::env::temp_dir().join(format!("luma-homebrew-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let app = |dir: &str, manifest: Option<Value>| {
        fs::create_dir_all(root.join(dir)).unwrap();
        if let Some(manifest) = manifest {
            fs::write(root.join(dir).join("appinfo.json"), manifest.to_string()).unwrap();
        }
    };
    app(
        "com.limelight.webos",
        Some(json!({"id": "com.limelight.webos", "version": "1.6.36"})),
    );
    // A folder whose manifest names another app, one without a manifest, one with broken JSON.
    app(
        "youtube.leanback.v4",
        Some(json!({"id": "com.limelight.webos", "version": "9.9.9"})),
    );
    app("empty", None);
    fs::create_dir_all(root.join("broken")).unwrap();
    fs::write(root.join("broken/appinfo.json"), "{").unwrap();
    let found = installed(&root);
    assert_eq!(
        found,
        BTreeMap::from([("com.limelight.webos".to_string(), "1.6.36".to_string())])
    );
    assert!(installed(&root.join("missing")).is_empty());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_worker_stays_idle_outside_tv_runs() {
    let worker = Worker::new(false);
    worker.set_enabled(true);
    assert!(worker.receiver.try_recv().is_err());
}
