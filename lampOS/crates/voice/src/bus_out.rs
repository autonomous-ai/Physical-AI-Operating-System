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
//!   it comes, with the number of the turn whose input began last before
//!   it. Between things said the voice says once a second that it is still
//!   there, on this topic and the next.
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

    /// Call every tick: it publishes when the room changes, and once a
    /// second says it again. Said again and not only kept alive: a reader
    /// that acts on a quiet room (`docs/expression.md`) takes a judgement
    /// a few seconds old as no judgement, and the room is judged all the
    /// time whether or not it changes.
    pub fn tell(&mut self, room: crate::room_told::Room, now_us: u64) {
        if self
            .last
            .is_some_and(|(told, at)| told == room && now_us.saturating_sub(at) < REPEAT_US)
        {
            return;
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

/// Where the coordinator reads whether what the microphone hears goes on
/// the bus. `on` only, and only with [`ENVIRONMENT`] on as well. Its own
/// switch, off unless someone set it: this is the one topic that carries
/// sound, and with it any program of the lamp's group can hear the room.
pub const AUDIO_ENVIRONMENT: &str = "LAMP_BUS_AUDIO";
/// The samples of one message: 20 ms at 16 kHz, as the topic promises.
const AUDIO_BLOCK_BYTES: usize = 640;

/// What the microphone hears, for a part of the lamp that listens for
/// itself (`mic1/audio` or `mic2/audio`, `lamp_msgs::Audio`): the sound as
/// the voice hears it, after its echo canceller, 16 kHz, one channel,
/// 20 ms to a message. Only what the voice was given: with the microphone
/// closed there is nothing to publish, and nothing is.
pub struct MicOut {
    audio: Publisher,
    held: Vec<u8>,
    sequence: u64,
    alive_at_us: u64,
}

impl MicOut {
    /// The publisher for the microphone called `microphone` (`mic1`,
    /// `mic2`), if both switches say `on` and the bus can be used. A bus
    /// that cannot is said once and the voice goes on without.
    pub fn from_environment(microphone: &str) -> Option<Self> {
        let on = |name| std::env::var(name).ok().as_deref() == Some("on");
        if !(on(ENVIRONMENT) && on(AUDIO_ENVIRONMENT)) {
            return None;
        }
        match Bus::from_environment().and_then(|bus| Self::on(&bus, microphone)) {
            Ok(out) => Some(out),
            Err(error) => {
                eprintln!(
                    "lamp-voice: the microphone's sound is not published on the bus: {error}"
                );
                None
            }
        }
    }

    pub fn on(bus: &Bus, microphone: &str) -> io::Result<Self> {
        let topic = lamp_msgs::Audio::TOPICS
            .into_iter()
            .find(|topic| topic.strip_suffix("/audio") == Some(microphone))
            .ok_or_else(|| io::Error::other("no audio topic for that microphone"))?;
        Ok(Self {
            audio: bus.publisher_declared(topic, &lamp_msgs::Audio::declaration(), SOURCE)?,
            held: Vec::with_capacity(AUDIO_BLOCK_BYTES * 2),
            sequence: 0,
            alive_at_us: 0,
        })
    }

    /// One block the voice was given, 10 ms of it. Every second block
    /// makes a message. Nothing waits for a listener, and a bus that
    /// fails costs the sound of that message and nothing else.
    pub fn block(&mut self, samples: &[i16], now_us: u64) {
        for sample in samples {
            self.held.extend_from_slice(&sample.to_le_bytes());
        }
        while self.held.len() >= AUDIO_BLOCK_BYTES {
            let header = lamp_msgs::Audio {
                sample_rate: 16_000,
                channels: 1,
                sample_format: lamp_msgs::Audio::S16LE.into(),
                sequence: self.sequence,
            };
            self.sequence += 1;
            if let Ok(payload) = header.block(&self.held[..AUDIO_BLOCK_BYTES]) {
                let _ = self
                    .audio
                    .publish_bytes(now_us, lamp_msgs::Audio::ENCODING, &payload);
            }
            self.held.drain(..AUDIO_BLOCK_BYTES);
            self.alive_at_us = now_us;
        }
    }

    /// Call every tick. With the microphone closed nothing is published,
    /// and this is what says the voice is still there.
    pub fn tick(&mut self, now_us: u64) {
        if now_us.saturating_sub(self.alive_at_us) >= 1_000_000 {
            self.alive_at_us = now_us;
            let _ = self.audio.alive();
        }
    }
}

/// The lamp's name, heard in the room's sound by the part that listens
/// for it (`name/heard`, `docs/name.md`): for the rule that decides
/// whether an answer is played (`crate::attention`).
pub struct NameIn {
    heard: lamp_bus::Subscriber,
    looked_at_us: Option<u64>,
}

/// The bus is looked at for a hearing this often. A hearing is said a
/// tenth of a second or more after the name began, and it is dated from
/// when it began, so this adds nothing that matters to the rule.
const NAME_LOOK_EVERY_US: u64 = 20_000;

impl NameIn {
    pub fn on(root: &Path) -> io::Result<Self> {
        Self::on_topic(root, lamp_msgs::NameHeard::TOPIC)
    }

    /// The same for `topic`: `name/heard`, each hearing when it is over, or
    /// `name/rising`, its first sign as the name is said (#782).
    pub fn on_topic(root: &Path, topic: &str) -> io::Result<Self> {
        Ok(Self {
            heard: Bus::new(root)?.subscribe_typed(topic, SOURCE, lamp_msgs::NameHeard::TYPE)?,
            looked_at_us: None,
        })
    }

    /// The subscriber, if the bus is switched on and can be used. A bus
    /// that cannot is said once and the voice goes on without.
    pub fn from_environment() -> Option<Self> {
        Self::from_environment_on(lamp_msgs::NameHeard::TOPIC)
    }

    /// The same for `topic`.
    pub fn from_environment_on(topic: &str) -> Option<Self> {
        if std::env::var(ENVIRONMENT).ok().as_deref() != Some("on") {
            return None;
        }
        let root = std::env::var_os(lamp_bus::ROOT_VARIABLE)
            .filter(|root| !root.is_empty())
            .unwrap_or_else(|| lamp_bus::DEFAULT_ROOT.into());
        match Self::on_topic(Path::new(&root), topic) {
            Ok(names) => Some(names),
            Err(error) => {
                eprintln!("lamp-voice: the lamp's name is not taken from the bus: {error}");
                None
            }
        }
    }

    /// When the newest hearing since the last look began, by the clock
    /// `now_us` is of, if there was one. Never waits, and looks at the
    /// bus at most every 20 ms however often it is called.
    pub fn newest(&mut self, now_us: u64) -> Option<u64> {
        self.newest_scored(now_us).map(|(began, _)| began)
    }

    /// The same, with the hearing's score.
    pub fn newest_scored(&mut self, now_us: u64) -> Option<(u64, f32)> {
        if self
            .looked_at_us
            .is_some_and(|at| now_us.saturating_sub(at) < NAME_LOOK_EVERY_US)
        {
            return None;
        }
        self.looked_at_us = Some(now_us);
        let mut newest: Option<(u64, f32)> = None;
        for _ in 0..8 {
            let Ok(Some(message)) = self.heard.recv(std::time::Duration::ZERO) else {
                break;
            };
            if let Ok(heard) = message.parse::<lamp_msgs::NameHeard>() {
                let began = now_us.saturating_sub(u64::from(heard.ago_ms) * 1_000);
                if newest.is_none_or(|(before, _)| began >= before) {
                    newest = Some((began, heard.score));
                }
            }
        }
        newest
    }
}

/// Whether the eyes see someone turned to the lamp now
/// (`humans/faces/tracked`, `docs/eyes.md`): true when a face in their
/// newest look is, false when that look saw nobody turned to it, nothing
/// when they have said nothing still fresh (not running, the camera closed,
/// the look older than its two seconds).
pub fn someone_turned_to_lamp(bus: &Bus) -> Option<bool> {
    let message = bus.latest(lamp_msgs::Faces::TOPIC).ok()??;
    turned_in(&message)
}

fn turned_in(message: &lamp_bus::Message) -> Option<bool> {
    if message.expired {
        return None;
    }
    let faces = message.parse::<lamp_msgs::Faces>().ok()?;
    Some(faces.faces.iter().any(|face| face.facing == Some(true)))
}

/// The link to the speech service may be down this long before the voice
/// says anything is the matter. It is down for a second or two each time
/// the voice starts and each time the service renews its session, which
/// is the link working as it should.
pub const LINK_GRACE_US: u64 = 5_000_000;

/// The voice's own report (`diagnostics`): well while it can reach the
/// speech service, or has only just lost it.
pub fn health(link_up: bool, down_for_us: u64) -> lamp_msgs::Diagnostic {
    use lamp_msgs::{Diagnostic, Level};
    if link_up {
        Diagnostic::ok(SOURCE).with("link", "ready")
    } else if down_for_us < LINK_GRACE_US {
        Diagnostic::ok(SOURCE).with("link", "connecting")
    } else {
        Diagnostic::new(
            Level::Warn,
            SOURCE,
            "it cannot reach the speech service, and keeps trying",
        )
        .with("link", "down")
        .with("down_for_s", down_for_us / 1_000_000)
    }
}

pub struct BusOut {
    state: Publisher,
    heard: Publisher,
    said: Publisher,
    health: Publisher,
    last: Option<(Voice, u64)>,
    /// When the voice last said how it is, and since when its link to the
    /// speech service has been down.
    reported_at_us: Option<u64>,
    down_since_us: Option<u64>,
    /// The turn whose input began last, if one has: what is heard is told
    /// as that turn's.
    heard_in: Option<u64>,
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
            health: bus.publisher_declared(
                lamp_msgs::Diagnostic::TOPIC,
                &lamp_msgs::Diagnostic::declaration(),
                SOURCE,
            )?,
            last: None,
            reported_at_us: None,
            down_since_us: None,
            heard_in: None,
        })
    }

    /// The service began to take in the person's words for turn `request`.
    pub fn input_started(&mut self, request: u64) {
        self.heard_in = Some(request);
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

    /// Whether the voice can reach the speech service now. Call every
    /// tick: how the voice is, is said a little more often than once a
    /// second, for the lamp's health.
    pub fn report(&mut self, link_up: bool, now_us: u64) {
        if link_up {
            self.down_since_us = None;
        } else {
            self.down_since_us.get_or_insert(now_us);
        }
        if self
            .reported_at_us
            .is_some_and(|at| now_us.saturating_sub(at) < 950_000)
        {
            return;
        }
        self.reported_at_us = Some(now_us);
        let down_for = self
            .down_since_us
            .map_or(0, |since| now_us.saturating_sub(since));
        let _ = self.health.publish_json(now_us, &health(link_up, down_for));
    }

    /// A piece of the provider's text of what a person said, with the turn
    /// it was heard in: the one whose input began last
    /// ([`Self::input_started`]), if one has. The service does not say which
    /// turn its text of what was heard is of; a piece it writes only after
    /// the next turn's input began is taken as that next turn's. The
    /// Policy's decision about the words carries the number, and the lamp's
    /// actions are paired with it (`crate::policy_holds`).
    pub fn heard(&mut self, text: &str, finished: bool, now_us: u64) {
        let _ = self
            .heard
            .publish_json(now_us, &speech(self.heard_in, text, finished));
    }

    /// A piece of the provider's text of the lamp's answer to `request`.
    pub fn said(&mut self, request: u64, text: &str, finished: bool, now_us: u64) {
        let _ = self
            .said
            .publish_json(now_us, &speech(Some(request), text, finished));
    }
}

