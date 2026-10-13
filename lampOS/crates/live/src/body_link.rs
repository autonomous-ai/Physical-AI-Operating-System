//! Tells the lamp's body what the lamp is doing: one small datagram, never
//! waited for. A body that is absent, slow or stopped changes nothing here,
//! and nothing comes back.
//!
//! The body is a separate process (`lamp-body` in `lamp-motor`) that owns the
//! servo port. This side only names a mood and a voice level; what moves, how
//! far and whether at all is decided there.
use crate::ring_wire::RingPhase;
use std::{io, os::unix::net::UnixDatagram, path::PathBuf};

/// The body's socket, when the service is to tell one. Unset: nothing is sent.
pub const ENVIRONMENT: &str = "LAMP_BODY_MOOD_SOCKET";
/// A change of mood is told at once; an unchanged one this often, so that
/// the voice level arrives and the body knows this side is alive.
const REPEAT_US: u64 = 40_000;

/// Turns in a row the lamp did not answer after which its head stops
/// lifting for what it hears: the talk around it is not for it.
const SETTLE_AFTER: u8 = 2;
/// Quiet after which that is forgotten and the next voice gets its head again.
const FORGET_US: u64 = 30_000_000;

/// Whether what the lamp has been hearing lately was for it. A lamp beside
/// two people talking to each other looks up at the first sentences and then
/// settles; it does not lift its head and look away at every one. The light
/// still shows that it hears, and it still answers when it is spoken to: the
/// answer brings the head back and clears this.
#[derive(Default)]
pub struct Attention {
    /// The turn in progress and whether the lamp has had anything to say in it.
    turn: Option<(u64, bool)>,
    unanswered: u8,
    ended_us: u64,
}

impl Attention {
    /// Call every tick with the turn that owns the lamp, if any, and whether
    /// an answer to it has arrived.
    pub fn observe(&mut self, turn: Option<(u64, bool)>, now_us: u64) {
        if self.turn.map(|(id, _)| id) != turn.map(|(id, _)| id)
            && let Some((_, answered)) = self.turn
        {
            self.unanswered = if answered {
                0
            } else {
                self.unanswered.saturating_add(1)
            };
            self.ended_us = now_us;
        }
        self.turn = turn;
        if turn.is_some_and(|(_, answered)| answered)
            || (turn.is_none() && now_us.saturating_sub(self.ended_us) >= FORGET_US)
        {
            self.unanswered = 0;
        }
    }

    /// The head stays down for listening and thinking.
    pub fn settled(&self) -> bool {
        self.unanswered >= SETTLE_AFTER
    }
}

pub struct BodyLink {
    socket: UnixDatagram,
    path: PathBuf,
    last: Option<(&'static str, u64)>,
}

impl BodyLink {
    pub fn from_environment() -> Option<Self> {
        let path = std::env::var_os(ENVIRONMENT).filter(|path| !path.is_empty())?;
        Self::to(path.into()).ok()
    }

    pub fn to(path: PathBuf) -> io::Result<Self> {
        let socket = UnixDatagram::unbound()?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            path,
            last: None,
        })
    }

    /// `phase` is the phase of the turn that owns the lamp, or none between
    /// turns. `level` is the voice in play, as for the ring's face.
    pub fn tell(&mut self, phase: Option<RingPhase>, level: u8, now_us: u64) {
        let mood = match phase {
            None => "resting",
            Some(RingPhase::Listening) => "listening",
            Some(RingPhase::Waiting) => "thinking",
            Some(RingPhase::Speaking) => "speaking",
        };
        if self
            .last
            .is_some_and(|(told, at)| told == mood && now_us.saturating_sub(at) < REPEAT_US)
        {
            return;
        }
        self.last = Some((mood, now_us));
        let message = format!(r#"{{"mood":"{mood}","level":{level}}}"#);
        // Nobody listening, a full queue, a missing path: all the same here.
        let _ = self.socket.send_to(message.as_bytes(), &self.path);
    }
}

/// The eyes' socket, when the service is to let a camera look. Unset:
/// nothing is sent and the eyes stay shut.
pub const EYES_ENVIRONMENT: &str = "LAMP_EYES_PERMIT_SOCKET";
/// Permission is repeated this often. The eyes close by themselves when it
/// has not come for a second and a half.
const PERMIT_REPEAT_US: u64 = 500_000;

/// Tells the lamp's eyes (`lamp-eyes`, a separate process that owns the
/// camera) whether they may be open: they may exactly while the microphone
/// is permitted. Yes is repeated; no is said once, at once. A service that
/// stops says nothing more, and the eyes close.
pub struct EyesLink {
    socket: UnixDatagram,
    path: PathBuf,
    last: Option<(bool, u64)>,
}

impl EyesLink {
    pub fn from_environment() -> Option<Self> {
        let path = std::env::var_os(EYES_ENVIRONMENT).filter(|path| !path.is_empty())?;
        Self::to(path.into()).ok()
    }

