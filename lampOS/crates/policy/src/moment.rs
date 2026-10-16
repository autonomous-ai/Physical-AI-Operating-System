//! The decision "is this a good moment for the lamp to start this itself?"
//! (`docs/policy.md`, "Asked by a skill: the gate"), for a skill's ask whose
//! act is `news` (news of work) or `unprompted` (anything else the lamp
//! would start: a stretch break, stuffy air), #939. A hello is the hello
//! question's ([`crate::hello`]).
//!
//! One hard rule sits before the decider: never while anyone is talking to
//! the lamp (the voice listening, thinking or speaking, or anything heard
//! or said in the last 15 s, as the hello question knows it). While it
//! holds the answer is `wait`, by the rule, and no model is asked. Then one
//! question, `good_moment`, from the progress question's own, put to the
//! decision model on a connection of its own; with no answer in time, a
//! rule. The answer is published for the skill to act on (`shadow`
//! false), naming the ask.
use crate::{
    attention::Room,
    decide::{Answer, Asked, Connection, Decider, Failed, Question, Usage},
    hello::Hold,
    progress::{Choice, LocalTime},
};
use lamp_msgs::{Decision, SkillAsk};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

/// What this decision is called on the bus and in the log.
pub const ABOUT: &str = "moment";
/// From this probability up, the lamp may.
pub const SAY_FROM: f32 = 0.5;
/// The most one asking may take: the asker waits about 2 s.
pub const MOST_ASKING: Duration = Duration::from_millis(1_500);
/// An ask not decided within this long is let go undecided: its asker has
/// stopped waiting.
pub const ASKER_WAITS: Duration = Duration::from_secs(2);
/// No more asks than this wait to be decided.
const MOST_WAITING: usize = 4;
/// What the rule gives when no model answers. Authored, not measured.
pub const RULE_YES: f32 = 0.6;
pub const RULE_NO: f32 = 0.1;

/// How the question is asked (`lamp-policy --gate`).
#[derive(Clone, Debug)]
pub struct Options {
    /// How long one asking may take: at most [`MOST_ASKING`].
    pub deadline: Duration,
    /// The lamp's local time, for the hour of the day: a fixed offset
    /// (`--utc-offset-minutes`) or a zone (`--zone`, #820), as the hello
    /// question's.
    pub local: Option<Arc<LocalTime>>,
}

impl Options {
    pub fn new(deadline: Duration, local: Option<Arc<LocalTime>>) -> Self {
        Self {
            deadline: deadline.min(MOST_ASKING),
            local,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Judgement {
    pub probability: f32,
    pub choice: Choice,
    /// No model's answers are behind it: a rule decided.
    pub from_rule: bool,
}

/// Everything known when the question was put: the ask, and the room as the
/// policy knows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Context {
    pub ask: u64,
    pub skill: String,
    pub act: String,
    /// Whether the eyes see someone near; not known without a list of faces
    /// younger than 2 s.
    pub someone_here: Option<bool>,
    /// Seconds since anything was said to or by the lamp.
    pub quiet_s: u32,
    pub room: Room,
    pub spoke_up_last_hour: u32,
    pub hour: Option<u8>,
}

const LAMP: &str = "A small desk lamp robot on the desk of the person it works for: it keeps quiet unless there is a good reason, and when it starts something itself it is brief.";

fn room_told(room: Room) -> &'static str {
    match room {
        Room::Quiet => "quiet: nobody else has been talking",
        Room::Busy => "busy: people are talking to each other",
        Room::Unknown => "unknown",
    }
}

fn what_told(act: &str) -> &'static str {
    match act {
        "news" => "tell one short piece of news about work it handed to a coding agent",
        _ => "start something small itself, such as inviting a stretch or noting the air",
    }
}

