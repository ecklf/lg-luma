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
    CloseAll,
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
            Self::CloseAll => (
                "luna://com.webos.service.recent/removeAllRecents",
                json!({}),
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
            updated: Instant::now(),
        }
    }
}

impl Recents {
    pub fn animate(&mut self, reduced: bool) -> bool {
        let target = self.selected as f32;
        let dt = self.updated.elapsed().as_secs_f32().min(0.1);
        self.updated = Instant::now();
        let changed = self.position != target;
        if reduced || (self.position - target).abs() < 0.002 {
            self.position = target;
        } else {
            self.position += (target - self.position) * (1. - (-22. * dt).exp());
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
        self.recents.entries.clear();
        self.recents.selected = 0;
        self.recents.position = 0.;
        self.recents.updated = Instant::now();
        self.recents.pending = None;
        self.focus = Target::RecentRetry;
        self.pointer = false;
        self.pressed = None;
        self.notice = None;
        self.panel_started = (!self.preferences.reduced_motion).then(Instant::now);
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
                        Target::Recent(id) | Target::RecentClose(id) => Some(id.clone()),
                        _ => None,
                    };
                    self.recents.entries = entries.unwrap_or_default();
                    self.recents.selected = old
                        .and_then(|id| self.recents.entries.iter().position(|e| e.app.id == id))
                        .unwrap_or(self.recents.selected)
                        .min(self.recents.entries.len().saturating_sub(1));
                    if self.focus != Target::RecentCloseAll || self.recents.entries.is_empty() {
                        self.focus = self
                            .recents
                            .entries
                            .get(self.recents.selected)
                            .map(|e| {
                                if matches!(self.focus, Target::RecentClose(_)) {
                                    Target::RecentClose(e.app.id.clone())
                                } else {
                                    Target::Recent(e.app.id.clone())
                                }
                            })
                            .unwrap_or(Target::RecentCloseAll);
                    }
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
                        .map(|e| Target::RecentClose(e.app.id.clone()))
                        .unwrap_or(Target::RecentCloseAll);
                    return self.recent_request(Command::List);
                }
                Command::CloseAll => {
                    self.recents.entries.clear();
                    self.recents.selected = 0;
                    self.focus = Target::RecentCloseAll;
                    return self.recent_request(Command::List);
                }
            },
        }
        vec![]
    }

    pub fn recent_direction(&mut self, direction: Direction) {
        let entries = &self.recents.entries;
        let index = match &self.focus {
            Target::Recent(id) | Target::RecentClose(id) => {
                entries.iter().position(|e| &e.app.id == id)
            }
            _ => None,
        };
        if let Some(index) = index {
            let close = matches!(self.focus, Target::RecentClose(_));
            let next = match direction {
                Direction::Left => index.saturating_sub(1),
                Direction::Right => (index + 1).min(entries.len() - 1),
                _ => index,
            };
            self.recents.selected = next;
            let id = entries[next].app.id.clone();
            self.focus = match direction {
                Direction::Up if close => Target::RecentCloseAll,
                Direction::Up => Target::RecentClose(id),
                Direction::Down if close => Target::Recent(id),
                Direction::Down if self.recents.error.is_some() => Target::RecentRetry,
                Direction::Down => Target::Recent(id),
                _ if close => Target::RecentClose(id),
                _ => Target::Recent(id),
            };
        } else {
            self.focus = match (&self.focus, direction) {
                (Target::RecentCloseAll, Direction::Down)
                | (Target::RecentRetry, Direction::Up | Direction::Right) => entries
                    .get(self.recents.selected)
                    .map(|e| Target::Recent(e.app.id.clone()))
                    .unwrap_or(if self.recents.error.is_some() {
                        Target::RecentRetry
                    } else {
                        Target::RecentCloseAll
                    }),
                (_, Direction::Left) if self.recents.error.is_some() => Target::RecentRetry,
                _ => self.focus.clone(),
            };
        }
    }
}
