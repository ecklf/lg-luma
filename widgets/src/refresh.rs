//! One refresh of a widget instance (docs/widgets.md §3.3–3.5): pick the source, run its
//! request chain (fan-out, vars, cache), then its transform, and parse the document.
use crate::Error;
use crate::document::Document;
use crate::fetch::{Fetch, FetchError};
use crate::jq::Vars;
use crate::manifest::{Manifest, Source, Step, When};
use crate::plan::{self, Placeholders};
use crate::sandbox::Job;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// Requests one refresh may make: the sum of its steps' fan-out.
pub const REQUESTS: usize = 32;
/// Replies handed to one transform.
pub const INPUT_LIMIT: usize = 4 * 1024 * 1024;
const VAR_LIMIT: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WidgetError {
    /// No source applies: the one that would needs this secret.
    NoSource {
        secret: String,
    },
    /// The API refused this secret.
    KeyRefused {
        secret: String,
    },
    Busy,
    Unavailable(String),
    /// The transform failed, ran out of limits or gave an unusable document.
    Broken(String),
}

impl std::fmt::Display for WidgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoSource { secret } => write!(f, "{secret} is needed"),
            Self::KeyRefused { secret } => write!(f, "{secret} was refused"),
            Self::Busy => f.write_str("too many requests"),
            Self::Unavailable(why) => write!(f, "unavailable: {why}"),
            Self::Broken(why) => write!(f, "broken: {why}"),
        }
    }
}

/// Replies by URL and headers, shared by every instance of a widget for their `max_age`.
#[derive(Default)]
pub struct Cache {
    entries: HashMap<u64, (Instant, Value)>,
}

impl Cache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn key(url: &str, headers: &[(String, String)]) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        url.hash(&mut hasher);
        headers.hash(&mut hasher);
        hasher.finish()
    }

    fn get(&self, key: u64, max_age: u64) -> Option<Value> {
        let (at, value) = self.entries.get(&key)?;
        (max_age > 0 && at.elapsed() < Duration::from_secs(max_age)).then(|| value.clone())
    }

    fn put(&mut self, key: u64, value: Value) {
        // Old replies go once the cache grows; the longest max_age is a day.
        if self.entries.len() > 256 {
            self.entries
                .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(86_400));
        }
        self.entries.insert(key, (Instant::now(), value));
    }
}

pub struct Context<'a> {
    pub manifest: &'a Manifest,
    /// The instance's settings, already checked with `Manifest::settings_values`.
    pub settings: &'a Map<String, Value>,
    pub secrets: &'a BTreeMap<String, String>,
    pub lang: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct Refreshed {
    pub document: Document,
    pub source: String,
    /// Seconds until the next refresh.
    pub refresh: u64,
    /// The replies, kept to rerun the transform (e.g. in another language) without the network.
    pub input: Value,
}

/// A source's program: the widget's library in front of it.
pub fn program(library: &str, transform: &str) -> String {
    format!("{library}\n{transform}")
}

/// One program computing a step's vars for every item: `$item` is the list of items, and the
/// result has an object of the vars per item (`null` where a var gave nothing).
pub fn vars_program(library: &str, step: &Step) -> String {
    let fields: Vec<String> = step
        .vars
        .iter()
        .map(|(name, expression)| format!("{name:?}: ([{expression}][0])"))
        .collect();
    program(
        library,
        &format!(
            ". as $__replies | [$item[] as $item | $__replies | {{{}}}]",
            fields.join(", ")
        ),
    )
}

fn secret_set(cx: &Context, id: &str) -> bool {
    cx.secrets.get(id).is_some_and(|s| !s.trim().is_empty())
}

