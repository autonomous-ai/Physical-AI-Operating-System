//! What the lamp did after a decision.
//!
//! In shadow mode nothing acts on a decision, so what the lamp does next
//! is the voice's own doing. Written beside the decision, it is two things
//! at once. It is the other half of an example: what was judged, and what
//! then happened. And it is a measurement: how long before the lamp's
//! first sound the decision was known, which is all the time anything
//! acting on it would have had.
//!
//! Which answer belongs to which utterance is told by time alone (the
//! turn's number that what is heard carries is not used here). The lamp's
//! first sound after an utterance began is taken as its answer to it. With
//! two utterances close together that can be wrong, and such an outcome is
//! marked `overtaken`.
use crate::attention::Choice;
use std::time::{Duration, Instant};

/// How long after an utterance's last piece the lamp is given to begin
/// speaking. An answer began 1.1 to 1.3 s after the person stopped on
/// lamp-4ace, and one that calls a function about a second later.
pub const WINDOW: Duration = Duration::from_secs(6);

/// How long after the lamp's first sound its words are waited for, so
/// that the outcome can carry the beginning of what it said.
pub const WORDS_FOR: Duration = Duration::from_secs(1);

/// When the lamp last began to speak, and to say something new.
#[derive(Default)]
pub struct Begun {
    speaking: bool,
    spoke: Option<Instant>,
    said: Option<(Instant, Option<u64>)>,
}

impl Begun {
    /// The voice said what it is doing. Whether that was the lamp
    /// beginning to speak.
    pub fn state(&mut self, speaking: bool, now: Instant) -> bool {
        let began = speaking && !self.speaking;
        if began {
            self.spoke = Some(now);
        }
        self.speaking = speaking;
        began
    }

    /// A piece of what the lamp said in answer to `request`. Whether it
    /// was the first piece of another answer than the one before.
    pub fn said(&mut self, request: Option<u64>, now: Instant) -> bool {
        let began = self.said.is_none_or(|(_, of)| of != request);
        if began {
            self.said = Some((now, request));
        }
        began
    }

    /// The voice has gone from the bus: it is not speaking now, and its
    /// next sound is a beginning.
    pub fn gone(&mut self) {
        self.speaking = false;
    }
}

/// A decision whose outcome is waited for.
#[derive(Clone, Copy, Debug)]
pub struct Watch {
    decision_at_unix_ms: u64,
    choice: Choice,
    decided: Instant,
    ended: Instant,
    spoke: Option<Instant>,
    said: Option<(Instant, Option<u64>)>,
}

/// What the lamp did after a decision. Counts and times: nothing of what
/// was heard or said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The decision's own time, which names its line in the log.
    pub decision_at_unix_ms: u64,
    pub choice: Choice,
    /// The lamp began to speak.
    pub spoke: bool,
    /// From the decision to the lamp's first sound, and to the first of
    /// its words in writing. Less than nothing if that came first.
    pub spoke_after_ms: Option<i64>,
    pub said_after_ms: Option<i64>,
    /// From the utterance's last piece to the lamp's first sound.
    pub heard_to_sound_ms: Option<i64>,
    /// The request the lamp's answer belongs to.
    pub request: Option<u64>,
    /// Another utterance was decided before this outcome was known.
    pub overtaken: bool,
}

/// From one moment to another, in milliseconds, either way round.
fn between(from: Instant, to: Instant) -> i64 {
    let millis = |time: Duration| i64::try_from(time.as_millis()).unwrap_or(i64::MAX);
    if to >= from {
        millis(to - from)
    } else {
        -millis(from - to)
    }
}

impl Watch {
    /// A decision made at `decided` about an utterance whose pieces came
    /// from `began` to `ended`. What the lamp has begun since the
    /// utterance began is its answer already.
    pub fn new(
        decision_at_unix_ms: u64,
        choice: Choice,
        decided: Instant,
        (began, ended): (Instant, Instant),
        begun: &Begun,
    ) -> Self {
        Self {
            decision_at_unix_ms,
            choice,
            decided,
            ended,
            spoke: begun.spoke.filter(|at| *at >= began),
            said: begun.said.filter(|(at, _)| *at >= began),
        }
    }

    /// The lamp began to speak.
    pub fn spoke(&mut self, now: Instant) {
        self.spoke.get_or_insert(now);
    }

    /// The lamp began to say something new, in answer to `request`.
    pub fn said(&mut self, request: Option<u64>, now: Instant) {
        self.said.get_or_insert((now, request));
    }

    /// Nothing more is waited for: the lamp spoke and has had time to
    /// write its first words, or it has had its time and did not speak.
    pub fn over(&self, now: Instant) -> bool {
        match self.spoke {
            Some(at) => now >= at + WORDS_FOR,
            None => now >= self.ended + WINDOW,
        }
    }

