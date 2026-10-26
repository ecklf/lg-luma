//! The signed catalog (docs/widgets.md §6): `index.json` signed with ed25519 by the catalog's
//! publish workflow, each manifest pinned by size and SHA-256, and a sequence that never goes
//! back.
use crate::Error;
use crate::manifest::{Access, Localized, MANIFEST_LIMIT, Manifest, Origin, SettingKind, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SecretEntry {
    pub label: Localized,
    pub send_to: Vec<Origin>,
}

/// A widget as the store lists it: enough to show it and ask for consent without its manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub version: Version,
    pub luma: Version,
    pub name: Localized,
    pub description: Localized,
    pub icon: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub origins: Vec<Origin>,
    #[serde(default)]
    pub secrets: Vec<SecretEntry>,
    pub url: String,
    pub size: usize,
    pub sha256: String,
}

impl Entry {
    pub fn access(&self) -> Access {
        Access {
            origins: self.origins.iter().map(|o| o.as_str().to_owned()).collect(),
            secret_destinations: self
                .secrets
                .iter()
                .flat_map(|s| &s.send_to)
                .map(|o| o.as_str().to_owned())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub schema: u32,
    pub sequence: u64,
    pub published: String,
    pub widgets: Vec<Entry>,
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The catalog's signing key.
pub struct KeyPair(ed25519_compact::KeyPair);

impl KeyPair {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self(ed25519_compact::KeyPair::from_seed(
            ed25519_compact::Seed::new(seed),
        ))
    }

    /// A new key from the system's random source.
    pub fn generate() -> Result<Self, Error> {
        use std::io::Read;
        let mut seed = [0; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut seed))
            .map_err(|e| Error(format!("no random source: {e}")))?;
        Ok(Self::from_seed(seed))
    }

    /// The secret half as base64 text (its seed), for the publish workflow's secret.
    pub fn secret_text(&self) -> String {
        BASE64.encode(self.0.sk.seed().as_ref())
    }

    pub fn from_secret_text(text: &str) -> Result<Self, Error> {
        let seed: [u8; 32] = BASE64
            .decode(text.trim())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| Error("the signing key must be 32 bytes of base64".into()))?;
        Ok(Self::from_seed(seed))
    }

    pub fn public(&self) -> [u8; 32] {
        *self.0.pk
    }

    /// The signature of `message`, as base64.
    pub fn sign(&self, message: &[u8]) -> String {
        BASE64.encode(self.0.sk.sign(message, None).as_ref())
    }
}

/// A public key written as base64, as compiled into Luma.
pub fn public_key(text: &str) -> Result<[u8; 32], Error> {
    BASE64
        .decode(text.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| Error("a public key is 32 bytes of base64".into()))
}

pub fn public_text(key: &[u8; 32]) -> String {
    BASE64.encode(key)
}

/// The index, if one of `keys` signed exactly these bytes and it is not older than the newest
/// this TV has accepted.
pub fn verify(
    index: &[u8],
    signature: &str,
    keys: &[[u8; 32]],
    last_sequence: u64,
) -> Result<Index, Error> {
    if index.len() > MANIFEST_LIMIT {
        return Err(Error("the catalog index is over 256 KiB".into()));
    }
    let signature = BASE64
        .decode(signature.trim())
        .ok()
        .and_then(|bytes| ed25519_compact::Signature::from_slice(&bytes).ok())
        .ok_or_else(|| Error("the catalog's signature is malformed".into()))?;
    let signed = keys.iter().any(|key| {
        ed25519_compact::PublicKey::new(*key)
            .verify(index, &signature)
            .is_ok()
    });
    if !signed {
        return Err(Error("the catalog's signature does not verify".into()));
    }
    let index: Index =
        serde_json::from_slice(index).map_err(|e| Error(format!("catalog index: {e}")))?;
    if index.schema != 1 {
        return Err(Error(format!(
            "catalog schema {} is not supported",
            index.schema
        )));
    }
    if index.sequence < last_sequence {
        return Err(Error(format!(
            "the catalog is older ({}) than one already seen ({last_sequence})",
            index.sequence
        )));
    }
    let mut ids = BTreeSet::new();
    for entry in &index.widgets {
        let url_ok = !entry.url.is_empty()
            && entry.url.len() <= 200
            && !entry.url.starts_with('/')
            && !entry.url.contains("..")
            && entry.url.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'-' | b'_' | b'.' | b'/')
            });
        let hash_ok = entry.sha256.len() == 64
            && entry
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !url_ok || !hash_ok || entry.size > MANIFEST_LIMIT || !ids.insert(&entry.id) {
            return Err(Error(format!("catalog entry {} is malformed", entry.id)));
        }
    }
    Ok(index)
}

/// The library and every program of a manifest compile against the restricted jq library.
pub fn check_programs(manifest: &Manifest) -> Result<(), Error> {
    for source in &manifest.sources {
        crate::jq::check(&crate::refresh::program(
            &manifest.library,
            &source.transform,
        ))
        .map_err(|e| Error(format!("source {}: {e}", source.id)))?;
        for step in &source.requests {
            if !step.vars.is_empty() {
                crate::jq::check(&crate::refresh::vars_program(&manifest.library, step))
                    .map_err(|e| Error(format!("{}.{} vars: {e}", source.id, step.id)))?;
            }
        }
    }
    Ok(())
}

/// The manifest an entry names, if these are exactly its bytes and it asks for exactly the
/// access the entry showed.
pub fn check_manifest(entry: &Entry, bytes: &[u8]) -> Result<Manifest, Error> {
    if bytes.len() != entry.size || sha256(bytes) != entry.sha256 {
        return Err(Error(format!(
            "{}'s manifest does not match the catalog",
            entry.id
        )));
    }
    let manifest = Manifest::parse(bytes)?;
    if manifest.id != entry.id || manifest.version != entry.version || manifest.luma != entry.luma {
        return Err(Error(format!(
            "{}'s manifest names another widget or version",
            entry.id
        )));
    }
    if manifest.access() != entry.access() {
        return Err(Error(format!(
            "{}'s manifest asks for other access than the catalog shows",
            entry.id
        )));
    }
    check_programs(&manifest)?;
    Ok(manifest)
}

/// The index entry for a packed manifest.
pub fn entry(manifest: &Manifest, bytes: &[u8], url: &str) -> Entry {
    Entry {
        id: manifest.id.clone(),
        version: manifest.version,
        luma: manifest.luma,
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        icon: manifest.icon.clone(),
        authors: manifest.authors.clone(),
        origins: manifest.origins.clone(),
        secrets: manifest
            .settings
            .iter()
            .filter_map(|s| match &s.kind {
                SettingKind::Secret { send_to, .. } => Some(SecretEntry {
                    label: s.label.clone(),
                    send_to: send_to.clone(),
                }),
                _ => None,
            })
            .collect(),
        url: url.into(),
        size: bytes.len(),
        sha256: sha256(bytes),
    }
}
