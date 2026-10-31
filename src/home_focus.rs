//! Per-card critically damped focus motion; retargeting preserves position and velocity.
use std::collections::BTreeMap;

#[derive(Default)]
struct Spring {
    value: f32,
    velocity: f32,
    target: f32,
}

impl Spring {
    fn moving(&self) -> bool {
        self.value != self.target || self.velocity != 0.
    }

    fn advance(&mut self, dt: f32, reduced: bool) {
        if reduced {
            self.value = self.target;
            self.velocity = 0.;
            return;
        }
        // Analytic solution: consistent motion at 30, 60 and 120 Hz, without Euler instability.
        let omega = 24.;
        let displacement = self.value - self.target;
        let coefficient = self.velocity + omega * displacement;
        let decay = (-omega * dt).exp();
        self.value = self.target + (displacement + coefficient * dt) * decay;
        self.velocity = (self.velocity - omega * coefficient * dt) * decay;
        if (self.value - self.target).abs() < 0.001 && self.velocity.abs() < 0.02 {
            self.value = self.target;
            self.velocity = 0.;
        }
    }
}

#[derive(Default)]
pub struct HomeFocus {
    cards: BTreeMap<String, Spring>,
}

impl HomeFocus {
    pub fn amount(&self, id: &str) -> f32 {
        self.cards
            .get(id)
            .map_or(0., |spring| spring.value.clamp(0., 1.))
    }

    pub fn animating(&self) -> bool {
        self.cards.values().any(Spring::moving)
    }

    pub fn advance(&mut self, selected: Option<&str>, dt: f32, reduced: bool) -> bool {
        if let Some(id) = selected {
            self.cards.entry(id.to_owned()).or_default();
        }
        let mut changed = false;
        for (id, spring) in &mut self.cards {
            spring.target = if selected == Some(id.as_str()) {
                1.
            } else {
                0.
            };
            changed |= spring.moving();
            spring.advance(dt.max(0.), reduced);
        }
        self.cards
            .retain(|_, spring| spring.target != 0. || spring.moving());
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retarget_preserves_motion_and_eventually_stops() {
        let mut focus = HomeFocus::default();
        focus.advance(Some("youtube"), 0.08, false);
        let before = focus.amount("youtube");
        assert!(before > 0. && before < 1.);
        focus.advance(Some("netflix"), 0., false);
        assert_eq!(focus.amount("youtube"), before);
        assert_eq!(focus.amount("netflix"), 0.);
        focus.advance(Some("netflix"), 0.03, false);
        let mid = focus.amount("youtube");
        focus.advance(Some("youtube"), 0., false);
        assert_eq!(focus.amount("youtube"), mid);
        for _ in 0..120 {
            focus.advance(Some("youtube"), 1. / 60., false);
        }
        assert_eq!(focus.amount("youtube"), 1.);
        assert_eq!(focus.cards.len(), 1);
        assert!(!focus.animating());
        focus.advance(None, 2., false);
        assert!(focus.cards.is_empty());
    }

    #[test]
    fn motion_is_frame_rate_independent() {
        let mut slow = HomeFocus::default();
        let mut fast = HomeFocus::default();
        for _ in 0..6 {
            slow.advance(Some("app"), 1. / 30., false);
        }
        for _ in 0..24 {
            fast.advance(Some("app"), 1. / 120., false);
        }
        assert!((slow.amount("app") - fast.amount("app")).abs() < 0.00001);
    }

    #[test]
    fn reduced_motion_snaps_and_does_not_keep_rendering() {
        let mut focus = HomeFocus::default();
        focus.advance(Some("a"), 0.02, false);
        focus.advance(Some("b"), 0., true);
        assert_eq!(focus.amount("a"), 0.);
        assert_eq!(focus.amount("b"), 1.);
        assert!(!focus.animating());
        assert!(!focus.advance(Some("b"), 1., true));
    }
}
