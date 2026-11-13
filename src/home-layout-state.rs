use crate::{
    home_layout::{Alignment, Editor, Kind, Layout, Mode},
    state::*,
};

impl State {
    pub fn widget_first_content(&self) -> Target {
        let mut widgets: Vec<_> = self
            .preferences
            .home_layout
            .as_ref()
            .unwrap()
            .widgets
            .iter()
            .collect();
        widgets.sort_by_key(|w| (w.row, w.column));
        for widget in widgets {
            match widget.kind {
                Kind::Favorites => {
                    return self.apps().first().map_or(
                        if self.error.is_some() {
                            Target::Retry
                        } else {
                            Target::Browse
                        },
                        |app| Target::App(app.id.clone()),
                    );
                }
                Kind::Inputs => {
                    if let Some(input) = self
                        .inventory
                        .inputs
                        .iter()
                        .find(|input| self.preferences.input_visible(input))
                    {
                        return Target::Input(input.id.clone());
                    }
                }
                Kind::Weather if self.preferences.weather_location.is_none() => {
                    return Target::WeatherConfigure;
                }
                _ => {}
            }
        }
        Target::Nav(2)
    }
    pub fn home_layout(&self) -> Option<&Layout> {
        self.layout_editor
            .as_ref()
            .map(|editor| &editor.draft)
            .or(self.preferences.home_layout.as_ref())
    }

    pub fn home_widget(&self, kind: Kind) -> Option<&crate::home_layout::Widget> {
        self.home_layout()?.widget(kind)
    }

    pub fn favorite_bounds(&self) -> (f32, f32) {
        self.home_widget(Kind::Favorites)
            .map_or((HOME_STRIP_LEFT, HOME_STRIP_RIGHT), |widget| {
                let rect = widget.rect();
                (rect.x, rect.x + rect.w)
            })
    }

    pub fn edit_layout(&mut self) {
        let draft = self
            .preferences
            .home_layout
            .clone()
            .unwrap_or_else(|| Layout::preset(0, &self.preferences));
        self.open(0);
        self.focus = draft
            .widgets
            .first()
            .map_or(Target::LayoutToolbar(0), |w| Target::LayoutWidget(w.id));
        self.layout_editor = Some(Editor {
            draft,
            mode: Mode::Select,
            show_grid: true,
        });
        self.home_scroll = 0.;
        self.input_scroll = 0.;
        self.pressed = None;
    }

    pub fn layout_activate(&mut self) -> Vec<Effect> {
        self.dirty = true;
        self.pressed = None;
        let target = self.focus.clone();
        let mode = self.layout_editor.as_ref().unwrap().mode.clone();
        if let Mode::Move(w) | Mode::Resize(w) = mode {
            self.layout_editor.as_mut().unwrap().mode = Mode::Menu(w.id);
            self.focus = Target::LayoutAction(0);
            return vec![];
        }
        if let Target::LayoutWidget(id) = target {
            self.layout_editor.as_mut().unwrap().mode = Mode::Menu(id);
            self.focus = Target::LayoutAction(0);
            return vec![];
        }
        if let Target::LayoutToolbar(index) = target {
            match index {
                0 => self.layout_editor.as_mut().unwrap().mode = Mode::Add,
                1 => self.layout_editor.as_mut().unwrap().mode = Mode::Presets,
                2 => {
                    self.layout_editor.as_mut().unwrap().draft = self.layout_template(0);
                    self.home_scroll = 0.;
                    self.input_scroll = 0.;
                    return vec![];
                }
                3 => return self.finish_layout(true),
                4 => return self.finish_layout(false),
                5 => {
                    let editor = self.layout_editor.as_mut().unwrap();
                    editor.show_grid = !editor.show_grid;
                    return vec![];
                }
                _ => return vec![],
            }
            self.focus = Target::LayoutAction(0);
            return vec![];
        }
        let Target::LayoutAction(index) = target else {
            return vec![];
        };
        match mode {
            Mode::Menu(id) => {
                let editor = self.layout_editor.as_mut().unwrap();
                let Some(widget) = editor.draft.widgets.iter().find(|w| w.id == id).cloned() else {
                    return vec![];
                };
                match index {
                    0 | 1 => {
                        editor.mode = if index == 0 {
                            Mode::Move(widget)
                        } else {
                            Mode::Resize(widget)
                        };
                        self.focus = Target::LayoutWidget(id);
                    }
                    2 => {
                        editor.mode = Mode::Configure(id);
                        self.focus = Target::LayoutAction(0);
                    }
                    3 => {
                        editor.draft.widgets.retain(|w| w.id != id);
                        editor.mode = Mode::Select;
                        self.focus = Target::LayoutToolbar(0);
                    }
                    _ => {}
                }
            }
            Mode::Add => {
                if let Some(kind) = Kind::ALL.get(index) {
                    if let Some(id) = self.layout_editor.as_mut().unwrap().draft.add(*kind) {
                        self.layout_editor.as_mut().unwrap().mode = Mode::Select;
                        self.focus = Target::LayoutWidget(id);
                    } else {
                        self.notify(self.preferences.language.t("layoutNoRoom"));
                    }
                }
            }
            Mode::Presets => {
                self.layout_editor.as_mut().unwrap().draft = self.layout_template(index);
                self.layout_editor.as_mut().unwrap().mode = Mode::Select;
                self.focus = Target::LayoutToolbar(1);
                self.home_scroll = 0.;
                self.input_scroll = 0.;
            }
            Mode::Configure(id) => {
                let editor = self.layout_editor.as_mut().unwrap();
                if let Some(mut widget) = editor.draft.widgets.iter().find(|w| w.id == id).cloned()
                {
                    match index {
                        0 => widget.size = (widget.size + 1) % 3,
                        1 => {
                            widget.alignment = match widget.alignment {
                                Alignment::Left => Alignment::Center,
                                Alignment::Center => Alignment::Right,
                                Alignment::Right => Alignment::Left,
                            }
                        }
                        3 => widget.labels = !widget.labels,
                        4 => widget.large_clock = !widget.large_clock,
                        _ => {}
                    }
                    if !editor.draft.replace(widget) {
                        self.notify(self.preferences.language.t("layoutBlocked"));
                    }
                }
                self.home_scroll = 0.;
                self.input_scroll = 0.;
            }
            Mode::Exit => match index {
                0 => return self.finish_layout(true),
                1 => return self.finish_layout(false),
                _ => {
                    self.layout_editor.as_mut().unwrap().mode = Mode::Select;
                    self.focus = Target::LayoutToolbar(3);
                }
            },
            _ => {}
        }
        vec![]
    }

