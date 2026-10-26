//! Store widgets on the SDL thread (docs/widgets.md §7): adding and removing them, their
//! settings and secrets, when each block refreshes, and what the worker answers.
use crate::home_grid::{Block, BlockKind};
use crate::model::{InstalledWidget, WidgetSlot};
use crate::state::{AddChoice, Effect, State, Target};
use crate::widget_store::{CatalogState, SLOTS};
use crate::widget_worker::{CatalogError, RefreshJob, WidgetRequest, WidgetResponse};
use luma_widgets::catalog::Entry;
use luma_widgets::manifest::{Manifest, SettingKind, Version};
use luma_widgets::refresh::WidgetError;
use serde_json::{Map, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A day between catalog checks while a store widget is on Home.
const CATALOG_EVERY: Duration = Duration::from_secs(24 * 3600);

/// This Luma's version, for the catalog's `luma` requirements.
pub fn luma_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version(0, 0, 0))
}

fn backoff(failures: u32) -> u64 {
    (60u64 << failures.saturating_sub(1).min(4)).min(900)
}

impl State {
    /// The widget a store block shows, if it is installed.
    pub fn slot_manifest(&self, slot: u8) -> Option<Arc<Manifest>> {
        let id = &self.preferences.widgets.slots.get(&slot)?.widget;
        self.widgets.store.manifest(id)
    }

    /// The slot's settings, checked against its manifest.
    pub fn slot_settings(&self, slot: u8) -> Map<String, Value> {
        let saved = self
            .preferences
            .widgets
            .slots
            .get(&slot)
            .map(|s| s.settings.clone())
            .unwrap_or_default();
        self.slot_manifest(slot)
            .map(|m| m.settings_values(&saved))
            .unwrap_or_default()
    }

