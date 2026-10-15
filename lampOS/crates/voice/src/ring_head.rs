//! A hand kept on the lamp's head, for the ring's countdown (#711,
//! `docs/creature.md` "Touch"): from one second the eye dims slowly, and at
//! three the lamp goes to sleep ([`crate::ring_face::Face::hand`]). The hand's
//! time is Controls' own (`controls/touch_state`, the head's `held_us`, from
//! `--touch` Controls), read from the bus by the ring's own worker. It only
//! shapes the face; it never decides whether the ring may show anything.
//!
//! Switched on with `LAMP_HEAD_TOUCH=on`, which the voice passes to its ring
//! worker as `--head-touch`. Off unless set.
use lamp_bus::{Bus, Message};
use lamp_msgs::{TouchSource, TouchState};
use std::{io, path::Path};

/// The ring worker's flag for it.
pub const FLAG: &str = "--head-touch";

/// Controls says the head's state at least every 20 ms; this looks every 50.
const LOOK_EVERY_US: u64 = 50_000;

pub struct RingHead {
    bus: Bus,
    looked_at_us: Option<u64>,
    held: Option<u64>,
}

impl RingHead {
    /// The head's state on the bus under `root`.
    pub fn on(root: &Path) -> io::Result<Self> {
        Ok(Self {
            bus: Bus::new(root)?,
            looked_at_us: None,
            held: None,
        })
    }

    /// Call from the ring's loop with its clock: how long a hand has been on
    /// the head, as Controls counts it, or `None`.
    pub fn tick(&mut self, now_us: u64) -> Option<u64> {
        if self
            .looked_at_us
            .is_none_or(|at| now_us.saturating_sub(at) >= LOOK_EVERY_US)
        {
            self.looked_at_us = Some(now_us);
            self.held = self
                .bus
                .latest(TouchState::TOPIC)
                .ok()
                .flatten()
                .and_then(|message| held(&message));
        }
        self.held
    }
}

/// The head's hand time in a fresh, valid, enabled state from Controls.
/// None for a hand a push from the body spoiled: Controls gives it no time.
pub fn held(message: &Message) -> Option<u64> {
    if message.expired || message.header.source != "controls" {
        return None;
    }
    let state = message.parse::<TouchState>().ok()?;
    (state.check().is_ok() && state.enabled).then_some(())?;
    state
        .sources
        .iter()
        .find(|source| source.source == TouchSource::Head)?
        .held_us
}

/// The ring worker's argument for the switch in the environment.
pub fn arguments_from_environment() -> Vec<String> {
    if crate::hand_controls::head_touch_from_environment() {
        vec![FLAG.to_owned()]
    } else {
        Vec::new()
    }
}

/// Take a leading `--head-touch` off the ring worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heads_hand_time_is_read_from_controls_state_and_nothing_else() {
        let root = std::env::temp_dir().join(format!("lamp-ring-head-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bus = Bus::new(&root).unwrap();
        let mut head = RingHead::on(&root).unwrap();
        assert_eq!(head.tick(1_000_000), None);
        let (state, _) =
            crate::hand_controls::test_head_touch("boot", lamp_ipc::monotonic_us() - 60_000);
        let held_us = state.sources[0].held_us;
        assert!(held_us.is_some());
        let mut said = bus
            .publisher_declared(TouchState::TOPIC, &TouchState::declaration(), "controls")
            .unwrap();
        said.publish_json(1, &state).unwrap();
        // Looked at most every 50 ms: not again yet, then the hand's time.
        assert_eq!(head.tick(1_000_001), None);
        assert_eq!(head.tick(1_000_000 + LOOK_EVERY_US), held_us);
        // Not from anyone but Controls.
        let mut other = bus
            .publisher_declared(TouchState::TOPIC, &TouchState::declaration(), "replay")
            .unwrap();
        other.publish_json(2, &state).unwrap();
        assert_eq!(head.tick(1_000_000 + 2 * LOOK_EVERY_US), None);
        drop((said, other));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_flag_comes_first_or_not_at_all() {
        let flags = [FLAG.to_owned(), "x".to_owned()];
        assert_eq!(take(&flags), (true, &flags[1..]));
        let none = ["x".to_owned()];
        assert_eq!(take(&none), (false, &none[..]));
    }
}
