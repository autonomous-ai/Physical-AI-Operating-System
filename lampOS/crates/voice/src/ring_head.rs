//! A hand kept on the lamp's head, for the ring's countdown (#711,
//! `docs/creature.md` "Touch"): from one second the eye dims slowly, and at
//! three the lamp goes to sleep ([`crate::ring_face::Face::hand`]). The hand's
//! time is Controls' own (`controls/touch_state`, the head's `held_us`, from
//! `--touch` Controls), read from the bus by the ring's own worker. It only
//! shapes the face; it never decides whether the ring may show anything.
//!
//! A tap on the head of a lamp at rest is listened for (#711, First's design
//! on #711, 05:05): the resting face is the listening face for
//! [`TAP_LISTENS_US`], then settles back ([`crate::ring_face::Face::tapped`]).
//! The tap is Controls' own (`controls/touch`), read by the same worker.
//!
//! Switched on with `LAMP_HEAD_TOUCH=on`, which the voice passes to its ring
//! worker as `--head-touch`. Off unless set.
use lamp_bus::{Bus, Message, Subscriber};
use lamp_msgs::{TouchEvent, TouchGesture, TouchSource, TouchState};
use std::{io, path::Path, time::Duration};

/// The ring worker's flag for it.
pub const FLAG: &str = "--head-touch";

/// Controls says the head's state at least every 20 ms; this looks every 50.
const LOOK_EVERY_US: u64 = 50_000;

/// How long a tap on the head is listened for at rest: as long as a call's
/// wait for the rest on lamp-4ace (`--call-wait-ms 2000`, #891). Speech that
/// begins in it is a turn, whose own listening face goes on from there.
pub const TAP_LISTENS_US: u64 = 2_000_000;

/// The ring's name among the head's subscribers.
const SUBSCRIBER: &str = "voice_ring";

/// At most this many head requests are read in one tick of the ring (2 ms):
/// far more than hands make, and never so many that a flood of them holds
/// back the worker's revocation and expiry (#849 review).
const MOST_AT_ONCE: usize = 4;

pub struct RingHead {
    bus: Bus,
    looked_at_us: Option<u64>,
    held: Option<u64>,
    taps: Subscriber,
    tapped_until_us: Option<u64>,
}

impl RingHead {
    /// The head's state and requests on the bus under `root`.
    pub fn on(root: &Path) -> io::Result<Self> {
        let bus = Bus::new(root)?;
        let taps = bus.subscribe_typed(TouchEvent::TOPIC, SUBSCRIBER, TouchEvent::TYPE)?;
        Ok(Self {
            bus,
            looked_at_us: None,
            held: None,
            taps,
            tapped_until_us: None,
        })
    }

    /// Until when the latest tap on the head is listened for at rest.
    pub fn tapped_until(&self) -> Option<u64> {
        self.tapped_until_us
    }

    /// Call from the ring's loop with its clock: how long a hand has been on
    /// the head, as Controls counts it, or `None`.
    pub fn tick(&mut self, now_us: u64) -> Option<u64> {
        // Taps on every tick, a few at a time: they wait in the queue, and
        // the face should answer within the same moment as the hum.
        for _ in 0..MOST_AT_ONCE {
            let Ok(Some(message)) = self.taps.recv(Duration::ZERO) else {
                break;
            };
            if tapped(&message) {
                self.tapped_until_us = Some(now_us + TAP_LISTENS_US);
            }
        }
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

/// A fresh, valid, enabled tap on the head, from Controls.
pub fn tapped(message: &Message) -> bool {
    if message.expired || message.header.source != "controls" {
        return false;
    }
    message.parse::<TouchEvent>().ok().is_some_and(|event| {
        event.check().is_ok()
            && event.enabled
            && event.source == TouchSource::Head
            && event.gesture == TouchGesture::Tap
    })
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
        let root = std::path::PathBuf::from(format!("/tmp/lrh-{}", std::process::id()));
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
    fn a_tap_on_the_head_from_controls_is_listened_for_and_nothing_else() {
        let root = std::path::PathBuf::from(format!("/tmp/lrt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bus = Bus::new(&root).unwrap();
        let mut head = RingHead::on(&root).unwrap();
        let now = lamp_ipc::monotonic_us();
        let mut recognizer = crate::hand_controls::TestHead::new("boot", now - 1_500_000);
        let mut ticks = recognizer.run(now - 500_000, false);
        ticks.extend(recognizer.run(now - 300_000, true));
        ticks.extend(recognizer.run(now, false));
        let events: Vec<_> = ticks
            .into_iter()
            .flat_map(|(_, _, events)| events)
            .collect();
        let tap = events
            .iter()
            .find(|event| event.gesture == TouchGesture::Tap)
            .expect("a tap")
            .clone();
        let touch = events
            .iter()
            .find(|event| event.gesture == TouchGesture::Touch)
            .expect("its touch")
            .clone();
        let mut said = bus
            .publisher_declared(TouchEvent::TOPIC, &TouchEvent::declaration(), "controls")
            .unwrap();
        let mut other = bus
            .publisher_declared(TouchEvent::TOPIC, &TouchEvent::declaration(), "replay")
            .unwrap();
        // A touch is not a tap; a tap from anyone but Controls is nothing.
        said.publish_json(1, &touch).unwrap();
        other.publish_json(2, &tap).unwrap();
        head.tick(5_000_000);
        assert_eq!(head.tapped_until(), None);
        // Controls' tap: listened for two seconds from when it was read.
        said.publish_json(3, &tap).unwrap();
        head.tick(6_000_000);
        assert_eq!(head.tapped_until(), Some(6_000_000 + TAP_LISTENS_US));
        // Nothing new: the window stands.
        head.tick(6_500_000);
        assert_eq!(head.tapped_until(), Some(6_000_000 + TAP_LISTENS_US));
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