    /// Store widget blocks on Home, by slot.
    pub(crate) fn widget_slots_on_home(&self) -> Vec<u8> {
        self.home_blocks()
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::Widget(slot) => Some(slot),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn slots_of(&self, id: &str) -> Vec<u8> {
        self.preferences
            .widgets
            .slots
            .iter()
            .filter(|(_, s)| s.widget == id)
            .map(|(slot, _)| *slot)
            .collect()
    }

    /// What "Add widget" offers: the built-in kinds, installed store widgets with room for
    /// another block, then the store itself.
    pub fn add_choices(&self) -> Vec<AddChoice> {
        let mut choices: Vec<AddChoice> = self
            .missing_blocks()
            .into_iter()
            .map(AddChoice::Kind)
            .collect();
        for id in self.widgets.store.ids() {
            let Some(manifest) = self.widgets.store.manifest(&id) else {
                continue;
            };
            let shown = self
                .widget_slots_on_home()
                .into_iter()
                .filter(|slot| self.slot_manifest(*slot).is_some_and(|m| m.id == id))
                .count();
            if shown < manifest.instances as usize {
                choices.push(AddChoice::Widget(id));
            }
        }
        choices.push(AddChoice::Store);
        choices
    }

    /// A block's name in the current language.
    pub fn block_name(&self, kind: BlockKind) -> String {
        let language = self.preferences.language;
        match kind {
            BlockKind::Widget(slot) => self.slot_manifest(slot).map_or_else(
                || language.t("widgetStoreWidget").to_owned(),
                |m| m.name.get(language.code()).to_owned(),
            ),
            kind => language.t(kind.name_key()).to_owned(),
        }
    }

    /// A block's size limits: a store widget's come from its manifest.
    pub fn block_limits(&self, kind: BlockKind) -> ((u8, u8), (u8, u8)) {
        match kind {
            BlockKind::Widget(slot) => self.slot_manifest(slot).map_or(kind.limits(), |m| {
                (
                    (m.size.min[0], m.size.min[1]),
                    (m.size.max[0], m.size.max[1]),
                )
            }),
            kind => kind.limits(),
        }
    }

    /// Bumps a slot's generation and makes it due now; results for the old one are dropped.
    pub(crate) fn refresh_now(&mut self, slot: u8) {
        self.widgets.generations += 1;
        let generation = self.widgets.generations;
        let state = self.widgets.slots.entry(slot).or_default();
        state.generation = generation;
        state.in_flight = false;
        state.next_at = None;
        state.failures = 0;
        self.dirty = true;
    }

    /// Keeps an installed (checked) manifest and records it; blocks of it refresh.
    pub fn widget_installed(&mut self, manifest: Manifest, bytes: &[u8]) -> Result<(), String> {
        let id = manifest.id.clone();
        let version = manifest.version.to_string();
        self.widgets.store.install(manifest, bytes)?;
        self.preferences.widgets.installed.insert(
            id.clone(),
            InstalledWidget {
                version,
                sha256: luma_widgets::catalog::sha256(bytes),
            },
        );
        for slot in self.slots_of(&id) {
            self.refresh_now(slot);
        }
        Ok(())
    }

    /// Adds a block of an installed widget to Home, in a new slot, and focuses it.
    pub fn add_widget(&mut self, id: &str) -> Result<u8, String> {
        let manifest = self
            .widgets
            .store
            .manifest(id)
            .ok_or_else(|| format!("{id} is not installed"))?;
        if !self.add_choices().contains(&AddChoice::Widget(id.into())) {
            return Err(format!("{id} is on Home as often as it can be"));
        }
        let slot = (0..SLOTS)
            .find(|slot| !self.preferences.widgets.slots.contains_key(slot))
            .ok_or("no free widget slot")?;
        let mut blocks = self.home_blocks();
        let [w, h] = manifest.size.default;
        let (x, y) = crate::home_grid::first_free(&blocks, w, h);
        let kind = BlockKind::Widget(slot);
        blocks.push(Block { kind, x, y, w, h });
        self.preferences.home_layout = blocks;
        self.preferences.widgets.slots.insert(
            slot,
            WidgetSlot {
                widget: id.into(),
                settings: manifest.settings_values(&Map::new()),
            },
        );
        self.widgets.slots.remove(&slot);
        self.refresh_now(slot);
        self.focus = Target::Block(kind);
        Ok(slot)
    }

    /// Removes one store block from Home with its settings.
    pub fn remove_widget_block(&mut self, slot: u8) {
        self.preferences
            .home_layout
            .retain(|b| b.kind != BlockKind::Widget(slot));
        self.preferences.widgets.slots.remove(&slot);
        self.widgets.slots.remove(&slot);
        self.row_scroll.remove(&BlockKind::Widget(slot));
    }

    /// Uninstalls a widget: its blocks, their settings, its secrets and its manifest.
    pub fn remove_widget(&mut self, id: &str) -> Vec<Effect> {
        for slot in self.slots_of(id) {
            self.remove_widget_block(slot);
        }
        self.preferences.widgets.installed.remove(id);
        self.widgets.store.remove(id);
        self.widgets.queue.push(WidgetRequest::Forget(id.into()));
        if matches!(
            self.focus,
            Target::Block(BlockKind::Widget(_)) | Target::WidgetItem(..)
        ) {
            self.focus = self.first_content();
        }
        self.dirty = true;
        vec![Effect::Save]
    }

    /// Saves a setting of one block and refreshes it.
    pub fn set_widget_setting(&mut self, slot: u8, setting: &str, value: Value) -> Vec<Effect> {
        let Some(manifest) = self.slot_manifest(slot) else {
            return vec![];
        };
        let Some(saved) = self.preferences.widgets.slots.get_mut(&slot) else {
            return vec![];
        };
        let mut settings = saved.settings.clone();
        settings.insert(setting.into(), value);
        saved.settings = manifest.settings_values(&settings);
        self.refresh_now(slot);
        vec![Effect::Save]
    }

    /// Stores (or with `None` forgets) a widget's secret; every block of it refreshes.
    pub fn set_widget_secret(
        &mut self,
        slot: u8,
        setting: &str,
        value: Option<String>,
    ) -> Result<(), String> {
        let manifest = self.slot_manifest(slot).ok_or("not installed")?;
        let entry = manifest.setting(setting).ok_or("no such setting")?;
        if !matches!(entry.kind, SettingKind::Secret { .. }) {
            return Err("not a secret".into());
        }
        let value = value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        if let Some(value) = &value
            && !entry.accepts(value)
        {
            return Err("invalid".into());
        }
        self.widgets
            .store
            .set_secret(&manifest.id, setting, value)?;
        self.widgets
            .queue
            .push(WidgetRequest::Forget(manifest.id.clone()));
        for slot in self.slots_of(&manifest.id) {
            self.refresh_now(slot);
        }
        Ok(())
    }

    pub(crate) fn job(&self, slot: u8, manifest: Arc<Manifest>, generation: u64) -> RefreshJob {
        RefreshJob {
            slot,
            generation,
            settings: self.slot_settings(slot),
            secrets: self.widgets.store.secrets(&manifest.id),
            manifest,
            lang: self.preferences.language.code().into(),
            now: time::OffsetDateTime::now_utc().unix_timestamp(),
        }
    }

    /// Requests for the worker: queued ones (catalog, installs, forgetting caches), the daily
    /// catalog check, and a refresh for every store block on a visible Home that is due.
    pub fn widget_requests(&mut self, now: Instant, home_visible: bool) -> Vec<WidgetRequest> {
        let mut requests = std::mem::take(&mut self.widgets.queue);
        if !home_visible {
            return requests;
        }
        let slots = self.widget_slots_on_home();
        if !slots.is_empty()
            && self
                .widgets
                .catalog_at
                .is_none_or(|at| now.saturating_duration_since(at) >= CATALOG_EVERY)
        {
            self.widgets.catalog_at = Some(now);
            requests.push(WidgetRequest::Catalog {
                last_sequence: self.preferences.widgets.catalog_sequence,
            });
        }
        let lang = self.preferences.language.code();
        for slot in slots {
            let Some(manifest) = self.slot_manifest(slot) else {
                continue;
            };
            if !self.widgets.slots.contains_key(&slot) {
                self.refresh_now(slot);
            }
            let state = &self.widgets.slots[&slot];
            if state.in_flight {
                continue;
            }
            let rerun = state.document.is_some() && state.lang != lang && state.input.is_some();
            if !rerun && state.next_at.is_some_and(|at| now < at) {
                continue;
            }
            let generation = state.generation;
            let input = state.input.clone();
            let job = self.job(slot, manifest, generation);
            let state = self.widgets.slots.entry(slot).or_default();
            state.in_flight = true;
            state.asked_at = Some(now);
            requests.push(match input {
                Some(input) if rerun => WidgetRequest::Rerun(job, input),
                _ => WidgetRequest::Refresh(job),
            });
        }
        requests
    }

    /// What the worker answered.
    pub fn widget_response(&mut self, response: WidgetResponse) -> Vec<Effect> {
        self.dirty = true;
        match response {
            WidgetResponse::Document {
                slot,
                generation,
                result,
            } => {
                let lang = self.preferences.language.code().to_owned();
                let Some(state) = self.widgets.slots.get_mut(&slot) else {
                    return vec![];
                };
                if state.generation != generation {
                    return vec![];
                }
                state.in_flight = false;
                let asked = state.asked_at.unwrap_or_else(Instant::now);
                match result {
                    Ok(refreshed) => {
                        state.next_at = Some(asked + Duration::from_secs(refreshed.refresh));
                        state.document = Some(Arc::new(refreshed.document));
                        state.input = Some(refreshed.input);
                        state.source = refreshed.source;
                        state.lang = lang;
                        state.error = None;
                        state.failures = 0;
                        self.feeds.images_due = true;
                        self.follow_widget_items(slot);
                    }
                    Err(error) => {
                        state.failures += 1;
                        let wait = match &error {
                            WidgetError::Busy => backoff(state.failures).max(300),
                            // Nothing changes until the user does; a new secret refreshes.
                            WidgetError::NoSource { .. } | WidgetError::KeyRefused { .. } => 3600,
                            _ => backoff(state.failures),
                        };
                        state.next_at = Some(asked + Duration::from_secs(wait));
                        state.error = Some(error);
                    }
                }
                vec![]
            }
            WidgetResponse::Catalog(Ok(index)) => {
                let changed = index.sequence != self.preferences.widgets.catalog_sequence;
                self.preferences.widgets.catalog_sequence = self
                    .preferences
                    .widgets
                    .catalog_sequence
                    .max(index.sequence);
                for entry in &index.widgets {
                    if self.update_installs_itself(entry) {
                        self.widgets.installing.insert(entry.id.clone());
                        self.widgets
                            .queue
                            .push(WidgetRequest::Install(entry.clone()));
                    }
                }
                self.widgets.catalog = CatalogState::Ready(index);
                if changed { vec![Effect::Save] } else { vec![] }
            }
            WidgetResponse::Catalog(Err(CatalogError::Unverified(why))) => {
                eprintln!("Luma widget catalog not verified: {why}");
                self.widgets.catalog = CatalogState::Unverified;
                vec![]
            }
            WidgetResponse::Catalog(Err(CatalogError::Unavailable(why))) => {
                eprintln!("Luma widget catalog unavailable: {why}");
                // An earlier catalog stays usable.
                if !matches!(self.widgets.catalog, CatalogState::Ready(_)) {
                    self.widgets.catalog = CatalogState::Unavailable;
                }
                vec![]
            }
            WidgetResponse::Installed(entry, result) => {
                self.widgets.installing.remove(&entry.id);
                let language = self.preferences.language;
                match result.and_then(|installed| {
                    let (manifest, bytes) = *installed;
                    self.widget_installed(manifest, &bytes)
                }) {
                    Ok(()) => {
                        if self.widgets.pending_add.as_deref() == Some(entry.id.as_str()) {
                            self.widgets.pending_add = None;
                            if self.add_widget(&entry.id).is_ok() {
                                self.overlay = None;
                                self.panel_started = None;
                            }
                        }
                        vec![Effect::Save]
                    }
                    Err(error) => {
                        eprintln!("Luma widget {}: {error}", entry.id);
                        if self.widgets.pending_add.as_deref() == Some(entry.id.as_str()) {
                            self.widgets.pending_add = None;
                        }
                        self.notify(language.format(
                            "widgetInstallFailed",
                            &[("name", entry.name.get(language.code()))],
                        ));
                        vec![]
                    }
                }
            }
        }
    }

    /// The installed version of a widget, if any.
    pub(crate) fn installed_version(&self, id: &str) -> Option<Version> {
        Version::parse(&self.preferences.widgets.installed.get(id)?.version)
    }

    /// A newer version this Luma can run that reaches no further than the installed one.
    pub(crate) fn update_installs_itself(&self, entry: &Entry) -> bool {
        let Some(installed) = self.widgets.store.manifest(&entry.id) else {
            return false;
        };
        self.installed_version(&entry.id)
            .is_some_and(|version| entry.version > version)
            && entry.luma <= luma_version()
            && !installed.access().widens(&entry.access())
            && !self.widgets.installing.contains(&entry.id)
    }

    /// Whether an update of an installed widget waits for the user to accept new access.
    pub fn widget_needs_consent(&self, id: &str) -> bool {
        let (Some(entry), Some(installed)) = (
            self.store_entries().into_iter().find(|e| e.id == id),
            self.widgets.store.manifest(id),
        ) else {
            return false;
        };
        self.installed_version(id)
            .is_some_and(|version| entry.version > version)
            && installed.access().widens(&entry.access())
    }

    /// The catalog's widgets as the store lists them; nothing unless it verified.
    pub fn store_entries(&self) -> Vec<&Entry> {
        match &self.widgets.catalog {
            CatalogState::Ready(index) => index.widgets.iter().collect(),
            _ => vec![],
        }
    }

    /// Asks for the catalog now (the store opened).
    pub fn request_catalog(&mut self) {
        if !matches!(self.widgets.catalog, CatalogState::Ready(_)) {
            self.widgets.catalog = CatalogState::Loading;
        }
        self.widgets.catalog_at = Some(Instant::now());
        self.widgets.queue.push(WidgetRequest::Catalog {
            last_sequence: self.preferences.widgets.catalog_sequence,
        });
    }

    /// Installs a catalog entry (and adds it to Home when `add` is set).
    pub fn install_widget(&mut self, entry: Entry, add: bool) {
        if add {
            self.widgets.pending_add = Some(entry.id.clone());
        }
        if self.widgets.installing.insert(entry.id.clone()) {
            self.widgets.queue.push(WidgetRequest::Install(entry));
        }
        self.dirty = true;
    }

    /// Images store widgets on Home show (and the open dialog's), with whether each is a
    /// poster.
    pub fn wanted_widget_images(&self) -> Vec<(String, bool)> {
        let mut urls: Vec<(String, bool)> = Vec::new();
        let mut add = |url: &Option<String>, poster: bool| {
            if let Some(url) = url
                && !urls.iter().any(|(u, _)| u == url)
            {
                urls.push((url.clone(), poster));
            }
        };
        for slot in self.widget_slots_on_home() {
            let Some(document) = self
                .widgets
                .slots
                .get(&slot)
                .and_then(|s| s.document.clone())
            else {
                continue;
            };
            for item in &document.items {
                match &item.card {
                    luma_widgets::document::Card::Scoreboard(card) => {
                        add(&card.logo, false);
                        for side in card.sides.iter().flatten() {
                            add(&side.image, false);
                        }
                    }
                    luma_widgets::document::Card::Media(card) => add(&card.image, true),
                    luma_widgets::document::Card::Ticker(_) => {}
                }
            }
        }
        if let Some(crate::state::Overlay::WidgetDialog { slot, id, .. }) = &self.overlay
            && let Some(document) = self
                .widgets
                .slots
                .get(slot)
                .and_then(|s| s.document.clone())
            && let Some(dialog) = document
                .items
                .iter()
                .find(|item| &item.id == id)
                .and_then(|item| item.dialog.as_ref())
        {
            if let Some(header) = &dialog.header {
                add(&header.logo, false);
            }
            if let Some(hero) = &dialog.hero {
                for side in hero.sides.iter().flatten() {
                    add(&side.image, false);
                }
            }
            for tile in dialog.tiles.iter().flat_map(|t| &t.items) {
                if let Some(result) = &tile.result {
                    add(&result.image, false);
                }
            }
            for list in &dialog.lists {
                add(&list.image, false);
                for row in &list.rows {
                    add(&row.image, false);
                }
            }
        }
        urls
    }
}

/// A row of a store widget's settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetRow {
    /// A toggle, choice, checklist or text setting, by its index in the manifest.
    Setting(usize),
    /// A secret: type it, send it from a phone, forget it.
    SecretType(usize),
    SecretPhone(usize),
    SecretForget(usize),
}

