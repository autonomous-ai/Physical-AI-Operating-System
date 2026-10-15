//! Whom the lamp answers: its own rule, applied before any sound is let out.
//!
//! The model decides what to say. Whether an answer is played is, for the
//! first things a new session hears, decided here.
//!
//! **What goes wrong.** A session that has just opened has no history of
//! leaving talk alone, and takes what it hears for the start of a
//! conversation with it. On lamp-4ace on 2026-10-13, over three runs of
//! new sessions that each heard two people talking to each other, 9 of 35
//! spoke words to them within their first two requests, whether the
//! session was told how the room was, given a function to decline with, or
//! neither. A session that has been running does this far less: 2 of 87
//! things heard, that afternoon.
//!
//! **The rule.** In a room the lamp knows to be busy, an answer to one of
//! a session's first [`FRESH_REQUESTS`] requests is held back, unless the
//! lamp is already in a conversation, began the turn itself, or the text
//! of what it heard shows it was called. Everywhere else the model's own
//! judgement stands, as before.
//!
//! **What the rule may not lean on.** The first form of it held back every
//! answer in a busy room unless the lamp's name was in the service's text
//! of what it heard. Played the same evening it stopped every answer to
//! other people's talk (0 of 11 sessions) and also held back both of two
//! questions that used the name: the service does not write the name. It
//! wrote "Milan, what is the capital of Italy?" for "Hey Lamp, ...",
//! dropped "Lamp," from the other, and in 310 lines heard that day wrote
//! "Lamp" not once. So the text can show that the lamp was called and can
//! never show that it was not. Here it is used the first way only, and
//! the hold is kept to the few requests where the model is known to be
//! unreliable. The cost that remains: someone who calls the lamp in a busy
//! room within its first two requests after a new session, in words the
//! text gives no sign of, is not answered until they say it again.
//!
//! What makes holding possible at all is timing: in those runs the text of
//! what was heard had arrived before the first sound of every one of 24
//! answers, by 0.32 s at the least.
//!
//! An answer held back is dropped in the provider worker like one the
//! model declined itself (docs/tools.md). Off unless the service's
//! environment says `on`. A better sign of being called (the lamp
//! spotting its name in its own sound, the eyes seeing someone face it)
//! is to take the text's place (docs/attention.md).

use crate::room_told::Room;
use std::sync::atomic::{AtomicU64, Ordering};

/// Where the coordinator reads whether the rule is applied. `on` only.
pub const ENVIRONMENT: &str = "LAMP_ATTENTION";
/// The provider worker's flag for it.
pub const FLAG: &str = "--attention";

/// Where the coordinator reads whether a request that continues one the lamp
/// ended early keeps what the rule knew of it (#782). `on` only.
///
/// Why: the 400 ms early end (`--endpoint smart-turn-fast`) ends a turn at a
/// pause the person goes on after, and the rest comes as a new request.
/// On lamp-4ace on 2026-10-15, 9 of 84 such ends in the played scenes fell
/// at a comma inside a line, and two more were ended by the 600 ms window
/// just after "Hey Lamp,". The name was then said 1.5 s or more before the
/// new request began, so it did not count for it, and in a busy room the
/// rest was held as not said to the lamp (as on 2026-10-14 at 13:32, four
/// times). With this, the continuation keeps the first part's name, its
/// surest score, the text heard so far and whether it was close.
pub const CUT_KEEPS_NAME_ENVIRONMENT: &str = "LAMP_CUT_KEEPS_NAME";

/// Whether the voice's environment says `LAMP_CUT_KEEPS_NAME=on`, read once.
pub fn cut_keeps_name() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| keeps_name(std::env::var(CUT_KEEPS_NAME_ENVIRONMENT).ok().as_deref()))
}

fn keeps_name(value: Option<&str>) -> bool {
    value == Some("on")
}

/// The newest request the lamp began itself, in writing: a hello, a look.
/// Nothing was said to it, so nothing can have been said to someone else.
pub static OWN_REQUEST: AtomicU64 = AtomicU64::new(0);

/// The provider worker's argument for the switch in the environment.
pub fn arguments_from_environment() -> Vec<String> {
    arguments(std::env::var(ENVIRONMENT).ok().as_deref())
}

fn arguments(value: Option<&str>) -> Vec<String> {
    match value {
        Some("on") => vec![FLAG.to_owned()],
        Some("busy") => vec![FLAG.to_owned(), BUSY_FLAG.to_owned()],
        Some("busy-near") => vec![FLAG.to_owned(), BUSY_FLAG.to_owned(), NEAR_FLAG.to_owned()],
        _ => Vec::new(),
    }
}

/// The provider worker's flag, right after [`FLAG`], for the rule's busy
/// form (`LAMP_ATTENTION=busy`): in a room the lamp knows to be busy, every
/// answer is held unless something shows it was meant for the lamp, and a
/// name heard in the sound counts only with someone turned to the lamp.
///
/// Why: on lamp-4ace on 2026-10-14 a meeting at the table beside it went
/// on for twenty minutes and the lamp answered it three times, none in a
/// new session's first requests. Silent probes the same day, with the
/// session told the office was busy, still had "What is 2 + 2?" answered
/// "Four." and "How was your weekend?" answered: told is not enough. A
/// person at the next desk answers a question in a busy room when it is
/// put to them by name or by a look.
pub const BUSY_FLAG: &str = "--attention-busy";

