//! Widgets from the store on this TV (docs/widgets.md §6–7): installed manifests in `widgets/`
//! next to the preferences, secrets in their own private file, and what each Home block of a
//! store widget shows.
use crate::model::InstalledWidget;
use luma_widgets::catalog::Index;
use luma_widgets::document::Document;
use luma_widgets::manifest::Manifest;
use luma_widgets::refresh::WidgetError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// The catalog Luma installs from, and the keys its index must be signed with.
pub const CATALOG_URL: &str = "https://eluz1ve.github.io/luma-widgets/";
pub const CATALOG_KEYS: [&str; 1] = ["WcsqVU58IK6AWfHrxsE+zx50QJbkN0GbKDFXVIUpOrw="];
/// Store widget blocks on Home, and widgets installed.
pub const SLOTS: u8 = 16;
pub const INSTALLED: usize = 16;

/// Installed manifests and secrets, kept in the preferences' folder (or only in memory).
#[derive(Default)]
pub struct Store {
    /// The folder holding `preferences.json`.
    dir: Option<PathBuf>,
    manifests: HashMap<String, Arc<Manifest>>,
    secrets: BTreeMap<String, BTreeMap<String, String>>,
    /// Desktop development: secrets from the environment, used but never saved.
    runtime: BTreeMap<String, BTreeMap<String, String>>,
    /// Installed widgets that could not be loaded, and why.
    pub broken: BTreeMap<String, String>,
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension("tmp");
    let _ = std::fs::remove_file(&temporary);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)
}

