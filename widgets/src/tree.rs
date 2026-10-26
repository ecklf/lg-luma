//! A widget's source folder in the catalog repository (docs/widgets.md §9): `manifest.json`
//! naming its jq files, `fixtures/<case>/` with a context and sample replies, and
//! `expected/<case>.json`. A case runs the source its context names, or the one named like it.
//! Packing inlines the files, hashes each manifest and writes the catalog's index.
use crate::Error;
use crate::catalog::{self, Index, KeyPair};
use crate::fetch::{Fetch, FetchError};
use crate::manifest::Manifest;
use crate::refresh::{self, Cache, Context};
use crate::sandbox::Job;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

fn read(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|e| Error(format!("{}: {e}", path.display())))
}

fn read_json(path: &Path) -> Result<Value, Error> {
    serde_json::from_str(&read(path)?).map_err(|e| Error(format!("{}: {e}", path.display())))
}

/// `{"file": "x.jq"}` replaced by the file's text, from the widget's own folder only.
fn inline(dir: &Path, value: &mut Value) -> Result<(), Error> {
    let Some(file) = value.get("file").and_then(Value::as_str) else {
        return Ok(());
    };
    if file.contains("..") || file.starts_with('/') {
        return Err(Error(format!(
            "{file}: files must be inside the widget's folder"
        )));
    }
    *value = Value::String(read(&dir.join(file))?);
    Ok(())
}

/// The widget in `dir` with its jq files inlined, and the bytes its catalog manifest has.
pub fn load(dir: &Path) -> Result<(Manifest, Vec<u8>), Error> {
    let mut value = read_json(&dir.join("manifest.json"))?;
    if let Some(library) = value.get_mut("library") {
        inline(dir, library)?;
    }
    for source in value
        .get_mut("sources")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(transform) = source.get_mut("transform") {
            inline(dir, transform)?;
        }
    }
    let bytes = serde_json::to_vec(&value).map_err(|e| Error(e.to_string()))?;
    let manifest = Manifest::parse(&bytes)?;
    catalog::check_programs(&manifest)?;
    Ok((manifest, bytes))
}

/// Replies from files, by URL, as `fixtures/<source>/index.json` maps them.
pub struct Fixtures {
    dir: std::path::PathBuf,
    files: BTreeMap<String, String>,
}

impl Fixtures {
    pub fn load(dir: &Path) -> Result<Self, Error> {
        let files = serde_json::from_value(read_json(&dir.join("index.json"))?)
            .map_err(|e| Error(format!("{}: {e}", dir.join("index.json").display())))?;
        Ok(Self {
            dir: dir.to_owned(),
            files,
        })
    }
}

impl Fetch for Fixtures {
    fn get(&self, url: &str, _: &[(String, String)], _: usize) -> Result<Value, FetchError> {
        let file = self
            .files
            .get(url)
            .ok_or_else(|| FetchError::Unavailable(format!("no fixture for {url}")))?;
        read_json(&self.dir.join(file)).map_err(|e| FetchError::Unavailable(e.0))
    }
}

/// Runs a fixture case and gives the transform's output as it is.
pub fn run_fixture(
    dir: &Path,
    case: &str,
    run: &mut dyn FnMut(Job) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let (manifest, _) = load(dir)?;
    let fixtures = dir.join("fixtures").join(case);
    let context = read_json(&fixtures.join("context.json"))?;
    let source = context
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or(case);
    let saved = context
        .get("settings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let settings = manifest.settings_values(&saved);
    let secrets: BTreeMap<String, String> = context
        .get("secrets")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| Error(format!("context secrets: {e}")))?
        .unwrap_or_default();
    let lang = context.get("lang").and_then(Value::as_str).unwrap_or("en");
    let now = context.get("now").and_then(Value::as_i64).unwrap_or(0);
    let cx = Context {
        manifest: &manifest,
        settings: &settings,
        secrets: &secrets,
        lang,
        now,
    };
    let chosen = refresh::source(&cx).map_err(|e| Error(e.to_string()))?;
    if chosen.id != source {
        return Err(Error(format!(
            "the context selects source {}, not {source}",
            chosen.id
        )));
    }
    let mut output = Value::Null;
    refresh::run(
        &cx,
        &Fixtures::load(&fixtures)?,
        &mut Cache::default(),
        &mut |job| {
            let result = run(job);
            if let Ok(value) = &result {
                output = value.clone();
            }
            result
        },
    )
    .map_err(|e| Error(format!("{case}: {e}")))?;
    Ok(output)
}

