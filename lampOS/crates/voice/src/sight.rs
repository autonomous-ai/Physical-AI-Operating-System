//! One picture from the lamp's eyes with what a person says.
//!
//! "What do you see in front of you?" was among the first things asked of
//! the lamp by people standing at it (2026-10-13), and it answered that it
//! could not see. Its eyes (`lamp-eyes --sight-file`) keep their newest
//! frame in a private file while the camera is open. As a person begins to
//! speak this offers that frame to the model, through the same
//! `Supervisor::try_image` as the camera context: inside the request's own
//! activity, never replayed. It does so at most once every thirty seconds.
//!
//! Not with everything said, because a picture is not free. On lamp-4ace
//! seven ordinary questions in each of six fresh sessions began to be
//! answered a median 1.32 s after the end of speech was decided with no
//! picture (19 replies) and 1.78 s with one in every question (20 replies).
//! Half a second on every answer is too much to pay for something asked
//! about now and then. Every thirty seconds the lamp has a recent look at
//! the room in what the model remembers, and most answers pay nothing. What
//! this gives up: something held up to the lamp less than thirty seconds
//! after its last look is not seen until the next one.
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
/// The same file used another way: no picture goes with what is said, and
/// one goes with a request that is the lamp taking a look because it was
/// asked to (`ProviderInput::Written` with `look`). Ordinary answers then
/// carry no picture and pay nothing for one.
pub const LOOK_FLAG: &str = "--eyes-look";
pub const LOOK_ENVIRONMENT: &str = "LAMP_EYES_LOOK_FILE";

const FRESH: Duration = Duration::from_secs(1);
/// The least time between two looks, whichever requests they fall in.
const BETWEEN_LOOKS: Duration = Duration::from_secs(30);
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
    let named = |environment: &str, flag: &str| {
        std::env::var(environment)
            .ok()
            .filter(|path| usable(Path::new(path)))
            .map(|path| vec![flag.to_owned(), path])
            .unwrap_or_default()
    };
    let mut arguments = named(ENVIRONMENT, FLAG);
    arguments.extend(named(LOOK_ENVIRONMENT, LOOK_FLAG));
    arguments
}

/// Take a leading `--eyes-look PATH` off a worker's flags.
pub fn take_look(flags: &[String]) -> io::Result<(Option<PathBuf>, &[String])> {
    match flags {
        [flag, path, rest @ ..] if flag == LOOK_FLAG && usable(Path::new(path)) => {
            Ok((Some(path.into()), rest))
        }
        [flag, ..] if flag == LOOK_FLAG => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the eyes' picture needs a short absolute path",
        )),
        _ => Ok((None, flags)),
    }
}

/// The eyes' newest frame and its age, if they are open: a file written in
/// the last second, of a size the provider takes.
pub fn fresh(path: &Path, now: SystemTime) -> Result<(Vec<u8>, Duration), &'static str> {
    let about = std::fs::metadata(path).map_err(|_| "the eyes have no picture")?;
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
    let jpeg = std::fs::read(path).map_err(|_| "the eyes have no picture")?;
    Ok((jpeg, age))
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
    /// look, in whichever request, was thirty seconds ago or more, and this
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
        fresh(&self.path, now)
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
                "lamp-voice provider: sight: a picture of {bytes} bytes, {} ms old, went with request {} (look {})",
                age.as_millis(),
                request.get(),
                self.looks
            ),
            Err(why) => eprintln!(
                "lamp-voice provider: sight: no picture with request {} (look {}): {why}",
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
        // The look flag is its own, after it.
        let both = flags(&[LOOK_FLAG, "/run/lampos-eyes/sight.jpg", "--camera-context"]);
        let (look, rest) = take_look(&both).unwrap();
        assert_eq!(look, Some(PathBuf::from("/run/lampos-eyes/sight.jpg")));
        assert_eq!(rest, &both[2..]);
        assert_eq!(take_look(&none).unwrap(), (None, &none[..]));
        assert!(take_look(&flags(&[LOOK_FLAG])).is_err());
        assert!(take_look(&flags(&[LOOK_FLAG, "sight.jpg"])).is_err());
        assert!(Sight::take(&flags(&[FLAG, "sight.jpg"])).is_err());
        assert!(Sight::take(&flags(&[FLAG, "/run/../etc/passwd"])).is_err());
    }

    #[test]
    fn it_looks_as_someone_begins_and_then_no_more_than_every_thirty_seconds() {
        let mut sight = Sight::new("/run/x.jpg".into()).unwrap();
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // The first thing said gets a look as it begins, and no second one
        // however it goes on in the next half minute.
        assert!(sight.due(request(5), at(0)));
        assert!(!sight.due(request(5), at(2)));
        assert!(!sight.due(request(5), at(29_999)));
        // A conversation: the questions that follow get none until thirty
        // seconds after that look, and then the next one does.
        assert!(!sight.due(request(6), at(9_000)));
        assert!(!sight.due(request(7), at(18_000)));
        assert!(!sight.due(request(8), at(27_000)));
        assert!(sight.due(request(9), at(36_000)));
        assert!(!sight.due(request(10), at(45_000)));
        // Someone who speaks for a very long time gets a second, no third.
        assert!(sight.due(request(11), at(100_000)));
        assert!(sight.due(request(11), at(130_000)));
        assert!(!sight.due(request(11), at(170_000)));
        assert_eq!(sight.looks, 2);
    }

    #[test]
    fn only_a_fresh_picture_of_a_size_the_provider_takes_is_read() {
        let directory = std::env::temp_dir().join(format!(
            "lamp-voice-sight-{}-{:?}",
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
