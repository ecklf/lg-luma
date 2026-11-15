//! Home Top Shelf dwell/reveal and TMDB request scheduling.
use crate::state::{Effect, Page, State, Target};
use crate::tmdb::{
    self, CROSSFADE_SECS, Catalog, Command, REVEAL_MS, ROTATE_MS, Response, VISIBILITY_SECS,
};
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

    fn shelf_show_for(&self, app_id: &str, selected: usize) -> Option<&tmdb::Show> {
        let shows = self.shelf.shows.get(app_id)?;
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
        let index = selected % with_art.len();
        Some(with_art[index].1)
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
            let previous = self.shelf.focus_app.clone();
            let was_showing = self.shelf.revealed && self.shelf.visibility > 0.05;
            self.shelf.focus_app = Some(app.id.clone());
            self.shelf.focus_since = Some(Instant::now());
            self.shelf.selected = 0;
            self.shelf.rotate_since = None;
            if ready {
                // Cached art: no dwell. Crossfade series→series (keep visibility).
                self.shelf.revealed = true;
                self.shelf.rotate_since = Some(Instant::now());
                if self.preferences.reduced_motion {
                    self.shelf.display_app = None;
                    self.shelf.crossfade = 1.;
                    self.shelf.visibility = 1.;
                } else if was_showing {
                    self.shelf.display_app = self.shelf.display_app.take().or(previous);
                    self.shelf.crossfade = 0.;
                } else {
                    self.shelf.display_app = None;
                    self.shelf.crossfade = 1.;
                }
            } else {
                self.shelf.revealed = false;
                self.shelf.crossfade = 1.;
                if was_showing && !self.preferences.reduced_motion {
                    // Fade outgoing art out to the wallpaper while the cold app dwells.
                    self.shelf.display_app = self.shelf.display_app.take().or(previous);
                } else {
                    self.shelf.display_app = None;
                    if self.preferences.reduced_motion {
                        self.shelf.visibility = 0.;
                    }
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
        // Dwell only gates the first fetch for a cold app.
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
            && self.shelf.display_app.is_none()
            && let Some(shows) = self.shelf.shows.get(&app.id)
            && shows.len() > 1
            && let Some(started) = self.shelf.rotate_since
            && started.elapsed().as_millis() as u64 >= ROTATE_MS
        {
            self.shelf.selected = (self.shelf.selected + 1) % shows.len();
            self.shelf.rotate_since = Some(Instant::now());
            self.dirty = true;
        }

        let dt = self.shelf_animation_dt();
        let crossfading = ready && self.shelf.display_app.is_some() && self.shelf.revealed;
        if crossfading {
            // Keep full shelf visibility; blend outgoing→incoming only.
            if self.shelf.visibility < 1. {
                self.shelf_step_visibility(1., dt);
            }
            if self.preferences.reduced_motion {
                self.shelf.crossfade = 1.;
            } else if self.shelf.crossfade < 1. {
                self.shelf.crossfade = (self.shelf.crossfade + dt / CROSSFADE_SECS).min(1.);
                self.dirty = true;
            }
            if self.shelf.crossfade >= 0.999 {
                self.shelf.crossfade = 1.;
                self.shelf.display_app = None;
                self.dirty = true;
            }
        } else if self.shelf.display_app.is_some() && !ready {
            // Cold target: shade outgoing down to wallpaper.
            self.shelf_step_visibility(0., dt);
            if self.shelf.visibility <= 0.001 {
                self.shelf.visibility = 0.;
                self.shelf.display_app = None;
                self.dirty = true;
            }
        } else {
            let goal = if self.shelf.revealed && ready { 1. } else { 0. };
            self.shelf_step_visibility(goal, dt);
        }
        effects
    }

    /// Fade the backdrop out when focus leaves a mapped app. Keep the outgoing
    /// show until visibility hits 0 so it can still paint during the fade.
    fn shelf_hide_tick(&mut self) -> Vec<Effect> {
        if self.shelf.revealed {
            self.shelf.revealed = false;
            self.dirty = true;
        }
        if self.shelf.display_app.is_none() {
            self.shelf.display_app = self.shelf.focus_app.clone();
        }
        self.shelf.crossfade = 0.;
        let dt = self.shelf_animation_dt();
        self.shelf_step_visibility(0., dt);
        if self.shelf.visibility <= 0.001 {
            self.shelf.visibility = 0.;
            if self.shelf.focus_app.is_some()
                || self.shelf.focus_since.is_some()
                || self.shelf.display_app.is_some()
            {
                self.shelf.focus_app = None;
                self.shelf.display_app = None;
                self.shelf.focus_since = None;
                self.shelf.crossfade = 1.;
                self.dirty = true;
            }
        }
        vec![]
    }

    fn shelf_animation_dt(&mut self) -> f32 {
        let now = Instant::now();
        let measured = self
            .shelf
            .updated
            .map(|previous| now.saturating_duration_since(previous).as_secs_f32())
            .unwrap_or(0.);
        self.shelf.updated = Some(now);
        // Sub-millisecond gaps (unit tests, or a busy loop before VSync) still
        // advance one nominal 60 Hz frame so fades do not stall.
        if measured < 1e-4 {
            1. / 60.
        } else {
            measured.min(0.05)
        }
    }

    fn shelf_step_visibility(&mut self, goal: f32, dt: f32) {
        if self.preferences.reduced_motion {
            if self.shelf.visibility != goal {
                self.shelf.visibility = goal;
                self.dirty = true;
            }
        } else if (self.shelf.visibility - goal).abs() > 0.001 {
            let step = dt / VISIBILITY_SECS;
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

    /// Incoming (focused) show — used when not crossfading, or as the fade-in layer.
    pub fn shelf_current_show(&self) -> Option<&tmdb::Show> {
        // Wallpaper fades only paint the outgoing layer.
        if self.shelf.display_app.is_some() && !self.shelf.revealed {
            return None;
        }
        let id = self.shelf.focus_app.as_ref()?;
        self.shelf_show_for(id, self.shelf.selected)
    }

    /// Outgoing show while crossfading or fading out to the wallpaper.
    pub fn shelf_outgoing_show(&self) -> Option<&tmdb::Show> {
        let id = self.shelf.display_app.as_ref()?;
        self.shelf_show_for(id, 0)
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
            id: id.bytes().map(u64::from).sum::<u64>().max(1),
            name: id.into(),
            overview: String::new(),
            backdrop: format!("/{id}.jpg"),
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
        state.shelf.crossfade = 1.;
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
    fn steady_shelf_reveal_is_not_an_animation() {
        let mut state = ready();
        state.shelf.revealed = true;
        state.shelf.visibility = 1.;
        state.shelf.crossfade = 1.;
        state.shelf.display_app = None;
        assert!(
            !state.animating(),
            "a settled Top Shelf must not busy-spin the event loop"
        );
        state.shelf.display_app = Some("com.netflix.app".into());
        state.shelf.crossfade = 0.4;
        assert!(
            state.animating(),
            "an in-flight crossfade still counts as animating"
        );
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
    fn cached_apps_crossfade_without_wallpaper_dip() {
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
        state.shelf.crossfade = 1.;

        state.focus = Target::App(b.id.clone());
        state.shelf_tick();
        assert!(state.shelf.revealed, "no dwell wait for cached target");
        assert_eq!(state.shelf.focus_app.as_deref(), Some(b.id.as_str()));
        assert_eq!(state.shelf.display_app.as_deref(), Some(a.id.as_str()));
        assert!(
            (state.shelf.visibility - 1.).abs() < 0.001,
            "series→series must keep visibility (no wallpaper dip)"
        );
        assert!(
            state.shelf.crossfade > 0. && state.shelf.crossfade < 1.,
            "crossfade should be in progress"
        );
        assert_eq!(
            state.shelf_outgoing_show().map(|show| show.name.as_str()),
            Some("a")
        );
        assert_eq!(
            state.shelf_current_show().map(|show| show.name.as_str()),
            Some("b")
        );

        for _ in 0..20 {
            state.shelf_tick();
        }
        assert!(state.shelf.display_app.is_none());
        assert!((state.shelf.crossfade - 1.).abs() < 0.001);
        assert!((state.shelf.visibility - 1.).abs() < 0.001);
        assert_eq!(
            state.shelf_current_show().map(|show| show.name.as_str()),
            Some("b")
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
