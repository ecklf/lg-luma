//! Partial redraw: which part of the logical 1920×1080 scene changed since an earlier frame.
//! Consecutive scenes are compared command by command; a back buffer that still holds the frame
//! `age` presents ago needs the union of the last `age` changes redrawn.
use crate::{renderer::Draw, state::Rect};
use std::{collections::VecDeque, sync::Arc};

/// EGL swap chains rotate a few buffers; older contents are redrawn in full.
const HISTORY: usize = 4;
/// Commands searched ahead to realign after an inserted or removed command (e.g. a focus ring).
const LOOKAHEAD: usize = 8;

/// The redraw region of one frame: `None` is the whole scene, an empty rect is no change.
pub type Damage = Option<Rect>;

#[derive(Default)]
pub struct Tracker {
    previous: Vec<Draw>,
    history: VecDeque<Damage>,
}

impl Tracker {
    /// Records the scene about to be drawn.
    pub fn push(&mut self, commands: &[Draw]) {
        let damage = if self.previous.is_empty() {
            None
        } else {
            diff(&self.previous, commands)
        };
        self.history.push_front(damage);
        self.history.truncate(HISTORY);
        self.previous = commands.to_vec();
    }

    /// What to redraw into a back buffer holding the frame `age` presents ago; 0 means unknown.
    pub fn damage(&self, age: u32) -> Damage {
        let age = age as usize;
        if age == 0 || age > self.history.len() {
            return None;
        }
        self.history
            .iter()
            .take(age)
            .try_fold(Rect::default(), |area, damage| {
                Some(union(area, (*damage)?))
            })
    }

    /// After anything drew into the window outside this tracker, e.g. a resized drawable.
    pub fn reset(&mut self) {
        self.previous.clear();
        self.history.clear();
    }
}

fn diff(old: &[Draw], new: &[Draw]) -> Damage {
    let mut area = Rect::default();
    let mut j = 0;
    for command in new {
        if j < old.len() && same(&old[j], command) {
            j += 1;
            continue;
        }
        // An inserted command leaves the old one in place; a removed one is skipped over.
        let found = (j..old.len().min(j + LOOKAHEAD)).find(|&k| same(&old[k], command));
        if let Some(k) = found {
            for removed in &old[j..k] {
                area = union(area, bounds(removed)?);
            }
            j = k + 1;
        } else {
            area = union(area, bounds(command)?);
        }
    }
    for removed in &old[j.min(old.len())..] {
        area = union(area, bounds(removed)?);
    }
    Some(area)
}

/// The area a command can paint. `None` for state that affects everything drawn after it.
pub fn bounds(command: &Draw) -> Option<Rect> {
    match command {
        Draw::Shape(rect, ..)
        | Draw::RoundedGradient(rect, ..)
        | Draw::Image(_, rect, ..)
        | Draw::TintedImage(_, rect, ..)
        | Draw::AppImage(_, rect, _) => Some(rect.inset(-1.)),
        Draw::Shadow(rect, _, blur, _) => Some(rect.inset(-blur - 1.)),
        // Glyphs may extend past their layout box: redraw the full width of the line.
        Draw::Text(_, rect, style, ..) => Some(Rect::new(
            0.,
            rect.y - style.size,
            1920.,
            rect.h + 2. * style.size,
        )),
        Draw::Clip(_) | Draw::EdgeFade(_) | Draw::EdgeFadeX(_) => None,
    }
}

fn same(a: &Draw, b: &Draw) -> bool {
    let rect = |a: &Rect, b: &Rect| a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h;
    match (a, b) {
        (Draw::Shape(r1, a1, b1, c1), Draw::Shape(r2, a2, b2, c2)) => {
            rect(r1, r2) && a1 == a2 && b1 == b2 && c1 == c2
        }
        (Draw::RoundedGradient(r1, a1, t1, b1), Draw::RoundedGradient(r2, a2, t2, b2)) => {
            rect(r1, r2) && a1 == a2 && t1 == t2 && b1 == b2
        }
        (Draw::Shadow(r1, a1, b1, c1), Draw::Shadow(r2, a2, b2, c2)) => {
            rect(r1, r2) && a1 == a2 && b1 == b2 && c1 == c2
        }
        (Draw::Text(t1, r1, s1, c1, x1), Draw::Text(t2, r2, s2, c2, x2)) => {
            t1 == t2
                && rect(r1, r2)
                && s1.size == s2.size
                && s1.weight == s2.weight
                && s1.tracking == s2.tracking
                && s1.line_height == s2.line_height
                && s1.align_right == s2.align_right
                && c1 == c2
                && x1 == x2
        }
        (Draw::Image(p1, r1, a1, c1), Draw::Image(p2, r2, a2, c2)) => {
            p1 == p2 && rect(r1, r2) && a1 == a2 && c1 == c2
        }
        (Draw::TintedImage(p1, r1, a1, c1, t1), Draw::TintedImage(p2, r2, a2, c2, t2)) => {
            p1 == p2 && rect(r1, r2) && a1 == a2 && c1 == c2 && t1 == t2
        }
        (Draw::AppImage(i1, r1, t1), Draw::AppImage(i2, r2, t2)) => {
            Arc::ptr_eq(i1, i2) && rect(r1, r2) && t1 == t2
        }
        (Draw::Clip(c1), Draw::Clip(c2)) => match (c1, c2) {
            (Some(r1), Some(r2)) => rect(r1, r2),
            (None, None) => true,
            _ => false,
        },
        (Draw::EdgeFade(f1), Draw::EdgeFade(f2)) | (Draw::EdgeFadeX(f1), Draw::EdgeFadeX(f2)) => {
            f1 == f2
        }
        _ => false,
    }
}

pub fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn union(a: Rect, b: Rect) -> Rect {
    if a.w <= 0. || a.h <= 0. {
        return b;
    }
    if b.w <= 0. || b.h <= 0. {
        return a;
    }
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        (a.x + a.w).max(b.x + b.w) - x,
        (a.y + a.h).max(b.y + b.h) - y,
    )
}

#[cfg(test)]
#[path = "damage-tests.rs"]
mod tests;
