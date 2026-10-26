//! Filling a step's URL and headers (docs/widgets.md §3.4). Values are percent-encoded into
//! URLs so they cannot change a URL's host, path or parameters, and the result is checked
//! against the widget's origins again; secrets go only into headers to their `send_to`.
use crate::Error;
use crate::manifest::{Manifest, Origin, SettingKind, Step, placeholders};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// What a step's placeholders can name.
pub struct Placeholders<'a> {
    pub item: Option<&'a Value>,
    pub settings: &'a Map<String, Value>,
    pub vars: &'a BTreeMap<String, String>,
    pub secrets: &'a BTreeMap<String, String>,
}

/// A setting's or item's value as text: a list joined by commas, a toggle as true or false.
pub fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(values) => values.iter().map(value_text).collect::<Vec<_>>().join(","),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// RFC 3986 percent-encoding of everything but unreserved characters.
pub fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn fill(
    template: &str,
    values: &Placeholders,
    secret_allowed: impl Fn(&str) -> bool,
    escape: impl Fn(&str) -> String,
) -> Result<String, Error> {
    let mut out = String::new();
    let mut rest = template;
    for name in placeholders(template)? {
        let start = rest.find('{').unwrap_or(0);
        out.push_str(&rest[..start]);
        rest = &rest[start + name.len() + 2..];
        let value = if name == "item" {
            values.item.map(value_text)
        } else if let Some(var) = name.strip_prefix("var.") {
            values.vars.get(var).cloned()
        } else if let Some(setting) = name.strip_prefix("settings.") {
            values.settings.get(setting).map(value_text)
        } else if let Some(secret) = name.strip_prefix("secret.") {
            if !secret_allowed(secret) {
                return Err(Error(format!("{{{name}}} may not be sent here")));
            }
            values.secrets.get(secret).cloned()
        } else {
            None
        };
        let value = value.ok_or_else(|| Error(format!("{{{name}}} has no value")))?;
        out.push_str(&escape(&value));
    }
    out.push_str(rest);
    Ok(out)
}

/// The step's URL with its placeholders filled in, on one of `origins`.
pub fn url(template: &str, values: &Placeholders, origins: &[Origin]) -> Result<String, Error> {
    let url = fill(template, values, |_| false, encode)?;
    match Origin::of(&url) {
        Some(origin) if origins.contains(&origin) => Ok(url),
        _ => Err(Error(format!("{url} is not on the widget's origins"))),
    }
}

/// The step's headers for `url`; a secret only goes to an origin in its `send_to`.
pub fn headers(
    manifest: &Manifest,
    step: &Step,
    url: &str,
    values: &Placeholders,
) -> Result<Vec<(String, String)>, Error> {
    let origin = Origin::of(url);
    let allowed = |secret: &str| {
        manifest.settings.iter().any(|s| {
            s.id == secret
                && matches!(&s.kind, SettingKind::Secret { send_to, .. }
                    if origin.as_ref().is_some_and(|o| send_to.contains(o)))
        })
    };
    step.headers
        .iter()
        .map(|(name, template)| {
            let value = fill(template, values, allowed, str::to_owned)?;
            if value.chars().any(char::is_control) {
                return Err(Error(format!("header {name} has a control character")));
            }
            Ok((name.clone(), value))
        })
        .collect()
}

/// Whether any of the step's headers carries a secret.
pub fn sends_secret(step: &Step) -> Option<String> {
    step.headers.values().find_map(|template| {
        placeholders(template)
            .ok()?
            .into_iter()
            .find_map(|name| name.strip_prefix("secret.").map(str::to_owned))
    })
}
