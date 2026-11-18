//! Use SDL's standard controller mapping, including DualSense where available.
//! Translate to keyboard actions so OK/Back holds share the remote's behavior.
use sdl2::{
    GameControllerSubsystem,
    controller::{Axis, Button, GameController},
    event::Event,
    keyboard::{Keycode, Mod},
};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Navigation {
    buttons: Vec<Button>,
    x: i16,
    y: i16,
    keys: Vec<Keycode>,
    repeat_at: Option<Instant>,
}

impl Navigation {
    fn update(&mut self, now: Instant) -> Vec<Event> {
        let mut keys: Vec<_> = self.buttons.iter().filter_map(|b| button_key(*b)).collect();
        for (value, negative, positive) in [
            (self.x, Keycode::Left, Keycode::Right),
            (self.y, Keycode::Up, Keycode::Down),
        ] {
            let key = if value < -16000 {
                Some(negative)
            } else if value > 16000 {
                Some(positive)
            } else {
                None
            };
            if let Some(key) = key
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        let mut events = vec![];
        for key in &self.keys {
            if !keys.contains(key) {
                events.push(key_event(*key, false, false));
            }
        }
        for key in &keys {
            if !self.keys.contains(key) {
                events.push(key_event(*key, true, false));
            }
        }
        if keys != self.keys {
            self.repeat_at = Some(now + Duration::from_millis(350));
        }
        self.keys = keys;
        events
    }

    fn repeats(&mut self, now: Instant) -> Vec<Event> {
        if self.repeat_at.is_none_or(|at| now < at) {
            return vec![];
        }
        self.repeat_at = Some(now + Duration::from_millis(100));
        self.keys
            .iter()
            .filter(|k| {
                matches!(
                    **k,
                    Keycode::Up | Keycode::Down | Keycode::Left | Keycode::Right
                )
            })
            .map(|k| key_event(*k, true, true))
            .collect()
    }
}

fn button_key(button: Button) -> Option<Keycode> {
    Some(match button {
        Button::A => Keycode::Return,
        Button::B => Keycode::Escape,
        Button::Start => Keycode::Menu,
        Button::DPadUp => Keycode::Up,
        Button::DPadDown => Keycode::Down,
        Button::DPadLeft => Keycode::Left,
        Button::DPadRight => Keycode::Right,
        _ => return None,
    })
}

fn key_event(key: Keycode, down: bool, repeat: bool) -> Event {
    if down {
        Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(key),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat,
        }
    } else {
        Event::KeyUp {
            timestamp: 0,
            window_id: 0,
            keycode: Some(key),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat: false,
        }
    }
}

pub struct Gamepads {
    subsystem: GameControllerSubsystem,
    controllers: Vec<GameController>,
    active: Option<u32>,
    navigation: Navigation,
}

impl Gamepads {
    pub fn new(sdl: &sdl2::Sdl) -> Result<Self, String> {
        let subsystem = sdl.game_controller()?;
        let mut pads = Self {
            subsystem,
            controllers: vec![],
            active: None,
            navigation: Navigation::default(),
        };
        for index in 0..pads.subsystem.num_joysticks()? {
            pads.open(index);
        }
        Ok(pads)
    }

    fn open(&mut self, index: u32) {
        if !self.subsystem.is_game_controller(index) {
            return;
        }
        match self.subsystem.open(index) {
            Ok(controller) => {
                if !self
                    .controllers
                    .iter()
                    .any(|c| c.instance_id() == controller.instance_id())
                {
                    eprintln!("Luma controller: {}", controller.name());
                    self.controllers.push(controller);
                }
            }
            Err(error) => eprintln!("Luma controller open: {error}"),
        }
    }

    pub fn reset(&mut self) {
        self.active = None;
        self.navigation = Navigation::default();
    }

    pub fn repeats(&mut self, now: Instant) -> Vec<Event> {
        self.navigation.repeats(now)
    }