    pub fn to(path: PathBuf) -> io::Result<Self> {
        let socket = UnixDatagram::unbound()?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            path,
            last: None,
        })
    }

    pub fn permit(&mut self, allowed: bool, now_us: u64) {
        if self.last.is_some_and(|(told, at)| {
            told == allowed && (!allowed || now_us.saturating_sub(at) < PERMIT_REPEAT_US)
        }) {
            return;
        }
        self.last = Some((allowed, now_us));
        let message: &[u8] = if allowed {
            br#"{"camera":true}"#
        } else {
            br#"{"camera":false}"#
        };
        // Nobody listening, a full queue, a missing path: all the same here.
        let _ = self.socket.send_to(message, &self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eyes_are_told_yes_twice_a_second_and_no_once_at_once() {
        let path = std::env::temp_dir().join(format!("lamp-eyes-permit-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let eyes = UnixDatagram::bind(&path).unwrap();
        eyes.set_nonblocking(true).unwrap();
        let mut link = EyesLink::to(path.clone()).unwrap();
        // Every 10 ms for a second and a quarter while permitted, then not.
        for step in 0..125u64 {
            link.permit(true, step * 10_000);
        }
        for step in 125..200u64 {
            link.permit(false, step * 10_000);
        }
        link.permit(true, 2_000_000);
        let mut told = Vec::new();
        let mut datagram = [0u8; 64];
        while let Ok(length) = eyes.recv(&mut datagram) {
            told.push(String::from_utf8_lossy(&datagram[..length]).into_owned());
        }
        std::fs::remove_file(&path).unwrap();
        let (yes, no) = (r#"{"camera":true}"#, r#"{"camera":false}"#);
        assert_eq!(told, vec![yes, yes, yes, no, yes]);
    }

    /// One turn: heard, thought about, perhaps answered, then over.
    fn turn(attention: &mut Attention, now: &mut u64, id: u64, answered: bool) {
        for step in 0..3 {
            *now += 500_000;
            attention.observe(Some((id, answered && step == 2)), *now);
        }
        *now += 500_000;
        attention.observe(None, *now);
    }

    #[test]
    fn the_head_settles_after_two_turns_it_did_not_answer_and_comes_back_for_one_it_does() {
        let mut attention = Attention::default();
        let mut now = 0;
        assert!(!attention.settled());
        turn(&mut attention, &mut now, 1, false);
        assert!(!attention.settled(), "one sentence is not a conversation");
        turn(&mut attention, &mut now, 2, false);
        assert!(attention.settled());
        // Spoken to: the answer arrives and the head is free again at once,
        // while that turn is still in progress.
        attention.observe(Some((3, false)), now + 1);
        assert!(attention.settled());
        attention.observe(Some((3, true)), now + 2);
        assert!(!attention.settled());
        turn(&mut attention, &mut now, 3, true);
        assert!(!attention.settled());
        // One turn replacing another before any answer counts each of them.
        attention.observe(Some((4, false)), now + 3);
        attention.observe(Some((5, false)), now + 4);
        attention.observe(None, now + 5);
        assert!(attention.settled());
    }

    #[test]
    fn half_a_minute_of_quiet_gives_the_next_voice_the_head_again() {
        let mut attention = Attention::default();
        for id in 1..=3 {
            attention.observe(Some((id, false)), id * 1_000_000);
            attention.observe(None, id * 1_000_000 + 500_000);
        }
        assert!(attention.settled());
        attention.observe(None, 3_500_000 + FORGET_US - 1);
        assert!(attention.settled());
        attention.observe(None, 3_500_000 + FORGET_US);
        assert!(!attention.settled());
        // A long turn is not quiet.
        let mut attention = Attention::default();
        attention.observe(Some((1, false)), 0);
        attention.observe(None, 1);
        attention.observe(Some((2, false)), 2);
        attention.observe(Some((2, false)), 2 + 2 * FORGET_US);
        attention.observe(None, 3 + 2 * FORGET_US);
        assert!(attention.settled());
    }
    use std::sync::atomic::{AtomicU32, Ordering};

    fn listener() -> (UnixDatagram, PathBuf) {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "lamp-body-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let socket = UnixDatagram::bind(&path).unwrap();
        socket.set_nonblocking(true).unwrap();
        (socket, path)
    }

    fn received(socket: &UnixDatagram) -> Vec<String> {
        let mut messages = Vec::new();
        let mut bytes = [0u8; 128];
        while let Ok(length) = socket.recv(&mut bytes) {
            messages.push(String::from_utf8(bytes[..length].to_vec()).unwrap());
        }
        messages
    }

    #[test]
    fn each_mood_is_told_at_once_and_an_unchanged_one_only_now_and_then() {
        let (socket, path) = listener();
        let mut link = BodyLink::to(path.clone()).unwrap();
        link.tell(None, 0, 1_000);
        link.tell(Some(RingPhase::Listening), 90, 1_001);
        // The same mood again within the interval: not repeated.
        link.tell(Some(RingPhase::Listening), 200, 20_000);
        link.tell(Some(RingPhase::Listening), 210, 41_001);
        link.tell(Some(RingPhase::Waiting), 0, 41_002);
        link.tell(Some(RingPhase::Speaking), 255, 41_003);
        assert_eq!(
            received(&socket),
            [
                r#"{"mood":"resting","level":0}"#,
                r#"{"mood":"listening","level":90}"#,
                r#"{"mood":"listening","level":210}"#,
                r#"{"mood":"thinking","level":0}"#,
                r#"{"mood":"speaking","level":255}"#,
            ]
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_body_that_is_not_there_changes_nothing() {
        let (socket, path) = listener();
        drop(socket);
        std::fs::remove_file(&path).unwrap();
        let mut link = BodyLink::to(path).unwrap();
        for at in 0..100 {
            link.tell(Some(RingPhase::Speaking), 255, 1 + at * 50_000);
        }
    }
}