/// A button on a store widget's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreAction {
    /// Install it and add a block to Home.
    Install,
    /// Add another block of an installed widget.
    Add,
    /// Install the newer version (accepting its new access).
    Update,
    Remove,
}

impl StoreAction {
    pub fn key(self) -> &'static str {
        match self {
            Self::Install => "widgetInstall",
            Self::Add => "widgetAdd",
            Self::Update => "widgetUpdate",
            Self::Remove => "widgetRemove",
        }
    }
}

impl State {
    /// The rows of a store widget's settings page, in order.
    pub fn widget_setting_rows(&self, slot: u8) -> Vec<WidgetRow> {
        let Some(manifest) = self.slot_manifest(slot) else {
            return vec![];
        };
        let mut rows = Vec::new();
        for (index, setting) in manifest.settings.iter().enumerate() {
            if matches!(setting.kind, SettingKind::Secret { .. }) {
                rows.push(WidgetRow::SecretType(index));
                rows.push(WidgetRow::SecretPhone(index));
                if self
                    .widgets
                    .store
                    .secret(&manifest.id, &setting.id)
                    .is_some()
                {
                    rows.push(WidgetRow::SecretForget(index));
                }
            } else {
                rows.push(WidgetRow::Setting(index));
            }
        }
        rows
    }