fn speech(request: Option<u64>, text: &str, finished: bool) -> Speech {
    Speech {
        text: text.to_owned(),
        finished,
        locale: None,
        request,
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
    fn someone_is_turned_to_the_lamp_only_when_the_eyes_newest_fresh_look_says_so() {
        use lamp_msgs::{Face, Faces};
        let (bus, root) = bus("faces");
        // No eyes: nothing known.
        assert_eq!(someone_turned_to_lamp(&bus), None);
        let mut eyes = bus
            .publisher_declared(Faces::TOPIC, &Faces::declaration(), "eyes")
            .unwrap();
        eyes.expires_after(Some(Duration::from_millis(300)));
        let face = |facing: Option<bool>| Face {
            id: "f1".into(),
            xmin: 0.4,
            ymin: 0.3,
            xmax: 0.6,
            ymax: 0.6,
            confidence: 1.0,
            yaw: facing.map(|_| 0.1),
            facing,
        };
        let say = |eyes: &mut lamp_bus::Publisher, faces: Vec<Face>| {
            eyes.publish_json(lamp_ipc::monotonic_us(), &Faces { faces })
                .unwrap();
        };
        say(&mut eyes, vec![]);
        assert_eq!(someone_turned_to_lamp(&bus), Some(false));
        say(&mut eyes, vec![face(Some(false)), face(None)]);
        assert_eq!(someone_turned_to_lamp(&bus), Some(false));
        say(&mut eyes, vec![face(None), face(Some(true))]);
        assert_eq!(someone_turned_to_lamp(&bus), Some(true));
        // A look older than its life says nothing.
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(someone_turned_to_lamp(&bus), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_voice_is_well_while_it_can_reach_the_service_or_has_only_just_lost_it() {
        use lamp_msgs::{Diagnostic, Level};
        let facts = |report: &Diagnostic| -> Vec<(String, String)> {
            report
                .values
                .iter()
                .map(|fact| (fact.key.clone(), fact.value.clone()))
                .collect()
        };
        let up = health(true, 0);
        assert_eq!(
            (up.level, up.name.as_str(), up.message.as_str()),
            (Level::Ok, "voice", "")
        );
        assert_eq!(facts(&up), [("link".to_string(), "ready".to_string())]);
        // Down for a moment, as at every start and every renewal: still well.
        let renewing = health(false, LINK_GRACE_US - 1);
        assert_eq!(renewing.level, Level::Ok);
        assert_eq!(
            facts(&renewing),
            [("link".to_string(), "connecting".to_string())]
        );
        // Down for longer: said, with for how long.
        let down = health(false, 42_500_000);
        assert_eq!(
            (down.level, down.message.as_str()),
            (
                Level::Warn,
                "it cannot reach the speech service, and keeps trying"
            )
        );
        assert_eq!(
            facts(&down),
            [
                ("link".to_string(), "down".to_string()),
                ("down_for_s".to_string(), "42".to_string())
            ]
        );
        // On the bus: a little more often than once a second, and the
        // time the link has been down is counted from when it went.
        let (bus, root) = bus("health");
        let mut hears = bus
            .subscribe_typed(Diagnostic::TOPIC, "test", Diagnostic::TYPE)
            .unwrap();
        let mut out = BusOut::on(&bus).unwrap();
        // Nine seconds of ticks, 2 ms apart: up for one, then down.
        for tick in 0..4_500u64 {
            out.report(tick < 500, tick * 2_000);
        }
        let mut said = Vec::new();
        while let Some(message) = hears.recv(Duration::from_millis(50)).unwrap() {
            said.push(message.parse::<Diagnostic>().unwrap());
        }
        assert_eq!(said.len(), 10, "{said:?}");
        assert_eq!(said[0].level, Level::Ok);
        assert_eq!(facts(&said[1])[0].1, "ready");
        assert_eq!(facts(&said[2])[0].1, "connecting");
        assert_eq!(said[5].level, Level::Ok);
        assert_eq!(said[7].level, Level::Warn);
        assert_eq!(
            facts(&said[9])[1],
            ("down_for_s".to_string(), "7".to_string())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_hearing_of_the_name_is_taken_from_the_bus_dated_from_when_it_began() {
        use lamp_msgs::NameHeard;
        let (bus, root) = bus("name");
        let mut names = NameIn::on(&root).unwrap();
        // Nothing heard: nothing, at once.
        assert_eq!(names.newest(5_000_000), None);
        let mut says = bus
            .publisher_declared(NameHeard::TOPIC, &NameHeard::declaration(), "name")
            .unwrap();
        let heard = |score, ago_ms| NameHeard { score, ago_ms };
        says.publish_json(1, &heard(0.95, 400)).unwrap();
        let since = std::time::Instant::now();
        let mut now = 9_000_000;
        let began = loop {
            now += NAME_LOOK_EVERY_US;
            if let Some(began) = names.newest(now) {
                break began;
            }
            assert!(since.elapsed() < WAIT * 4);
            std::thread::sleep(Duration::from_millis(2));
        };
        // 400 ms before it was taken up.
        assert_eq!(began, now - 400_000);
        // Two that came together: the one that began latest.
        says.publish_json(2, &heard(0.91, 900)).unwrap();
        says.publish_json(3, &heard(0.99, 100)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        // Asked again within 20 ms of the last look: the bus is not
        // looked at, and nothing is lost by that.
        assert_eq!(names.newest(now + NAME_LOOK_EVERY_US - 1), None);
        assert_eq!(names.newest_scored(20_000_000), Some((19_900_000, 0.99)));
        assert_eq!(names.newest(21_000_000), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn what_the_microphone_hears_goes_out_twenty_milliseconds_to_a_message() {
        use lamp_msgs::Audio;
        let (bus, root) = bus("mic");
        let mut hears = bus
            .subscribe_typed("mic2/audio", "test", Audio::TYPE)
            .unwrap();
        let mut out = MicOut::on(&bus, "mic2").unwrap();
        assert_eq!(
            bus.declaration("mic2/audio").unwrap(),
            Some(Audio::declaration())
        );
        // Five blocks of 10 ms, each a ramp of its own: two messages, and
        // half of a third kept for the next block.
        for block in 0..5i16 {
            let samples: Vec<i16> = (0..160).map(|n| block * 1_000 + n - 80).collect();
            out.block(&samples, 10_000 * u64::from(block as u16));
        }
        for expected in 0..2u64 {
            let message = hears.recv(WAIT).unwrap().expect("a message");
            assert_eq!(message.header.enc, Audio::ENCODING);
            assert_eq!(message.header.source, "voice");
            let (header, samples) = Audio::split(&message.payload).unwrap();
            assert_eq!(
                header,
                Audio {
                    sample_rate: 16_000,
                    channels: 1,
                    sample_format: "S16LE".into(),
                    sequence: expected,
                }
            );
            assert_eq!(samples.len(), 640);
            // The samples as they were, low byte first, in order.
            let first = i16::from_le_bytes([samples[0], samples[1]]);
            let last = i16::from_le_bytes([samples[638], samples[639]]);
            let block = expected as i16 * 2;
            assert_eq!(
                (first, last),
                (block * 1_000 - 80, (block + 1) * 1_000 + 79)
            );
        }
        assert!(hears.recv(Duration::from_millis(50)).unwrap().is_none());
        out.block(&[7; 160], 60_000);
        let third = hears.recv(WAIT).unwrap().expect("the third");
        assert_eq!(Audio::split(&third.payload).unwrap().0.sequence, 2);
        // A microphone the bus has no topic for is refused, and with the
        // bus gone nothing fails.
        assert!(MicOut::on(&bus, "mic3").is_err());
        std::fs::remove_dir_all(root).unwrap();
        out.block(&[0; 320], 80_000);
        out.tick(2_000_000);
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
        // Before any turn's input began, then in turn 7's, then in 8's.
        out.heard("Hm.", true, 1);
        out.input_started(7);
        out.heard("How do I take my coffee?", true, 2);
        out.said(7, "Black.", false, 3);
        out.input_started(8);
        out.heard("Thanks.", true, 4);
        let before: Speech = heard.recv(WAIT).unwrap().unwrap().parse().unwrap();
        assert_eq!((before.text.as_str(), before.request), ("Hm.", None));
        let person: Speech = heard.recv(WAIT).unwrap().unwrap().parse().unwrap();
        assert_eq!(
            (person.text.as_str(), person.finished, person.request),
            ("How do I take my coffee?", true, Some(7))
        );
        let next: Speech = heard.recv(WAIT).unwrap().unwrap().parse().unwrap();
        assert_eq!((next.text.as_str(), next.request), ("Thanks.", Some(8)));
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
    fn the_room_is_published_when_it_changes_and_every_second_and_the_worker_is_given_the_root_by_a_flag()
     {
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
            // Within the second: not said again.
            (Judged::Unknown, 900_000),
            // A change, at once.
            (Judged::Busy, 950_000),
            (Judged::Busy, 1_900_000),
            // A second on with no change: said again, for a reader that
            // takes an old judgement as none.
            (Judged::Busy, 1_950_000),
            (Judged::Quiet, 2_000_000),
        ] {
            out.tell(room, at_us);
            while let Some(message) = watch.recv(Duration::from_millis(30)).unwrap() {
                seen.push(message.parse::<RoomState>().unwrap().state);
            }
        }
        assert_eq!(seen, [Room::Unknown, Room::Busy, Room::Busy, Room::Quiet]);
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
        out.heard("anyone?", true, 1);
        std::fs::remove_dir_all(&root).unwrap();
        out.state(Some(RingPhase::Speaking), 2);
        out.said(1, "still talking", true, 3);
    }
}
