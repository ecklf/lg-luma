//! Home Top Shelf dwell/reveal and TMDB request scheduling.
use crate::state::{Effect, Page, State, Target};
use crate::tmdb::{self, Catalog, Command, REVEAL_MS, ROTATE_MS, Response};
use std::path::Path;
use std::time::Instant;

fn path_exists(path: &str) -> bool {
    Path::new(path).is_file()
}

impl State {
    pub fn shelf_focus_app(&self) -> Option<&crate::model::App> {
        let id = match &self.focus {
            Target::DockApp(id) | Target::App(id) => id.as_str(),
            _ => return None,
        };
        self.inventory.apps.iter().find(|app| app.id == id)
    }

    fn shelf_app_ready(&self, app_id: &str) -> bool {
        self.shelf.shows.get(app_id).is_some_and(|shows| {
            shows.iter().any(|show| {
                show.backdrop_path
                    .as_ref()
                    .is_some_and(|path| path_exists(path))
            })
        })
    }

    pub fn shelf_tick(&mut self) -> Vec<Effect> {
        if self.page != Page::Home
            || !self.preferences.show_shelf
            || self.overlay.is_some()
            || self.moving.is_some()
            || self.drawer_visibility > 0.01
        {
            return self.shelf_hide_tick();
        }
        let Some(app) = self.shelf_focus_app().cloned() else {
            return self.shelf_hide_tick();
        };
        let Some(catalog) = tmdb::catalog_for(&app) else {
            return self.shelf_hide_tick();
        };
        let key = tmdb::effective_api_key(&self.preferences.tmdb_api_key);
        if key.len() < 16 {
            return self.shelf_hide_tick();
        }
        let ready = self.shelf_app_ready(&app.id);
        if self.shelf.focus_app.as_deref() != Some(app.id.as_str()) {
            self.shelf.focus_app = Some(app.id.clone());
            self.shelf.focus_since = Some(Instant::now());
            self.shelf.selected = 0;
            self.shelf.rotate_since = None;
            if ready {
                // Cached art: swap immediately — no dwell dip back to the wallpaper.
                self.shelf.revealed = true;
                self.shelf.rotate_since = Some(Instant::now());
                if self.preferences.reduced_motion {
                    self.shelf.visibility = 1.;
                }
            } else {
                self.shelf.revealed = false;
                if self.preferences.reduced_motion {
                    self.shelf.visibility = 0.;
                }
            }
            self.dirty = true;
        }
        let dwell = self
            .shelf
            .focus_since
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);
        let mut effects = vec![];
        // Dwell only gates the first fetch for a cold app. Once art is ready, reveal
        // without waiting for the full reveal timer (and without fading to wallpaper).
        if !ready
            && self.shelf.pending.as_deref() != Some(app.id.as_str())
            && (dwell >= REVEAL_MS / 2 || self.preferences.reduced_motion)
        {
            effects.extend(self.shelf_request(&app.id, catalog, key));
        }
        if ready && !self.shelf.revealed {
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
        self.shelf_step_visibility(goal);
        effects
    }

    /// Fade the backdrop out when focus leaves a mapped app. Keep `focus_app` until
    /// visibility hits 0 so the last show can still paint during the fade.
    fn shelf_hide_tick(&mut self) -> Vec<Effect> {
        if self.shelf.revealed {
            self.shelf.revealed = false;
            self.dirty = true;
        }
        self.shelf_step_visibility(0.);
        if self.shelf.visibility <= 0.001 {
            self.shelf.visibility = 0.;
            if self.shelf.focus_app.is_some() || self.shelf.focus_since.is_some() {
                self.shelf.focus_app = None;
                self.shelf.focus_since = None;
                self.dirty = true;
            }
        }
        vec![]
    }

    fn shelf_step_visibility(&mut self, goal: f32) {
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
        } else {
            self.shelf.visibility = goal;
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
        let with_art: Vec<_> = shows
            .iter()
            .enumerate()
            .filter(|(_, show)| {
                show.backdrop_path
                    .as_ref()
                    .is_some_and(|path| path_exists(path))
            })
            .collect();
        if with_art.is_empty() {
            return None;
        }
        let index = self.shelf.selected % with_art.len();
        Some(with_art[index].1)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Preferences, demo_inventory};
    use crate::tmdb::Show;
    use std::time::{Duration, Instant};

    fn ready() -> State {
        let mut state = State::new(Preferences::default());
        state.preferences.tmdb_api_key = "0123456789abcdef".into();
        state.preferences.show_shelf = true;
        state.set_inventory(demo_inventory(), None);
        state.page = Page::Home;
        state
    }