/// The state as the text that is sent, its fields in this order. The
/// skill's name is a folder's name, not anything a person said.
pub fn state_text(context: &Context) -> String {
    format!(
        "{{\"lamp\":{},\"what\":{{\"skill\":{},\"the_lamp_would\":{}}},\"room\":{{\
         \"someone_at_the_lamp\":{},\"seconds_since_anything_was_said\":{},\"room\":{},\
         \"times_the_lamp_spoke_up_unasked_in_the_last_hour\":{},\"hour_of_day\":{}}}}}",
        json!(LAMP),
        json!(context.skill),
        json!(what_told(&context.act)),
        json!(context.someone_here),
        context.quiet_s,
        json!(room_told(context.room)),
        context.spoke_up_last_hour,
        json!(context.hour),
    )
}

/// [`state_text`] as a value, for the log.
pub fn state(context: &Context) -> Value {
    serde_json::from_str(&state_text(context)).expect("the state is JSON")
}

const OPENING: &str = "state.what is something the lamp would start itself, unasked; state.room is how things are around the lamp now. ";

/// The one question: the progress question's `good_moment`, for anything
/// the lamp would start. Authored, not fitted.
const ASKED: (&str, &str) = (
    "good_moment",
    "Is this a good moment for the lamp to speak up unasked, briefly: someone is at the lamp, nobody has spoken for a little while, the room is not busy with other people's talk, the lamp has not spoken up unasked often in the last hour, and it is not late at night or early in the morning?",
);

/// The question, as it is asked.
pub fn questions() -> Vec<(String, Question)> {
    vec![(
        ASKED.0.to_owned(),
        Question::Noul {
            instructions: format!("{OPENING}{}", ASKED.1),
        },
    )]
}

/// The answer to `good_moment` is the probability; or the question's name,
/// if it is missing.
pub fn combine(
    answers: &BTreeMap<String, Answer>,
) -> Result<(Judgement, BTreeMap<String, f32>), &'static str> {
    let yes = match answers.get(ASKED.0) {
        Some(Answer::Noul(yes)) if (0.0..=1.0).contains(yes) => *yes,
        _ => return Err(ASKED.0),
    };
    Ok((
        Judgement {
            probability: yes,
            choice: if yes >= SAY_FROM {
                Choice::SayNow
            } else {
                Choice::Wait
            },
            from_rule: false,
        },
        BTreeMap::from([(ASKED.0.to_owned(), yes)]),
    ))
}

/// The judgement when no model answers: yes unless nobody is known to be
/// there, the room is known to be busy, the lamp spoke up unasked twice or
/// more in the hour, or it is before eight in the morning or after ten at
/// night (when the hour is known).
pub fn fallback(context: &Context) -> Judgement {
    let yes = context.someone_here != Some(false)
        && context.room != Room::Busy
        && context.spoke_up_last_hour < 2
        && context.hour.is_none_or(|hour| (8..22).contains(&hour));
    Judgement {
        probability: if yes { RULE_YES } else { RULE_NO },
        choice: if yes { Choice::SayNow } else { Choice::Wait },
        from_rule: true,
    }
}

/// One line of the decisions log for this decision.
#[derive(Clone, Debug, Serialize)]
pub struct Line {
    pub at_unix_ms: u64,
    pub about: &'static str,
    pub shadow: bool,
    pub context: Context,
    pub state: Value,
    pub asked: String,
    pub answers: BTreeMap<String, f32>,
    pub judgement: Judgement,
    pub by: String,
    pub took_ms: u32,
    /// A hard rule that answered `wait`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard: Option<Hold>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<Connection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// A decision made: what goes on the bus, what is written down.
#[derive(Clone, Debug)]
pub struct Decided {
    pub decision: Decision,
    pub line: Line,
}

/// What was counted from start to stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub decided: u64,
    pub by_rule: u64,
    pub said: u64,
    pub held_talking: u64,
    /// Asks let go undecided: their askers had stopped waiting.
    pub late: u64,
}

type Answered = (Result<Asked, Failed>, Duration);

/// What the room is like now, as the policy and the hello question know it,
/// for a turn of the question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Now {
    pub room: Room,
    pub someone_here: Option<bool>,
    pub quiet_s: u32,
    pub talking: bool,
    pub spoke_up_last_hour: u32,
}

