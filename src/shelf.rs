//! Home Top Shelf dwell/reveal and TMDB request scheduling.
use crate::state::{Effect, Page, State, Target};
use crate::tmdb::{self, Catalog, Command, Response, REVEAL_MS, ROTATE_MS};
use std::time::Instant;

impl State {
    pub fn shelf_focus_app(&self) -> Option<&crate::model::App> {
        let id = match &self.focus {
            Target::DockApp(id) | Target::App(id) => id.as_str(),
            _ => return None,
        };
        self.inventory.apps.iter().find(|app| app.id == id)
    }

    pub fn shelf_tick(&mut self) -> Vec<Effect> {
        if self.page != Page::Home
            || !self.preferences.show_shelf
            || self.overlay.is_some()
            || self.moving.is_some()
            || self.drawer_visibility > 0.01
        {
            self.shelf_hide(false);
            return vec![];
        }
        let Some(app) = self.shelf_focus_app().cloned() else {
            self.shelf_hide(false);
            return vec![];
        };
        let Some(catalog) = tmdb::catalog_for(&app) else {
            self.shelf_hide(false);
            return vec![];
        };
        let key = tmdb::sanitize_key(&self.preferences.tmdb_api_key);
        if key.len() < 16 {
            self.shelf_hide(false);
            return vec![];
        }
        if self.shelf.focus_app.as_deref() != Some(app.id.as_str()) {
            self.shelf.focus_app = Some(app.id.clone());
            self.shelf.focus_since = Some(Instant::now());
            self.shelf.revealed = false;
            self.shelf.selected = 0;
            self.shelf.rotate_since = None;
            self.shelf.visibility = if self.preferences.reduced_motion {
                0.
            } else {
                self.shelf.visibility
            };
            self.dirty = true;
        }
        let dwell = self
            .shelf
            .focus_since
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);
        let ready = self
            .shelf
            .shows
            .get(&app.id)
            .is_some_and(|shows| !shows.is_empty());
        let mut effects = vec![];
        if !ready
            && self.shelf.pending.as_deref() != Some(app.id.as_str())
            && (dwell >= REVEAL_MS / 2 || self.preferences.reduced_motion)
        {
            effects.extend(self.shelf_request(&app.id, catalog, key));
        }
        let want_reveal = ready && (dwell >= REVEAL_MS || self.preferences.reduced_motion);
        if want_reveal && !self.shelf.revealed {
            self.shelf.revealed = true;
            self.shelf.rotate_since = Some(Instant::now());
            self.dirty = true;
        }
        if self.shelf.revealed
            && let Some(shows) = self.shelf.shows.get(&app.id)
            && shows.len() > 1
            && let Some(started) = self.shelf.rotate_since
            && started.elapsed().as_millis() as u64 >= ROTATE_MS
        {
            self.shelf.selected = (self.shelf.selected + 1) % shows.len();
            self.shelf.rotate_since = Some(Instant::now());
            self.dirty = true;
        }
        let goal = if self.shelf.revealed && ready { 1. } else { 0. };
        if self.preferences.reduced_motion {
            if self.shelf.visibility != goal {
                self.shelf.visibility = goal;
                self.dirty = true;
            }
        } else if (self.shelf.visibility - goal).abs() > 0.001 {
            let step = 0.08;
            self.shelf.visibility = if goal > self.shelf.visibility {
                (self.shelf.visibility + step).min(goal)
            } else {
                (self.shelf.visibility - step).max(goal)
            };
            self.dirty = true;
        }
        effects
    }

    fn shelf_hide(&mut self, clear_focus: bool) {
        if clear_focus {
            self.shelf.focus_app = None;
            self.shelf.focus_since = None;
        }
        if self.shelf.revealed || self.shelf.visibility > 0. {
            self.shelf.revealed = false;
            if self.preferences.reduced_motion {
                self.shelf.visibility = 0.;
            }
            self.dirty = true;
        }
    }

    fn shelf_request(&mut self, app_id: &str, catalog: Catalog, key: String) -> Vec<Effect> {
        self.shelf.pending = Some(app_id.into());
        self.shelf.generation = self.shelf.generation.wrapping_add(1);
        self.shelf.error = None;
        let region = tmdb::resolve_region(&self.preferences.tmdb_region, self.preferences.language);
        let language =
            tmdb::resolve_language(&self.preferences.tmdb_language, self.preferences.language);
        vec![Effect::Shelf(Command::Refresh {
            app_id: app_id.into(),
            catalog,
            key,
            region,
            language,
        })]
    }

    pub fn shelf_response(&mut self, response: Response) {
        if response.generation != self.shelf.generation
            && self.shelf.pending.as_deref() != Some(response.app_id.as_str())
        {
            // Still accept if it matches the pending app.
        }
        if self.shelf.pending.as_deref() == Some(response.app_id.as_str()) {
            self.shelf.pending = None;
        }
        match response.result {
            Ok(shows) => {
                self.shelf.error = None;
                self.shelf.shows.insert(response.app_id, shows);
                self.shelf.selected = 0;
                self.shelf.rotate_since = Some(Instant::now());
                self.dirty = true;
            }
            Err(error) => {
                eprintln!("Luma TMDB: {error}");
                self.shelf.error = Some(error);
                self.dirty = true;
            }
        }
    }

    pub fn shelf_current_show(&self) -> Option<&tmdb::Show> {
        let id = self.shelf.focus_app.as_ref()?;
        let shows = self.shelf.shows.get(id)?;
        shows.get(self.shelf.selected.min(shows.len().saturating_sub(1)))
    }

    pub fn shelf_fallback_colors(app: &crate::model::App) -> (u32, u32) {
        match app.brand.as_str() {
            "netflix" => (0x141414, 0x5b0f0f),
            "prime-video" => (0x0f171e, 0x1a9fff),
            "disney-plus" => (0x0b1c3f, 0x1a3a6b),
            _ => (0x1a1d24, 0x0b0e15),
        }
    }
}