    /// The row of a setting (its first, for a secret).
    pub fn widget_row_of(&self, slot: u8, setting: usize) -> usize {
        self.widget_setting_rows(slot)
            .iter()
            .position(|row| {
                matches!(row, WidgetRow::Setting(i) | WidgetRow::SecretType(i) if *i == setting)
            })
            .unwrap_or(0)
    }

    pub fn open_widget_settings(&mut self, slot: u8) {
        self.section = 0;
        self.subpage = Some(crate::state::Subpage::Widget(slot));
        self.focus = Target::WidgetSetting(0);
        self.scroll = 0.;
        if !self.preferences.reduced_motion {
            self.panel_started = Some(Instant::now());
        }
        self.dirty = true;
    }

    /// The options of a choice or checklist setting: label and whether it is chosen.
    pub fn widget_options(&self, slot: u8, setting: usize) -> Vec<(String, bool)> {
        let Some(manifest) = self.slot_manifest(slot) else {
            return vec![];
        };
        let Some(entry) = manifest.settings.get(setting) else {
            return vec![];
        };
        let value = self.slot_settings(slot).get(&entry.id).cloned();
        let language = self.preferences.language.code();
        let options = match &entry.kind {
            SettingKind::Choice { options, .. } | SettingKind::Choices { options, .. } => options,
            _ => return vec![],
        };
        options
            .iter()
            .map(|option| {
                let chosen = match &value {
                    Some(Value::String(v)) => *v == option.value,
                    Some(Value::Array(values)) => values.iter().any(|v| v == &option.value),
                    _ => false,
                };
                (option.label.get(language).to_owned(), chosen)
            })
            .collect()
    }

