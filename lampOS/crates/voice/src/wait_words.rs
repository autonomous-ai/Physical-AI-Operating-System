//! A short sound while an answer waits, judged by its words (#1123, step 2,
//! `LAMP_WAIT_KEEPS_ANSWER=words`).
//!
//! With `on` (step 1) a new sound while an answer is awaited and has not
//! sounded waits, and one that ends, or goes quiet, before it has gone on is
//! let go, so a cough doesn't cut the answer off. So is a real short "No."
//! or "Wait." (#1130's "most likely wrong"). With `words` its audio, from
//! its start, goes to a speech-to-text stream of its own, the second voice's
//! as the stop words use it (`crate::stop_words::RelayEars::for_waits`), and
//! its words are asked for once it has been quiet for 100 ms
//! ([`ASK_AFTER_QUIET_BLOCKS`]), well before its endpoint (600 ms). Then:
//! - words back, past fillers ([`FILLERS`]): it is decided as a sound that
//!   went on is, at once;
//! - words back with nothing past fillers: `on` decides it, as before;
//! - where `on` would let it go and its words are not back, it waits for
//!   them while it has not ended and is short of its own deadline; then it
//!   is let go, as with `on`. An ended sound cannot be held: the next one's
//!   start would find it still pending.
//!
//! Only the count of words is recorded, never the words.
use crate::{activity::ObservedAudio, admission::CandidateId, stop_words::Ears};

/// `LAMP_WAIT_KEEPS_ANSWER=words`: the wait of `on`, with the words.
pub const VALUE: &str = "words";

/// Blocks of quiet in a waited sound after which its words are asked for:
/// 100 ms, as the stop words ask
/// (`crate::stop_words::FINALIZE_AFTER_STILL_BLOCKS`).
/// Through the relay on lamp-4ace they came 225 and 269 ms after the audio
/// (`stt-check`, #1047). A sound of 0.5 s or less is let go at its endpoint,
/// 600 ms after it stops, so they have 490 ms; asked after the 300 ms of
/// quiet that `on` lets go on, they had 290 ms, and 250 down to 90 ms for
/// sounds of 0.6 to 0.76 s, quiet at their 1 s mark. Its cost: a sound
/// with a pause of more than 100 ms in it is judged by the words before the
/// pause, so "Uh, … no" is let go, as with `on`.
pub const ASK_AFTER_QUIET_BLOCKS: u16 = 10;

/// Sounds a person makes that ask nothing: a waited sound of only these is
/// judged as one of no words is.
pub const FILLERS: &[&str] = &[
    "um", "uh", "er", "erm", "hm", "hmm", "mm", "mhm", "mmm", "ah", "oh", "huh",
];

/// How many words of `text` ask something: its words past [`FILLERS`], and
/// past any hum of only m and h ("mm-hm", "hmmm").
pub fn words_in(text: &str) -> usize {
    crate::not_speech::plain_words(text)
        .iter()
        .filter(|word| {
            let bare = word.replace('-', "");
            !FILLERS.contains(&word.as_str())
                && !FILLERS.contains(&bare.as_str())
                && !bare.chars().all(|c| c == 'm' || c == 'h')
        })
        .count()
}

/// What the words say of the waited sound so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Not asked yet, or asked and not back.
    Pending,
    /// Back, with this many words that ask something.
    Words(usize),
}

/// What a waited sound does with its words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// As `on` would have it.
    AsBefore,
    /// It said something: decided at once, as a sound that went on.
    Take(usize),
    /// `on` would let it go and its words are not back: it waits for them.
    Wait,
    /// `on` lets it go, and its words said nothing or did not come in time.
    LetGo,
}

/// [`Then`] for a waited sound: `lets_go` if `on` would let it go now,
/// `verdict` as it stands at `at_us`, whether it has `ended`, and when it
/// must be decided by (`deadline_us`).
pub fn then(lets_go: bool, verdict: Verdict, ended: bool, at_us: u64, deadline_us: u64) -> Then {
    match verdict {
        Verdict::Words(count) if count > 0 => Then::Take(count),
        _ if !lets_go => Then::AsBefore,
        Verdict::Words(_) => Then::LetGo,
        Verdict::Pending if !ended && at_us < deadline_us => Then::Wait,
        Verdict::Pending => Then::LetGo,
    }
}

/// One waited sound's words at a time, from a stream kept ready while an
/// answer is awaited.
pub struct WaitWords {
    ears: Box<dyn Ears>,
    /// A stream is kept ready ([`Ears::ready`]) until [`Ears::rest`].
    warm: bool,
    candidate: Option<CandidateId>,
    /// The sequence of its last block sent: its kept audio may lose its
    /// oldest blocks while it waits, so place in it is no guide.
    sent_through: Option<u64>,
    asked_at_us: Option<u64>,
    verdict: Verdict,
    /// What its words did has been told.
    told: bool,
}