    fn finish_layout(&mut self, save: bool) -> Vec<Effect> {
        let editor = self.layout_editor.take().unwrap();
        if save && editor.draft.valid() {
            self.preferences.show_date = editor.draft.widget(Kind::Date).is_some();
            self.preferences.show_greeting = editor.draft.widget(Kind::Greeting).is_some();
            self.preferences.show_weather = editor.draft.widget(Kind::Weather).is_some();
            if let Some(widget) = editor.draft.widget(Kind::Favorites) {
                self.preferences.show_home_names = widget.labels;
            }
            self.preferences.home_layout = Some(editor.draft);
        }
        self.home_scroll = 0.;
        self.input_scroll = 0.;
        self.open(2);
        self.section = WIDGETS_SECTION;
        self.focus = Target::LayoutEdit;
        if save { vec![Effect::Save] } else { vec![] }
    }

    pub fn layout_back(&mut self) {
        let editor = self.layout_editor.as_mut().unwrap();
        match editor.mode.clone() {
            Mode::Move(before) | Mode::Resize(before) => {
                let id = before.id;
                editor.draft.replace(before);
                editor.mode = Mode::Menu(id);
                self.focus = Target::LayoutAction(0);
            }
            Mode::Menu(id) => {
                editor.mode = Mode::Select;
                self.focus = Target::LayoutWidget(id);
            }
            Mode::Configure(id) => {
                editor.mode = Mode::Menu(id);
                self.focus = Target::LayoutAction(2);
            }
            Mode::Select => {
                editor.mode = Mode::Exit;
                self.focus = Target::LayoutAction(2);
            }
            _ => {
                editor.mode = Mode::Select;
                self.focus = Target::LayoutToolbar(0);
            }
        }
        self.pressed = None;
    }

    pub fn layout_direction(&mut self, direction: Direction, controls: &[Control]) {
        let editor = self.layout_editor.as_mut().unwrap();
        match &editor.mode {
            Mode::Move(before) | Mode::Resize(before) => {
                let resize = matches!(editor.mode, Mode::Resize(_));
                if resize
                    && before.is_strip()
                    && matches!(direction, Direction::Up | Direction::Down)
                {
                    return;
                }
                let mut candidate = editor
                    .draft
                    .widgets
                    .iter()
                    .find(|w| w.id == before.id)
                    .unwrap()
                    .clone();
                let value = match (resize, direction) {
                    (false, Direction::Left | Direction::Right) => &mut candidate.column,
                    (false, _) => &mut candidate.row,
                    (true, Direction::Left | Direction::Right) => &mut candidate.width,
                    (true, _) => &mut candidate.height,
                };
                *value = if matches!(direction, Direction::Left | Direction::Up) {
                    value.saturating_sub(1)
                } else {
                    value.saturating_add(1)
                };
                if !editor.draft.replace(candidate) {
                    self.notify(self.preferences.language.t("layoutBlocked"));
                }
            }
            _ => self.spatial_layout_focus(direction, controls),
        }
    }

