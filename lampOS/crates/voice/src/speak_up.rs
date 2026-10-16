//! Speaking first: the lamp says hello to someone who comes and sits down.
//!
//! Everything else the lamp says is an answer. This is the one thing it
//! says unasked, and it is meant to be as small as what a person at the
//! next desk does when a colleague comes back: look up, a word or two, and
//! nothing more. It does not announce itself to a room, greet people
//! walking past, or speak into a conversation.
//!
//! The eyes (`lamp-eyes --presence-file`) keep a small file fresh while a
//! face is near; its text is how large the face is in the picture. From
//! that, [`Arrival`] decides. The lamp speaks when all of these hold:
//!
//! - someone has been there for three seconds without a break;
//! - before that nobody was, for five minutes or more (a face lost for
//!   under twenty seconds is someone who looked away, not someone who left);
//! - nothing has been said to the lamp or by it for half a minute;
//! - it has not greeted anyone in the last twenty minutes.
//!
//! What it then says is the model's: the turn is a written note in place of
//! speech (`ProviderInput::Written`), not words chosen here.
use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

/// Where the coordinator reads the path of the eyes' presence file. Unset:
/// the lamp never speaks first and nothing here runs.
pub const ENVIRONMENT: &str = "LAMP_EYES_PRESENCE_FILE";
/// `on` for the lamp to say its hello. Anything else, with the file set:
/// the moments it would have are recorded and nothing is said, which is how
/// the rule is watched in a room before it is given a voice.
pub const SPEAK_ENVIRONMENT: &str = "LAMP_SPEAK_FIRST";

/// What the model is given in place of something heard.
pub const NOTE: &str = "(Nobody has spoken. Someone has just come and sat down in front of you after being away for a while. Say hello the way a friend at the next desk would: a few warm words and no question they have to answer.)";

/// What the model is given with the picture when the lamp takes a look,
/// and without one when its eyes are shut.
pub const LOOK_NOTE: &str = "(You are looking now, and this is what your eye sees. Answer what they just asked you in a sentence or two, the way you would tell a friend what you see. It is not a picture or a camera to you: it is what you see.)";
/// The same with a full look (`LAMP_LOOK_FULL=on`, #836): what the lamp
/// cannot make out it says, rather than guess.
pub const LOOK_FULL_NOTE: &str = "(You are looking now, and this is what your eye sees. Answer what they just asked you in a sentence or two, the way you would tell a friend what you see. It is not a picture or a camera to you: it is what you see. If you cannot make out what they are showing you, say so plainly and ask them to hold it a bit closer to you; never guess what it might be.)";
/// The written turn that asks again when what was heard in a conversation
/// made no sense in it (#926): another script, or a number alone. On
/// lamp-4ace (09:42) the model took a softer note as its cue to go on with
/// the story it had been cut off in, so this one says that is over.
pub const ASK_AGAIN_NOTE: &str = "(The person just cut in and you could not make out what they said: it was misheard. What you were saying before is over: do not go on with it, do not finish it, do not sum it up, and do not answer what was misheard. Say only one word, in the language of the conversation, as a question: \"Sorry?\" Then stop and listen.)";
pub const NO_LOOK_NOTE: &str = "(You tried to look, but your eye is shut just now and you saw nothing. Tell them so in a few light words.)";