    pub(crate) fn choose_widget_option(
        &mut self,
        slot: u8,
        setting: usize,
        index: usize,
    ) -> Vec<Effect> {
        let Some(manifest) = self.slot_manifest(slot) else {
            return vec![];
        };
        let Some(entry) = manifest.settings.get(setting) else {
            return vec![];
        };
        match &entry.kind {
            SettingKind::Choice { options, .. } => {
                let Some(option) = options.get(index) else {
                    return vec![];
                };
                let effects =
                    self.set_widget_setting(slot, &entry.id, Value::from(option.value.clone()));
                self.close_subpage();
                effects
            }
            SettingKind::Choices {
                options, min, max, ..
            } => {
                let Some(option) = options.get(index) else {
                    return vec![];
                };
                let mut chosen: Vec<String> = self
                    .slot_settings(slot)
                    .get(&entry.id)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                if let Some(at) = chosen.iter().position(|v| *v == option.value) {
                    if chosen.len() <= *min {
                        return vec![];
                    }
                    chosen.remove(at);
                } else {
                    if chosen.len() >= *max {
                        return vec![];
                    }
                    chosen.push(option.value.clone());
                }
                self.set_widget_setting(slot, &entry.id, Value::from(chosen))
            }
            _ => vec![],
        }
    }

