//! Refresh only HDMI status, without reloading apps or queueing repeated reads.
use crate::model::Input;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Refresh {
    next: Option<Instant>,
    in_flight: bool,
    pending: Option<Vec<Input>>,
}

impl Refresh {
    const INTERVAL: Duration = Duration::from_secs(2);
    const RETRY: Duration = Duration::from_secs(10);

    pub fn due(&mut self, now: Instant, active: bool) -> bool {
        if !active {
            self.next = None;
            return false;
        }
        if self.in_flight || now < *self.next.get_or_insert(now + Self::INTERVAL) {
            return false;
        }
        self.in_flight = true;
        true
    }

    pub fn completed(
        &mut self,
        now: Instant,
        result: Result<Vec<Input>, String>,
    ) -> Result<(), String> {
        self.in_flight = false;
        self.next = Some(
            now + if result.is_ok() {
                Self::INTERVAL
            } else {
                Self::RETRY
            },
        );
        self.pending = Some(result?);
        Ok(())
    }

    /// Keep the latest snapshot until the current interaction has finished.
    pub fn take_pending(&mut self, ready: bool) -> Option<Vec<Input>> {
        ready.then(|| self.pending.take()).flatten()
    }

    /// A later complete inventory already contains newer input data.
    pub fn superseded(&mut self, now: Instant) {
        self.pending = None;
        self.next = Some(now + Self::INTERVAL);
    }
}
