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
/// A full look (#836), `on` only: as the lamp says it will look, the eyes
/// are asked for one larger still taken once the head has settled
/// (`lamp-eyes --full-look`), the look waits for it a little, and the model
/// is told to say what it cannot make out rather than guess. The worker is
/// told with a flag after the look's path.
pub const LOOK_FULL_ENVIRONMENT: &str = "LAMP_LOOK_FULL";
pub const LOOK_FULL_FLAG: &str = "--eyes-look-full";
/// How old a full look's still may be when the look goes: it is taken while
/// the lamp is still saying it will look, and the eyes keep it no longer.
pub const FULL_FRESH: Duration = Duration::from_secs(3);
/// How long the look waits for the still after the lamp said it would
/// look, at most: the eyes give it up after 2.5 s.
pub const STILL_WAIT: Duration = Duration::from_millis(2_600);

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
    let look = named(LOOK_ENVIRONMENT, LOOK_FLAG);
    let full = !look.is_empty() && look_full_on();
    arguments.extend(look);
    if full {
        arguments.push(LOOK_FULL_FLAG.to_owned());
    }
    arguments
}

/// Whether a look is a full look (`LAMP_LOOK_FULL=on`).
pub fn look_full_on() -> bool {
    std::env::var(LOOK_FULL_ENVIRONMENT).is_ok_and(|value| value == "on")
}

/// Take a leading `--eyes-look-full` off a worker's flags.
pub fn take_look_full(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == LOOK_FULL_FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// Whether the eyes have written their picture since `since`: the still a
/// full look asked for, or a working frame from eyes that keep one.
fn written_since(path: &Path, since: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|about| about.modified())
        .is_ok_and(|at| at >= since)
}

/// The coordinator's side of a look, full or not.
#[derive(Debug)]
pub struct Look {
    /// The eyes' picture, when a look is a full look.
    full: Option<PathBuf>,
    /// The turn whose still was asked for, and when.
    asked: Option<(u64, SystemTime)>,
}

impl Look {
    /// From `LAMP_LOOK_FULL` and the look's file, for a lamp that looks.
    pub fn from_environment() -> Self {
        let path = std::env::var_os(LOOK_ENVIRONMENT).map(PathBuf::from);
        Self::new(path.filter(|_| look_full_on()))
    }

    pub fn new(full: Option<PathBuf>) -> Self {
        Self { full, asked: None }
    }

    pub fn full(&self) -> bool {
        self.full.is_some()
    }

    /// The lamp said in `turn` that it will look: true once, when the eyes
    /// are to be asked for the still. Never without a full look.
    pub fn ask(&mut self, turn: u64, now: SystemTime) -> bool {
        if self.full.is_none() || self.asked.is_some_and(|(of, _)| of == turn) {
            return false;
        }
        self.asked = Some((turn, now));
        true
    }

    /// Whether the still has been written since it was asked for.
    pub fn still(&self) -> bool {
        self.full
            .as_deref()
            .zip(self.asked)
            .is_some_and(|(path, (_, since))| written_since(path, since))
    }

    /// Whether the look may go, `waited` after the lamp said it would look:
    /// at once without a full look; with one, once the still is there or
    /// after [`STILL_WAIT`].
    pub fn due(&self, waited: Duration) -> bool {
        self.full.is_none() || waited >= STILL_WAIT || self.still()
    }

    /// What the model is told with the picture.
    pub fn note(&self) -> &'static str {
        if self.full() {
            crate::speak_up::LOOK_FULL_NOTE
        } else {
            crate::speak_up::LOOK_NOTE
        }
    }
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
    fresh_within(path, now, FRESH)
}

/// The same, written in the last `within`.
pub fn fresh_within(
    path: &Path,
    now: SystemTime,
    within: Duration,
) -> Result<(Vec<u8>, Duration), &'static str> {
    let about = std::fs::metadata(path).map_err(|_| "the eyes have no picture")?;
    let age = about
        .modified()
        .ok()
        .and_then(|at| now.duration_since(at).ok())
        .unwrap_or_default();
    if age > within {
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

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "lamp-voice-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn without_a_full_look_a_look_goes_at_once_with_the_old_note_and_asks_nothing() {
        let mut look = Look::new(None);
        assert!(!look.full());
        assert!(!look.ask(7, SystemTime::now()));
        assert!(look.due(Duration::ZERO));
        assert!(!look.still());
        assert_eq!(look.note(), crate::speak_up::LOOK_NOTE);
        // And the worker is given no flag, so it reads as before.
        let flags = vec!["--camera-context".to_owned()];
        assert_eq!(take_look_full(&flags), (false, &flags[..]));
    }

    #[test]
    fn a_full_look_asks_once_and_waits_for_the_still_taken_after_it() {
        let directory = scratch("full-look");
        let path = directory.join("sight.jpg");
        // A frame from before the ask is not the still.
        std::fs::write(&path, b"before").unwrap();
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        let mut look = Look::new(Some(path.clone()));
        assert!(look.full());
        let asked = written + Duration::from_millis(5);
        assert!(look.ask(7, asked));
        assert!(!look.ask(7, asked + Duration::from_millis(40)));
        assert!(!look.still());
        assert!(!look.due(Duration::from_millis(900)));
        // The eyes write the still: the look goes.
        std::fs::write(&path, b"still").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(asked + Duration::from_millis(400))
            .unwrap();
        assert!(look.still());
        assert!(look.due(Duration::from_millis(900)));
        // The next look asks again.
        assert!(look.ask(9, asked + Duration::from_secs(10)));
        assert!(!look.still());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_full_look_without_a_still_goes_after_the_wait_and_is_told_not_to_guess() {
        let directory = scratch("full-look-none");
        let mut look = Look::new(Some(directory.join("sight.jpg")));
        assert!(look.ask(3, SystemTime::now()));
        assert!(!look.due(STILL_WAIT - Duration::from_millis(1)));
        assert!(look.due(STILL_WAIT));
        assert_eq!(look.note(), crate::speak_up::LOOK_FULL_NOTE);
        assert!(look.note().contains("hold it a bit closer"));
        assert!(look.note().contains("never guess"));
        assert!(!crate::speak_up::LOOK_NOTE.contains("closer"));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_full_look_s_still_may_be_older_than_a_working_frame() {
        let directory = scratch("full-fresh");
        let path = directory.join("sight.jpg");
        std::fs::write(&path, b"still").unwrap();
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        let later = written + Duration::from_millis(1_800);
        assert_eq!(fresh(&path, later), Err("the eyes' picture is old"));
        assert_eq!(
            fresh_within(&path, later, FULL_FRESH).unwrap().0,
            b"still".to_vec()
        );
        assert!(fresh_within(&path, written + Duration::from_millis(3_001), FULL_FRESH).is_err());
        let flags = vec![LOOK_FULL_FLAG.to_owned(), "--camera-context".to_owned()];
        assert_eq!(take_look_full(&flags), (true, &flags[1..]));
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