/// Take a leading `--attention-busy` off a worker's flags, after
/// [`take`].
pub fn take_busy(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == BUSY_FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// The provider worker's flag, right after [`BUSY_FLAG`], for a name heard
/// close to the lamp counting as a call (`LAMP_ATTENTION=busy-near`): in a
/// busy room, a hearing of the name at [`NEAR_SCORE`] or more, while the
/// room microphone rose [`NEAR_RISE_DB`] or more over the room's quiet
/// during the request, is let out though nobody is seen turned to the lamp.
///
/// Why: in played busy-room scenes on lamp-4ace on 2026-10-14 the calls
/// that were held had their name heard at 0.97 to 0.99 with no face seen
/// (the busy rule wants 0.999 or a face). With the owner at the desk the
/// same evening, the room microphone rose 19.5 to 26.5 dB over its quiet
/// for the ten lines said to the lamp, and 9.8 to 15.1 dB for people
/// farther off (`docs/ears.md`). Someone at the desk can also be talking to
/// someone else: so the name must be heard as well.
pub const NEAR_FLAG: &str = "--attention-near";
/// A name heard at least this surely counts as a call when it is close.
pub const NEAR_SCORE: f32 = 0.95;
/// Close: the room microphone this far over its quiet during the request.
pub const NEAR_RISE_DB: f32 = 18.0;
const NAMED_NEAR: &str = "its name was heard close to the lamp";
/// The reason an answer is let out for a request said to the lamp from
/// close to it (`crate::addressed`, `LAMP_ADDRESSED=on`).
pub const ADDRESSED: &str = "someone close to the lamp and turned to it spoke";

/// Take a leading `--attention-near` off a worker's flags, after
/// [`take_busy`].
pub fn take_near(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == NEAR_FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// Where the coordinator reads whether the model is asked to say when it
/// hears the lamp's name. `on` only. A measurement switch: it changes
/// nothing the lamp does, and exists to count whether the model's own sign
/// can stand where the heard text could not.
pub const NAME_ENVIRONMENT: &str = "LAMP_NAME_SIGNAL";
/// The provider worker's flag for it.
pub const NAME_FLAG: &str = "--name-signal";
/// The function the model calls when it hears the name.
pub const HEARD_MY_NAME: &str = "heard_my_name";

/// The provider worker's argument for that switch in the environment.
pub fn name_arguments_from_environment() -> Vec<String> {
    if std::env::var(NAME_ENVIRONMENT).ok().as_deref() == Some("on") {
        vec![NAME_FLAG.to_owned()]
    } else {
        Vec::new()
    }
}

/// Take a leading `--name-signal` off a worker's flags.
pub fn take_name(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == NAME_FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// The function itself: silent, so that calling it neither delays the
/// answer nor adds a word to it.
pub fn name_tool() -> lamp_gemini::Tool {
    lamp_gemini::Tool {
        name: HEARD_MY_NAME.into(),
        description: "Call this every time someone says your name, Lamp, when speaking to you, \
                      whatever else they say. It makes no sound and nobody sees it. Then answer \
                      them exactly as you would have."
            .into(),
        parameters: None,
        silent: true,
    }
}

/// Take a leading `--attention` off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// How many of a session's first requests the rule covers. Counted in the
/// played runs: the answers to other people's talk came on a new session's
/// first or second request.
pub const FRESH_REQUESTS: u32 = 2;

/// Whether the text of what was heard shows the lamp was called. Its name,
/// or one of the words the service has written in its place at the start
/// of a called request in the lamp's own records ("Milan, what is the
/// capital of Italy?", "Man, tell me a joke.", "Lam", "Lan", "plan", "Helen,
/// what color is the sky on a clear day?", "Hey Mom", "Amen", "Weiland").
/// Each was written for "Lamp" or "Hey Lamp" at least once on lamp-4ace.
/// Positive evidence only: a text without them says nothing either way.
pub fn called(heard: &str) -> bool {
    let mut words = heard
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty());
    let is = |word: &str, list: &[&str]| list.iter().any(|known| word.eq_ignore_ascii_case(known));
    let all: Vec<&str> = words.by_ref().collect();
    if all.iter().any(|word| is(word, &["lamp", "lamb"])) {
        return true;
    }
    // A misheard name stands where the name stood: first, or after a
    // greeting.
    let first = all
        .iter()
        .find(|word| !is(word, &["hey", "hi", "ok", "okay", "oh", "hello"]));
    first.is_some_and(|word| {
        is(
            word,
            &[
                "lam", "lan", "milan", "man", "plan", "helen", "mom", "amen", "weiland",
            ],
        )
    })
}

/// Whether a call for this function is the start of an answer, to be
/// decided like one: every call but the two that only say something about
/// what was heard and do nothing.
pub fn answers(function: &str) -> bool {
    function != HEARD_MY_NAME && function != crate::tools::NOT_FOR_ME
}

/// Whether an answer is played, and why, for the record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Play(&'static str),
    Hold(&'static str),
}

/// The lamp's name counts for a request if it began to be heard no longer
/// than this before the request began: the name is the first thing said,
/// and the voice takes a moment to be sure someone is speaking.
pub const NAME_BEFORE_US: u64 = 1_500_000;
const NAMED: &str = "its name was heard in the room's sound";

/// The rule. `fresh`: the request is one of its session's first
/// [`FRESH_REQUESTS`]. `heard`: the text of what was heard since the
/// request began, empty when none has arrived. `named`: the lamp's name
/// was heard in the sound itself as the request began (`docs/name.md`),
/// which like the text can speak for playing and never against.
pub fn decide(
    room: Room,
    going_on: bool,
    own: bool,
    fresh: bool,
    heard: &str,
    named: bool,
) -> Verdict {
    decide_busy(room, going_on, own, fresh, heard, named, false, None)
}

const TURNED: &str = "someone is turned to the lamp";
const SOUND_A_WORD: &str = "a short sound, heard as a short word said to the lamp";
const SOUND_NEAR: &str = "a short sound, said close to the lamp";
const SOUND_TURNED: &str = "a short sound, with someone turned to the lamp";
const SOUND_HELD: &str = "a short sound, and nothing shows it was said to the lamp: not a short word, not close, nobody turned to it";
const NAMED_PLAINLY: &str = "its name was heard plainly in the room's sound";

/// A hearing of the name this sure counts as a call in a busy room even
/// with nobody seen turned to the lamp. On lamp-4ace on 2026-10-14, from
/// 07:45 to 13:02 with nobody calling it, the part that listens for the
/// name heard it 17 times in people's talk, at 0.903 to 0.997; played calls
/// that afternoon scored 1.000, 1.000, 0.999 and 0.922.
pub const SURE_SCORE: f32 = 0.999;

/// The same, in its busy form when `strict` ([`BUSY_FLAG`]): in a busy room
/// every request is covered, not only a new session's first, and the name
/// heard in the sound counts only with someone turned to the lamp.
/// `turned`: whether the eyes' newest fresh look has a face turned to the
/// lamp (`crate::bus_out::someone_turned_to_lamp`), which speaks for
/// playing and never against.
#[allow(clippy::too_many_arguments)]
pub fn decide_busy(
    room: Room,
    going_on: bool,
    own: bool,
    fresh: bool,
    heard: &str,
    named: bool,
    strict: bool,
    turned: Option<bool>,
) -> Verdict {
    if own {
        Verdict::Play("the lamp began this turn itself")
    } else if !(fresh || strict && room == Room::Busy) {
        Verdict::Play("the session has heard enough to be trusted")
    } else if room != Room::Busy {
        Verdict::Play("the room is not known to be busy")
    } else if going_on {
        Verdict::Play("a conversation with the lamp is going on")
    } else if called(heard) {
        Verdict::Play("the text shows it was called")
    } else if turned == Some(true) {
        Verdict::Play(TURNED)
    } else if named && !strict {
        Verdict::Play(NAMED)
    } else if strict && named {
        Verdict::Hold(
            "a busy room: its name was heard in the sound, and nobody is turned to the lamp",
        )
    } else if strict {
        Verdict::Hold(
            "a busy room, and nothing shows it was called or that anyone is turned to the lamp",
        )
    } else {
        Verdict::Hold("a new session in a busy room, and nothing shows it was called")
    }
}

/// What the worker keeps for the request in hand.
#[derive(Default)]
pub struct Attention {
    request: u64,
    /// The session the request began on, and how many requests that were
    /// heard have begun on it. A turn the lamp wrote itself is not one:
    /// the session heard nothing in it.
    session: u64,
    begun: u32,
    /// What was heard since it began, as far as the service has written it.
    heard: String,
    verdict: Option<Verdict>,
    /// When the request began, if that is known, and when its sound ended,
    /// if it has, by this process's clock.
    began_us: Option<u64>,
    ended_us: Option<u64>,
    /// When the lamp's name last began to be heard in the room's sound.
    named_us: Option<u64>,
    /// The name was heard for the request in hand: as it began, or in its
    /// sound.
    named: bool,
    /// When the name was last heard at [`SURE_SCORE`] or more, and whether
    /// such a hearing counts for the request in hand.
    named_sure_us: Option<u64>,
    named_sure: bool,
    /// The surest the name was heard for the request in hand, and
    /// whether a voice close to the lamp was heard in it.
    best_score: f32,
    near: bool,
    /// Whether it was said to the lamp from close to it, as
    /// [`crate::addressed::judge`] found.
    addressed: bool,
    /// A short sound let through in a quiet room is held unless something
    /// shows it was said to the lamp (`LAMP_SHORT_SOUNDS=quiet-room-checked`,
    /// #800), and whether the request in hand ended as such a sound.
    sounds_checked: bool,
    sound: bool,
    /// Whether the request judged last was the lamp's own turn, as the
    /// judging read it, for the record ([`Attention::baseline`]).
    judged_own: bool,
}

/// The most text kept for one request.
const MOST_HEARD_BYTES: usize = 2_048;

impl Attention {
    /// A request began on `session`, at `now_us`.
    pub fn began_at(&mut self, request: u64, session: u64, now_us: u64) {
        self.began(request, session);
        self.began_us = Some(now_us);
        self.named = self
            .named_us
            .is_some_and(|named| named.saturating_add(NAME_BEFORE_US) >= now_us);
        self.named_sure = self
            .named_sure_us
            .is_some_and(|named| named.saturating_add(NAME_BEFORE_US) >= now_us);
    }

    /// A request began on `session` at `now_us` that continues the one in
    /// hand, which the lamp ended early: the same person going on with the
    /// same utterance. What the rule knew of the first part stays: its name,
    /// heard surely or not and how surely, the text heard so far, whether it
    /// was close, and whether it was said to the lamp from close to it. Its window begins where the first part's did.
    /// Anything else is as for [`Self::began_at`].
    pub fn continued_at(&mut self, request: u64, session: u64, now_us: u64) {
        let before = (session == self.session && self.began_us.is_some()).then(|| {
            (
                self.named,
                self.named_sure,
                self.best_score,
                std::mem::take(&mut self.heard),
                self.near,
                self.addressed,
                self.began_us,
            )
        });
        self.began_at(request, session, now_us);
        if let Some((named, named_sure, best_score, heard, near, addressed, began_us)) = before {
            self.named |= named;
            self.named_sure |= named_sure;
            self.best_score = self.best_score.max(best_score);
            self.heard = heard;
            self.near |= near;
            self.addressed |= addressed;
            self.began_us = began_us;
        }
    }

    /// The sound of `request` stopped being sent at `at_us`: the person
    /// had finished. A name that begins to be heard after this was not
    /// said in the request, whoever said it.
    pub fn ended(&mut self, request: u64, at_us: u64) {
        if request == self.request && self.began_us.is_some() {
            self.ended_us = Some(at_us);
        }
    }

    /// The lamp's name began to be heard in the room's sound at `at_us`.
    /// It counts for the request in hand if it began as the request did or
    /// in its sound, and once it has counted a later hearing does not take
    /// that away.
    pub fn name_heard(&mut self, at_us: u64) {
        self.name_heard_scored(at_us, 0.0);
    }

    /// The same, with the hearing's score: one at [`SURE_SCORE`] or more is
    /// also kept as a sure one.
    pub fn name_heard_scored(&mut self, at_us: u64, score: f32) {
        self.named_us = Some(self.named_us.map_or(at_us, |before| before.max(at_us)));
        let sure = score >= SURE_SCORE;
        if sure {
            self.named_sure_us = Some(self.named_sure_us.map_or(at_us, |b| b.max(at_us)));
        }
        if let Some(began) = self.began_us
            && at_us.saturating_add(NAME_BEFORE_US) >= began
            && self.ended_us.is_none_or(|ended| at_us <= ended)
        {
            self.named = true;
            self.named_sure |= sure;
            self.best_score = self.best_score.max(score);
        }
    }

    /// A request began on `session`.
    pub fn began(&mut self, request: u64, session: u64) {
        if session != self.session {
            self.session = session;
            self.begun = 0;
        }
        if OWN_REQUEST.load(Ordering::Acquire) != request {
            self.begun = self.begun.saturating_add(1);
        }
        self.request = request;
        self.heard.clear();
        self.verdict = None;
        self.began_us = None;
        self.ended_us = None;
        self.named = false;
        self.named_sure = false;
        self.best_score = 0.0;
        self.near = false;
        self.addressed = false;
        self.sound = false;
    }

    /// Short sounds let through are checked (`--sounds-checked`, #800).
    pub fn check_sounds(&mut self, on: bool) {
        self.sounds_checked = on;
    }

    /// The sound of `request` was ended as a short sound let through in a
    /// quiet room (`ProviderInput::End { sound: true }`).
    pub fn ended_as_sound(&mut self, request: u64) {
        if request == self.request {
            self.sound = true;
        }
    }

    /// Whether the request in hand is a short sound to be checked: the
    /// eyes are then read for it whatever the room.
    pub fn sound_in_hand(&self) -> bool {
        self.sounds_checked && self.sound
    }

    /// When the request in hand began and, if it has, when its sound
    /// ended, by this process's clock.
    pub fn window(&self) -> Option<(u64, Option<u64>)> {
        self.began_us.map(|began| (began, self.ended_us))
    }

    /// Whether a voice close to the lamp was heard in the request in hand
    /// (the room microphone, `NEAR_RISE_DB` over its quiet).
    pub fn set_near(&mut self, near: bool) {
        self.near = near;
    }

    /// Whether the request in hand was said to the lamp from close to it
    /// (`crate::addressed`): in a busy room, an answer the rule would hold
    /// is let out.
    pub fn set_addressed(&mut self, addressed: bool) {
        self.addressed = addressed;
    }

    /// The request in hand as `crate::addressed` judges it, once it began.
    /// None for a turn the lamp wrote itself: nobody spoke in it.
    pub fn in_hand(&self) -> Option<crate::addressed::Request> {
        if OWN_REQUEST.load(Ordering::Acquire) == self.request {
            return None;
        }
        self.began_us.map(|began_us| crate::addressed::Request {
            session: self.session,
            request: self.request,
            began_us,
            ended_us: self.ended_us,
            name_score: self.best_score,
        })
    }

    /// The service wrote down more of what it heard.
    pub fn heard(&mut self, text: &str) {
        let room = MOST_HEARD_BYTES.saturating_sub(self.heard.len());
        let cut = text
            .char_indices()
            .map(|(index, c)| index + c.len_utf8())
            .take_while(|end| *end <= room)
            .last()
            .unwrap_or(0);
        self.heard.push_str(&text[..cut]);
    }

    /// The first of an answer to `request` has come: decide once, and say
    /// so. `None` for a request this is not following, or already decided.
    pub fn first_output(&mut self, request: u64, room: Room, going_on: bool) -> Option<Verdict> {
        self.first_output_busy(request, room, going_on, false, None)
    }

    /// The same, in the rule's busy form when `strict`, with what the eyes
    /// say of who is turned to the lamp.
    pub fn first_output_busy(
        &mut self,
        request: u64,
        room: Room,
        going_on: bool,
        strict: bool,
        turned: Option<bool>,
    ) -> Option<Verdict> {
        if request != self.request || self.verdict.is_some() {
            return None;
        }
        let own = OWN_REQUEST.load(Ordering::Acquire) == request;
        self.judged_own = own;
        let fresh = self.begun <= FRESH_REQUESTS;
        // A short sound let through in a quiet room (#800): on lamp-52e6 on
        // 2026-10-15 at 13:03 a room sound of 0.7 s was heard as "¿Qué?" and
        // answered, nobody having spoken to the lamp. It is answered only
        // when what was heard is a short word said to the lamp, or it was
        // said close to the lamp, or someone is turned to it.
        if self.sound_in_hand() && !own {
            let verdict = if crate::not_speech::short_word(&self.heard) {
                Verdict::Play(SOUND_A_WORD)
            } else if self.near {
                Verdict::Play(SOUND_NEAR)
            } else if turned == Some(true) {
                Verdict::Play(SOUND_TURNED)
            } else {
                Verdict::Hold(SOUND_HELD)
            };
            self.verdict = Some(verdict);
            return Some(verdict);
        }
        if strict && room == Room::Busy && self.named_sure {
            let verdict = Verdict::Play(NAMED_PLAINLY);
            self.verdict = Some(verdict);
            return Some(verdict);
        }
        if strict && room == Room::Busy && self.near && self.best_score >= NEAR_SCORE {
            let verdict = Verdict::Play(NAMED_NEAR);
            self.verdict = Some(verdict);
            return Some(verdict);
        }
        let verdict = match decide_busy(
            room,
            going_on,
            own,
            fresh,
            &self.heard,
            self.named,
            strict,
            turned,
        ) {
            Verdict::Hold(_) if room == Room::Busy && self.addressed => Verdict::Play(ADDRESSED),
            verdict => verdict,
        };
        self.verdict = Some(verdict);
        Some(verdict)
    }

    /// What the rule saw when it last judged, with the room, the
    /// conversation and the eyes it was given, as the Policy's fitting rows
    /// take it ([`crate::first_output`]).
    pub fn baseline(
        &self,
        room: Room,
        going_on: bool,
        turned: Option<bool>,
    ) -> crate::first_output::Baseline {
        crate::first_output::Baseline {
            kind: "voice_first_output",
            room: match room {
                Room::Quiet => "quiet",
                Room::Busy => "busy",
                Room::Unknown => "unknown",
            },
            own_turn: self.judged_own,
            going_on,
            named_sure: self.named_sure,
            text: self.heard.clone(),
            facing: turned,
        }
    }

    /// Whether the answer to the request in hand was let out because the
    /// name was heard, for the record: the one case the sound decided.
    pub fn played_for_its_name(&self) -> bool {
        self.verdict == Some(Verdict::Play(NAMED))
    }

    /// Whether it was let out because the name was heard close to the
    /// lamp ([`NEAR_FLAG`]), for the record.
    pub fn played_for_near(&self) -> bool {
        self.verdict == Some(Verdict::Play(NAMED_NEAR))
    }

    /// Whether `request`'s answer is being held back.
    pub fn holds(&self, request: u64) -> bool {
        request == self.request && matches!(self.verdict, Some(Verdict::Hold(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The calls held in the played scenes of 2026-10-14 had their name
    /// heard at 0.97 to 0.99 with no face: close to the lamp, with
    /// `--attention-near`, they are calls; from farther off, or with a
    /// weaker name, they are not; and in a room not busy nothing changes.
    /// #800: with `--sounds-checked`, a short sound let through in a quiet
    /// room is answered only when something shows it was said to the lamp.
    /// Without the switch, or for a request that did not end as such a
    /// sound, the rule is as it was.
    #[test]
    fn a_short_sound_is_answered_only_when_something_shows_it_was_said_to_the_lamp() {
        let decide = |checked: bool, sound: bool, heard: &str, near: bool, turned: Option<bool>| {
            let mut attention = Attention::default();
            attention.check_sounds(checked);
            attention.began_at(7, 1, 70_000_000);
            attention.heard(heard);
            attention.set_near(near);
            if sound {
                // Another request's sound is not this one's.
                attention.ended_as_sound(6);
                attention.ended_as_sound(7);
            }
            attention
                .first_output_busy(7, Room::Quiet, true, false, turned)
                .unwrap()
        };
        // The office sound of 13:03 on lamp-52e6.
        assert_eq!(
            decide(true, true, "¿Qué?", false, None),
            Verdict::Hold(SOUND_HELD)
        );
        assert_eq!(
            decide(true, true, "¿Qué?", false, Some(false)),
            Verdict::Hold(SOUND_HELD)
        );
        assert_eq!(
            decide(true, true, "", false, None),
            Verdict::Hold(SOUND_HELD)
        );
        // A short word, said close, or with someone turned to the lamp.
        for word in ["Yes.", "Stop.", "What?", "Yes, go ahead.", " hey Lamp "] {
            assert_eq!(
                decide(true, true, word, false, None),
                Verdict::Play(SOUND_A_WORD),
                "{word}"
            );
        }
        assert_eq!(
            decide(true, true, "Claro.", true, None),
            Verdict::Play(SOUND_NEAR)
        );
        assert_eq!(
            decide(true, true, "¿Qué?", false, Some(true)),
            Verdict::Play(SOUND_TURNED)
        );
        // Without the switch, or not a sound: as before.
        let before = decide(false, false, "¿Qué?", false, None);
        assert!(matches!(before, Verdict::Play(_)));
        assert_eq!(decide(false, true, "¿Qué?", false, None), before);
        assert_eq!(decide(true, false, "¿Qué?", false, None), before);
        // The next request forgets that the last one was a sound.
        let mut attention = Attention::default();
        attention.check_sounds(true);
        attention.began_at(7, 1, 70_000_000);
        attention.ended_as_sound(7);
        assert!(attention.sound_in_hand());
        attention.began_at(8, 1, 80_000_000);
        assert!(!attention.sound_in_hand());
    }

    #[test]
    fn a_name_heard_close_to_the_lamp_is_a_call_in_a_busy_room() {
        let decide = |score: f32, near: bool, room: Room| {
            let mut attention = Attention::default();
            for request in 1..=3 {
                attention.began_at(request, 1, request * 10_000_000);
            }
            attention.name_heard_scored(30_000_000, score);
            attention.set_near(near);
            let verdict = attention
                .first_output_busy(3, room, false, true, None)
                .unwrap();
            (verdict, attention.played_for_near())
        };
        assert_eq!(
            decide(0.97, true, Room::Busy),
            (Verdict::Play(NAMED_NEAR), true)
        );
        assert!(matches!(
            decide(0.97, false, Room::Busy).0,
            Verdict::Hold(_)
        ));
        assert!(matches!(decide(0.93, true, Room::Busy).0, Verdict::Hold(_)));
        assert!(matches!(decide(0.0, true, Room::Busy).0, Verdict::Hold(_)));
        assert!(matches!(
            decide(0.97, true, Room::Quiet).0,
            Verdict::Play(_)
        ));
        // A new request forgets that the last one was close.
        let mut attention = Attention::default();
        attention.began_at(1, 1, 10_000_000);
        attention.set_near(true);
        attention.began_at(2, 1, 20_000_000);
        attention.name_heard_scored(20_000_000, 0.97);
        assert!(matches!(
            attention.first_output_busy(2, Room::Busy, false, true, None),
            Some(Verdict::Hold(_))
        ));
        assert_eq!(attention.window(), Some((20_000_000, None)));
    }

    #[test]
    fn said_from_close_and_facing_lets_out_an_answer_the_busy_rule_would_hold() {
        let decide = |addressed: bool, room: Room, strict: bool, request: u64| {
            let mut attention = Attention::default();
            for began in 1..=request {
                attention.began_at(began, 1, began * 10_000_000);
            }
            attention.set_addressed(addressed);
            attention
                .first_output_busy(request, room, false, strict, None)
                .unwrap()
        };
        // The busy form holds a request nothing shows was for the lamp.
        assert_eq!(decide(true, Room::Busy, true, 3), Verdict::Play(ADDRESSED));
        assert!(matches!(
            decide(false, Room::Busy, true, 3),
            Verdict::Hold(_)
        ));
        // So does the plain form, for a new session's first requests.
        assert_eq!(decide(true, Room::Busy, false, 1), Verdict::Play(ADDRESSED));
        assert!(matches!(
            decide(false, Room::Busy, false, 1),
            Verdict::Hold(_)
        ));
        // Where the rule plays anyway, its own reason stands.
        for (room, strict, request) in [(Room::Quiet, true, 3), (Room::Busy, false, 9)] {
            let verdict = decide(true, room, strict, request);
            assert!(
                matches!(verdict, Verdict::Play(why) if why != ADDRESSED),
                "{verdict:?}"
            );
        }
        // A new request forgets that the last one was said close.
        let mut attention = Attention::default();
        attention.began_at(1, 1, 10_000_000);
        attention.set_addressed(true);
        attention.began_at(2, 1, 20_000_000);
        assert!(matches!(
            attention.first_output_busy(2, Room::Busy, false, true, None),
            Some(Verdict::Hold(_))
        ));
    }

    #[test]
    fn the_request_in_hand_is_what_the_addressed_judgement_takes() {
        let mut attention = Attention::default();
        assert_eq!(attention.in_hand(), None);
        attention.began_at(4, 2, 10_000_000);
        attention.name_heard_scored(10_200_000, 0.97);
        attention.ended(4, 12_000_000);
        assert_eq!(
            attention.in_hand(),
            Some(crate::addressed::Request {
                session: 2,
                request: 4,
                began_us: 10_000_000,
                ended_us: Some(12_000_000),
                name_score: 0.97,
            })
        );
        // The lamp's own turn: nobody spoke in it.
        OWN_REQUEST.store(4_242, Ordering::Release);
        attention.began_at(4_242, 2, 13_000_000);
        assert_eq!(attention.in_hand(), None);
        OWN_REQUEST.store(0, Ordering::Release);
    }

    #[test]
    fn a_request_that_continues_an_early_ended_one_keeps_its_name_in_a_busy_room() {
        // "Hey Lamp, can you ask Claude Code ... about lamps," ended at the
        // comma, and "just in its reply" came as a new request 3.5 s after
        // the name began.
        let first_part = |attention: &mut Attention| {
            for request in 1..=3 {
                attention.began_at(request, 1, request * 10_000_000);
            }
            attention.name_heard_scored(30_100_000, 0.97);
            attention.heard("Hey Lamp, can you ask Claude Code about lamps,");
            attention.set_near(true);
            attention.ended(3, 33_000_000);
        };
        let mut continued = Attention::default();
        first_part(&mut continued);
        continued.continued_at(4, 1, 33_600_000);
        assert_eq!(continued.window(), Some((30_000_000, None)));
        assert_eq!(
            continued.first_output_busy(4, Room::Busy, false, true, None),
            Some(Verdict::Play(NAMED_NEAR))
        );
        // The same request begun afresh: the name was too long ago, and the
        // busy rule holds it, as on lamp-4ace.
        let mut afresh = Attention::default();
        first_part(&mut afresh);
        afresh.began_at(4, 1, 33_600_000);
        assert!(matches!(
            afresh.first_output_busy(4, Room::Busy, false, true, None),
            Some(Verdict::Hold(_))
        ));
        // The text heard so far goes on too: the service's "Lamp," shows
        // the call, as it would have in one request.
        let mut text = Attention::default();
        text.began_at(1, 1, 10_000_000);
        text.heard("Lamp, ");
        text.continued_at(2, 1, 12_000_000);
        text.heard("what is the time?");
        assert_eq!(
            text.first_output_busy(2, Room::Busy, false, false, None),
            Some(Verdict::Play("the text shows it was called"))
        );
    }

    #[test]
    fn a_continuation_keeps_nothing_across_sessions_or_without_a_first_part() {
        let mut other = Attention::default();
        other.began_at(1, 1, 10_000_000);
        other.name_heard_scored(10_100_000, 0.999);
        other.heard("Lamp, ");
        other.continued_at(2, 2, 12_000_000);
        assert_eq!(other.window(), Some((12_000_000, None)));
        assert!(matches!(
            other.first_output_busy(2, Room::Busy, false, true, None),
            Some(Verdict::Hold(_))
        ));
        // Said to the lamp from close to it in the first part: still so.
        let mut close = Attention::default();
        close.began_at(1, 1, 10_000_000);
        close.set_addressed(true);
        close.continued_at(2, 1, 12_000_000);
        assert_eq!(
            close.first_output_busy(2, Room::Busy, false, true, None),
            Some(Verdict::Play(ADDRESSED))
        );
        let mut none = Attention::default();
        none.continued_at(1, 1, 10_000_000);
        assert_eq!(none.window(), Some((10_000_000, None)));
        assert!(keeps_name(Some("on")));
        for value in [None, Some(""), Some("off"), Some("ON"), Some("1")] {
            assert!(!keeps_name(value), "{value:?}");
        }
    }

    #[test]
    fn busy_near_is_the_busy_rule_with_the_near_call() {
        assert_eq!(arguments(Some("busy-near")), [FLAG, BUSY_FLAG, NEAR_FLAG]);
        assert_eq!(arguments(Some("busy")), [FLAG, BUSY_FLAG]);
        let flags = vec![NEAR_FLAG.to_owned(), "--x".to_owned()];
        assert_eq!(take_near(&flags), (true, &flags[1..]));
        assert_eq!(take_near(&flags[1..]), (false, &flags[1..]));
    }

    /// Played calls on 2026-10-14 afternoon: the name heard at 1.000 in a
    /// busy room with no face to see is a call; at 0.997, as people's talk
    /// scored that morning, it is not.
    #[test]
    fn in_a_busy_room_a_name_heard_plainly_is_a_call_without_a_face() {
        for (score, plays) in [(1.0f32, true), (0.999, true), (0.997, false)] {
            let mut attention = Attention::default();
            for request in 1..=3 {
                attention.began_at(request, 1, request * 10_000_000);
            }
            attention.name_heard_scored(30_000_000, score);
            let verdict = attention
                .first_output_busy(3, Room::Busy, false, true, None)
                .unwrap();
            assert_eq!(
                matches!(verdict, Verdict::Play(_)),
                plays,
                "{score}: {verdict:?}"
            );
        }
    }

    /// The meeting of 2026-10-14: a session long past its first requests,
    /// a busy room. The busy form holds what nothing shows was meant for the
    /// lamp, a name heard in the sound with nobody turned to it included,
    /// and lets out what was called in the text or asked with a look.
    #[test]
    fn in_its_busy_form_a_busy_room_needs_a_sign_on_every_request() {
        let busy = |heard: &str, named: bool, turned: Option<bool>| {
            decide_busy(Room::Busy, false, false, false, heard, named, true, turned)
        };
        assert!(matches!(
            busy("What is 2 + 2?", false, None),
            Verdict::Hold(_)
        ));
        assert!(matches!(busy("", false, Some(false)), Verdict::Hold(_)));
        assert_eq!(
            busy("", true, Some(false)),
            Verdict::Hold(
                "a busy room: its name was heard in the sound, and nobody is turned to the lamp"
            )
        );
        assert!(matches!(busy("", true, None), Verdict::Hold(_)));
        assert_eq!(busy("", true, Some(true)), Verdict::Play(TURNED));
        assert_eq!(busy("", false, Some(true)), Verdict::Play(TURNED));
        assert_eq!(
            busy("Lamp, what is 2 + 2?", false, None),
            Verdict::Play("the text shows it was called")
        );
        // A conversation with the lamp, and its own turns, go on.
        assert!(matches!(
            decide_busy(Room::Busy, true, false, false, "", false, true, None),
            Verdict::Play(_)
        ));
        assert!(matches!(
            decide_busy(Room::Busy, false, true, false, "", false, true, None),
            Verdict::Play(_)
        ));
        // Any other room: as before, whatever the form.
        for room in [Room::Quiet, Room::Unknown] {
            assert!(matches!(
                decide_busy(room, false, false, false, "", false, true, None),
                Verdict::Play(_)
            ));
            assert!(matches!(
                decide_busy(room, false, false, true, "", false, true, None),
                Verdict::Play(_)
            ));
        }
        // And without the busy form, nothing changes past a session's
        // first requests.
        assert!(matches!(
            decide_busy(
                Room::Busy,
                false,
                false,
                false,
                "",
                true,
                false,
                Some(false)
            ),
            Verdict::Play(_)
        ));
        assert_eq!(
            decide_busy(Room::Busy, false, false, true, "", true, false, None),
            Verdict::Play(NAMED)
        );
    }

    #[test]
    fn busy_adds_its_flag_after_the_rules_own() {
        assert_eq!(arguments(Some("busy")), [FLAG, BUSY_FLAG]);
        let flags = [FLAG.to_owned(), BUSY_FLAG.to_owned(), "rest".to_owned()];
        let (on, rest) = take(&flags);
        let (busy, rest) = take_busy(rest);
        assert!(on && busy);
        assert_eq!(rest, ["rest"]);
        let (busy, rest) = take_busy(&flags[2..]);
        assert!(!busy);
        assert_eq!(rest, ["rest"]);
    }

    #[test]
    fn only_on_turns_it_on_and_the_flag_is_taken_from_the_front() {
        assert_eq!(arguments(Some("on")), [FLAG]);
        assert!(arguments(Some("yes")).is_empty());
        assert!(arguments(None).is_empty());
        let flags = [FLAG.to_owned(), "rest".to_owned()];
        assert_eq!(take(&flags), (true, &flags[1..]));
        assert_eq!(take(&flags[1..]), (false, &flags[1..]));
    }

    /// What the service really wrote for called requests in the lamp's
    /// records of 2026-10-13, and what it wrote for other people's talk.
    #[test]
    fn the_text_can_show_a_call_by_the_name_or_by_what_the_service_writes_in_its_place() {
        for shows in [
            "Lamp, what is two plus two?",
            "Hey Lamp, tell me a joke.",
            "What time is it, Lamp?",
            "Hey, Lamb, how many days are there in a week?",
            "Milan, what is the capital of Italy?",
            "Milan, how are you feeling right now?",
            "Man, tell me a joke.",
            "Ok Lam, stop.",
            "Hey Lan",
            "plan",
            "Helen, what color is the sky on a clear day?",
            "Hey Mom, what time is it?",
            "Amen.",
            "Weiland, tell me a joke.",
        ] {
            assert!(called(shows), "{shows}");
        }
        for silent in [
            // Called, and written with nothing left of the name: the text
            // cannot show it, which is why it is never read as "not called".
            "What color is the sky on a clear day?",
            "What is your favorite color?",
            // Other people's talk.
            "How was your weekend?",
            "Do you know where the new monitors went?",
            "The lamps in the storage room are new.",
            "We planned it with the man from Milan.",
            "Did you ask Helen about the report?",
            "Bây giờ mình có một vấn đề hơi khó.",
            "",
        ] {
            assert!(!called(silent), "{silent}");
        }
    }

    /// Each line of the rule. Only one case holds an answer back.
    #[test]
    fn only_a_new_sessions_first_requests_in_a_busy_room_are_held_and_every_doubt_plays() {
        let weekend = "How was your weekend?";
        assert_eq!(
            decide(Room::Busy, false, false, true, weekend, false),
            Verdict::Hold("a new session in a busy room, and nothing shows it was called")
        );
        // No text at all is held too: the text can only ever speak for
        // playing, and its absence says nothing.
        assert!(matches!(
            decide(Room::Busy, false, false, true, "", false),
            Verdict::Hold(_)
        ));
        // The same request on a session that has heard enough: played.
        assert!(matches!(
            decide(Room::Busy, false, false, false, weekend, false),
            Verdict::Play("the session has heard enough to be trusted")
        ));
        assert!(matches!(
            decide(
                Room::Busy,
                false,
                false,
                true,
                "Milan, how was your weekend?",
                false,
            ),
            Verdict::Play("the text shows it was called")
        ));
        assert!(matches!(
            decide(Room::Busy, true, false, true, weekend, false),
            Verdict::Play("a conversation with the lamp is going on")
        ));
        assert!(matches!(
            decide(Room::Busy, false, true, true, weekend, false),
            Verdict::Play("the lamp began this turn itself")
        ));
        for room in [Room::Quiet, Room::Unknown] {
            assert!(matches!(
                decide(room, false, false, true, weekend, false),
                Verdict::Play("the room is not known to be busy")
            ));
        }
        // The name heard in the sound plays what would have been held,
        // whatever the text says or does not say.
        for text in [weekend, ""] {
            assert_eq!(
                decide(Room::Busy, false, false, true, text, true),
                Verdict::Play("its name was heard in the room's sound")
            );
        }
    }

    /// The name is the first thing said, a little before the voice is
    /// sure someone is speaking. It counts for the request that begins
    /// then, and for no later one.
    #[test]
    fn the_name_heard_as_a_request_began_plays_it_and_is_not_carried_to_the_next() {
        let second = 1_000_000u64;
        let mut attention = Attention::default();
        // Heard 0.4 s before the request began.
        attention.name_heard(10 * second - 400_000);
        attention.began_at(1, 7, 10 * second);
        assert_eq!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Play("its name was heard in the room's sound"))
        );
        assert!(attention.played_for_its_name() && !attention.holds(1));
        // The next request, twenty seconds on, with no name: held.
        attention.began_at(2, 7, 30 * second);
        assert!(matches!(
            attention.first_output(2, Room::Busy, false),
            Some(Verdict::Hold(_))
        ));
        assert!(!attention.played_for_its_name() && attention.holds(2));
        // A name heard in the middle of a request counts for it too.
        let mut attention = Attention::default();
        attention.began_at(1, 8, 50 * second);
        attention.name_heard(51 * second);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Play(_))
        ));
        // To the limit: 1.5 s before counts, a moment more does not.
        for (before, plays) in [(NAME_BEFORE_US, true), (NAME_BEFORE_US + 1, false)] {
            let mut attention = Attention::default();
            attention.name_heard(80 * second - before);
            attention.began_at(1, 9, 80 * second);
            let verdict = attention.first_output(1, Room::Busy, false).unwrap();
            assert_eq!(matches!(verdict, Verdict::Play(_)), plays, "{before}");
        }
        // Nothing is listening for the name, or it was not heard: as before.
        let mut attention = Attention::default();
        attention.began_at(1, 10, 90 * second);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Hold(_))
        ));
        // A request that began at no known time takes no name, however
        // lately one was heard.
        let mut attention = Attention::default();
        attention.name_heard(95 * second);
        attention.began(1, 11);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Hold(_))
        ));
    }

    /// A name that begins to be heard after a request's sound has ended
    /// was not said in it, and one heard in it is not lost to a later one.
    #[test]
    fn a_name_counts_for_a_request_only_if_it_began_before_the_requests_sound_ended() {
        let second = 1_000_000;
        // Someone else's "Hey Liam" a second after the person finished.
        let mut attention = Attention::default();
        attention.began_at(1, 20, 10 * second);
        attention.ended(1, 12 * second);
        attention.name_heard(13 * second);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Hold(_))
        ));
        // Said at the very end of the request, and read by the worker only
        // after its sound had ended: it is dated, and counts.
        let mut attention = Attention::default();
        attention.began_at(1, 21, 10 * second);
        attention.ended(1, 12 * second);
        attention.name_heard(12 * second);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Play(_))
        ));
        // Called by name, and then the other line: the call still counts.
        let mut attention = Attention::default();
        attention.name_heard(10 * second - 200_000);
        attention.began_at(1, 22, 10 * second);
        attention.ended(1, 12 * second);
        attention.name_heard(13 * second);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Play(_))
        ));
        // The late name is the next request's, if that begins soon enough,
        // and another request's end changes nothing.
        attention.began_at(2, 22, 14 * second);
        attention.ended(1, 14 * second);
        attention.name_heard(15 * second);
        assert!(matches!(
            attention.first_output(2, Room::Busy, false),
            Some(Verdict::Play(_))
        ));
    }

    /// A session's first two requests are the fresh ones; the count starts
    /// again with each new session, and a request is decided once.
    #[test]
    fn the_first_two_requests_of_each_session_are_the_ones_held() {
        let mut attention = Attention::default();
        let outcome = |attention: &mut Attention, request: u64, session: u64| {
            attention.began(request, session);
            attention.heard("How was your weekend?");
            attention.first_output(request, Room::Busy, false)
        };
        assert!(matches!(
            outcome(&mut attention, 1, 5),
            Some(Verdict::Hold(_))
        ));
        assert!(attention.holds(1) && !attention.holds(2));
        // Decided once: the room changing mid-answer changes nothing.
        assert_eq!(attention.first_output(1, Room::Quiet, true), None);
        assert!(attention.holds(1));
        assert!(matches!(
            outcome(&mut attention, 2, 5),
            Some(Verdict::Hold(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 3, 5),
            Some(Verdict::Play(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 4, 5),
            Some(Verdict::Play(_))
        ));
        // A new session: its first two again.
        assert!(matches!(
            outcome(&mut attention, 5, 6),
            Some(Verdict::Hold(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 6, 6),
            Some(Verdict::Hold(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 7, 6),
            Some(Verdict::Play(_))
        ));
        // Another request's output decides nothing.
        attention.began(8, 7);
        assert_eq!(attention.first_output(7, Room::Busy, false), None);
        // A turn the lamp began itself is played whatever was heard.
        attention.began(9, 8);
        attention.heard("so did you send the report");
        OWN_REQUEST.store(9, Ordering::Release);
        assert!(matches!(
            attention.first_output(9, Room::Busy, false),
            Some(Verdict::Play("the lamp began this turn itself"))
        ));
        OWN_REQUEST.store(0, Ordering::Release);
    }

    #[test]
    fn what_is_kept_of_one_request_is_bounded_and_cut_at_a_character() {
        let mut attention = Attention::default();
        attention.began(1, 1);
        attention.heard(&"é".repeat(MOST_HEARD_BYTES));
        assert_eq!(attention.heard.len(), MOST_HEARD_BYTES);
        attention.heard("Lamp");
        assert_eq!(attention.heard.len(), MOST_HEARD_BYTES);
        assert!(attention.heard.chars().all(|c| c == 'é'));
    }
}