impl WaitWords {
    pub fn new(ears: Box<dyn Ears>) -> Self {
        Self {
            ears,
            warm: false,
            candidate: None,
            sent_through: None,
            asked_at_us: None,
            verdict: Verdict::Pending,
            told: false,
        }
    }

    /// Each block: whether an answer to someone is `awaited` unheard, and
    /// whether the sound in hand, if any, is `still_pending`. A stream is
    /// kept ready while an answer is awaited, so that a waited sound's words
    /// do not wait for one to connect (0.6 to 0.8 s); none once it is not
    /// and no sound is in hand.
    pub fn follow(&mut self, awaited: bool, still_pending: bool) {
        if !still_pending {
            self.done();
        }
        if awaited && !self.warm {
            self.ears.ready();
            self.warm = true;
        } else if !awaited && self.warm && self.candidate.is_none() {
            self.ears.rest();
            self.warm = false;
        }
    }

    /// The sound in hand, if any.
    pub fn holds(&self) -> Option<CandidateId> {
        self.candidate
    }

    /// `candidate` waits, with `audio` kept from its start: what is new of
    /// it goes to the stream, a new candidate taking it over.
    pub fn hear(&mut self, candidate: CandidateId, audio: &[ObservedAudio]) {
        if self.candidate != Some(candidate) {
            self.done();
            self.candidate = Some(candidate);
            if !self.warm {
                self.ears.ready();
                self.warm = true;
            }
        }
        if self.asked_at_us.is_some() {
            return;
        }
        for block in audio {
            if self
                .sent_through
                .is_none_or(|through| block.sequence > through)
            {
                self.ears.hear(block.captured_at_us, &block.samples);
                self.sent_through = Some(block.sequence);
            }
        }
    }

    /// Its words are asked for at `at_us`, once, with what is new of its
    /// audio first. When they were first asked for.
    pub fn ask(&mut self, candidate: CandidateId, audio: &[ObservedAudio], at_us: u64) -> u64 {
        self.hear(candidate, audio);
        *self.asked_at_us.get_or_insert_with(|| {
            self.ears.finalize();
            at_us
        })
    }

    /// What is known of `candidate`'s words now.
    pub fn verdict(&mut self, candidate: CandidateId) -> Verdict {
        if self.candidate != Some(candidate) {
            return Verdict::Pending;
        }
        if self.verdict == Verdict::Pending
            && self.asked_at_us.is_some()
            && let Some(words) = self.ears.words()
        {
            self.verdict = Verdict::Words(words_in(&words));
        }
        self.verdict
    }

    /// When the sound in hand's words were asked for, if they were.
    pub fn asked_at_us(&self) -> Option<u64> {
        self.asked_at_us
    }

    /// Whether what the words did to the sound in hand is still to be told:
    /// true once a sound.
    pub fn tell(&mut self) -> bool {
        !std::mem::replace(&mut self.told, true)
    }

    /// Done with the sound in hand: the stream forgets it, and a new one is
    /// kept ready while the stream is.
    pub fn done(&mut self) {
        if self.candidate.take().is_some() {
            self.ears.forget();
        }
        self.sent_through = None;
        self.asked_at_us = None;
        self.verdict = Verdict::Pending;
        self.told = false;
    }

