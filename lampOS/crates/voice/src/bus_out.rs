//! What the voice says on the lamp's bus (`docs/bus.md`): what it is doing,
//! what it heard and what it said.
//!
//! The bus never holds a publisher up: a send is non-blocking, and one that
//! nobody takes costs nothing. So this is called from the coordinator's own
//! loop, and nothing here can fail the voice: a bus that cannot be used is
//! said once at start, and after that every error is dropped.
//!
//! Three topics, with the types of `lamp-msgs`:
//!
//! - `voice/state`: resting, listening, thinking or speaking. When it
//!   changes, and once a second so that a subscriber can tell a voice that
//!   is resting from one that has gone.
//! - `voice/heard`: the provider's text of what a person said, in pieces as
//!   it comes.
//! - `voice/said`: the provider's text of what the lamp said, with the
//!   number of the request it answered.
//!
//! The text is what the provider wrote down, which is not always what was
//! said. It stays on this machine: the bus's root is a directory in memory
//! that only the lamp's own programs can enter.
use crate::ring_wire::RingPhase;
use lamp_bus::{Bus, Publisher};
use lamp_msgs::{Speech, Voice, VoiceState};
use std::io;

/// Where the coordinator reads whether to publish. `on` only. The root is
/// the bus's own to say (`LAMP_BUS_ROOT`, or `/run/lampos-bus`).
pub const ENVIRONMENT: &str = "LAMP_BUS";
/// The name the voice publishes under.
pub const SOURCE: &str = "voice";
/// An unchanged state is said again this often.
const REPEAT_US: u64 = 1_000_000;

pub struct BusOut {
    state: Publisher,
    heard: Publisher,
    said: Publisher,
    last: Option<(Voice, u64)>,
}

impl BusOut {
    /// The publishers, if the environment says `on` and the bus can be
    /// used. A bus that cannot is said once and the voice goes on without.
    pub fn from_environment() -> Option<Self> {
        if std::env::var(ENVIRONMENT).ok().as_deref() != Some("on") {
            return None;
        }
        match Bus::from_environment().and_then(|bus| Self::on(&bus)) {
            Ok(out) => Some(out),
            Err(error) => {
                eprintln!("lamp-voice: nothing is published on the bus: {error}");
                None
            }
        }
    }

    pub fn on(bus: &Bus) -> io::Result<Self> {
        let [heard, said] = Speech::TOPICS;
        Ok(Self {
            state: bus.publisher_declared(VoiceState::TOPIC, &VoiceState::declaration(), SOURCE)?,
            heard: bus.publisher_declared(heard, &Speech::declaration(), SOURCE)?,
            said: bus.publisher_declared(said, &Speech::declaration(), SOURCE)?,
            last: None,
        })
    }

    /// The phase of the turn that owns the lamp, or none between turns.
    /// Call every tick: it publishes on a change and once a second.
    pub fn state(&mut self, phase: Option<RingPhase>, now_us: u64) {
        let state = match phase {
            None => Voice::Resting,
            Some(RingPhase::Listening) => Voice::Listening,
            Some(RingPhase::Waiting) => Voice::Thinking,
            Some(RingPhase::Speaking) => Voice::Speaking,
        };
        if self
            .last
            .is_some_and(|(told, at)| told == state && now_us.saturating_sub(at) < REPEAT_US)
        {
            return;
        }
        self.last = Some((state, now_us));
        let _ = self.state.publish_json(now_us, &VoiceState { state });
    }

    /// A piece of the provider's text: of the lamp's answer to `request`,
    /// or with none of what a person said.
    pub fn speech(&mut self, request: Option<u64>, text: &str, finished: bool, now_us: u64) {
        let to = match request {
            Some(_) => &mut self.said,
            None => &mut self.heard,
        };
        let _ = to.publish_json(
            now_us,
            &Speech {
                text: text.to_owned(),
                finished,
                locale: None,
                request,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A root short enough for a socket's path, which the system's own
    /// temporary directory on a Mac is not.
    fn bus(name: &str) -> (Bus, std::path::PathBuf) {
        let root = std::path::PathBuf::from(format!("/tmp/lvb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        (Bus::new(&root).unwrap(), root)
    }
    const WAIT: Duration = Duration::from_millis(500);

    #[test]
    fn the_state_is_published_when_it_changes_and_once_a_second() {
        let (bus, root) = bus("state");
        let mut out = BusOut::on(&bus).unwrap();
        let mut watch = bus
            .subscribe_typed(VoiceState::TOPIC, "watch", VoiceState::TYPE)
            .unwrap();
        // A subscriber is found within the bus's 200 ms.
        std::thread::sleep(Duration::from_millis(250));
        let mut said = Vec::new();
        for (phase, at_us) in [
            (None, 0),
            (None, 500_000),
            (Some(RingPhase::Listening), 600_000),
            (Some(RingPhase::Listening), 900_000),
            (Some(RingPhase::Waiting), 1_000_000),
            (Some(RingPhase::Speaking), 1_100_000),
            (Some(RingPhase::Speaking), 2_099_999),
            (Some(RingPhase::Speaking), 2_100_000),
        ] {
            out.state(phase, at_us);
            while let Some(message) = watch.recv(Duration::from_millis(30)).unwrap() {
                said.push((message.parse::<VoiceState>().unwrap().state, at_us));
            }
        }
        assert_eq!(
            said,
            [
                (Voice::Resting, 0),
                (Voice::Listening, 600_000),
                (Voice::Thinking, 1_000_000),
                (Voice::Speaking, 1_100_000),
                (Voice::Speaking, 2_100_000),
            ]
        );
        // Whoever starts late reads how things stand.
        let latest = bus.latest(VoiceState::TOPIC).unwrap().unwrap();
        assert_eq!(latest.parse::<VoiceState>().unwrap().state, Voice::Speaking);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn what_was_heard_and_what_was_said_go_to_their_own_topics() {
        let (bus, root) = bus("speech");
        let mut out = BusOut::on(&bus).unwrap();
        let mut heard = bus
            .subscribe_typed("voice/heard", "h", Speech::TYPE)
            .unwrap();
        let mut said = bus
            .subscribe_typed("voice/said", "s", Speech::TYPE)
            .unwrap();
        std::thread::sleep(Duration::from_millis(250));
        out.speech(None, "How do I take my coffee?", true, 1);
        out.speech(Some(7), "Black.", false, 2);
        let person: Speech = heard.recv(WAIT).unwrap().unwrap().parse().unwrap();
        assert_eq!(
            (person.text.as_str(), person.finished, person.request),
            ("How do I take my coffee?", true, None)
        );
        let lamp: Speech = said.recv(WAIT).unwrap().unwrap().parse().unwrap();
        assert_eq!(
            (lamp.text.as_str(), lamp.finished, lamp.request),
            ("Black.", false, Some(7))
        );
        assert!(heard.recv(Duration::from_millis(30)).unwrap().is_none());
        assert!(said.recv(Duration::from_millis(30)).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn with_nobody_listening_and_with_the_bus_gone_nothing_fails() {
        let (bus, root) = bus("gone");
        let mut out = BusOut::on(&bus).unwrap();
        out.state(None, 0);
        out.speech(None, "anyone?", true, 1);
        std::fs::remove_dir_all(&root).unwrap();
        out.state(Some(RingPhase::Speaking), 2);
        out.speech(Some(1), "still talking", true, 3);
    }
}