/// The source that applies: the first whose `when` holds.
pub fn source<'a>(cx: &Context<'a>) -> Result<&'a Source, WidgetError> {
    cx.manifest
        .sources
        .iter()
        .find(|source| match &source.when {
            When::Always => true,
            When::Secret(id) => secret_set(cx, id),
            When::NoSecret(id) => !secret_set(cx, id),
        })
        .ok_or_else(|| WidgetError::NoSource {
            secret: cx
                .manifest
                .sources
                .iter()
                .find_map(|s| match &s.when {
                    When::Secret(id) => Some(id.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
        })
}

fn vars(cx: &Context, source: &Source, item: Option<Value>) -> Vars {
    let mut settings = cx.settings.clone();
    for setting in &cx.manifest.settings {
        if matches!(setting.kind, crate::manifest::SettingKind::Secret { .. }) {
            settings.insert(setting.id.clone(), Value::Bool(secret_set(cx, &setting.id)));
        }
    }
    Vars {
        settings: Value::Object(settings),
        t: Value::Object(
            cx.manifest
                .strings
                .iter()
                .map(|(id, text)| (id.clone(), Value::from(text.get(cx.lang))))
                .collect(),
        ),
        lang: cx.lang.into(),
        now: cx.now,
        source: source.id.clone(),
        item,
    }
}

fn items(cx: &Context, step: &Step) -> Vec<Option<Value>> {
    match &step.for_each {
        Some(setting) => cx
            .settings
            .get(setting)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Some)
            .collect(),
        None => vec![None],
    }
}

/// The transform on these replies, in this context.
pub fn transform(
    cx: &Context,
    source: &Source,
    input: Value,
    run: &mut dyn FnMut(Job) -> Result<Value, Error>,
) -> Result<(Document, u64), WidgetError> {
    let size = input.to_string().len();
    if size > INPUT_LIMIT {
        return Err(WidgetError::Broken("the replies are over 4 MiB".into()));
    }
    let output = run(Job {
        program: program(&cx.manifest.library, &source.transform),
        input,
        vars: vars(cx, source, None),
    })
    .map_err(|e| WidgetError::Broken(e.0))?;
    let document = Document::parse(&output, cx.manifest.card, &cx.manifest.origins)
        .map_err(|e| WidgetError::Broken(e.0))?;
    let minimum = source.refresh.min.max(30);
    let refresh = document
        .refresh
        .unwrap_or(source.refresh.every)
        .clamp(minimum, 3600);
    Ok((document, refresh))
}

/// Runs the source that applies: its requests, then its transform.
pub fn run(
    cx: &Context,
    fetch: &dyn Fetch,
    cache: &mut Cache,
    run: &mut dyn FnMut(Job) -> Result<Value, Error>,
) -> Result<Refreshed, WidgetError> {
    let source = source(cx)?;
    let planned: usize = source.requests.iter().map(|s| items(cx, s).len()).sum();
    if planned > REQUESTS {
        return Err(WidgetError::Unavailable("too many requests".into()));
    }
    let no_secrets = BTreeMap::new();
    let mut replies = Map::new();
    for step in &source.requests {
        let items = items(cx, step);
        // Each item's vars, from one run of the step's vars program.
        let vars: Vec<Option<BTreeMap<String, String>>> = if step.vars.is_empty() {
            items.iter().map(|_| Some(BTreeMap::new())).collect()
        } else {
            let list = Value::Array(
                items
                    .iter()
                    .map(|i| i.clone().unwrap_or(Value::Null))
                    .collect(),
            );
            let output = run(Job {
                program: vars_program(&cx.manifest.library, step),
                input: Value::Object(replies.clone()),
                vars: vars(cx, source, Some(list)),
            })
            .map_err(|e| WidgetError::Broken(format!("{}: {}", step.id, e.0)))?;
            let rows = output.as_array().cloned().unwrap_or_default();
            items
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    let row = rows.get(index)?.as_object()?;
                    step.vars
                        .keys()
                        .map(|name| {
                            let text = match row.get(name)? {
                                Value::String(text) => text.clone(),
                                Value::Number(number) => number.to_string(),
                                _ => return None,
                            };
                            (text.len() <= VAR_LIMIT).then(|| (name.clone(), text))
                        })
                        .collect()
                })
                .collect()
        };
        let secret = plan::sends_secret(step);
        let mut by_item = Map::new();
        let mut single = Value::Null;
        for (item, vars) in items.iter().zip(vars) {
            let reply = match vars {
                None => Value::Null,
                Some(vars) => {
                    let values = Placeholders {
                        item: item.as_ref(),
                        settings: cx.settings,
                        vars: &vars,
                        secrets: if secret.is_some() {
                            cx.secrets
                        } else {
                            &no_secrets
                        },
                    };
                    let url = plan::url(&step.url, &values, &cx.manifest.origins)
                        .map_err(|e| WidgetError::Broken(e.0))?;
                    let headers = plan::headers(cx.manifest, step, &url, &values)
                        .map_err(|e| WidgetError::Broken(e.0))?;
                    let key = Cache::key(&url, &headers);
                    let max_age = step.max_age.unwrap_or(source.refresh.every);
                    match cache.get(key, max_age) {
                        Some(value) => value,
                        None => match fetch.get(&url, &headers, step.limit) {
                            Ok(value) => {
                                cache.put(key, value.clone());
                                value
                            }
                            Err(FetchError::Refused) if secret.is_some() => {
                                return Err(WidgetError::KeyRefused {
                                    secret: secret.unwrap_or_default(),
                                });
                            }
                            Err(_) if !step.required => Value::Null,
                            Err(FetchError::Busy) => return Err(WidgetError::Busy),
                            Err(error) => {
                                return Err(WidgetError::Unavailable(format!(
                                    "{}: {error}",
                                    step.id
                                )));
                            }
                        },
                    }
                }
            };
            match item {
                Some(item) => {
                    by_item.insert(plan::value_text(item), reply);
                }
                None => single = reply,
            }
        }
        replies.insert(
            step.id.clone(),
            if step.for_each.is_some() {
                Value::Object(by_item)
            } else {
                single
            },
        );
    }
    let input = Value::Object(replies);
    let (document, refresh) = transform(cx, source, input.clone(), run)?;
    Ok(Refreshed {
        document,
        source: source.id.clone(),
        refresh,
        input,
    })
}
