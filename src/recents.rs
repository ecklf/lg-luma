//! System recent history, deliberately independent of favorites and running apps.
use crate::{
    app_artwork::Source,
    model::{App, demo_inventory, valid_app_id},
    platform::{APP_ID, HOME_ID},
    state::{Direction, Effect, Overlay, State, Target},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::HashSet, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Command {
    List,
    Launch(String),
    Close(String),
}

impl Command {
    /// Exact operations observed from stock Home. Never synthesize a kill-all.
    pub fn request(&self) -> Result<(&'static str, Value), String> {
        if let Self::Launch(id) | Self::Close(id) = self
            && (!valid_app_id(id) || excluded(id) || crate::scroll_preview::is_placeholder(id))
        {
            return Err("Invalid recent application identifier".into());
        }
        Ok(match self {
            Self::List => ("luna://com.webos.service.recent/listRecents", json!({})),
            Self::Close(id) => (
                "luna://com.webos.service.recent/removeRecent",
                json!({"appId": id}),
            ),
            Self::Launch(id) => (
                "luna://com.webos.service.applicationmanager/launch",
                json!({"id": id, "params": {}, "reason": "recent",
                    "checkUpdateOnLaunch": true, "autoInstallation": true}),
            ),
        })
    }
}

fn excluded(id: &str) -> bool {
    matches!(
        id,
        APP_ID | HOME_ID | "com.luma.native" | "com.luma.launcher" | "com.luma.launcher.native"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub app: App,
}

pub fn normalize(reply: &Value) -> Result<Vec<Entry>, String> {
    if reply.get("ready") == Some(&json!(false)) {
        return Err("Recent apps are not ready".into());
    }
    let entries = reply
        .get("recentsAppList")
        .and_then(Value::as_array)
        .filter(|entries| entries.len() <= 1000)
        .ok_or("Invalid recent apps reply")?;
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for entry in entries {
        let Some(id) = entry
            .get("appId")
            .and_then(Value::as_str)
            .filter(|id| valid_app_id(id) && !excluded(id) && seen.insert(*id))
        else {
            continue;
        };
        let title: String = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect();
        let mut app = App::new(
            id,
            if title.trim().is_empty() {
                id
            } else {
                title.trim()
            },
        );
        app.icon = Some(Source::new(id, &json!({})));
        result.push(Entry { app });
    }
    Ok(result)
}

/// Desktop-only sample history. Never substituted for failed TV reads.
pub fn demo() -> Vec<Entry> {
    demo_inventory()
        .apps
        .into_iter()
        .skip(1)
        .take(8)
        .map(|app| Entry { app })
        .collect()
}

pub struct Recents {
    pub entries: Vec<Entry>,
    pub generation: u64,
    pub pending: Option<Command>,
    pub error: Option<String>,
    pub selected: usize,
    pub position: f32,
    pub visibility: f32,
    pub closing_focus: Option<Target>,
    pub updated: Instant,
}

impl Default for Recents {
    fn default() -> Self {
        Self {
            entries: vec![],
            generation: 0,
            pending: None,
            error: None,
            selected: 0,
            position: 0.,
            visibility: 0.,
            closing_focus: None,
            updated: Instant::now(),
        }
    }
}

impl Recents {
    pub fn animate(&mut self, open: bool, reduced: bool, pointer: bool, now: Instant) -> bool {
        let dt = now
            .saturating_duration_since(self.updated)
            .as_secs_f32()
            .min(0.1);
        self.updated = now;
        let visibility = if open { 1. } else { 0. };
        let changed =
            self.visibility != visibility || (!pointer && self.position != self.selected as f32);
        let approach = |current: &mut f32, target: f32| {
            if reduced || (*current - target).abs() < 0.002 {
                *current = target;
            } else {
                *current += (target - *current) * (1. - (-24. * dt).exp());
            }
        };
        // Opacity always settles, even after pointer input interrupts carousel
        // movement. Retargeting starts from the current presentation value.
        approach(&mut self.visibility, visibility);
        if !pointer {
            approach(&mut self.position, self.selected as f32);
        }
        if !open && self.visibility == 0. {
            self.closing_focus = None;
        }
        changed
    }
}

impl State {
    pub fn recents_open(&self) -> bool {
        matches!(self.overlay, Some(Overlay::Recents { .. }))
    }

    pub fn open_recents(&mut self) -> Vec<Effect> {
        if self.overlay.is_some() || self.moving.is_some() {
            return vec![];
        }
        self.overlay = Some(Overlay::Recents {
            restore: self.focus.clone(),
        });
        if self.recents.visibility == 0. {
            self.recents.entries.clear();
            self.recents.selected = 0;
            self.recents.position = 0.;
            self.recents.closing_focus = None;
        }
        self.recents.updated = Instant::now();
        self.recents.pending = None;
        self.focus = self.recents.closing_focus.take().unwrap_or_else(|| {
            self.recents
                .entries
                .get(self.recents.selected)
                .map(|entry| Target::Recent(entry.app.id.clone()))
                .unwrap_or(Target::RecentEmpty)
        });
        if self.preferences.reduced_motion {
            self.recents.visibility = 1.;
        }
        self.pointer = false;
        self.pressed = None;
        self.notice = None;
        self.panel_started = None;
        self.recent_request(Command::List)
    }

    pub fn recent_request(&mut self, command: Command) -> Vec<Effect> {
        if !self.recents_open() || self.recents.pending.is_some() {
            return vec![];
        }
        self.recents.generation += 1;
        self.recents.pending = Some(command.clone());
        self.recents.error = None;
        self.dirty = true;
        vec![Effect::Recent {
            generation: self.recents.generation,
            command,
        }]
    }

    pub fn recent_response(
        &mut self,
        generation: u64,
        command: Command,
        result: Result<Option<Vec<Entry>>, String>,
    ) -> Vec<Effect> {
        if !self.recents_open()
            || generation != self.recents.generation
            || self.recents.pending.as_ref() != Some(&command)
        {
            return vec![];
        }
        self.recents.pending = None;
        self.dirty = true;
        match result {
            Err(error) => {
                eprintln!("Luma recent apps: {error}");
                self.recents.error = Some(error);
                if self.recents.entries.is_empty() {
                    self.focus = Target::RecentRetry;
                }
            }
            Ok(entries) => match command {
                Command::List => {
                    let old = match &self.focus {
                        Target::Recent(id) => Some(id.clone()),
                        _ => None,
                    };
                    self.recents.entries = entries.unwrap_or_default();
                    self.recents.selected = old
                        .and_then(|id| self.recents.entries.iter().position(|e| e.app.id == id))
                        .unwrap_or(self.recents.selected)
                        .min(self.recents.entries.len().saturating_sub(1));
                    self.focus = self
                        .recents
                        .entries
                        .get(self.recents.selected)
                        .map(|e| Target::Recent(e.app.id.clone()))
                        .unwrap_or(Target::RecentEmpty);
                }
                Command::Launch(_) => {
                    return self.back();
                }
                Command::Close(id) => {
                    self.recents.entries.retain(|e| e.app.id != id);
                    self.recents.selected = self
                        .recents
                        .selected
                        .min(self.recents.entries.len().saturating_sub(1));
                    self.focus = self
                        .recents
                        .entries
                        .get(self.recents.selected)
                        .map(|e| Target::Recent(e.app.id.clone()))
                        .unwrap_or(Target::RecentEmpty);
                    return self.recent_request(Command::List);
                }
            },
        }
        vec![]
    }

    pub fn recent_direction(&mut self, direction: Direction) -> Vec<Effect> {
        if matches!(direction, Direction::Up)
            && let Target::Recent(id) = &self.focus
        {
            return self.recent_request(Command::Close(id.clone()));
        }
        let entries = &self.recents.entries;
        let index = match &self.focus {
            Target::Recent(id) => entries.iter().position(|e| &e.app.id == id),
            _ => None,
        };
        if let Some(index) = index {
            let next = match direction {
                Direction::Left => index.saturating_sub(1),
                Direction::Right => (index + 1).min(entries.len() - 1),
                _ => index,
            };
            self.recents.selected = next;
            let id = entries[next].app.id.clone();
            self.focus = match direction {
                Direction::Down if self.recents.error.is_some() => Target::RecentRetry,
                _ => Target::Recent(id),
            };
        } else {
            self.focus = match (&self.focus, direction) {
                (Target::RecentRetry, Direction::Up | Direction::Right) => entries
                    .get(self.recents.selected)
                    .map(|e| Target::Recent(e.app.id.clone()))
                    .unwrap_or(if self.recents.error.is_some() {
                        Target::RecentRetry
                    } else {
                        Target::RecentEmpty
                    }),
                (_, Direction::Left) if self.recents.error.is_some() => Target::RecentRetry,
                _ => self.focus.clone(),
            };
        }
        vec![]
    }
}