    /// The microphone was closed, or the conversation ends: nothing heard
    /// is kept, and no stream either.
    pub fn close(&mut self) {
        self.done();
        if self.warm {
            self.ears.rest();
            self.warm = false;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// What the ears were told, in order, and the words they give next.
    #[derive(Default)]
    pub(crate) struct Script {
        pub(crate) told: Vec<String>,
        pub(crate) words: Option<String>,
    }
    pub(crate) struct Scripted(pub(crate) Arc<Mutex<Script>>);
    impl Ears for Scripted {
        fn ready(&mut self) {
            self.0.lock().unwrap().told.push("ready".into());
        }
        fn hear(&mut self, _: u64, samples: &[i16]) {
            self.0
                .lock()
                .unwrap()
                .told
                .push(format!("hear {}", samples[0]));
        }
        fn finalize(&mut self) {
            self.0.lock().unwrap().told.push("finalize".into());
        }
        fn words(&mut self) -> Option<String> {
            self.0.lock().unwrap().words.take()
        }
        fn forget(&mut self) {
            self.0.lock().unwrap().told.push("forget".into());
        }
        fn rest(&mut self) {
            self.0.lock().unwrap().told.push("rest".into());
        }
    }

    /// Wait words on scripted ears.
    pub(crate) fn scripted() -> (WaitWords, Arc<Mutex<Script>>) {
        let script = Arc::new(Mutex::new(Script::default()));
        (
            WaitWords::new(Box::new(Scripted(Arc::clone(&script)))),
            script,
        )
    }

    fn audio(values: &[i16]) -> Vec<ObservedAudio> {
        values
            .iter()
            .enumerate()
            .map(|(index, &value)| ObservedAudio {
                sequence: index as u64 + 1,
                captured_at_us: index as u64 * 10_000,
                samples: [value; 160],
            })
            .collect()
    }

    fn candidates() -> (CandidateId, CandidateId) {
        (CandidateId::for_test(1), CandidateId::for_test(2))
    }

    /// A short "No." waited on: a stream kept ready while the answer is
    /// awaited, its audio from its start, then each new block; asked once;
    /// its one word back. Then the next sound has the stream, and once
    /// neither an answer is awaited nor a sound in hand, no stream is kept.
    #[test]
    fn a_waited_sound_is_heard_from_its_start_on_a_stream_kept_ready() {
        let (mut words, script) = scripted();
        let (first, second) = candidates();
        words.follow(true, true);
        words.follow(true, true);
        words.hear(first, &audio(&[1, 2]));
        words.hear(first, &audio(&[1, 2, 3]));
        assert_eq!(words.verdict(first), Verdict::Pending);
        assert_eq!(words.ask(first, &audio(&[1, 2, 3, 4]), 40_000), 40_000);
        assert_eq!(words.ask(first, &audio(&[1, 2, 3, 4, 5]), 50_000), 40_000);
        assert_eq!(words.verdict(first), Verdict::Pending);
        script.lock().unwrap().words = Some("No.".into());
        assert_eq!(words.verdict(first), Verdict::Words(1));
        assert_eq!(words.verdict(first), Verdict::Words(1), "it stays");
        assert!(words.tell());
        assert!(!words.tell(), "told once");
        words.hear(second, &audio(&[9]));
        assert_eq!(words.verdict(second), Verdict::Pending);
        // The answer arrives: the sound in hand keeps the stream while it
        // is pending, and once it is not, the stream rests.
        words.follow(false, true);
        words.follow(false, false);
        words.follow(false, false);
        assert_eq!(
            script.lock().unwrap().told,
            [
                "ready", "hear 1", "hear 2", "hear 3", "hear 4", "finalize", "forget", "hear 9",
                "forget", "rest"
            ]
        );
        assert_eq!(words.holds(), None);
    }

    /// Its kept audio losing its oldest blocks while it waits neither sends
    /// them again nor skips what is new; a sound with no stream ready asks
    /// for one.
    #[test]
    fn a_trimmed_front_sends_nothing_twice_and_skips_nothing() {
        let (mut words, script) = scripted();
        let (first, _) = candidates();
        let kept = audio(&[1, 2, 3, 4, 5]);
        words.hear(first, &kept[..3]);
        words.hear(first, &kept[2..]);
        words.ask(first, &kept[3..], 50_000);
        assert_eq!(
            script.lock().unwrap().told,
            [
                "ready", "hear 1", "hear 2", "hear 3", "hear 4", "hear 5", "finalize"
            ]
        );
    }

    /// Closed: what it heard is dropped and its stream rests; closed again,
    /// nothing more.
    #[test]
    fn closing_drops_the_sound_and_the_stream() {
        let (mut words, script) = scripted();
        let (first, _) = candidates();
        words.hear(first, &audio(&[1]));
        words.close();
        words.close();
        assert_eq!(
            script.lock().unwrap().told,
            ["ready", "hear 1", "forget", "rest"]
        );
        assert_eq!(words.verdict(first), Verdict::Pending);
    }

    /// Fillers say nothing; words decide at once, even before `on` would;
    /// where `on` lets it go, missing words hold it only while it has not
    /// ended and is short of its deadline.
    #[test]
    fn words_decide_and_missing_ones_hold_only_an_unended_sound() {
        for (said, count) in [
            ("", 0),
            ("Um.", 0),
            ("Hmm, mm-hm.", 0),
            ("Uh, no.", 1),
            ("Wait, what?", 2),
        ] {
            assert_eq!(words_in(said), count, "{said:?}");
        }
        for lets_go in [false, true] {
            assert_eq!(then(lets_go, Verdict::Words(1), true, 0, 1), Then::Take(1));
        }
        assert_eq!(then(false, Verdict::Words(0), false, 0, 1), Then::AsBefore);
        assert_eq!(then(false, Verdict::Pending, false, 0, 1), Then::AsBefore);
        assert_eq!(then(true, Verdict::Words(0), false, 0, 1), Then::LetGo);
        assert_eq!(then(true, Verdict::Pending, false, 99, 100), Then::Wait);
        assert_eq!(then(true, Verdict::Pending, false, 100, 100), Then::LetGo);
        assert_eq!(then(true, Verdict::Pending, true, 0, 100), Then::LetGo);
    }
}
