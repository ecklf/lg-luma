//! Home channel Back overlay: Recents-style carousel over favorite tuner channels.
use crate::state::{Direction, Effect, Overlay, Page, State, Target};
use std::time::Instant;

#[derive(Debug)]
pub struct ChannelSwitcher {
    pub selected: usize,
    pub position: f32,
    pub visibility: f32,
    pub closing_focus: Option<Target>,
    pub updated: Instant,
}

impl Default for ChannelSwitcher {
    fn default() -> Self {
        Self {
            selected: 0,
            position: 0.,
            visibility: 0.,
            closing_focus: None,
            updated: Instant::now(),
        }
    }
}

impl ChannelSwitcher {
    pub fn cards_animating(&self, pointer: bool) -> bool {
        !pointer && self.position != self.selected as f32
    }

    pub fn offset(&self, index: usize) -> f32 {
        index as f32 - self.position
    }

    pub fn animate(&mut self, open: bool, reduced: bool, pointer: bool, now: Instant) -> bool {
        let dt = now
            .saturating_duration_since(self.updated)
            .as_secs_f32()
            .min(0.1);
        self.updated = now;
        let visibility = if open { 1. } else { 0. };
        let changed = self.visibility != visibility || self.cards_animating(pointer);
        let approach = |current: &mut f32, target: f32, speed: f32| {
            if reduced || (*current - target).abs() < 0.002 {
                *current = target;
            } else {
                *current += (target - *current) * (1. - (-speed * dt).exp());
            }
        };
        approach(&mut self.visibility, visibility, 24.);
        if !pointer {
            approach(&mut self.position, self.selected as f32, 18.);
        }
        if !open && self.visibility == 0. {
            self.closing_focus = None;
        }
        changed
    }
}

impl State {
    pub fn channel_switcher_open(&self) -> bool {
        matches!(self.overlay, Some(Overlay::ChannelSwitcher { .. }))
    }

    /// Short or long Back: channel carousel when a Home favorite is focused.
    pub fn open_back_overlay(&mut self) -> Vec<Effect> {
        if self.channel_switcher_open() || self.recents_open() {
            return vec![];
        }
        if self.page == Page::Home
            && matches!(self.focus, Target::Channel(_))
            && !self.home_channels().is_empty()
        {
            self.open_channel_switcher()
        } else {
            self.open_recents()
        }
    }

    pub fn open_channel_switcher(&mut self) -> Vec<Effect> {
        if self.overlay.is_some() || self.moving.is_some() {
            return vec![];
        }
        let ids: Vec<String> = self
            .home_channels()
            .iter()
            .map(|channel| channel.id.clone())
            .collect();
        if ids.is_empty() {
            return vec![];
        }
        let selected = match &self.focus {
            Target::Channel(id) => ids.iter().position(|channel| channel == id).unwrap_or(0),
            _ => 0,
        };
        let id = ids[selected].clone();
        self.overlay = Some(Overlay::ChannelSwitcher {
            restore: self.focus.clone(),
        });
        if self.channel_switcher.visibility == 0. {
            self.channel_switcher.selected = selected;
            self.channel_switcher.position = selected as f32;
            self.channel_switcher.closing_focus = None;
        }
        self.channel_switcher.updated = Instant::now();
        self.focus = self
            .channel_switcher
            .closing_focus
            .take()
            .unwrap_or(Target::ChannelSwitch(id));
        if self.preferences.reduced_motion {
            self.channel_switcher.visibility = 1.;
        }
        self.pointer = false;
        self.pressed = None;
        self.notice = None;
        self.panel_started = None;
        self.dirty = true;
        vec![Effect::RefreshChannelGuide]
    }

    pub fn channel_switcher_direction(&mut self, direction: Direction) -> Vec<Effect> {
        if matches!(direction, Direction::Up) {
            return self.back();
        }
        let ids: Vec<String> = self
            .home_channels()
            .iter()
            .map(|channel| channel.id.clone())
            .collect();
        if ids.is_empty() {
            return vec![];
        }
        let index = match &self.focus {
            Target::ChannelSwitch(id) => ids.iter().position(|channel| channel == id),
            _ => None,
        };
        if let Some(index) = index {
            let next = match direction {
                Direction::Left => index.saturating_sub(1),
                Direction::Right => (index + 1).min(ids.len() - 1),
                _ => index,
            };
            self.channel_switcher.selected = next;
            self.focus = Target::ChannelSwitch(ids[next].clone());
        } else {
            let selected = self
                .channel_switcher
                .selected
                .min(ids.len().saturating_sub(1));
            self.channel_switcher.selected = selected;
            self.focus = Target::ChannelSwitch(ids[selected].clone());
        }
        vec![]
    }
}
