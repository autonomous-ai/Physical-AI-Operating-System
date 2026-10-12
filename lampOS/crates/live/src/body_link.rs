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

#[cfg(test)]
mod tests {
    use super::*;
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