impl Store {
    /// The store next to `preferences`, with the installed widgets loaded and checked again.
    pub fn open(preferences: &Path, installed: &BTreeMap<String, InstalledWidget>) -> Self {
        let dir = preferences
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_owned);
        let mut store = Self {
            dir: Some(dir),
            ..Self::default()
        };
        for (id, record) in installed {
            match store.load(id, record) {
                Ok(manifest) => {
                    store.manifests.insert(id.clone(), Arc::new(manifest));
                }
                Err(error) => {
                    eprintln!("Luma widget {id}: {error}");
                    store.broken.insert(id.clone(), error);
                }
            }
        }
        if let Some(path) = store.secrets_path()
            && let Ok(bytes) = std::fs::read(&path)
        {
            store.secrets = serde_json::from_slice(&bytes).unwrap_or_default();
        }
        store
    }

    fn manifest_path(&self, id: &str) -> Option<PathBuf> {
        Some(
            self.dir
                .as_ref()?
                .join("widgets")
                .join(format!("{id}.json")),
        )
    }

    fn secrets_path(&self) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join("secrets.json"))
    }

    fn load(&self, id: &str, record: &InstalledWidget) -> Result<Manifest, String> {
        let path = self.manifest_path(id).ok_or("no folder")?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if luma_widgets::catalog::sha256(&bytes) != record.sha256 {
            return Err("its manifest changed on disk".into());
        }
        let manifest = Manifest::parse(&bytes).map_err(|e| e.0)?;
        luma_widgets::catalog::check_programs(&manifest).map_err(|e| e.0)?;
        if manifest.id != id {
            return Err("its manifest names another widget".into());
        }
        Ok(manifest)
    }

    pub fn manifest(&self, id: &str) -> Option<Arc<Manifest>> {
        self.manifests.get(id).cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.manifests.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Keeps a checked manifest, written to `widgets/<id>.json` when the store has a folder.
    pub fn install(&mut self, manifest: Manifest, bytes: &[u8]) -> Result<Arc<Manifest>, String> {
        if !self.manifests.contains_key(&manifest.id) && self.manifests.len() >= INSTALLED {
            return Err("too many widgets installed".into());
        }
        if let Some(path) = self.manifest_path(&manifest.id) {
            let folder = path.parent().ok_or("no folder")?;
            std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
            let temporary = path.with_extension("tmp");
            std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
            std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        }
        self.broken.remove(&manifest.id);
        let manifest = Arc::new(manifest);
        self.manifests
            .insert(manifest.id.clone(), Arc::clone(&manifest));
        Ok(manifest)
    }

    /// Forgets a widget: its manifest file and its secrets.
    pub fn remove(&mut self, id: &str) {
        self.manifests.remove(id);
        self.broken.remove(id);
        if let Some(path) = self.manifest_path(id) {
            let _ = std::fs::remove_file(path);
        }
        if self.secrets.remove(id).is_some() {
            let _ = self.save_secrets();
        }
    }

    pub fn secret(&self, widget: &str, setting: &str) -> Option<&str> {
        [&self.runtime, &self.secrets]
            .into_iter()
            .find_map(|secrets| secrets.get(widget)?.get(setting))
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    pub fn secrets(&self, widget: &str) -> BTreeMap<String, String> {
        let mut secrets = self.secrets.get(widget).cloned().unwrap_or_default();
        secrets.extend(self.runtime.get(widget).cloned().unwrap_or_default());
        secrets
    }

    /// A secret for this run only (desktop development), never written anywhere.
    pub fn set_runtime_secret(&mut self, widget: &str, setting: &str, value: String) {
        self.runtime
            .entry(widget.into())
            .or_default()
            .insert(setting.into(), value);
    }

    pub fn set_secret(
        &mut self,
        widget: &str,
        setting: &str,
        value: Option<String>,
    ) -> Result<(), String> {
        let secrets = self.secrets.entry(widget.into()).or_default();
        match value {
            Some(value) => {
                secrets.insert(setting.into(), value);
            }
            None => {
                secrets.remove(setting);
            }
        }
        if secrets.is_empty() {
            self.secrets.remove(widget);
        }
        self.save_secrets()
    }

    fn save_secrets(&self) -> Result<(), String> {
        let Some(path) = self.secrets_path() else {
            return Ok(());
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec(&self.secrets).map_err(|e| e.to_string())?;
        write_private(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// The environment variable a desktop run reads a widget's secret from, e.g.
/// `LUMA_WIDGET_SECRET_ESPORTS_KEY`.
pub fn runtime_secret_variable(widget: &str, setting: &str) -> String {
    format!("LUMA_WIDGET_SECRET_{widget}_{setting}")
        .to_uppercase()
        .replace('-', "_")
}

/// The catalog as the store last saw it.
#[derive(Clone, Debug, Default)]
pub enum CatalogState {
    #[default]
    Idle,
    Loading,
    Ready(Index),
    Unavailable,
    /// The signature, the sequence or a manifest did not check out: nothing is offered.
    Unverified,
}

/// What one store widget block shows and when it refreshes.
#[derive(Clone, Debug, Default)]
pub struct SlotState {
    pub document: Option<Arc<Document>>,
    pub error: Option<WidgetError>,
    /// Results for an older generation (before a settings, secret or language change) are
    /// dropped.
    pub generation: u64,
    pub in_flight: bool,
    /// When the request in flight (or the last one) was sent.
    pub asked_at: Option<Instant>,
    pub next_at: Option<Instant>,
    pub failures: u32,
    /// The last replies, to rerun the transform in another language without the network.
    pub input: Option<Value>,
    pub source: String,
    /// The language of the document shown.
    pub lang: String,
}

/// The store's state on the SDL thread.
#[derive(Default)]
pub struct WidgetState {
    pub store: Store,
    pub catalog: CatalogState,
    /// When the catalog was last asked for.
    pub catalog_at: Option<Instant>,
    pub slots: HashMap<u8, SlotState>,
    /// Requests for the worker that the next `State::widget_requests` hands out.
    pub queue: Vec<crate::widget_worker::WidgetRequest>,
    /// Hands out generations, unique across slots.
    pub generations: u64,
    /// A widget to add to Home once its install finishes.
    pub pending_add: Option<String>,
    pub installing: BTreeSet<String>,
}

impl WidgetState {
    pub fn slot(&self, slot: u8) -> Option<&SlotState> {
        self.slots.get(&slot)
    }
}
