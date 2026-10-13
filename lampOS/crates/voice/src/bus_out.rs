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
//!   it comes. Between things said the voice says once a second that it is
//!   still there, on this topic and the next.
//! - `voice/said`: the provider's text of what the lamp said, with the
//!   number of the request it answered.
//!
//! A fourth, `room/state` (quiet, busy or not known), comes from the
//! provider worker, which is where the room is judged
//! ([`crate::room_told`]): [`RoomOut`], given the bus's root by a flag,
//! since a worker is started with no environment.
//!
//! The text is what the provider wrote down, which is not always what was
//! said. It stays on this machine: the bus's root is a directory in memory
//! that only the lamp's own programs can enter.
use crate::ring_wire::RingPhase;
use lamp_bus::{Bus, Publisher};
use lamp_msgs::{Room, RoomState, Speech, Voice, VoiceState};
use std::{
    io,
    path::{Path, PathBuf},
};

/// Where the coordinator reads whether to publish. `on` only. The root is
/// the bus's own to say (`LAMP_BUS_ROOT`, or `/run/lampos-bus`).
pub const ENVIRONMENT: &str = "LAMP_BUS";
/// The name the voice publishes under.
pub const SOURCE: &str = "voice";
/// An unchanged state is said again this often.
const REPEAT_US: u64 = 1_000_000;

/// The provider worker's flag for the bus's root, followed by the path.
pub const ROOT_FLAG: &str = "--bus-root";

/// The provider worker's arguments for what the environment says: the
/// bus's root, if the voice publishes at all.
pub fn worker_arguments_from_environment() -> Vec<String> {
    worker_arguments(
        std::env::var(ENVIRONMENT).ok().as_deref(),
        std::env::var("LAMP_BUS_ROOT").ok().as_deref(),
    )
}

fn worker_arguments(switch: Option<&str>, root: Option<&str>) -> Vec<String> {
    if switch != Some("on") {
        return Vec::new();
    }
    let root = root
        .filter(|root| Path::new(root).is_absolute())
        .unwrap_or("/run/lampos-bus");
    vec![ROOT_FLAG.to_owned(), root.to_owned()]
}

/// Take a leading `--bus-root PATH` off a worker's flags.
pub fn take_root(flags: &[String]) -> io::Result<(Option<PathBuf>, &[String])> {
    match flags {
        [flag, root, rest @ ..] if flag == ROOT_FLAG => {
            if !Path::new(root).is_absolute() {
                return Err(io::Error::other("--bus-root must be absolute"));
            }
            Ok((Some(PathBuf::from(root)), rest))
        }
        [flag] if flag == ROOT_FLAG => Err(io::Error::other("--bus-root needs a path")),
        _ => Ok((None, flags)),
    }
}

/// How the room is, from the process that judges it.
pub struct RoomOut {
    room: Publisher,
    last: Option<(crate::room_told::Room, u64)>,
}

impl RoomOut {
    pub fn on(root: &Path) -> io::Result<Self> {
        let bus = Bus::new(root)?;
        Ok(Self {
            room: bus.publisher_declared(RoomState::TOPIC, &RoomState::declaration(), SOURCE)?,
            last: None,
        })
    }

    /// Call every tick: it publishes when the room changes, and otherwise
    /// says once a second that the voice is still there.
    pub fn tell(&mut self, room: crate::room_told::Room, now_us: u64) {
        match self.last {
            Some((told, _)) if told != room => {}
            None => {}
            Some((_, at)) => {
                if now_us.saturating_sub(at) >= REPEAT_US {
                    self.last = Some((room, now_us));
                    let _ = self.room.alive();
                }
                return;
            }
        }
        self.last = Some((room, now_us));
        let state = match room {
            crate::room_told::Room::Unknown => Room::Unknown,
            crate::room_told::Room::Quiet => Room::Quiet,
            crate::room_told::Room::Busy => Room::Busy,
        };
        let _ = self.room.publish_json(now_us, &RoomState { state });
    }
}

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
        // The two streams are silent between things said, for longer than
        // the bus's lease: without this their subscribers are told, each
        // time, that the voice has gone. On lamp-4ace on 2026-10-14 they
        // were, before and after every sentence.
        let _ = self.heard.alive();
        let _ = self.said.alive();
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
    fn a_silent_stream_is_not_taken_for_a_voice_that_has_gone() {
        let root = std::path::PathBuf::from(format!("/tmp/lvb-alive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap().lease(Duration::from_millis(300));
        let mut out = BusOut::on(&bus).unwrap();
        let mut heard = bus
            .subscribe_typed("voice/heard", "h", Speech::TYPE)
            .unwrap();
        // Nothing is said for four leases, and the state is told as it
        // would be once a second on a lamp, here every 100 ms.
        for tick in 0..12u64 {
            out.state(None, tick * REPEAT_US);
            std::thread::sleep(Duration::from_millis(100));
            assert!(heard.recv(Duration::from_millis(1)).unwrap().is_none());
            assert!(!heard.source_gone(), "tick {tick}");
        }
        // Without it, the lease runs out.
        std::thread::sleep(Duration::from_millis(700));
        let _ = heard.recv(Duration::from_millis(1)).unwrap();
        assert!(heard.source_gone());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_room_is_published_when_it_changes_and_the_worker_is_given_the_root_by_a_flag() {
        use crate::room_told::Room as Judged;
        let (bus, root) = bus("room");
        let mut out = RoomOut::on(&root).unwrap();
        let mut watch = bus
            .subscribe_typed(RoomState::TOPIC, "watch", RoomState::TYPE)
            .unwrap();
        std::thread::sleep(Duration::from_millis(250));
        let mut seen = Vec::new();
        for (room, at_us) in [
            (Judged::Unknown, 0),
            (Judged::Unknown, 2_000_000),
            (Judged::Busy, 2_100_000),
            (Judged::Busy, 9_000_000),
            (Judged::Quiet, 9_500_000),
        ] {
            out.tell(room, at_us);
            while let Some(message) = watch.recv(Duration::from_millis(30)).unwrap() {
                seen.push(message.parse::<RoomState>().unwrap().state);
            }
        }
        assert_eq!(seen, [Room::Unknown, Room::Busy, Room::Quiet]);
        std::fs::remove_dir_all(root).unwrap();

        assert!(worker_arguments(None, None).is_empty());
        assert!(worker_arguments(Some("yes"), Some("/x")).is_empty());
        assert_eq!(
            worker_arguments(Some("on"), None),
            [ROOT_FLAG, "/run/lampos-bus"]
        );
        assert_eq!(
            worker_arguments(Some("on"), Some("relative")),
            [ROOT_FLAG, "/run/lampos-bus"]
        );
        assert_eq!(
            worker_arguments(Some("on"), Some("/tmp/b")),
            [ROOT_FLAG, "/tmp/b"]
        );
        let flags = [ROOT_FLAG.to_owned(), "/tmp/b".to_owned(), "rest".to_owned()];
        let (taken, rest) = take_root(&flags).unwrap();
        assert_eq!((taken, rest), (Some(PathBuf::from("/tmp/b")), &flags[2..]));
        let (taken, rest) = take_root(&flags[2..]).unwrap();
        assert_eq!((taken, rest), (None, &flags[2..]));
        assert!(take_root(&[ROOT_FLAG.to_owned()]).is_err());
        assert!(take_root(&[ROOT_FLAG.to_owned(), "relative".to_owned()]).is_err());
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