    fn spatial_layout_focus(&mut self, direction: Direction, controls: &[Control]) {
        let Some(current) = controls.iter().find(|c| c.target == self.focus) else {
            if let Some(first) = controls.first() {
                self.focus = first.target.clone();
            }
            return;
        };
        let (x, y) = current.rect.center();
        let next = controls
            .iter()
            .filter(|c| c.target != self.focus)
            .filter_map(|c| {
                let (cx, cy) = c.rect.center();
                let (primary, cross, overlap) = match direction {
                    Direction::Up => (y - cy, (x - cx).abs(), (current.rect.w + c.rect.w) / 2.),
                    Direction::Down => (cy - y, (x - cx).abs(), (current.rect.w + c.rect.w) / 2.),
                    Direction::Left => (x - cx, (y - cy).abs(), (current.rect.h + c.rect.h) / 2.),
                    Direction::Right => (cx - x, (y - cy).abs(), (current.rect.h + c.rect.h) / 2.),
                };
                (primary > 1.).then_some((
                    c.target.clone(),
                    primary + cross * 2. + if cross >= overlap { 10000. } else { 0. },
                ))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((target, _)) = next {
            self.focus(target, controls);
        }
    }

    /// Each strip owns horizontal navigation; crossing its end uses actual screen geometry.
    pub fn widget_direction(&mut self, direction: Direction, controls: &[Control]) {
        if let Target::Nav(index) = self.focus {
            let next = match direction {
                Direction::Left => index.checked_sub(1),
                Direction::Right if index < 2 => Some(index + 1),
                _ => None,
            };
            if let Some(next) = next {
                self.open(next);
                return;
            }
        }
        if matches!(direction, Direction::Left | Direction::Right) {
            let targets: Vec<_> = match &self.focus {
                Target::App(_) => self
                    .apps()
                    .iter()
                    .map(|app| Target::App(app.id.clone()))
                    .collect(),
                Target::Input(_) => self
                    .inventory
                    .inputs
                    .iter()
                    .filter(|input| self.preferences.input_visible(input))
                    .map(|input| Target::Input(input.id.clone()))
                    .collect(),
                _ => Vec::new(),
            };
            if let Some(index) = targets.iter().position(|t| *t == self.focus) {
                let next = if matches!(direction, Direction::Left) {
                    index.checked_sub(1)
                } else {
                    (index + 1 < targets.len()).then_some(index + 1)
                };
                if let Some(next) = next {
                    self.focus(targets[next].clone(), controls);
                    return;
                }
            }
        }
        let (left, right) = self.favorite_bounds();
        let input_rect = self.home_widget(Kind::Inputs).map(|w| w.rect());
        let visible: Vec<_> = controls
            .iter()
            .filter(|c| match c.target {
                Target::App(_) => c.rect.x >= left - 0.1 && c.rect.x + c.rect.w <= right + 0.1,
                Target::Input(_) => input_rect.is_some_and(|r| {
                    c.rect.x >= r.x - 0.1 && c.rect.x + c.rect.w <= r.x + r.w + 0.1
                }),
                _ => true,
            })
            .cloned()
            .collect();
        self.spatial_layout_focus(direction, &visible);
    }

    pub fn scroll_inputs(&mut self, delta: f32) {
        let Some(widget) = self.home_widget(Kind::Inputs) else {
            self.input_scroll = 0.;
            return;
        };
        let count = self
            .inventory
            .inputs
            .iter()
            .filter(|input| self.preferences.input_visible(input))
            .count();
        let width = widget.card_width();
        let max = if count == 0 {
            0.
        } else {
            ((count - 1) as f32 * (width + 22.35) + width - widget.rect().w).max(0.)
        };
        self.input_scroll = (self.input_scroll + delta).clamp(0., max);
    }

    pub fn widget_wheel(&mut self, position: Option<(f32, f32)>, x: i32, y: i32) -> bool {
        if self.layout_editor.is_some() {
            return false;
        }
        for kind in [Kind::Favorites, Kind::Inputs] {
            if let Some(widget) = self.home_widget(kind)
                && position.is_some_and(|(x, y)| widget.rect().contains(x, y))
            {
                let delta = (if x != 0 { x } else { -y }) as f32 * (widget.card_width() + 25.6);
                if kind == Kind::Favorites {
                    self.scroll_favorites(delta);
                } else {
                    self.scroll_inputs(delta);
                }
                self.pointer = true;
                self.dirty = true;
                return true;
            }
        }
        false
    }

    /// Keep the pre-existing visibility controls useful after adopting a custom layout.
    pub fn toggle_layout_widget(&mut self, kind: Kind, enabled: bool) -> bool {
        let Some(layout) = self.preferences.home_layout.as_mut() else {
            return true;
        };
        if !enabled {
            layout.widgets.retain(|w| w.kind != kind);
            return true;
        }
        if layout.widget(kind).is_some() || layout.add(kind).is_some() {
            true
        } else {
            self.notify(self.preferences.language.t("layoutNoRoom"));
            false
        }
    }

    fn layout_template(&self, index: usize) -> Layout {
        // Explicit presets/reset are predictable even after saving Minimal. Only the
        // initial migration inherits legacy visibility toggles.
        let preferences = crate::model::Preferences {
            show_date: true,
            show_greeting: true,
            show_weather: true,
            ..self.preferences.clone()
        };
        Layout::preset(index, &preferences)
    }
}