/// Whether an answer that opens like this is the lamp saying it will look:
/// its own words are how it asks for its eyes. Only "let me look" and its
/// two longer forms; "let me see" is something people say before any answer.
pub fn asks_to_look(opening: &str) -> bool {
    let plain: String = opening
        .chars()
        .map(|c| {
            if c.is_alphabetic() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    let words: Vec<&str> = plain.split_whitespace().collect();
    [
        &["let", "me", "look"][..],
        &["let", "me", "take", "a", "look"][..],
        &["let", "me", "have", "a", "look"][..],
    ]
    .iter()
    .any(|phrase| words.len() >= phrase.len() && words[..phrase.len()] == **phrase)
}

const THERE_FOR_US: u64 = 3_000_000;
const AWAY_FOR_US: u64 = 300_000_000;
const LOOKED_AWAY_US: u64 = 20_000_000;
const QUIET_FOR_US: u64 = 30_000_000;
const AT_MOST_EVERY_US: u64 = 1_200_000_000;
/// The file is read this often, and counts while it is this fresh.
const READ_EVERY_US: u64 = 250_000;
const FRESH: Duration = Duration::from_millis(1_500);
/// A face smaller than this share of the picture's height is across the
/// room. People at the lamp were 0.12 to 0.27 on lamp-4ace.
const NEAR: f32 = 0.14;

pub struct Arrival {
    speaks: bool,
    path: Option<PathBuf>,
    read_us: u64,
    near: bool,
    /// Since when someone has been there without a break.
    there_since: Option<u64>,
    /// When a face was last seen.
    seen_us: Option<u64>,
    /// Since when nobody has been there; none while unknown or while someone is.
    gone_since: Option<u64>,
    /// How long nobody had been there when the person now there arrived.
    away_us: u64,
    greeted_us: Option<u64>,
    greeted_this_arrival: bool,
}

impl Arrival {
    pub fn from_environment(now_us: u64) -> Option<Self> {
        let path = std::env::var_os(ENVIRONMENT).filter(|path| !path.is_empty())?;
        let path = PathBuf::from(path);
        let speaks = std::env::var(SPEAK_ENVIRONMENT).is_ok_and(|value| value == "on");
        path.is_absolute().then(|| Self {
            speaks,
            ..Self::new(Some(path), now_us)
        })
    }

    /// Whether a hello that is due is said, or only recorded.
    pub fn speaks(&self) -> bool {
        self.speaks
    }

    fn new(path: Option<PathBuf>, now_us: u64) -> Self {
        Self {
            speaks: false,
            path,
            read_us: 0,
            near: false,
            there_since: None,
            seen_us: None,
            // Nobody is known to be there when the service starts; whoever
            // is turns up in the first read and was never away.
            gone_since: Some(now_us),
            away_us: 0,
            greeted_us: None,
            greeted_this_arrival: false,
        }
    }

    /// Whether the eyes have a near face now, from their file.
    fn near_now(&mut self, now_us: u64) -> bool {
        if now_us.saturating_sub(self.read_us) < READ_EVERY_US {
            return self.near;
        }
        self.read_us = now_us;
        self.near = self.path.as_ref().is_some_and(|path| {
            let fresh = std::fs::metadata(path)
                .and_then(|about| about.modified())
                .ok()
                .and_then(|at| SystemTime::now().duration_since(at).ok())
                .is_some_and(|age| age < FRESH);
            fresh
                && std::fs::read_to_string(path)
                    .ok()
                    .and_then(|text| text.trim().parse::<f32>().ok())
                    .is_some_and(|size| size.is_finite() && size >= NEAR)
        });
        self.near
    }

    /// Call every tick: takes in whether someone is there.
    pub fn watch(&mut self, now_us: u64) {
        let near = self.near_now(now_us);
        self.note(near, now_us);
    }

    fn note(&mut self, near: bool, now_us: u64) {
        if near {
            if self.there_since.is_none() {
                // An arrival, unless it is the same person looking back.
                if let Some(gone) = self.gone_since.take() {
                    self.away_us = now_us.saturating_sub(gone);
                    self.greeted_this_arrival = false;
                }
                self.there_since = Some(now_us);
            }
            self.seen_us = Some(now_us);
        } else {
            self.there_since = None;
            if self.gone_since.is_none()
                && self
                    .seen_us
                    .is_none_or(|seen| now_us.saturating_sub(seen) >= LOOKED_AWAY_US)
            {
                // Gone since they were last seen, not since this was noticed.
                self.gone_since = Some(self.seen_us.unwrap_or(now_us));
            }
        }
    }

    /// Whether the lamp should say hello now. `quiet_us` is how long it has
    /// been since anything was said to the lamp or by it. Nothing changes
    /// until [`Self::said`]: a hello that could not be begun is still due.
    pub fn due(&self, quiet_us: u64, now_us: u64) -> bool {
        !self.greeted_this_arrival
            && self
                .there_since
                .is_some_and(|since| now_us.saturating_sub(since) >= THERE_FOR_US)
            && self.away_us >= AWAY_FOR_US
            && quiet_us >= QUIET_FOR_US
            && self
                .greeted_us
                .is_none_or(|at| now_us.saturating_sub(at) >= AT_MOST_EVERY_US)
    }

    /// Whether someone has been at the lamp for a moment now.
    pub fn present(&self, now_us: u64) -> bool {
        self.there_since
            .is_some_and(|since| now_us.saturating_sub(since) >= THERE_FOR_US)
    }

    /// The hello has been begun.
    pub fn said(&mut self, now_us: u64) {
        self.greeted_this_arrival = true;
        self.greeted_us = Some(now_us);
    }

    #[cfg(test)]
    fn decide(&mut self, near: bool, quiet_us: u64, now_us: u64) -> bool {
        self.note(near, now_us);
        let due = self.due(quiet_us, now_us);
        if due {
            self.said(now_us);
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;
    const QUIET: u64 = 60 * S;

    /// Run `seconds` of ticks with a face there or not; whether it greeted.
    fn pass(arrival: &mut Arrival, now: &mut u64, seconds: u64, near: bool, quiet: u64) -> u32 {
        let mut greetings = 0;
        for _ in 0..seconds * 10 {
            *now += S / 10;
            greetings += u32::from(arrival.decide(near, quiet, *now));
        }
        greetings
    }

    #[test]
    fn it_says_hello_once_to_someone_who_comes_back_after_five_minutes() {
        let mut now = 1_000 * S;
        let mut arrival = Arrival::new(None, now);
        // An empty desk for six minutes, then someone sits down.
        assert_eq!(pass(&mut arrival, &mut now, 360, false, QUIET), 0);
        assert_eq!(
            pass(&mut arrival, &mut now, 2, true, QUIET),
            0,
            "not yet three seconds"
        );
        assert_eq!(pass(&mut arrival, &mut now, 2, true, QUIET), 1);
        // They stay for an hour, looking away now and then: nothing more.
        for _ in 0..60 {
            assert_eq!(pass(&mut arrival, &mut now, 50, true, QUIET), 0);
            assert_eq!(pass(&mut arrival, &mut now, 10, false, QUIET), 0);
        }
    }

    #[test]
    fn it_keeps_quiet_for_everything_else() {
        // Someone already there when the service starts was never away.
        let mut now = 1_000 * S;
        let mut arrival = Arrival::new(None, now);
        assert_eq!(pass(&mut arrival, &mut now, 600, true, QUIET), 0);
        // Away for two minutes only.
        assert_eq!(pass(&mut arrival, &mut now, 120, false, QUIET), 0);
        assert_eq!(pass(&mut arrival, &mut now, 60, true, QUIET), 0);
        // Someone walking past: a face for two seconds after a long absence.
        assert_eq!(pass(&mut arrival, &mut now, 600, false, QUIET), 0);
        assert_eq!(pass(&mut arrival, &mut now, 2, true, QUIET), 0);
        assert_eq!(pass(&mut arrival, &mut now, 600, false, QUIET), 0);
        // A room in conversation: they arrive, but it is not quiet. Once it
        // has been quiet for half a minute with them still there, it does.
        assert_eq!(pass(&mut arrival, &mut now, 30, true, 5 * S), 0);
        assert_eq!(pass(&mut arrival, &mut now, 1, true, QUIET), 1);
        // A hello that could not be begun stays due until it is.
        let mut waiting = Arrival::new(None, now);
        let mut later = now;
        for _ in 0..4_000 {
            later += S / 10;
            waiting.note(later > now + 310 * S, later);
        }
        assert!(waiting.due(QUIET, later) && waiting.due(QUIET, later + S));
        waiting.said(later + S);
        assert!(!waiting.due(QUIET, later + 2 * S));
        // And then not again for twenty minutes, whoever comes.
        assert_eq!(pass(&mut arrival, &mut now, 400, false, QUIET), 0);
        assert_eq!(pass(&mut arrival, &mut now, 30, true, QUIET), 0);
        assert_eq!(pass(&mut arrival, &mut now, 800, false, QUIET), 0);
        assert_eq!(pass(&mut arrival, &mut now, 10, true, QUIET), 1);
    }

    #[test]
    fn only_saying_it_will_look_asks_for_a_look() {
        for (opening, asks) in [
            ("Let me look.", true),
            ("Let me look... okay.", true),
            ("let me take a look", true),
            ("Let me have a look!", true),
            ("Let me see... that's four!", false),
            ("Let me think.", false),
            ("Let", false),
            ("Let me", false),
            ("I'll let me look", false),
            ("Look, it's simple.", false),
            ("", false),
        ] {
            assert_eq!(asks_to_look(opening), asks, "{opening:?}");
        }
    }

    #[test]
    fn the_eyes_file_counts_only_fresh_and_only_for_a_near_face() {
        let directory = std::env::temp_dir().join(format!(
            "lamp-voice-arrival-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("present");
        let mut arrival = Arrival::new(Some(path.clone()), 0);
        assert!(!arrival.near_now(S));
        std::fs::write(&path, "0.21\n").unwrap();
        assert!(!arrival.near_now(S + 1), "read at most four times a second");
        assert!(arrival.near_now(2 * S));
        std::fs::write(&path, "0.09\n").unwrap();
        assert!(!arrival.near_now(3 * S), "a face across the room");
        std::fs::write(&path, "not a number").unwrap();
        assert!(!arrival.near_now(4 * S));
        std::fs::write(&path, "0.30").unwrap();
        let old = SystemTime::now() - Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert!(!arrival.near_now(5 * S), "the eyes stopped writing");
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