    /// The setting the keyboard or phone page fills, and its row.
    pub(crate) fn editing_setting(&self) -> Option<(u8, luma_widgets::manifest::Setting)> {
        let (slot, id) = self.widget_editing.as_ref()?;
        Some((*slot, self.slot_manifest(*slot)?.setting(id)?.clone()))
    }

    pub fn widget_editing_target(&self) -> Target {
        let Some((slot, setting)) = self.editing_setting() else {
            return Target::WidgetSetting(0);
        };
        let index = self
            .slot_manifest(slot)
            .and_then(|m| m.settings.iter().position(|s| s.id == setting.id))
            .unwrap_or(0);
        Target::WidgetSetting(self.widget_row_of(slot, index))
    }

    pub fn widget_text_limit(&self) -> usize {
        match self.editing_setting().map(|(_, s)| s.kind) {
            Some(SettingKind::Text { max_length, .. } | SettingKind::Secret { max_length, .. }) => {
                max_length
            }
            _ => 64,
        }
    }

    /// The keyboard closed on a store widget's text or secret: an empty field keeps what was
    /// saved.
    pub fn commit_widget_text(&mut self) -> Vec<Effect> {
        let text = std::mem::take(&mut self.widget_text_draft);
        let text = text.trim().to_owned();
        let Some((slot, setting)) = self.editing_setting() else {
            return vec![];
        };
        self.widget_editing = None;
        if text.is_empty() {
            return vec![];
        }
        match setting.kind {
            SettingKind::Secret { .. } => {
                if self
                    .set_widget_secret(slot, &setting.id, Some(text))
                    .is_err()
                {
                    let language = self.preferences.language;
                    self.notify(language.format(
                        "widgetSecretInvalid",
                        &[("secret", setting.label.get(language.code()))],
                    ));
                }
                vec![]
            }
            SettingKind::Text { .. } if setting.accepts(&text) => {
                self.set_widget_setting(slot, &setting.id, Value::from(text))
            }
            _ => vec![],
        }
    }

