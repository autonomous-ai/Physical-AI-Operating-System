//! Whether the camera may be open: `{"camera":true}` on the permit socket
//! at least every [`STALE`], and `{"camera":false}` closes it at once.

use std::os::unix::net::UnixDatagram;
use std::time::{Duration, Instant};

/// A permission older than this has run out.
pub const STALE: Duration = Duration::from_millis(1_500);

/// What the voice service says about the camera: `Some(true)` it may be
/// open, `Some(false)` it must close now. Anything else says nothing.
pub fn of(datagram: &[u8]) -> Option<bool> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Message {
        camera: bool,
    }
    serde_json::from_slice::<Message>(datagram)
        .ok()
        .map(|message| message.camera)
}

/// The newest permission heard, if it has not been taken back.
#[derive(Debug, Default)]
pub struct Permit {
    at: Option<Instant>,
}

impl Permit {
    /// Takes in what has arrived on the (non-blocking) socket.
    pub fn hear(&mut self, socket: &UnixDatagram) {
        let mut datagram = [0u8; 64];
        for _ in 0..32 {
            match socket.recv(&mut datagram) {
                Ok(length) => match of(&datagram[..length]) {
                    Some(true) => self.at = Some(Instant::now()),
                    Some(false) => self.at = None,
                    None => {}
                },
                Err(_) => break,
            }
        }
    }

    /// Whether the camera may be open now.
    pub fn holds(&self) -> bool {
        self.at.is_some_and(|at| at.elapsed() < STALE)
    }

    /// A camera that has just opened, kept only if the permission still
    /// holds: an open can take longer than a permission lasts, and one taken
    /// back meanwhile closes the camera before any frame is read.
    pub fn opened<P>(
        &mut self,
        socket: &UnixDatagram,
        port: P,
        close: impl FnOnce(P),
    ) -> Option<P> {
        self.hear(socket);
        if self.holds() {
            Some(port)
        } else {
            close(port);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permitted() -> (Permit, UnixDatagram, UnixDatagram) {
        let (voice, eyes) = UnixDatagram::pair().unwrap();
        eyes.set_nonblocking(true).unwrap();
        let mut permit = Permit::default();
        voice.send(br#"{"camera":true}"#).unwrap();
        permit.hear(&eyes);
        assert!(permit.holds());
        (permit, voice, eyes)
    }

    #[test]
    fn a_permission_taken_back_during_the_open_closes_the_camera() {
        let (mut permit, voice, eyes) = permitted();
        // The camera is opening; meanwhile the voice service says no.
        voice.send(br#"{"camera":false}"#).unwrap();
        let mut closed = false;
        assert_eq!(permit.opened(&eyes, "port", |_| closed = true), None);
        assert!(closed);
    }

    #[test]
    fn a_permission_run_out_during_the_open_closes_the_camera() {
        let (mut permit, _voice, eyes) = permitted();
        // An open of 1.6 s outlives a permission of 1.5 s.
        permit.at = Instant::now().checked_sub(Duration::from_millis(1_600));
        let mut closed = false;
        assert_eq!(permit.opened(&eyes, "port", |_| closed = true), None);
        assert!(closed);
    }

    #[test]
    fn a_permission_that_still_holds_keeps_the_camera_open() {
        let (mut permit, voice, eyes) = permitted();
        voice.send(br#"{"camera":true}"#).unwrap();
        let mut closed = false;
        assert_eq!(
            permit.opened(&eyes, "port", |_| closed = true),
            Some("port")
        );
        assert!(!closed);
    }

    #[test]
    fn a_permission_is_one_exact_object_and_anything_else_says_nothing() {
        assert_eq!(of(br#"{"camera":true}"#), Some(true));
        assert_eq!(of(br#"{"camera":false}"#), Some(false));
        for ignored in [
            &br#"{"camera":1}"#[..],
            br#"{"camera":true,"save":true}"#,
            br#"{"mood":"listening","level":3}"#,
            b"true",
            b"",
        ] {
            assert_eq!(of(ignored), None);
        }
    }
}
