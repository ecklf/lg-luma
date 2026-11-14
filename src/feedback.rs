//! Original, quiet TV-style cues. No external audio assets, codecs, or Apple recordings.
use crate::state::{Overlay, Page, State, Subpage, Target};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use std::{
    f32::consts::TAU,
    mem::{Discriminant, discriminant},
    time::{Duration, Instant},
};

const RATE: i32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    Navigate,
    Select,
    Back,
}

/// Only deliberate input changes produce cues, never redraws or worker responses.
#[derive(PartialEq)]
pub struct Route {
    focus: Target,
    page: Page,
    section: usize,
    subpage: Option<Subpage>,
    overlay: Option<Discriminant<Overlay>>,
    moving: Option<String>,
}

impl Route {
    pub fn capture(state: &State) -> Self {
        Self {
            focus: state.focus.clone(),
            page: state.page,
            section: state.section,
            subpage: state.subpage.clone(),
            overlay: state.overlay.as_ref().map(discriminant),
            moving: state.moving.clone(),
        }
    }

    pub fn cue(&self, requested: Option<Cue>, state: &State, has_effects: bool) -> Option<Cue> {
        requested.filter(|cue| *cue == Cue::Select || *self != Self::capture(state) || has_effects)
    }
}

#[derive(Default)]
pub struct Feedback {
    device: Option<AudioQueue<f32>>,
    samples: [Vec<f32>; 3],
    failed: bool,
    last_navigation: Option<Instant>,
}

impl Feedback {
    /// Release the device when disabled/backgrounded, so other TV apps own audio.
    pub fn sync(&mut self, enabled: bool, active: bool) {
        if !enabled || !active {
            self.device = None;
            self.last_navigation = None;
        }
        if !enabled {
            self.failed = false;
        }
    }

    pub fn play(&mut self, sdl: &sdl2::Sdl, cue: Cue) {
        if self.failed {
            return;
        }
        let now = Instant::now();
        if cue == Cue::Navigate
            && self
                .last_navigation
                .is_some_and(|last| now.duration_since(last) < Duration::from_millis(65))
        {
            return;
        }
        if self.device.is_none() {
            let opened = sdl.audio().and_then(|audio| {
                audio.open_queue(
                    None,
                    &AudioSpecDesired {
                        freq: Some(RATE),
                        channels: Some(1),
                        samples: Some(512),
                    },
                )
            });
            match opened {
                Ok(device) => {
                    self.samples = [
                        synthesize(Cue::Navigate),
                        synthesize(Cue::Select),
                        synthesize(Cue::Back),
                    ];
                    device.resume();
                    self.device = Some(device);
                }
                Err(error) => {
                    eprintln!("Luma UI sounds unavailable: {error}");
                    self.failed = true;
                    return;
                }
            }
        }
        let device = self.device.as_ref().unwrap();
        // Never accumulate stale cues behind rapid repeats; at most one short cue is queued.
        device.clear();
        let index = match cue {
            Cue::Navigate => 0,
            Cue::Select => 1,
            Cue::Back => 2,
        };
        if let Err(error) = device.queue_audio(&self.samples[index]) {
            eprintln!("Luma UI sounds stopped: {error}");
            self.device = None;
            self.failed = true;
        }
        if cue == Cue::Navigate {
            self.last_navigation = Some(now);
        }
    }
}

fn synthesize(cue: Cue) -> Vec<f32> {
    let (duration, start, end) = match cue {
        Cue::Navigate => (0.045, 540., 420.),
        Cue::Select => (0.085, 660., 990.),
        Cue::Back => (0.075, 540., 330.),
    };
    let count = (duration * RATE as f32) as usize;
    (0..count)
        .map(|index| {
            let t = index as f32 / RATE as f32;
            let progress = index as f32 / (count - 1) as f32;
            let envelope = (std::f32::consts::PI * progress).sin().powi(2) * (-3. * progress).exp();
            let phase = TAU * (start * t + (end - start) * t * t / (2. * duration));
            0.12 * envelope * (phase.sin() + 0.15 * (2. * phase).sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Preferences;

    #[test]
    fn cues_are_short_quiet_finite_and_fade_at_both_ends() {
        for cue in [Cue::Navigate, Cue::Select, Cue::Back] {
            let samples = synthesize(cue);
            assert!(samples.len() <= RATE as usize / 10);
            assert!(
                samples
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() < 0.14)
            );
            assert!(samples.iter().any(|sample| sample.abs() > 0.01));
            assert!(samples.first().unwrap().abs() < 0.00001);
            assert!(samples.last().unwrap().abs() < 0.00001);
        }
        assert_ne!(synthesize(Cue::Navigate), synthesize(Cue::Select));
        assert_ne!(synthesize(Cue::Select), synthesize(Cue::Back));
    }

    #[test]
    fn unchanged_navigation_and_background_updates_are_silent() {
        let mut state = State::new(Preferences::default());
        let route = Route::capture(&state);
        assert_eq!(route.cue(Some(Cue::Navigate), &state, false), None);
        assert_eq!(route.cue(Some(Cue::Back), &state, false), None);
        state.focus = Target::SettingsMenu;
        assert_eq!(
            route.cue(Some(Cue::Navigate), &state, false),
            Some(Cue::Navigate)
        );
        assert_eq!(route.cue(None, &state, true), None);
        assert_eq!(
            route.cue(Some(Cue::Select), &state, false),
            Some(Cue::Select)
        );
    }

    #[test]
    fn disabled_audio_is_lazy_and_clears_failure_for_an_explicit_retry() {
        let mut feedback = Feedback::default();
        feedback.sync(false, true);
        assert!(feedback.device.is_none());
        assert!(feedback.samples.iter().all(Vec::is_empty));
        feedback.failed = true;
        feedback.sync(true, true);
        assert!(feedback.failed);
        feedback.sync(false, true);
        assert!(!feedback.failed);
    }

    #[test]
    #[ignore = "run alone with SDL_AUDIODRIVER=dummy; opens an SDL audio device"]
    fn dummy_audio_queues_cues_and_releases_the_device() {
        let sdl = sdl2::init().unwrap();
        let mut feedback = Feedback::default();
        feedback.sync(true, true);
        feedback.play(&sdl, Cue::Select);
        assert!(!feedback.failed);
        assert!(feedback.device.is_some());
        feedback.play(&sdl, Cue::Navigate);
        let last = feedback.last_navigation;
        feedback.play(&sdl, Cue::Navigate);
        assert_eq!(
            feedback.last_navigation, last,
            "rapid navigation is throttled"
        );
        feedback.play(&sdl, Cue::Back);
        assert!(feedback.device.as_ref().unwrap().size() <= (RATE as u32 / 10) * 4);
        feedback.sync(true, false);
        assert!(feedback.device.is_none());
        feedback.play(&sdl, Cue::Select);
        assert!(feedback.device.is_some());
        feedback.sync(false, true);
        assert!(feedback.device.is_none());
    }

    #[test]
    #[ignore = "run alone with SDL_AUDIODRIVER=luma-unavailable; checks audio failure"]
    fn unavailable_audio_fails_silently_without_repeated_initialization() {
        let sdl = sdl2::init().unwrap();
        let mut feedback = Feedback::default();
        feedback.play(&sdl, Cue::Select);
        assert!(feedback.failed);
        assert!(feedback.device.is_none());
        feedback.play(&sdl, Cue::Navigate);
        assert!(feedback.failed);
        assert!(feedback.samples.iter().all(Vec::is_empty));
        feedback.sync(false, true);
        assert!(!feedback.failed);
    }
}