    pub(crate) fn activate_widget_setting(&mut self, row: usize) -> Vec<Effect> {
        let Some(crate::state::Subpage::Widget(slot)) = self.subpage else {
            return vec![];
        };
        let Some(manifest) = self.slot_manifest(slot) else {
            return vec![];
        };
        let rows = self.widget_setting_rows(slot);
        let Some(row_kind) = rows.get(row).copied() else {
            return vec![];
        };
        match row_kind {
            WidgetRow::Setting(index) => {
                let setting = &manifest.settings[index];
                match &setting.kind {
                    SettingKind::Toggle { .. } => {
                        let on = self
                            .slot_settings(slot)
                            .get(&setting.id)
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        self.set_widget_setting(slot, &setting.id, Value::Bool(!on))
                    }
                    SettingKind::Choice { .. } | SettingKind::Choices { .. } => {
                        let first = self
                            .widget_options(slot, index)
                            .iter()
                            .position(|(_, chosen)| *chosen)
                            .unwrap_or(0);
                        self.subpage = Some(crate::state::Subpage::WidgetOptions(slot, index));
                        self.focus = Target::Choice(first);
                        self.scroll = 0.;
                        vec![]
                    }
                    SettingKind::Text { .. } => {
                        self.widget_editing = Some((slot, setting.id.clone()));
                        self.open_keyboard(crate::state::TextField::Widget);
                        vec![]
                    }
                    SettingKind::Secret { .. } => vec![],
                }
            }
            WidgetRow::SecretType(index) => {
                self.widget_editing = Some((slot, manifest.settings[index].id.clone()));
                self.open_keyboard(crate::state::TextField::Widget);
                vec![]
            }
            WidgetRow::SecretPhone(index) => {
                self.widget_editing = Some((slot, manifest.settings[index].id.clone()));
                self.overlay = Some(crate::state::Overlay::PhoneKey {
                    restore: Target::WidgetSetting(row),
                });
                self.focus = Target::CloseDialog;
                self.phone_key = None;
                if !self.preferences.reduced_motion {
                    self.panel_started = Some(Instant::now());
                }
                vec![Effect::PhoneKeyStart]
            }
            WidgetRow::SecretForget(index) => {
                let _ = self.set_widget_secret(slot, &manifest.settings[index].id, None);
                self.focus = Target::WidgetSetting(self.widget_row_of(slot, index));
                vec![]
            }
        }
    }

    /// A key from the phone page for the store widget setting it was opened for.
    pub fn widget_secret_from_phone(&mut self, key: String) -> bool {
        let Some((slot, setting)) = self.editing_setting() else {
            return false;
        };
        self.widget_editing = None;
        self.set_widget_secret(slot, &setting.id, Some(key)).is_ok()
    }

    pub(crate) fn open_widget_dialog(&mut self, slot: u8, index: usize) -> Vec<Effect> {
        let Some(item) = self
            .widgets
            .slot(slot)
            .and_then(|s| s.document.as_ref())
            .and_then(|d| d.items.get(index))
            .filter(|item| item.dialog.is_some())
        else {
            return vec![];
        };
        self.overlay = Some(crate::state::Overlay::WidgetDialog {
            slot,
            id: item.id.clone(),
            restore: Target::WidgetItem(slot, index),
        });
        self.focus = Target::CloseDialog;
        self.feeds.images_due = true;
        if !self.preferences.reduced_motion {
            self.panel_started = Some(Instant::now());
        }
        vec![]
    }

    /// Opens the store's list and asks for the catalog.
    pub fn open_store(&mut self, restore: Target) {
        self.overlay = Some(crate::state::Overlay::Store { restore });
        self.focus = Target::StoreEntry(0);
        self.request_catalog();
        if !self.preferences.reduced_motion {
            self.panel_started = Some(Instant::now());
        }
        self.dirty = true;
    }

    pub(crate) fn back_to_store(&mut self, id: &str, restore: Target) {
        let index = self
            .store_entries()
            .iter()
            .position(|e| e.id == id)
            .unwrap_or(0);
        self.overlay = Some(crate::state::Overlay::Store { restore });
        self.focus = Target::StoreEntry(index);
    }

    pub(crate) fn open_store_entry(&mut self, index: usize) -> Vec<Effect> {
        let Some(crate::state::Overlay::Store { restore }) = self.overlay.clone() else {
            return vec![];
        };
        let Some(entry) = self.store_entries().get(index).map(|e| e.id.clone()) else {
            return vec![];
        };
        let actions = self.store_actions(&entry);
        self.overlay = Some(crate::state::Overlay::StoreEntry { id: entry, restore });
        self.focus = if actions.is_empty() {
            Target::CloseDialog
        } else {
            Target::StoreAction(0)
        };
        vec![]
    }

