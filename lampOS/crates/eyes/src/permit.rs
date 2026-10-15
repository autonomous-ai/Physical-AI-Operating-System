//! Whether the camera may be open: `{"camera":true}` on the permit socket
//! at least every [`STALE`], and `{"camera":false}` closes it at once.

use std::os::unix::net::UnixDatagram;
use std::sync::{Arc, Mutex};
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
///
/// A thread of its own waits on the socket and stamps each permission when it
/// arrives. A permission is as old as when it came, not as when the eyes got
/// round to it: one that queued up while the camera was blocked opening has
/// aged all that time and cannot renew a permission that has run out.
#[derive(Debug, Default)]
pub struct Permit {
    at: Arc<Mutex<Option<Instant>>>,
}

impl Permit {
    /// Hears the socket from now on. If the socket fails, nothing renews the
    /// permission and the camera closes once it has run out.
    pub fn listen(socket: UnixDatagram) -> std::io::Result<Self> {
        socket.set_nonblocking(false)?;
        let permit = Permit::default();
        let at = Arc::clone(&permit.at);
        std::thread::Builder::new()
            .name("permit".into())
            .spawn(move || {
                let mut datagram = [0u8; 64];
                while let Ok(length) = socket.recv(&mut datagram) {
                    let heard = match of(&datagram[..length]) {
                        Some(true) => Some(Instant::now()),
                        Some(false) => None,
                        None => continue,
                    };
                    *at.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = heard;
                }
            })?;
        Ok(permit)
    }

    /// Whether the camera may be open now.
    pub fn holds(&self) -> bool {
        self.at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some_and(|at| at.elapsed() < STALE)
    }

    /// A camera that has just opened, kept only if the permission still
    /// holds: an open can take longer than a permission lasts, and one taken
    /// back meanwhile closes the camera before any frame is read.
    pub fn opened<P>(&self, port: P, close: impl FnOnce(P)) -> Option<P> {
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

    /// How long the camera takes to open in these tests, unless said.
    const OPENING: Duration = Duration::from_millis(100);

    fn permitted() -> (Permit, UnixDatagram) {
        let (voice, eyes) = UnixDatagram::pair().unwrap();
        let permit = Permit::listen(eyes).unwrap();
        voice.send(br#"{"camera":true}"#).unwrap();
        let waited = Instant::now();
        while !permit.holds() {
            assert!(waited.elapsed() < Duration::from_secs(1), "never heard");
            std::thread::sleep(Duration::from_millis(1));
        }
        (permit, voice)
    }

    #[test]
    fn a_permission_taken_back_during_the_open_closes_the_camera() {
        let (permit, voice) = permitted();
        // The camera is opening; meanwhile the voice service says no.
        voice.send(br#"{"camera":false}"#).unwrap();
        std::thread::sleep(OPENING);
        let mut closed = false;
        assert_eq!(permit.opened("port", |_| closed = true), None);
        assert!(closed);
    }

    #[test]
    fn a_permission_run_out_during_the_open_closes_the_camera() {
        let (permit, _voice) = permitted();
        // An open of 1.6 s outlives a permission of 1.5 s.
        *permit.at.lock().unwrap() = Instant::now().checked_sub(Duration::from_millis(1_600));
        let mut closed = false;
        assert_eq!(permit.opened("port", |_| closed = true), None);
        assert!(closed);
    }

    #[test]
    fn a_renewal_sent_early_in_a_long_open_has_run_out_when_it_returns() {
        let (permit, voice) = permitted();
        // About 40 ms into an open of 1.6 s comes the last renewal: the voice
        // service then stops. When the open returns, that renewal is over
        // 1.5 s old, however late the eyes read it.
        std::thread::sleep(Duration::from_millis(40));
        voice.send(br#"{"camera":true}"#).unwrap();
        std::thread::sleep(Duration::from_millis(1_600));
        let mut closed = false;
        assert_eq!(permit.opened("port", |_| closed = true), None);
        assert!(closed);
    }

    #[test]
    fn a_permission_that_still_holds_keeps_the_camera_open() {
        let (permit, voice) = permitted();
        voice.send(br#"{"camera":true}"#).unwrap();
        std::thread::sleep(OPENING);
        let mut closed = false;
        assert_eq!(permit.opened("port", |_| closed = true), Some("port"));
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
