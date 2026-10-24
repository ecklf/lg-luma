//! Find the package owning the running binary, including an executable-only Home bind mount.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn directories() -> impl Iterator<Item = PathBuf> {
    // The linker uses this same list for libraries needed before Rust startup.
    include_str!("../tv/install-directories.txt")
        .lines()
        .map(PathBuf::from)
}

fn same_executable(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::metadata(a), fs::metadata(b)) {
            (Ok(a), Ok(b)) => {
                a.is_file() && b.is_file() && a.dev() == b.dev() && a.ino() == b.ino()
            }
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        fs::canonicalize(a)
            .ok()
            .zip(fs::canonicalize(b).ok())
            .is_some_and(|(a, b)| a == b)
    }
}

fn owns_executable(directory: &Path, executable: &Path) -> bool {
    if !directory.join("assets").is_dir() || !same_executable(&directory.join("luma"), executable) {
        return false;
    }
    let manifest = fs::read(directory.join("appinfo.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    manifest.is_some_and(|manifest| {
        manifest.get("id").and_then(serde_json::Value::as_str) == Some(super::APP_ID)
            && manifest.get("type").and_then(serde_json::Value::as_str) == Some("native")
            && manifest.get("main").and_then(serde_json::Value::as_str) == Some("luma")
    })
}

pub fn find_directory(
    executable: &Path,
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Result<PathBuf, String> {
    executable
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .chain(candidates)
        .find(|directory| owns_executable(directory, executable))
        .ok_or_else(|| {
            format!(
                "No installed native Luma package matches {}",
                executable.display()
            )
        })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "luma-install-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }

        fn install(&self, location: &str) -> PathBuf {
            let directory = self.0.join(location).join(super::super::APP_ID);
            fs::create_dir_all(directory.join("assets")).unwrap();
            fs::write(directory.join("luma"), b"native executable").unwrap();
            fs::write(
                directory.join("appinfo.json"),
                serde_json::json!({
                    "id": super::super::APP_ID, "type": "native", "main": "luma"
                })
                .to_string(),
            )
            .unwrap();
            directory
        }

        fn mounted(&self, directory: &Path) -> PathBuf {
            let executable = self.0.join("usr/bin/com.webos.app.home");
            fs::create_dir_all(executable.parent().unwrap()).unwrap();
            // A hard link preserves the same device/inode relationship as a file bind mount.
            fs::hard_link(directory.join("luma"), &executable).unwrap();
            executable
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn normal_launch_keeps_its_own_package_even_with_another_installation() {
        let fixture = Fixture::new();
        let developer = fixture.install("developer");
        let production = fixture.install("cryptofs");
        for directory in [&developer, &production] {
            assert_eq!(
                find_directory(
                    &directory.join("luma"),
                    [production.clone(), developer.clone()]
                )
                .unwrap(),
                *directory
            );
        }
    }

    #[test]
    fn home_mount_selects_its_source_for_both_developer_and_production_installs() {
        for source in ["developer", "cryptofs"] {
            let fixture = Fixture::new();
            let developer = fixture.install("developer");
            let production = fixture.install("cryptofs");
            let selected = if source == "developer" {
                &developer
            } else {
                &production
            };
            let executable = fixture.mounted(selected);
            // Both installations contain identical binary bytes; only the inode identifies the source.
            assert_eq!(
                find_directory(&executable, [developer.clone(), production.clone()]).unwrap(),
                *selected
            );
        }
    }

    #[test]
    fn missing_or_unrelated_package_never_silently_uses_another_installation() {
        let fixture = Fixture::new();
        let developer = fixture.install("developer");
        let production = fixture.install("cryptofs");
        let executable = fixture.mounted(&production);
        assert!(find_directory(&executable, [developer.clone()]).is_err());
        fs::remove_dir(production.join("assets")).unwrap();
        assert!(find_directory(&executable, [developer, production]).is_err());
    }

    #[test]
    fn home_mount_requires_the_matching_native_package_manifest() {
        for manifest in [
            "not json",
            r#"{"id":"other","type":"native","main":"luma"}"#,
            r#"{"id":"com.ecklf.luma","type":"web","main":"luma"}"#,
            r#"{"id":"com.ecklf.luma","type":"native","main":"other"}"#,
        ] {
            let fixture = Fixture::new();
            let directory = fixture.install("cryptofs");
            let executable = fixture.mounted(&directory);
            fs::write(directory.join("appinfo.json"), manifest).unwrap();
            assert!(find_directory(&executable, [directory]).is_err());
        }
    }
}