    /// What a widget's page offers.
    pub fn store_actions(&self, id: &str) -> Vec<StoreAction> {
        let Some(entry) = self.store_entries().into_iter().find(|e| e.id == id) else {
            return if self.widgets.store.manifest(id).is_some() {
                vec![StoreAction::Remove]
            } else {
                vec![]
            };
        };
        if self.widgets.installing.contains(id) {
            return vec![];
        }
        let runs = entry.luma <= luma_version();
        match self.installed_version(id) {
            None if runs => vec![StoreAction::Install],
            None => vec![],
            Some(version) => {
                let mut actions = Vec::new();
                if self.add_choices().contains(&AddChoice::Widget(id.into())) {
                    actions.push(StoreAction::Add);
                }
                if runs && entry.version > version {
                    actions.push(StoreAction::Update);
                }
                actions.push(StoreAction::Remove);
                actions
            }
        }
    }

    pub(crate) fn store_action(&mut self, index: usize) -> Vec<Effect> {
        let Some(crate::state::Overlay::StoreEntry { id, restore }) = self.overlay.clone() else {
            return vec![];
        };
        let Some(action) = self.store_actions(&id).get(index).copied() else {
            return vec![];
        };
        let entry = self
            .store_entries()
            .into_iter()
            .find(|e| e.id == id)
            .cloned();
        match action {
            StoreAction::Install | StoreAction::Update => {
                if let Some(entry) = entry {
                    self.install_widget(entry, action == StoreAction::Install);
                }
                self.focus = Target::CloseDialog;
                vec![]
            }
            StoreAction::Add => {
                self.overlay = None;
                self.panel_started = None;
                match self.add_widget(&id) {
                    Ok(_) => vec![Effect::Save],
                    Err(_) => {
                        self.focus = restore;
                        vec![]
                    }
                }
            }
            StoreAction::Remove => {
                let effects = self.remove_widget(&id);
                self.back_to_store(&id, restore);
                effects
            }
        }
    }

    /// After a new document: an open dialog follows its item and closes when it is gone; focus
    /// never points past the row's end.
    pub fn follow_widget_items(&mut self, slot: u8) {
        let items: Vec<String> = self
            .widgets
            .slot(slot)
            .and_then(|s| s.document.as_ref())
            .map(|d| d.items.iter().map(|i| i.id.clone()).collect())
            .unwrap_or_default();
        if let Some(crate::state::Overlay::WidgetDialog { slot: open, id, .. }) = &self.overlay
            && *open == slot
        {
            match items.iter().position(|item| item == id) {
                Some(index) => {
                    if let Some(crate::state::Overlay::WidgetDialog { restore, .. }) =
                        &mut self.overlay
                    {
                        *restore = Target::WidgetItem(slot, index);
                    }
                }
                None => {
                    self.overlay = None;
                    self.panel_started = None;
                    self.focus = Target::Block(BlockKind::Widget(slot));
                }
            }
        }
        if let Target::WidgetItem(focused, index) = self.focus
            && focused == slot
            && index >= items.len()
        {
            self.focus = if items.is_empty() {
                Target::Block(BlockKind::Widget(slot))
            } else {
                Target::WidgetItem(slot, items.len() - 1)
            };
        }
    }
}

impl State {
    /// The label of the store widget setting being edited, for the keyboard's title.
    pub fn widget_editing_label(&self) -> String {
        self.editing_setting()
            .map(|(_, s)| s.label.get(self.preferences.language.code()).to_owned())
            .unwrap_or_default()
    }

    /// The secret the phone page is open for, with its label and hint in the TV's language.
    pub fn phone_widget_secret(
        &self,
    ) -> Option<(String, Option<String>, luma_widgets::manifest::Setting)> {
        let (_, setting) = self.editing_setting()?;
        let code = self.preferences.language.code();
        let hint = match &setting.kind {
            SettingKind::Secret { hint, .. } => hint.as_ref().map(|h| h.get(code).to_owned()),
            _ => None,
        };
        Some((setting.label.get(code).to_owned(), hint, setting))
    }
}

impl State {
    /// A key sent from the phone page: saved as the store widget's secret it was opened for,
    /// and the dialog closes.
    pub fn phone_key_received(&mut self, key: String) -> Vec<Effect> {
        self.phone_key = None;
        if let Some(crate::state::Overlay::PhoneKey { restore }) = self.overlay.take() {
            self.focus = restore;
            self.panel_started = None;
        }
        let language = self.preferences.language;
        let label = self.widget_editing_label();
        if self.widget_secret_from_phone(key) {
            self.notify(language.format("widgetSecretSaved", &[("secret", &label)]));
        }
        vec![]
    }
}
