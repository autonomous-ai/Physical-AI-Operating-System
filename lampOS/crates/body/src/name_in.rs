//! The lamp's name, heard (`name/heard`, `docs/name.md`), for a body that
//! turns to a call (`body::Limits::look_at_name`). Looked at once a step and
//! never waited for: the body's loop reads and writes the servos every
//! 20 ms and a hearing is a few bytes, a few times a minute at most.

use lamp_bus::{Bus, Subscriber};
use lamp_msgs::NameHeard;
use std::{io, time::Duration};

/// A hearing whose name began longer ago than this is not a call to turn
/// to now: the listener says one a tenth of a second or more after the
/// name began, so anything later is a body that was busy or a bus behind.
const FRESH_MS: u32 = 1_500;

/// The body's end of `name/heard`.
pub struct NameIn {
    heard: Subscriber,
}

impl NameIn {
    pub fn on(bus: &Bus) -> io::Result<Self> {
        Ok(Self {
            heard: bus.subscribe_typed(
                NameHeard::TOPIC,
                crate::bus_out::SOURCE,
                NameHeard::TYPE,
            )?,
        })
    }

    /// Whether a fresh hearing came since the last look. Never waits.
    pub fn called(&mut self) -> bool {
        let mut called = false;
        for _ in 0..8 {
            let Ok(Some(message)) = self.heard.recv(Duration::ZERO) else {
                break;
            };
            called |= message
                .parse::<NameHeard>()
                .is_ok_and(|heard| fresh(&heard));
        }
        called
    }
}

fn fresh(heard: &NameHeard) -> bool {
    heard.check().is_ok() && heard.ago_ms <= FRESH_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_sound_and_recent_hearing_is_a_call() {
        let heard = |score, ago_ms| NameHeard { score, ago_ms };
        assert!(fresh(&heard(0.8, 300)));
        assert!(fresh(&heard(0.8, FRESH_MS)));
        assert!(!fresh(&heard(0.8, FRESH_MS + 1)));
        assert!(!fresh(&heard(1.5, 300)));
    }
}