    /// None means this is not a controller event. Only one pad owns a press at a time.
    pub fn handle(&mut self, event: &Event, enabled: bool, now: Instant) -> Option<Vec<Event>> {
        match *event {
            Event::ControllerDeviceAdded { which, .. } => self.open(which),
            Event::ControllerDeviceRemoved { which, .. } => {
                self.controllers.retain(|c| c.instance_id() != which);
                if self.active == Some(which) {
                    self.reset();
                }
            }
            Event::ControllerButtonDown { which, button, .. }
            | Event::ControllerButtonUp { which, button, .. } => {
                if enabled && self.accept(which) {
                    self.navigation.buttons.retain(|b| *b != button);
                    if matches!(event, Event::ControllerButtonDown { .. }) {
                        self.navigation.buttons.push(button);
                    }
                    return Some(self.navigation.update(now));
                }
            }
            Event::ControllerAxisMotion {
                which, axis, value, ..
            } => {
                if enabled && matches!(axis, Axis::LeftX | Axis::LeftY) && self.accept(which) {
                    match axis {
                        Axis::LeftX => self.navigation.x = value,
                        Axis::LeftY => self.navigation.y = value,
                        _ => {}
                    }
                    return Some(self.navigation.update(now));
                }
            }
            _ => return None,
        }
        Some(vec![])
    }

    fn accept(&mut self, which: u32) -> bool {
        if !self.controllers.iter().any(|c| c.instance_id() == which) {
            return false;
        }
        if self.active.is_none() || self.navigation.keys.is_empty() {
            if self.active != Some(which) {
                self.navigation = Navigation::default();
            }
            self.active = Some(which);
        }
        self.active == Some(which)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dualsense_buttons_use_existing_remote_actions() {
        assert_eq!(button_key(Button::A), Some(Keycode::Return));
        assert_eq!(button_key(Button::B), Some(Keycode::Escape));
        assert_eq!(button_key(Button::Start), Some(Keycode::Menu));
        assert_eq!(button_key(Button::Guide), None);
    }

    #[test]
    fn stick_deadzone_and_direction_changes_release_old_key() {
        let now = Instant::now();
        let mut navigation = Navigation {
            x: 8000,
            ..Default::default()
        };
        assert!(navigation.update(now).is_empty());
        navigation.x = -24000;
        assert_eq!(
            navigation.update(now),
            vec![key_event(Keycode::Left, true, false)]
        );
        navigation.x = 24000;
        assert_eq!(
            navigation.update(now),
            vec![
                key_event(Keycode::Left, false, false),
                key_event(Keycode::Right, true, false)
            ]
        );
    }

    #[test]
    fn repeats_only_directions_and_deduplicates_stick_and_dpad() {
        let now = Instant::now();
        let mut navigation = Navigation {
            buttons: vec![Button::A, Button::DPadRight],
            x: 24000,
            ..Default::default()
        };
        assert_eq!(navigation.update(now).len(), 2);
        assert!(
            navigation
                .repeats(now + Duration::from_millis(349))
                .is_empty()
        );
        assert_eq!(
            navigation.repeats(now + Duration::from_millis(350)),
            vec![key_event(Keycode::Right, true, true)]
        );
        navigation.x = 0;
        assert!(navigation.update(now).is_empty());
        navigation.buttons.clear();
        assert_eq!(navigation.update(now).len(), 2);
        assert!(navigation.repeats(now + Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn ok_and_back_release_once_without_repeats() {
        let now = Instant::now();
        for (button, key) in [(Button::A, Keycode::Return), (Button::B, Keycode::Escape)] {
            let mut navigation = Navigation {
                buttons: vec![button],
                ..Default::default()
            };
            assert_eq!(navigation.update(now), vec![key_event(key, true, false)]);
            assert!(navigation.update(now).is_empty());
            assert!(navigation.repeats(now + Duration::from_secs(1)).is_empty());
            navigation.buttons.clear();
            assert_eq!(navigation.update(now), vec![key_event(key, false, false)]);
            assert!(navigation.update(now).is_empty());
        }
    }
}