/// Every case with an `expected/<case>.json`, run on its fixtures; the cases that passed.
pub fn test(
    dir: &Path,
    run: &mut dyn FnMut(Job) -> Result<Value, Error>,
) -> Result<Vec<String>, Error> {
    load(dir)?;
    let mut cases: Vec<String> = std::fs::read_dir(dir.join("fixtures"))
        .map_err(|e| Error(format!("{}: {e}", dir.join("fixtures").display())))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    cases.sort();
    let mut passed = Vec::new();
    for case in cases {
        let expected = dir.join("expected").join(format!("{case}.json"));
        if !expected.exists() {
            continue;
        }
        let output = run_fixture(dir, &case, run)?;
        if output != read_json(&expected)? {
            let actual = dir.join("expected").join(format!("{case}.actual.json"));
            let _ = std::fs::write(
                &actual,
                serde_json::to_string_pretty(&output).unwrap_or_default() + "\n",
            );
            return Err(Error(format!(
                "{case}: the output differs from {} (written to {})",
                expected.display(),
                actual.display()
            )));
        }
        passed.push(case);
    }
    if passed.is_empty() {
        return Err(Error("no fixture case has an expected output".into()));
    }
    Ok(passed)
}

/// Packs every widget folder in `widgets` (folders starting with `_` are templates) into
/// `out/manifests/` and `out/index.json`.
pub fn pack(widgets: &Path, out: &Path, sequence: u64, published: &str) -> Result<Index, Error> {
    let io = |e: std::io::Error| Error(e.to_string());
    std::fs::create_dir_all(out.join("manifests")).map_err(io)?;
    let mut folders: Vec<_> = std::fs::read_dir(widgets)
        .map_err(io)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| !n.starts_with('_') && !n.starts_with('.'))
                && path.join("manifest.json").exists()
        })
        .collect();
    folders.sort();
    let mut entries = Vec::new();
    for folder in folders {
        let (manifest, bytes) = load(&folder)?;
        let name = folder.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name != manifest.id {
            return Err(Error(format!(
                "{}: the folder must be named after the widget's id",
                folder.display()
            )));
        }
        let url = format!("manifests/{}-{}.json", manifest.id, manifest.version);
        std::fs::write(out.join(&url), &bytes).map_err(io)?;
        entries.push(catalog::entry(&manifest, &bytes, &url));
    }
    let index = Index {
        schema: 1,
        sequence,
        published: published.into(),
        widgets: entries,
    };
    let text = serde_json::to_vec_pretty(&index).map_err(|e| Error(e.to_string()))?;
    std::fs::write(out.join("index.json"), text).map_err(io)?;
    Ok(index)
}

/// Signs `out/index.json` into `out/index.json.sig`.
pub fn sign(out: &Path, keys: &KeyPair) -> Result<(), Error> {
    let index = std::fs::read(out.join("index.json")).map_err(|e| Error(e.to_string()))?;
    std::fs::write(out.join("index.json.sig"), keys.sign(&index) + "\n")
        .map_err(|e| Error(e.to_string()))
}

/// Checks a packed catalog the way Luma will: the signature, then every manifest.
pub fn verify(out: &Path, key: &[u8; 32]) -> Result<Index, Error> {
    let index = std::fs::read(out.join("index.json")).map_err(|e| Error(e.to_string()))?;
    let signature = read(&out.join("index.json.sig"))?;
    let index = catalog::verify(&index, &signature, &[*key], 0)?;
    for entry in &index.widgets {
        let bytes = std::fs::read(out.join(&entry.url)).map_err(|e| Error(e.to_string()))?;
        catalog::check_manifest(entry, &bytes)?;
    }
    Ok(index)
}
