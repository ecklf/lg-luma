//! The store's worker thread (docs/widgets.md §7.2): the signed catalog, installs, and every
//! refresh of a store widget: its requests over HTTPS and its transform in a limited child
//! process (Luma itself, started with `--widget-transform`). Nothing here runs on the SDL thread.
use luma_widgets::catalog::{self, Entry, Index};
use luma_widgets::fetch::{FetchError, Http};
use luma_widgets::manifest::Manifest;
use luma_widgets::refresh::{self, Cache, Context, Refreshed, WidgetError};
use luma_widgets::sandbox::{self, Job, Limits};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

pub const TRANSFORM_FLAG: &str = "--widget-transform";
const INDEX_LIMIT: usize = 256 * 1024;

/// One refresh of one block, with everything the worker needs.
#[derive(Clone, Debug, PartialEq)]
pub struct RefreshJob {
    pub slot: u8,
    pub generation: u64,
    pub manifest: Arc<Manifest>,
    pub settings: Map<String, Value>,
    pub secrets: BTreeMap<String, String>,
    pub lang: String,
    pub now: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WidgetRequest {
    Catalog {
        last_sequence: u64,
    },
    Install(Entry),
    Refresh(RefreshJob),
    /// The transform again on the last replies, e.g. in another language.
    Rerun(RefreshJob, Value),
    /// Drop a widget's cached replies (its secret changed, or it was removed).
    Forget(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum CatalogError {
    Unavailable(String),
    Unverified(String),
}

#[derive(Debug)]
pub enum WidgetResponse {
    Catalog(Result<Index, CatalogError>),
    Installed(Box<Entry>, Result<Box<(Manifest, Vec<u8>)>, String>),
    Document {
        slot: u8,
        generation: u64,
        result: Result<Refreshed, WidgetError>,
    },
}

/// Where the catalog comes from: its site, or a folder for desktop development.
#[derive(Clone, Debug)]
pub enum CatalogSource {
    Url(String),
    Dir(PathBuf),
}

pub struct Config {
    pub catalog: CatalogSource,
    pub keys: Vec<[u8; 32]>,
    /// The executable that runs transforms: Luma itself.
    pub exe: PathBuf,
    /// Desktop development: a fixture folder's replies instead of the network.
    pub fixtures: Option<PathBuf>,
}

pub struct Worker {
    sender: mpsc::Sender<WidgetRequest>,
    pub receiver: mpsc::Receiver<WidgetResponse>,
}

impl Worker {
    pub fn new(config: Config) -> Self {
        let (sender, requests) = mpsc::channel();
        let (responses, receiver) = mpsc::channel();
        thread::spawn(move || {
            let http = Http::new();
            let mut caches: HashMap<String, Cache> = HashMap::new();
            while let Ok(request) = requests.recv() {
                let response = match request {
                    WidgetRequest::Catalog { last_sequence } => Some(WidgetResponse::Catalog(
                        fetch_catalog(&http, &config, last_sequence),
                    )),
                    WidgetRequest::Install(entry) => {
                        let result = fetch_manifest(&http, &config, &entry).map(Box::new);
                        Some(WidgetResponse::Installed(Box::new(entry), result))
                    }
                    WidgetRequest::Refresh(job) => {
                        let cache = caches.entry(job.manifest.id.clone()).or_default();
                        let fixtures = config
                            .fixtures
                            .as_ref()
                            .and_then(|dir| luma_widgets::tree::Fixtures::load(dir).ok());
                        Some(run(&job, &config, |cx, transform| match &fixtures {
                            Some(fixtures) => refresh::run(cx, fixtures, cache, transform),
                            None => refresh::run(cx, &http, cache, transform),
                        }))
                    }
                    WidgetRequest::Rerun(job, input) => {
                        Some(run(&job, &config, |cx, transform| {
                            let source = refresh::source(cx)?;
                            let (document, refresh) =
                                refresh::transform(cx, source, input.clone(), transform)?;
                            Ok(Refreshed {
                                document,
                                source: source.id.clone(),
                                refresh,
                                input,
                            })
                        }))
                    }
                    WidgetRequest::Forget(id) => {
                        caches.remove(&id);
                        None
                    }
                };
                if let Some(response) = response
                    && responses.send(response).is_err()
                {
                    break;
                }
            }
        });
        Self { sender, receiver }
    }

    pub fn send(&self, request: WidgetRequest) {
        let _ = self.sender.send(request);
    }
}

fn run(
    job: &RefreshJob,
    config: &Config,
    refresh: impl FnOnce(
        &Context,
        &mut dyn FnMut(Job) -> Result<Value, luma_widgets::Error>,
    ) -> Result<Refreshed, WidgetError>,
) -> WidgetResponse {
    let cx = Context {
        manifest: &job.manifest,
        settings: &job.settings,
        secrets: &job.secrets,
        lang: &job.lang,
        now: job.now,
    };
    let mut transform =
        |job: Job| sandbox::run(&config.exe, TRANSFORM_FLAG, &job, &Limits::default());
    let result = refresh(&cx, &mut transform);
    if let Err(error) = &result {
        eprintln!("Luma widget {} ({}): {error}", job.manifest.id, job.slot);
    }
    WidgetResponse::Document {
        slot: job.slot,
        generation: job.generation,
        result,
    }
}

fn read(http: &Http, config: &Config, path: &str, limit: usize) -> Result<Vec<u8>, String> {
    match &config.catalog {
        CatalogSource::Url(base) => http
            .bytes(&format!("{base}{path}"), &[], limit)
            .map_err(|e| match e {
                FetchError::Unavailable(why) => why,
                other => format!("{path}: {other}"),
            }),
        CatalogSource::Dir(dir) => {
            let bytes = std::fs::read(dir.join(path)).map_err(|e| format!("{path}: {e}"))?;
            if bytes.len() > limit {
                return Err(format!("{path}: too large"));
            }
            Ok(bytes)
        }
    }
}

fn fetch_catalog(http: &Http, config: &Config, last_sequence: u64) -> Result<Index, CatalogError> {
    let index = read(http, config, "index.json", INDEX_LIMIT).map_err(CatalogError::Unavailable)?;
    let signature =
        read(http, config, "index.json.sig", 1024).map_err(CatalogError::Unavailable)?;
    let signature = String::from_utf8_lossy(&signature);
    catalog::verify(&index, &signature, &config.keys, last_sequence).map_err(|e| {
        eprintln!("Luma widget catalog: {e}");
        CatalogError::Unverified(e.0)
    })
}

fn fetch_manifest(
    http: &Http,
    config: &Config,
    entry: &Entry,
) -> Result<(Manifest, Vec<u8>), String> {
    let bytes = read(http, config, &entry.url, entry.size)?;
    let manifest = catalog::check_manifest(entry, &bytes).map_err(|e| e.0)?;
    Ok((manifest, bytes))
}
