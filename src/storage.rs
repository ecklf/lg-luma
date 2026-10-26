use crate::model::Preferences;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn default_path(webos: bool) -> Result<PathBuf, String> {
    if webos {
        #[cfg(all(feature = "webos", unix))]
        return tv_path();
        #[cfg(not(all(feature = "webos", unix)))]
        return Err("TV storage requires a webOS build".into());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .ok_or("Set HOME, XDG_CONFIG_HOME or --preferences for native preferences")?;
    Ok(base.join("luma-native/preferences.json"))
}

#[cfg(all(feature = "webos", unix))]
fn tv_path() -> Result<PathBuf, String> {
    // The executable takeover runs at LG's path, but must retain the source
    // installation's storage precedence and installation-local migration path.
    let installation = crate::platform::install_dir()?;
    let legacy = installation.join("preferences.json");
    let id = crate::platform::APP_ID;
    // These volumes persist across boots. The app-specific directory also
    // survives replacement of the installed payload on an ordinary update.
    // Production native jails expose internal storage instead of developer.
    let developer = PathBuf::from("/media/developer")
        .join(id)
        .join("preferences.json");
    let shared_developer = PathBuf::from("/media/developer/temp")
        .join(id)
        .join("preferences.json");
    let internal = PathBuf::from("/media/internal")
        .join(format!(".{id}"))
        .join("preferences.json");
    let candidates = if installation.starts_with("/media/cryptofs") {
        [internal, developer, shared_developer]
    } else {
        [developer, shared_developer, internal]
    };
    tv_path_in(candidates, &legacy, unsafe { libc::geteuid() })
}

#[cfg(all(feature = "webos", unix))]
fn storage_rank(existing: bool, owner: Option<u32>, user: u32) -> (bool, bool) {
    // Root Home must prefer the app's existing settings over a root-owned
    // fallback left by an earlier takeover. Never merge or overwrite either.
    (!existing, user == 0 && owner == Some(0))
}

#[cfg(all(feature = "webos", unix))]
fn tv_path_in(mut candidates: [PathBuf; 3], legacy: &Path, user: u32) -> Result<PathBuf, String> {
    use std::{
        io::Write,
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    };

    // Keep existing settings ahead of new destinations, including after a reboot.
    candidates.sort_by_key(|path| {
        let owner = path
            .parent()
            .and_then(|directory| fs::symlink_metadata(directory).ok())
            .filter(|metadata| metadata.is_dir())
            .map(|metadata| metadata.uid());
        storage_rank(path.exists(), owner, user)
    });
    let mut failures = Vec::new();
    for path in candidates {
        let directory = path.parent().ok_or("Invalid TV storage directory")?;
        // Create only this app's leaf, not a missing system mount hierarchy.
        let created = fs::DirBuilder::new().mode(0o700).create(directory);
        if let Err(error) = created
            && error.kind() != std::io::ErrorKind::AlreadyExists
        {
            failures.push(format!("{}: {error}", directory.display()));
            continue;
        }
        let metadata = match fs::symlink_metadata(directory) {
            Ok(metadata) => metadata,
            Err(error) => {
                failures.push(format!("{}: {error}", directory.display()));
                continue;
            }
        };
        // Home runs as root rather than the jailed app user. It may reuse the
        // app's directory, but a normal app must still reject foreign owners.
        // symlink_metadata keeps symlink directories rejected in both modes.
        if !metadata.is_dir() || (user != 0 && metadata.uid() != user) {
            failures.push(format!(
                "{} is not owned by the app user",
                directory.display()
            ));
            continue;
        }
        let probe = directory.join(format!(".write-probe-{}", std::process::id()));
        let writable = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&probe)?;
            let result = file
                .write_all(b"Luma storage probe")
                .and_then(|_| file.sync_all());
            drop(file);
            // Only remove a probe that this process created successfully.
            let cleanup = fs::remove_file(&probe);
            result.and(cleanup)
        })();
        if let Err(error) = writable {
            failures.push(format!("{}: {error}", directory.display()));
            continue;
        }
        if !path.exists() {
            // Import JSON only from the old path in THIS installation, never
            // browser storage or another app ID's preferences.
            match fs::read(legacy) {
                Ok(bytes) => {
                    let mut preferences: Preferences =
                        serde_json::from_slice(&bytes).map_err(|e| {
                            format!(
                                "Existing preferences {} could not be read: {e}",
                                legacy.display()
                            )
                        })?;
                    preferences.sanitize();
                    save(&path, &preferences)?;
                    eprintln!(
                        "Luma preferences migrated from {} to {}",
                        legacy.display(),
                        path.display()
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Existing preferences {} could not be read: {error}",
                        legacy.display()
                    ));
                }
            }
            let old_cache = crate::weather_service::cache_path(legacy);
            let cache = crate::weather_service::cache_path(&path);
            if !cache.exists()
                && let Some(reading) = crate::weather_service::load_cache(&old_cache)
                && let Err(error) = crate::weather_service::save_cache(&cache, &reading)
            {
                eprintln!("Luma weather cache migration: {error}");
            }
        }
        eprintln!("Luma persistent storage: {}", directory.display());
        return Ok(path);
    }
    Err(format!(
        "No writable persistent TV storage found: {}",
        failures.join("; ")
    ))
}