    fn show_with_art(id: &str) -> Show {
        let path = std::env::temp_dir().join(format!("luma-shelf-test-{id}.jpg"));
        let _ = std::fs::write(&path, b"jpeg");
        Show {
            id: 1,
            name: "Silo".into(),
            overview: String::new(),
            backdrop: "/a.jpg".into(),
            backdrop_path: Some(path.to_string_lossy().into_owned()),
        }
    }

    #[test]
    fn leaving_a_shelf_app_fades_backdrop_out_to_the_wallpaper() {
        let mut state = ready();
        let netflix = state
            .inventory
            .apps
            .iter()
            .find(|app| tmdb::catalog_for(app).is_some())
            .expect("demo inventory has a mapped streaming app")
            .clone();
        let plain = state
            .inventory
            .apps
            .iter()
            .find(|app| tmdb::catalog_for(app).is_none())
            .expect("demo inventory has an unmapped app")
            .clone();

        state.focus = Target::App(netflix.id.clone());
        state.shelf.focus_app = Some(netflix.id.clone());
        state.shelf.focus_since = Some(Instant::now() - Duration::from_secs(10));
        state.shelf.revealed = true;
        state.shelf.visibility = 1.;
        state
            .shelf
            .shows
            .insert(netflix.id.clone(), vec![show_with_art("netflix")]);

        state.focus = Target::App(plain.id.clone());
        let before = state.shelf.visibility;
        state.shelf_tick();
        assert!(!state.shelf.revealed);
        assert!(
            state.shelf.visibility < before,
            "visibility should start fading toward the wallpaper"
        );
        assert!(
            state.shelf.focus_app.as_deref() == Some(netflix.id.as_str()),
            "keep the last show while fading out"
        );

        for _ in 0..40 {
            state.shelf_tick();
        }
        assert_eq!(state.shelf.visibility, 0.);
        assert!(state.shelf.focus_app.is_none());
        assert!(state.shelf_current_show().is_none());
        let _ = plain;
    }

    #[test]
    fn dwelling_on_a_mapped_app_keeps_focus_since_so_requests_can_fire() {
        let mut state = ready();
        let netflix = state
            .inventory
            .apps
            .iter()
            .find(|app| tmdb::catalog_for(app).is_some())
            .expect("demo inventory has a mapped streaming app")
            .clone();
        state.focus = Target::App(netflix.id.clone());
        state.shelf_tick();
        let started = state.shelf.focus_since.expect("dwell clock starts");
        for _ in 0..5 {
            state.shelf_tick();
        }
        assert_eq!(state.shelf.focus_app.as_deref(), Some(netflix.id.as_str()));
        assert_eq!(
            state.shelf.focus_since,
            Some(started),
            "dwell must not reset focus_since every tick at visibility 0"
        );
        state.shelf.focus_since = Some(Instant::now() - Duration::from_millis(REVEAL_MS));
        let effects = state.shelf_tick();
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::Shelf(_))),
            "after the dwell threshold a fetch must be scheduled: {effects:?}"
        );
    }

    #[test]
    fn cached_apps_reveal_immediately_without_wallpaper_dip() {
        let mut state = ready();
        let mapped: Vec<_> = state
            .inventory
            .apps
            .iter()
            .filter(|app| tmdb::catalog_for(app).is_some())
            .cloned()
            .collect();
        assert!(mapped.len() >= 2, "need two mapped apps");
        let a = &mapped[0];
        let b = &mapped[1];
        state
            .shelf
            .shows
            .insert(a.id.clone(), vec![show_with_art("a")]);
        state
            .shelf
            .shows
            .insert(b.id.clone(), vec![show_with_art("b")]);

        state.focus = Target::App(a.id.clone());
        state.shelf_tick();
        assert!(state.shelf.revealed);
        assert_eq!(state.shelf.focus_app.as_deref(), Some(a.id.as_str()));
        state.shelf.visibility = 1.;

        state.focus = Target::App(b.id.clone());
        state.shelf_tick();
        assert!(
            state.shelf.revealed,
            "cached target must reveal without waiting for dwell"
        );
        assert_eq!(state.shelf.focus_app.as_deref(), Some(b.id.as_str()));
        assert!(
            (state.shelf.visibility - 1.).abs() < 0.001,
            "must not fade toward wallpaper when the next app is already cached"
        );
        assert_eq!(
            state.shelf_current_show().map(|show| show.name.as_str()),
            Some("Silo")
        );
    }

    #[test]
    fn cold_app_still_waits_before_fetching() {
        let mut state = ready();
        let netflix = state
            .inventory
            .apps
            .iter()
            .find(|app| tmdb::catalog_for(app).is_some())
            .expect("mapped app")
            .clone();
        state.focus = Target::App(netflix.id.clone());
        let effects = state.shelf_tick();
        assert!(
            effects.is_empty(),
            "cold focus must not fetch before the dwell: {effects:?}"
        );
        assert!(!state.shelf.revealed);
    }
}