/// The question as it runs beside the policy's loop, with its own thread
/// and its own decision model.
pub struct Moment {
    options: Options,
    judge_name: String,
    states: mpsc::SyncSender<(String, Duration)>,
    answers: mpsc::Receiver<Answered>,
    asker_gone: bool,
    /// The question out: what it is about, when it was sent, and when its
    /// ask was received, which its 2 s count from.
    asking: Option<(Context, Instant, Instant)>,
    waiting: VecDeque<(SkillAsk, Instant)>,
    totals: Totals,
}

impl Moment {
    /// Start the thread that asks `decider`.
    pub fn start<D: Decider + Send + 'static>(
        mut decider: D,
        options: Options,
    ) -> io::Result<Self> {
        let judge_name = decider.name().to_owned();
        let (states, asked) = mpsc::sync_channel::<(String, Duration)>(1);
        let (answered, answers) = mpsc::channel();
        thread::Builder::new()
            .name("ask-moment".into())
            .spawn(move || {
                let questions = questions();
                // Each state with what is left of its asker's wait.
                for (state, deadline) in asked {
                    let started = Instant::now();
                    let answer = decider.ask(&state, &questions, deadline);
                    if answered.send((answer, started.elapsed())).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            options,
            judge_name,
            states,
            answers,
            asker_gone: false,
            asking: None,
            waiting: VecDeque::new(),
            totals: Totals::default(),
        })
    }

    pub fn totals(&self) -> Totals {
        self.totals
    }

    /// Whether a question is out or an ask waits: its asker waits, so the
    /// loop looks again sooner.
    pub fn busy(&self) -> bool {
        self.asking.is_some() || !self.waiting.is_empty()
    }

    /// A skill asks whether the lamp may start `news` or something
    /// `unprompted` now. Any other ask is passed over. Past [`MOST_WAITING`]
    /// the oldest is let go.
    pub fn asked(&mut self, ask: &SkillAsk, now: Instant) {
        if ask.check().is_err() || !["news", "unprompted"].contains(&ask.act.as_str()) {
            return;
        }
        if self.waiting.len() == MOST_WAITING {
            self.waiting.pop_front();
            self.totals.late += 1;
        }
        self.waiting.push_back((ask.clone(), now));
    }

    /// Take up an answer if one has come; else decide the next ask, by the
    /// hard rule or by putting the question. At most one decision a turn.
    pub fn turn(&mut self, now: Instant, unix_ms: u64, room_now: Now) -> Option<Decided> {
        if self.asking.is_some() {
            let (answer, took) = match self.answers.try_recv() {
                Ok((answer, took)) => Some((Some(answer), took)),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.asker_gone = true;
                    Some((None, Duration::ZERO))
                }
            }?;
            let (context, sent, received) = self.asking.take().expect("a question is out");
            // An answer after its asker stopped waiting is let go, and
            // counted late, never published as decided (the reviewer on
            // #958, 12:19).
            if now.saturating_duration_since(received) >= ASKER_WAITS {
                self.totals.late += 1;
                return None;
            }
            let took = match answer {
                Some(_) => took,
                None => now.saturating_duration_since(sent),
            };
            // Someone began to talk while the model was asked, or while its
            // asker went away: the hard rule holds by how things are now,
            // never the fallback's `say_now` (Fourth on #958).
            let held = room_now.talking.then_some(Hold::Talking);
            return Some(self.decide(context, answer, held, took, unix_ms));
        }
        while self
            .waiting
            .front()
            .is_some_and(|(_, at)| now.saturating_duration_since(*at) >= ASKER_WAITS)
        {
            self.waiting.pop_front();
            self.totals.late += 1;
        }
        let (ask, received) = self.waiting.pop_front()?;
        let context = Context {
            ask: ask.ask,
            skill: ask.skill,
            act: ask.act,
            someone_here: room_now.someone_here,
            quiet_s: room_now.quiet_s,
            room: room_now.room,
            spoke_up_last_hour: room_now.spoke_up_last_hour,
            hour: self
                .options
                .local
                .as_ref()
                .map(|local| local.hour_of_day(unix_ms)),
        };
        if room_now.talking {
            return Some(self.decide(context, None, Some(Hold::Talking), Duration::ZERO, unix_ms));
        }
        if self.asker_gone {
            return Some(self.decide(context, None, None, Duration::ZERO, unix_ms));
        }
        // The model is given only what is left of the 2 s; with nothing
        // left, the rule answers now.
        let deadline = crate::hello::time_left(
            ASKER_WAITS,
            received,
            now,
            self.options.deadline.min(MOST_ASKING),
        );
        if deadline.is_zero() {
            let failed = Some(Err(Failed::TimedOut));
            return Some(self.decide(context, failed, None, Duration::ZERO, unix_ms));
        }
        match self.states.try_send((state_text(&context), deadline)) {
            Ok(()) => {
                self.asking = Some((context, now, received));
                None
            }
            Err(_) => {
                self.asker_gone = true;
                Some(self.decide(context, None, None, Duration::ZERO, unix_ms))
            }
        }
    }

    /// Decide from the model's answers, by the hard rule (`held`), or by the
    /// rule when there are no answers to use.
    fn decide(
        &mut self,
        context: Context,
        answer: Option<Result<Asked, Failed>>,
        held: Option<Hold>,
        took: Duration,
        unix_ms: u64,
    ) -> Decided {
        let connection = answer
            .as_ref()
            .and_then(|answer| answer.as_ref().ok())
            .and_then(|asked| asked.connection);
        let rule = || crate::policy::RULE.to_owned();
        let (mut judgement, answers, by, failed, usage) = match (held, answer) {
            (Some(_), _) => (
                Judgement {
                    probability: RULE_NO,
                    choice: Choice::Wait,
                    from_rule: true,
                },
                BTreeMap::new(),
                rule(),
                None,
                None,
            ),
            (None, Some(Ok(asked))) => match combine(&asked.answers) {
                Ok((judgement, answers)) => (judgement, answers, asked.model, None, asked.usage),
                Err(question) => (
                    fallback(&context),
                    BTreeMap::new(),
                    rule(),
                    Some(Failed::Missing(question.to_owned())),
                    asked.usage,
                ),
            },
            (None, Some(Err(failed))) => (
                fallback(&context),
                BTreeMap::new(),
                rule(),
                Some(failed),
                None,
            ),
            (None, None) => (
                fallback(&context),
                BTreeMap::new(),
                rule(),
                Some(Failed::Unreachable {
                    during: "asking",
                    kind: io::ErrorKind::BrokenPipe,
                }),
                None,
            ),
        };
        if held.is_some() {
            judgement.choice = Choice::Wait;
            self.totals.held_talking += 1;
        }
        if judgement.choice == Choice::SayNow {
            self.totals.said += 1;
        }
        self.totals.decided += 1;
        self.totals.by_rule += u64::from(judgement.from_rule);
        let took_ms = u32::try_from(took.as_millis()).unwrap_or(u32::MAX);
        let decision = Decision {
            about: ABOUT.to_owned(),
            choice: judgement.choice.as_str().to_owned(),
            probability: judgement.probability,
            answers: answers.clone(),
            by: by.clone(),
            took_ms,
            shadow: false,
            request: None,
            task: None,
            arrival: None,
            ask: Some(context.ask),
        };
        let line = Line {
            at_unix_ms: unix_ms,
            about: ABOUT,
            shadow: false,
            state: state(&context),
            context,
            asked: self.judge_name.clone(),
            answers,
            judgement,
            by,
            took_ms,
            guard: held,
            connection,
            failed: failed.as_ref().map(Failed::kind),
            failed_why: failed.as_ref().map(Failed::to_string),
            usage,
        };
        Decided { decision, line }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(act: &str) -> Context {
        Context {
            ask: 1_792_115_940_123_456,
            skill: "stretch-break".into(),
            act: act.into(),
            someone_here: Some(true),
            quiet_s: 340,
            room: Room::Quiet,
            spoke_up_last_hour: 0,
            hour: Some(14),
        }
    }

    #[test]
    fn the_state_is_this_text_to_the_byte_and_holds_no_words_of_anyone() {
        assert_eq!(
            state_text(&context("unprompted")),
            "{\"lamp\":\"A small desk lamp robot on the desk of the person it works for: it keeps quiet unless there is a good reason, and when it starts something itself it is brief.\",\
             \"what\":{\"skill\":\"stretch-break\",\"the_lamp_would\":\"start something small itself, such as inviting a stretch or noting the air\"},\
             \"room\":{\"someone_at_the_lamp\":true,\"seconds_since_anything_was_said\":340,\"room\":\"quiet: nobody else has been talking\",\
             \"times_the_lamp_spoke_up_unasked_in_the_last_hour\":0,\"hour_of_day\":14}}"
        );
        assert!(state_text(&context("news")).contains("one short piece of news"));
        assert_eq!(questions().len(), 1);
    }

    #[test]
    fn good_moment_is_the_probability_and_the_rule_keeps_quiet_when_it_may_not_be() {
        let answers = BTreeMap::from([("good_moment".to_owned(), Answer::Noul(0.71))]);
        let (judgement, by_name) = combine(&answers).unwrap();
        assert_eq!(
            (
                judgement.choice,
                judgement.probability,
                by_name["good_moment"]
            ),
            (Choice::SayNow, 0.71, 0.71)
        );
        assert_eq!(combine(&BTreeMap::new()).unwrap_err(), "good_moment");
        assert_eq!(fallback(&context("news")).choice, Choice::SayNow);
        for not_now in [
            Context {
                someone_here: Some(false),
                ..context("news")
            },
            Context {
                room: Room::Busy,
                ..context("news")
            },
            Context {
                spoke_up_last_hour: 2,
                ..context("news")
            },
            Context {
                hour: Some(23),
                ..context("news")
            },
        ] {
            assert_eq!(fallback(&not_now).choice, Choice::Wait, "{not_now:?}");
        }
        // Not known is not no.
        assert_eq!(
            fallback(&Context {
                someone_here: None,
                hour: None,
                ..context("news")
            })
            .choice,
            Choice::SayNow
        );
    }

    /// Holds each asking until let go, and keeps the time each was given.
    struct Held {
        go: mpsc::Receiver<()>,
        given: std::sync::Arc<std::sync::Mutex<Vec<Duration>>>,
    }
    impl Decider for Held {
        fn ask(
            &mut self,
            _: &str,
            _: &[(String, Question)],
            deadline: Duration,
        ) -> Result<Asked, Failed> {
            self.given.lock().unwrap().push(deadline);
            self.go.recv().map_err(|_| Failed::TimedOut)?;
            Ok(Asked {
                answers: BTreeMap::from([("good_moment".to_owned(), Answer::Noul(0.9))]),
                model: "held".into(),
                took: Duration::ZERO,
                usage: None,
                connection: None,
            })
        }
        fn name(&self) -> &str {
            "held"
        }
    }

    /// The reviewer on #958 (12:19): an ask's 2 s count from when it was
    /// received. A, asked at once, is given 1.5 s and answered in time; B,
    /// received 0.1 s after A and taken up when A is decided at 1.5 s, is
    /// given what is left (0.4 s), and its answer at 2.2 s is let go and
    /// counted late, never published. One with too little left is answered
    /// by the rule at once.
    #[test]
    fn an_asks_two_seconds_count_from_its_receipt_and_a_late_answer_is_let_go() {
        let ms = Duration::from_millis;
        let (go, gate) = mpsc::channel();
        let given = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let decider = Held {
            go: gate,
            given: given.clone(),
        };
        let mut moment =
            Moment::start(decider, Options::new(Duration::from_secs(3), None)).unwrap();
        let ask = |number: u64| SkillAsk {
            ask: number,
            skill: "stretch-break".into(),
            act: "unprompted".into(),
            person: None,
            away_min: None,
            there_s: None,
        };
        let room_now = Now {
            room: Room::Quiet,
            someone_here: Some(true),
            quiet_s: 300,
            talking: false,
            spoke_up_last_hour: 0,
        };
        // What the asker was given, once it has taken up `count` askings.
        let given_now = |count: usize| {
            for _ in 0..500 {
                if given.lock().unwrap().len() >= count {
                    break;
                }
                std::thread::sleep(ms(2));
            }
            given.lock().unwrap().clone()
        };
        let t = Instant::now();
        moment.asked(&ask(1), t);
        moment.asked(&ask(2), t + ms(100));
        assert!(moment.turn(t, 1_000_000, room_now).is_none(), "A is out");
        go.send(()).unwrap();
        let a = (0..500)
            .find_map(|_| {
                std::thread::sleep(ms(2));
                moment.turn(t + ms(1_500), 1_000_000, room_now)
            })
            .expect("A decided");
        assert_eq!(
            (a.decision.ask, a.decision.choice.as_str()),
            (Some(1), "say_now")
        );
        assert!(
            moment.turn(t + ms(1_500), 1_000_000, room_now).is_none(),
            "B is out"
        );
        assert_eq!(given_now(2), [MOST_ASKING, ms(400)]);
        go.send(()).unwrap();
        for _ in 0..500 {
            if moment.totals().late == 1 {
                break;
            }
            let late = moment.turn(t + ms(2_200), 1_000_000, room_now);
            assert!(late.is_none(), "B published late");
            std::thread::sleep(ms(2));
        }
        assert_eq!((moment.totals().late, moment.totals().decided), (1, 1));
        // Received at 3 s and taken up at 4.85 s: the rule, at once.
        moment.asked(&ask(3), t + ms(3_000));
        let ruled = moment
            .turn(t + ms(4_850), 1_000_000, room_now)
            .expect("the rule at once");
        assert_eq!(
            (ruled.decision.ask, ruled.decision.by.as_str()),
            (Some(3), crate::policy::RULE)
        );
        assert_eq!(ruled.line.failed, Some("timed_out"));
        assert_eq!(given_now(2).len(), 2);
    }

    /// Its thread ends at the first asking, as a model's client that failed
    /// hard would.
    struct Gone;
    impl Decider for Gone {
        fn ask(&mut self, _: &str, _: &[(String, Question)], _: Duration) -> Result<Asked, Failed> {
            panic!("this test's asker is gone, on purpose");
        }
        fn name(&self) -> &str {
            "gone"
        }
    }

    /// Fourth on #958: an asker gone while someone talks is held by the hard
    /// rule, as an answer is; the fallback's `say_now` is never taken then.
    #[test]
    fn an_asker_gone_while_someone_talks_is_held_by_the_hard_rule() {
        let mut moment = Moment::start(Gone, Options::new(Duration::from_secs(3), None)).unwrap();
        let quiet = Now {
            room: Room::Quiet,
            someone_here: Some(true),
            quiet_s: 300,
            talking: false,
            spoke_up_last_hour: 0,
        };
        // With nobody talking, the fallback would say now.
        assert_eq!(fallback(&context("unprompted")).choice, Choice::SayNow);
        let t = Instant::now();
        assert!(!moment.busy());
        moment.asked(
            &SkillAsk {
                ask: 1,
                skill: "stretch-break".into(),
                act: "unprompted".into(),
                person: None,
                away_min: None,
                there_s: None,
            },
            t,
        );
        assert!(moment.busy());
        assert!(moment.turn(t, 1_000_000, quiet).is_none(), "out");
        let talking = Now {
            talking: true,
            ..quiet
        };
        let held = (0..500)
            .find_map(|_| {
                std::thread::sleep(Duration::from_millis(2));
                moment.turn(t + Duration::from_millis(100), 1_000_000, talking)
            })
            .expect("decided once the asker went");
        assert_eq!(
            (
                held.decision.choice.as_str(),
                held.decision.by.as_str(),
                held.line.guard
            ),
            ("wait", crate::policy::RULE, Some(Hold::Talking))
        );
        assert_eq!(moment.totals().held_talking, 1);
        assert!(!moment.busy());
    }
}