    pub fn outcome(&self, overtaken: bool) -> Outcome {
        Outcome {
            decision_at_unix_ms: self.decision_at_unix_ms,
            choice: self.choice,
            spoke: self.spoke.is_some(),
            spoke_after_ms: self.spoke.map(|at| between(self.decided, at)),
            said_after_ms: self.said.map(|(at, _)| between(self.decided, at)),
            heard_to_sound_ms: self.spoke.map(|at| between(self.ended, at)),
            request: self.said.and_then(|(_, request)| request),
            overtaken,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_lamp_that_answers_after_the_decision_spoke_and_the_times_are_from_the_decision() {
        let start = Instant::now();
        let at = |n| start + ms(n);
        let mut begun = Begun::default();
        // An answer of some time before, and its sound.
        assert!(begun.said(Some(6), at(0)));
        assert!(begun.state(true, at(20)));
        assert!(!begun.state(true, at(40)));
        assert!(!begun.said(Some(6), at(60)));
        assert!(!begun.state(false, at(900)));
        // Heard from 2.0 to 2.1 s, decided at 2.7 s.
        let mut watch = Watch::new(
            77,
            Choice::Answer,
            at(2_700),
            (at(2_000), at(2_100)),
            &begun,
        );
        assert!(!watch.over(at(2_700)));
        assert!(begun.said(Some(7), at(3_050)));
        watch.said(Some(7), at(3_050));
        assert!(begun.state(true, at(3_200)));
        watch.spoke(at(3_200));
        // What comes after the first of each changes nothing.
        watch.said(Some(8), at(3_300));
        watch.spoke(at(3_400));
        assert!(!watch.over(at(4_199)));
        assert!(watch.over(at(4_200)));
        assert_eq!(
            watch.outcome(false),
            Outcome {
                decision_at_unix_ms: 77,
                choice: Choice::Answer,
                spoke: true,
                spoke_after_ms: Some(500),
                said_after_ms: Some(350),
                heard_to_sound_ms: Some(1_100),
                request: Some(7),
                overtaken: false,
            }
        );
    }

    #[test]
    fn a_lamp_that_began_before_the_decision_is_counted_and_the_time_is_less_than_nothing() {
        let start = Instant::now();
        let at = |n| start + ms(n);
        let mut begun = Begun::default();
        // Heard from 1.0 to 1.2 s. The lamp is quicker than the judge.
        begun.said(Some(3), at(1_900));
        begun.state(true, at(2_000));
        let watch = Watch::new(
            5,
            Choice::StayQuiet,
            at(2_400),
            (at(1_000), at(1_200)),
            &begun,
        );
        let outcome = watch.outcome(false);
        assert!(outcome.spoke);
        assert_eq!(outcome.spoke_after_ms, Some(-400));
        assert_eq!(outcome.said_after_ms, Some(-500));
        assert_eq!(outcome.heard_to_sound_ms, Some(800));
        assert_eq!(outcome.request, Some(3));
        assert!(!watch.over(at(2_999)));
        assert!(watch.over(at(3_000)));
    }

    #[test]
    fn a_lamp_that_says_nothing_is_silent_once_its_time_is_up() {
        let start = Instant::now();
        let at = |n| start + ms(n);
        let mut begun = Begun::default();
        // What it began before the utterance is not an answer to it.
        begun.said(Some(1), at(100));
        begun.state(true, at(200));
        begun.state(false, at(800));
        let watch = Watch::new(9, Choice::LookUp, at(1_700), (at(1_000), at(1_200)), &begun);
        assert!(!watch.over(at(7_199)));
        assert!(watch.over(at(7_200)));
        assert_eq!(
            watch.outcome(true),
            Outcome {
                decision_at_unix_ms: 9,
                choice: Choice::LookUp,
                spoke: false,
                spoke_after_ms: None,
                said_after_ms: None,
                heard_to_sound_ms: None,
                request: None,
                overtaken: true,
            }
        );
    }

    #[test]
    fn words_that_are_written_and_never_played_are_not_speaking() {
        let start = Instant::now();
        let at = |n| start + ms(n);
        let mut begun = Begun::default();
        let mut watch = Watch::new(2, Choice::StayQuiet, at(600), (at(0), at(100)), &begun);
        assert!(begun.said(Some(4), at(900)));
        watch.said(Some(4), at(900));
        let outcome = watch.outcome(false);
        assert!(!outcome.spoke);
        assert_eq!(outcome.said_after_ms, Some(300));
        assert_eq!(outcome.request, Some(4));
        assert!(!watch.over(at(6_099)));
        assert!(watch.over(at(6_100)));
    }

    #[test]
    fn a_voice_that_went_away_while_speaking_begins_again_when_it_is_back() {
        let start = Instant::now();
        let at = |n| start + ms(n);
        let mut begun = Begun::default();
        assert!(begun.state(true, at(0)));
        begun.gone();
        assert!(begun.state(true, at(500)));
        // An answer without a number is one answer until a number comes.
        assert!(begun.said(None, at(600)));
        assert!(!begun.said(None, at(700)));
        assert!(begun.said(Some(1), at(800)));
    }
}
