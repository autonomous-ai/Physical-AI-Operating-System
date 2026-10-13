//! One picture from the lamp's eyes with what a person says.
//!
//! "What do you see in front of you?" was among the first things asked of
//! the lamp by people standing at it (2026-10-13), and it answered that it
//! could not see. Its eyes (`lamp-eyes --sight-file`) keep their newest
//! frame in a private file while the camera is open. While a person is
//! speaking this offers that frame to the model, at most once every four
//! seconds and twice a request, through the same `Supervisor::try_image` as
//! the camera context: inside the request's own activity, never replayed.
//! So one thing said gets one picture as it begins, a long one a second, and
//! talk that goes on and on around the lamp gets one every four seconds
//! rather than two with every pause in it.
//!
//! A frame older than a second is not offered: the eyes are closed, and the
//! model is told nothing rather than something stale. The eyes open only
//! while this service permits them, so a muted lamp has no file to read.
//! Pictures offered here go to the provider as the person's voice does;
//! nothing is kept on the lamp but that one newest frame.
use lamp_gemini::{Connect, MAX_IMAGE_BYTES, RequestId, Supervisor};
use std::{
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// The provider worker's flag, and where the coordinator reads the path.
pub const FLAG: &str = "--eyes-sight";
pub const ENVIRONMENT: &str = "LAMP_EYES_SIGHT_FILE";

const FRESH: Duration = Duration::from_secs(1);
/// The least time between two looks, whichever requests they fall in.
const BETWEEN_LOOKS: Duration = Duration::from_secs(4);
/// The provider takes two pictures a request.
const LOOKS_A_REQUEST: u8 = 2;
/// One standing permission: the eyes' own permit is what closes them.
const GRANT: u64 = 1;

pub struct Sight {
    path: PathBuf,
    request: Option<RequestId>,
    looked_at: Option<Instant>,
    looks: u8,
    frames: u64,
}

fn usable(path: &Path) -> bool {
    path.is_absolute()
        && path.as_os_str().len() <= 200
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

/// The provider worker's arguments for the path in the environment: none
/// when it is unset, so a worker started without it is started as before.
pub fn arguments_from_environment() -> Vec<String> {
    std::env::var(ENVIRONMENT)
        .ok()
        .filter(|path| usable(Path::new(path)))
        .map(|path| vec![FLAG.to_owned(), path])
        .unwrap_or_default()
}

impl Sight {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        if !usable(&path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the eyes' picture needs a short absolute path",
            ));
        }
        Ok(Self {
            path,
            request: None,
            looked_at: None,
            looks: 0,
            frames: 0,
        })
    }

    /// Take a leading `--eyes-sight PATH` off a worker's flags.
    pub fn take(flags: &[String]) -> io::Result<(Option<Self>, &[String])> {
        match flags {
            [flag, path, rest @ ..] if flag == FLAG => Ok((Some(Self::new(path.into())?), rest)),
            [flag] if flag == FLAG => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the eyes' picture needs a path",
            )),
            _ => Ok((None, flags)),
        }
    }

    /// Whether to look now, with this request's input open: when the last
    /// look, in whichever request, was four seconds ago or more, and this
    /// request has not had its two.
    fn due(&mut self, request: RequestId, now: Instant) -> bool {
        if self.request != Some(request) {
            self.request = Some(request);
            self.looks = 0;
        }
        let due = self.looks < LOOKS_A_REQUEST
            && self
                .looked_at
                .is_none_or(|at| now.saturating_duration_since(at) >= BETWEEN_LOOKS);
        if due {
            self.looks += 1;
            self.looked_at = Some(now);
        }
        due
    }

    /// The newest frame and its age, if the eyes are open.
    fn picture(&self, now: SystemTime) -> Result<(Vec<u8>, Duration), &'static str> {
        let about = std::fs::metadata(&self.path).map_err(|_| "the eyes have no picture")?;
        let age = about
            .modified()
            .ok()
            .and_then(|at| now.duration_since(at).ok())
            .unwrap_or_default();
        if age > FRESH {
            return Err("the eyes' picture is old");
        }
        if about.len() > MAX_IMAGE_BYTES as u64 {
            return Err("the eyes' picture is too large");
        }
        let jpeg = std::fs::read(&self.path).map_err(|_| "the eyes have no picture")?;
        Ok((jpeg, age))
    }

    /// Call every tick with the request whose input is open, if any.
    pub fn offer<C: Connect>(&mut self, request: Option<RequestId>, provider: &Supervisor<C>) {
        let Some(request) = request else { return };
        let now = Instant::now();
        if !self.due(request, now) {
            return;
        }
        let outcome = self.picture(SystemTime::now()).and_then(|(jpeg, age)| {
            self.frames += 1;
            let captured = now.checked_sub(age).unwrap_or(now);
            provider
                .try_image(request, self.frames, GRANT, captured, &jpeg)
                .map(|_| (jpeg.len(), age))
                .map_err(|_| "the provider did not take it")
        });
        match outcome {
            Ok((bytes, age)) => eprintln!(
                "lamp-live provider: sight: a picture of {bytes} bytes, {} ms old, went with request {} (look {})",
                age.as_millis(),
                request.get(),
                self.looks
            ),
            Err(why) => eprintln!(
                "lamp-live provider: sight: no picture with request {} (look {}): {why}",
                request.get(),
                self.looks
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(number: u64) -> RequestId {
        RequestId::new(number).unwrap()
    }

    #[test]
    fn the_path_travels_as_one_leading_flag_or_not_at_all() {
        let flags = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>();
        let given = flags(&[FLAG, "/run/lampos-eyes/sight.jpg", "--camera-context", "x"]);
        let (sight, rest) = Sight::take(&given).unwrap();
        assert_eq!(
            sight.unwrap().path,
            PathBuf::from("/run/lampos-eyes/sight.jpg")
        );
        assert_eq!(rest, &given[2..]);
        let none = flags(&["--camera-context", "x"]);
        let (sight, rest) = Sight::take(&none).unwrap();
        assert!(sight.is_none());
        assert_eq!(rest, &none[..]);
        assert!(Sight::take(&flags(&[FLAG])).is_err());
        assert!(Sight::take(&flags(&[FLAG, "sight.jpg"])).is_err());
        assert!(Sight::take(&flags(&[FLAG, "/run/../etc/passwd"])).is_err());
    }

    #[test]
    fn it_looks_as_someone_begins_and_then_no_more_than_every_four_seconds() {
        let mut sight = Sight::new("/run/x.jpg".into()).unwrap();
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // One thing said: a look as it begins, a second if it is long, no third.
        assert!(sight.due(request(5), at(0)));
        assert!(!sight.due(request(5), at(2)));
        assert!(!sight.due(request(5), at(3_999)));
        assert!(sight.due(request(5), at(4_000)));
        assert!(!sight.due(request(5), at(30_000)));
        // Talk that goes on in pieces: each new piece is a new request, and
        // none gets a look until four seconds after the last one.
        assert!(sight.due(request(6), at(30_001)));
        assert!(!sight.due(request(7), at(31_000)));
        assert!(!sight.due(request(8), at(33_000)));
        assert!(sight.due(request(9), at(34_001)));
        assert!(!sight.due(request(9), at(35_000)));
        assert_eq!(sight.looks, 1);
    }

    #[test]
    fn only_a_fresh_picture_of_a_size_the_provider_takes_is_read() {
        let directory = std::env::temp_dir().join(format!(
            "lamp-live-sight-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sight.jpg");
        let sight = Sight::new(path.clone()).unwrap();
        let now = SystemTime::now();
        assert_eq!(sight.picture(now), Err("the eyes have no picture"));
        std::fs::write(&path, b"frame").unwrap();
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        let (jpeg, age) = sight.picture(written + Duration::from_millis(300)).unwrap();
        assert_eq!(
            (jpeg.as_slice(), age),
            (&b"frame"[..], Duration::from_millis(300))
        );
        // A clock a moment behind the file's is no age, not an error.
        assert_eq!(
            sight.picture(written - Duration::from_millis(5)).unwrap().1,
            Duration::ZERO
        );
        assert_eq!(
            sight.picture(written + Duration::from_millis(1_001)),
            Err("the eyes' picture is old")
        );
        std::fs::write(&path, vec![0u8; MAX_IMAGE_BYTES + 1]).unwrap();
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(
            sight.picture(written),
            Err("the eyes' picture is too large")
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