pub fn load(path: &Path) -> Result<Preferences, String> {
    match fs::read(path) {
        Ok(bytes) => {
            let mut preferences: Preferences = serde_json::from_slice(&bytes)
                .map_err(|e| format!("Preferences could not be read: {e}"))?;
            preferences.sanitize();
            Ok(preferences)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(e) => Err(format!("Preferences could not be read: {e}")),
    }
}

pub fn save(path: &Path, preferences: &Preferences) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(preferences)
        .map_err(|e| format!("Could not encode preferences: {e}"))?;
    write_atomic(path, &bytes)
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::{
        io::Write,
        sync::atomic::{AtomicUsize, Ordering},
    };

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Could not prepare preferences directory {}: {e}",
                parent.display()
            )
        })?;
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let temporary = path.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Never follow an app-created temporary symlink while running as root.
    let mut file = options
        .open(&temporary)
        .map_err(|e| format!("Could not create {}: {e}", temporary.display()))?;
    let result = (|| {
        file.write_all(bytes)?;
        #[cfg(all(feature = "webos", unix))]
        if unsafe { libc::geteuid() } == 0 {
            use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let metadata = fs::symlink_metadata(parent)?;
            if !metadata.is_dir() {
                return Err(std::io::Error::other(
                    "Storage directory is not a real directory",
                ));
            }
            // Atomic replacement must not turn private app files into root-only
            // files: disabling Home should leave the normal app able to read them.
            if unsafe { libc::fchown(file.as_raw_fd(), metadata.uid(), metadata.gid()) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("Could not save {}: {e}", path.display()))
}

#[cfg(all(test, feature = "webos", unix))]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::{MetadataExt, PermissionsExt, symlink},
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "luma-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }

        fn candidates(&self) -> [PathBuf; 3] {
            ["developer", "shared", "internal"]
                .map(|name| self.0.join(name).join("preferences.json"))
        }

        fn legacy(&self) -> PathBuf {
            self.0.join("preferences.json")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn root_home_reuses_each_existing_app_storage_destination() {
        for index in 0..3 {
            let fixture = Fixture::new();
            let candidates = fixture.candidates();
            let existing = &candidates[index];
            let preferences = Preferences {
                background: "forest".into(),
                favorites: Some(vec!["youtube.leanback.v4".into()]),
                ..Preferences::default()
            };
            save(existing, &preferences).unwrap();
            let original = fs::read(existing).unwrap();
            // Model root Home on a host-owned directory without requiring sudo.
            let selected = tv_path_in(candidates.clone(), &fixture.legacy(), 0).unwrap();
            assert_eq!(&selected, existing);
            assert_eq!(load(&selected).unwrap().background, "forest");
            assert_eq!(fs::read(existing).unwrap(), original);
            for (other, path) in candidates.iter().enumerate() {
                if other != index {
                    assert!(!path.parent().unwrap().exists());
                }
            }
            let normal =
                tv_path_in(candidates, &fixture.legacy(), unsafe { libc::geteuid() }).unwrap();
            assert_eq!(normal, selected);
        }
    }

    #[test]
    fn existing_app_preferences_rank_before_root_fallbacks_without_changing_normal_order() {
        assert!(storage_rank(true, Some(5000), 0) < storage_rank(true, Some(0), 0));
        assert!(storage_rank(true, Some(0), 0) < storage_rank(false, Some(5000), 0));
        assert_eq!(
            storage_rank(true, Some(5000), 5000),
            storage_rank(true, Some(0), 5000)
        );
    }

    #[test]
    fn normal_app_still_rejects_foreign_owned_directories() {
        let fixture = Fixture::new();
        let candidates = fixture.candidates();
        let foreign_user = unsafe { libc::geteuid() } + 1;
        let error = tv_path_in(candidates.clone(), &fixture.legacy(), foreign_user).unwrap_err();
        assert!(error.contains("not owned by the app user"));
        assert!(candidates.iter().all(|path| !path.exists()));
    }

    #[test]
    fn root_and_normal_app_both_reject_symlink_storage_directories() {
        for user in [0, unsafe { libc::geteuid() }] {
            let fixture = Fixture::new();
            let candidates = fixture.candidates();
            save(&candidates[1], &Preferences::default()).unwrap();
            symlink(
                candidates[1].parent().unwrap(),
                candidates[0].parent().unwrap(),
            )
            .unwrap();
            assert_eq!(
                tv_path_in(candidates.clone(), &fixture.legacy(), user).unwrap(),
                candidates[1]
            );
        }
    }

    #[test]
    fn existing_corrupt_app_preferences_are_not_replaced_with_defaults() {
        let fixture = Fixture::new();
        let candidates = fixture.candidates();
        fs::create_dir(candidates[1].parent().unwrap()).unwrap();
        fs::write(&candidates[1], "broken").unwrap();
        let selected = tv_path_in(candidates.clone(), &fixture.legacy(), 0).unwrap();
        assert_eq!(selected, candidates[1]);
        assert!(load(&selected).is_err());
        assert_eq!(fs::read_to_string(&selected).unwrap(), "broken");
    }

    #[test]
    fn root_home_can_still_import_installation_local_preferences_once() {
        let fixture = Fixture::new();
        let candidates = fixture.candidates();
        save(
            &fixture.legacy(),
            &Preferences {
                background: "forest".into(),
                ..Preferences::default()
            },
        )
        .unwrap();
        let selected = tv_path_in(candidates.clone(), &fixture.legacy(), 0).unwrap();
        assert_eq!(selected, candidates[0]);
        assert_eq!(load(&selected).unwrap().background, "forest");
        fs::write(fixture.legacy(), "broken").unwrap();
        assert_eq!(
            tv_path_in(candidates, &fixture.legacy(), 0).unwrap(),
            selected
        );
        assert_eq!(load(&selected).unwrap().background, "forest");
    }

    #[test]
    fn atomic_saves_keep_private_app_owned_files_and_clean_up_failed_writes() {
        let fixture = Fixture::new();
        let preferences = fixture.legacy();
        for bytes in [b"first".as_slice(), b"second".as_slice()] {
            write_atomic(&preferences, bytes).unwrap();
            let owner = fs::metadata(&fixture.0).unwrap();
            let saved = fs::metadata(&preferences).unwrap();
            assert_eq!((saved.uid(), saved.gid()), (owner.uid(), owner.gid()));
            assert_eq!(saved.permissions().mode() & 0o777, 0o600);
            assert_eq!(fs::read(&preferences).unwrap(), bytes);
            assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
        }
        let blocked = fixture.0.join("blocked.json");
        fs::create_dir(&blocked).unwrap();
        assert!(write_atomic(&blocked, b"cannot replace a directory").is_err());
        assert!(blocked.is_dir());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
        assert_eq!(fs::read(&preferences).unwrap(), b"second");
    }
}
