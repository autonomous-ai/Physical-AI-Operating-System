//! The decision "say hello now?" (`docs/policy.md`, "Hello on arrival").
//!
//! The voice says hello to someone who comes back and sits down
//! (`speak_up`), by a fixed rule: someone there for three seconds after five
//! minutes away, half a minute of quiet, no hello in twenty minutes. With
//! `LAMP_HELLO_ASKS_POLICY=on` it says so on `voice/arrival` when a hello
//! is due and holds it up to 2 s for this decision: `say_now` says it,
//! `wait` keeps quiet, and no answer in time leaves it to its rule.
//!
//! Two hard rules sit around the decider and are not left to it: never
//! while anyone is talking to the lamp (the voice listening, thinking or
//! speaking, or anything heard or said in the last 15 s), and no hello
//! within twenty minutes of the last, the voice's own limit. While either
//! holds the answer is `wait`, by the rule, and no model is asked: the voice
//! is waiting, and a question not answered would leave it to its rule.
//!
//! Shadow unless `lamp-policy --act hello`: decided, published and written
//! down, and the voice says hello by its own rule.
use crate::{
    attention::Room,
    decide::{Answer, Asked, Connection, Decider, Failed, Question, Usage},
    progress::{Choice, hour_of_day},
};
use lamp_msgs::{Decision, Voice, VoiceArrival};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// What this decision is called on the bus and in the log.
pub const ABOUT: &str = "hello";
/// From this probability up, the lamp says hello now.
pub const SAY_FROM: f32 = 0.5;
/// No hello within this long of the last: the voice's own limit
/// (`speak_up`, twenty minutes).
pub const AT_MOST_EVERY: Duration = Duration::from_secs(20 * 60);
/// Nobody is talking to the lamp: the same quiet as the progress question.
pub const QUIET_FOR: Duration = Duration::from_secs(15);
/// A turn of the lamp's that began with nobody listened to in this long
/// before it is one it began itself, unasked (as the progress question).
pub const UNASKED_AFTER: Duration = Duration::from_secs(10);
/// The voice holds a due hello this long for the decision.
pub const VOICE_WAITS: Duration = Duration::from_secs(2);
/// In shadow, a hello chosen and a turn the lamp began itself (its first
/// `thinking` or `speaking`) this close together are one greeting: the voice
/// said its own rule's hello for the same arrival, after holding it for the
/// decision (the reviewer on #883). The turn is followed from its beginning
/// to its end, however late its first sound comes (the model may take up to
/// 30 s to answer): spoken, it is counted once, as a turn the lamp began;
/// ended without a sound, the hello chosen is counted once, as chosen.
pub const ONE_GREETING_WITHIN: Duration = Duration::from_secs(5);

/// A turn the lamp began itself, unasked, from its beginning to its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnTurn {
    /// The shadow hello it is the greeting of, taken from `notes`.
    claimed: Option<Instant>,
    /// Its sound began: it is counted in `unasked`.
    spoke: bool,
}
/// The most one asking may take, whatever `--deadline-ms` says: a model late
/// past it leaves the rule time to answer before the voice stops waiting.
pub const MOST_ASKING: Duration = Duration::from_millis(1_500);
const HOUR: Duration = Duration::from_secs(3_600);
/// No more arrivals than this wait to be decided; there is one voice.
const MOST_WAITING: usize = 4;
/// What the rule gives when no model answers. Authored, not measured.
pub const RULE_YES: f32 = 0.6;
pub const RULE_NO: f32 = 0.1;

/// How the hello question is asked (`lamp-policy --hello`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// How long one asking may take: at most [`MOST_ASKING`].
    pub deadline: Duration,
    /// The lamp's local time, east of UTC, for the hour of the day.
    pub utc_offset_minutes: Option<i32>,
    /// The hard rules ([`QUIET_FOR`], [`AT_MOST_EVERY`]); shorter only in a
    /// test.
    pub quiet_for: Duration,
    pub at_most_every: Duration,
    /// The decisions act (`--act hello`): published with `shadow: false`,
    /// and the voice says hello or not as they say. Off unless set.
    pub act: bool,
}

impl Options {
    pub fn new(deadline: Duration, utc_offset_minutes: Option<i32>) -> Self {
        Self {
            deadline: deadline.min(MOST_ASKING),
            utc_offset_minutes,
            quiet_for: QUIET_FOR,
            at_most_every: AT_MOST_EVERY,
            act: false,
        }
    }
}

/// A hard rule that answered `wait` without a model, or turned a yes to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hold {
    /// Someone is talking to the lamp, or was a moment ago.
    Talking,
    /// The lamp said hello less than twenty minutes ago.
    GreetedRecently,
}

impl Hold {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Talking => "talking",
            Self::GreetedRecently => "greeted_recently",
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

/// Everything known when the question was put: the arrival, from
/// `voice/arrival`, and the room, from the lamp's own topics.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Context {
    pub arrival: u64,
    pub away_min: u32,
    pub there_s: u32,
    /// Seconds since anything was said to or by the lamp, as the voice
    /// says it.
    pub quiet_s: u32,
    pub since_hello_min: Option<u32>,
    /// Whether the eyes see someone near, and whether such a face is turned
    /// to the lamp; not known without a list of faces younger than 2 s.
    pub someone_here: Option<bool>,
    pub facing: Option<bool>,
    pub room: Room,
    pub spoke_up_last_hour: u32,
    pub hour: Option<u8>,
}

const LAMP: &str = "A small desk lamp robot on the desk of the person it works for, like a friend at the next desk: when someone comes back and sits down it may greet them with a few warm words, and otherwise it keeps quiet.";

fn room_told(room: Room) -> &'static str {
    match room {
        Room::Quiet => "quiet: nobody else has been talking",
        Room::Busy => "busy: people are talking to each other",
        Room::Unknown => "unknown",
    }
}

/// The state as the text that is sent, its fields in this order.
pub fn state_text(context: &Context) -> String {
    format!(
        "{{\"lamp\":{},\"arrival\":{{\"minutes_away\":{},\"seconds_there\":{},\
         \"minutes_since_the_lamp_last_said_hello\":{}}},\"room\":{{\"someone_at_the_lamp\":{},\
         \"facing_the_lamp\":{},\"seconds_since_anything_was_said\":{},\"room\":{},\
         \"times_the_lamp_spoke_up_unasked_in_the_last_hour\":{},\"hour_of_day\":{}}}}}",
        json!(LAMP),
        context.away_min,
        context.there_s,
        json!(context.since_hello_min),
        json!(context.someone_here),
        json!(context.facing),
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

const OPENING: &str = "state.arrival says someone has just come back to the lamp's desk and sat down in front of it; state.room is how things are around the lamp now. ";

/// The two questions: each one's name and the rest of its instruction after
/// [`OPENING`]. Authored, not fitted.
const ASKED: [(&str, &str); 2] = [
    (
        "welcome",
        "Would a friend at the next desk greet them now with a few warm words, given how long they were away, whether they are turned to the lamp, and the hour?",
    ),
    (
        "intrusive",
        "Would a hello now more likely interrupt or annoy them than please them: the room is busy with other people's talk, it is late at night or early in the morning, they were away only a short while, or the lamp has spoken up unasked often in the last hour?",
    ),
];

/// The two questions, in the order they are asked in.
pub fn questions() -> Vec<(String, Question)> {
    ASKED
        .iter()
        .map(|(name, rest)| {
            (
                (*name).to_owned(),
                Question::Noul {
                    instructions: format!("{OPENING}{rest}"),
                },
            )
        })
        .collect()
}

/// Welcome, and not intrusive, as if the two were independent; each answer
/// by its name, or the name of the first that is missing.
pub fn combine(
    answers: &BTreeMap<String, Answer>,
) -> Result<(Judgement, BTreeMap<String, f32>), &'static str> {
    let mut two = [0.0f32; 2];
    for (to, (name, _)) in two.iter_mut().zip(ASKED) {
        match answers.get(name) {
            Some(Answer::Noul(yes)) if (0.0..=1.0).contains(yes) => *to = *yes,
            _ => return Err(name),
        }
    }
    let [welcome, intrusive] = two;
    let probability = (welcome * (1.0 - intrusive)).clamp(0.0, 1.0);
    let by_name = ASKED
        .iter()
        .zip(two)
        .map(|((name, _), yes)| ((*name).to_owned(), yes))
        .collect();
    Ok((
        Judgement {
            probability,
            choice: if probability >= SAY_FROM {
                Choice::SayNow
            } else {
                Choice::Wait
            },
            from_rule: false,
        },
        by_name,
    ))
}

/// The judgement when no model answers: a hello unless the room is known to
/// be busy, the lamp spoke up unasked twice or more in the hour, or it is
/// before eight in the morning or after ten at night (when the hour is
/// known). The voice's own rule has held everything else already.
pub fn fallback(context: &Context) -> Judgement {
    let yes = context.room != Room::Busy
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
    /// A hard rule that answered `wait`, or turned a yes to it.
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
    /// Arrivals answered `wait` by each hard rule.
    pub held_talking: u64,
    pub held_recent: u64,
    /// Arrivals not decided before the voice stopped waiting for them.
    pub late: u64,
}

type Answered = (Result<Asked, Failed>, Duration);

/// The hello question as it runs beside the policy's loop, with its own
/// thread and its own decision model.
pub struct Hello {
    options: Options,
    judge_name: String,
    states: mpsc::SyncSender<String>,
    answers: mpsc::Receiver<Answered>,
    asker_gone: bool,
    asking: Option<(Context, Instant)>,
    waiting: VecDeque<(VoiceArrival, Instant)>,
    voice: Option<Voice>,
    last_talk: Option<Instant>,
    last_listening: Option<Instant>,
    unasked: VecDeque<Instant>,
    /// Hellos chosen, which in shadow nobody hears, and the last one that
    /// acted.
    notes: VecDeque<Instant>,
    /// The turn the lamp began itself, while it lasts.
    own_turn: Option<OwnTurn>,
    greeted: Option<Instant>,
    totals: Totals,
}

impl Hello {
    /// Start the thread that asks `decider`.
    pub fn start<D: Decider + Send + 'static>(
        mut decider: D,
        options: Options,
    ) -> io::Result<Self> {
        let judge_name = decider.name().to_owned();
        let (states, asked) = mpsc::sync_channel::<String>(1);
        let (answered, answers) = mpsc::channel();
        let deadline = options.deadline.min(MOST_ASKING);
        thread::Builder::new()
            .name("ask-hello".into())
            .spawn(move || {
                let questions = questions();
                for state in asked {
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
            voice: None,
            last_talk: None,
            last_listening: None,
            unasked: VecDeque::new(),
            notes: VecDeque::new(),
            own_turn: None,
            greeted: None,
            totals: Totals::default(),
        })
    }

    pub fn totals(&self) -> Totals {
        self.totals
    }

    /// Whether its decisions act ([`Options::act`]).
    pub fn acts(&self) -> bool {
        self.options.act
    }

    /// Whether a question is out or an arrival waits: the voice is waiting,
    /// so the loop looks again sooner.
    pub fn busy(&self) -> bool {
        self.asking.is_some() || !self.waiting.is_empty()
    }

    /// A hello is due: it waits to be decided. One that fails its check is
    /// passed over; past [`MOST_WAITING`] the oldest is let go.
    pub fn arrival(&mut self, arrival: VoiceArrival, now: Instant) {
        if arrival.check().is_err() {
            return;
        }
        if self.waiting.len() == MOST_WAITING {
            self.waiting.pop_front();
            self.totals.late += 1;
        }
        self.waiting.push_back((arrival, now));
    }

    /// What the voice is doing, as the progress question takes it.
    pub fn voice(&mut self, state: Voice, now: Instant) {
        let unasked = self
            .last_listening
            .is_none_or(|at| now.saturating_duration_since(at) > UNASKED_AFTER);
        // A turn the lamp begins itself: its first `thinking` or `speaking`
        // with nobody listened to lately. In shadow, the hello chosen just
        // before is this very greeting, said by the voice's own rule after
        // holding it for the decision: it is taken from the notes now, and
        // given back if the turn ends without a sound ([`ONE_GREETING_WITHIN`]).
        if matches!(state, Voice::Thinking | Voice::Speaking)
            && !matches!(
                self.voice,
                Some(Voice::Listening | Voice::Thinking | Voice::Speaking)
            )
            && unasked
            && self.own_turn.is_none()
        {
            let claimed = (!self.options.act)
                .then(|| {
                    self.notes.iter().rposition(|chosen| {
                        now.saturating_duration_since(*chosen) <= ONE_GREETING_WITHIN
                    })
                })
                .flatten()
                .and_then(|at| self.notes.remove(at));
            self.own_turn = Some(OwnTurn {
                claimed,
                spoke: false,
            });
        }
        match state {
            Voice::Listening => {
                self.end_own_turn();
                self.last_listening = Some(now);
                self.last_talk = Some(now);
            }
            Voice::Speaking => {
                if self.voice != Some(Voice::Speaking) && unasked {
                    self.unasked.push_back(now);
                    if let Some(turn) = self.own_turn.as_mut() {
                        turn.spoke = true;
                    }
                }
                self.last_talk = Some(now);
            }
            Voice::Thinking => self.last_talk = Some(now),
            // The quiet begins when the voice stops, not at its last
            // heartbeat before: an active state is said again once a
            // second, so that one can be most of a second old (the
            // reviewer's finding on #751's progress question, the same).
            Voice::Resting => {
                self.end_own_turn();
                if matches!(
                    self.voice,
                    Some(Voice::Listening | Voice::Thinking | Voice::Speaking)
                ) {
                    self.last_talk = Some(now);
                }
            }
        }
        self.voice = Some(state);
    }

    /// The turn the lamp began itself is over. One that never made a sound
    /// was not heard: the hello it had taken is a hello chosen again, in its
    /// place among the notes.
    fn end_own_turn(&mut self) {
        if let Some(OwnTurn {
            claimed: Some(at),
            spoke: false,
        }) = self.own_turn.take()
        {
            let place = self.notes.partition_point(|chosen| *chosen <= at);
            self.notes.insert(place, at);
        }
    }

    pub fn voice_gone(&mut self) {
        self.end_own_turn();
        self.voice = None;
    }

    /// Something was heard, or said by the lamp.
    pub fn talk(&mut self, now: Instant) {
        self.last_talk = Some(now);
    }

    fn hold(&self, arrival: &VoiceArrival, now: Instant) -> Option<Hold> {
        let talking = matches!(
            self.voice,
            Some(Voice::Listening | Voice::Thinking | Voice::Speaking)
        ) || self
            .last_talk
            .is_some_and(|at| now.saturating_duration_since(at) < self.options.quiet_for);
        let every_min =
            u32::try_from(self.options.at_most_every.as_secs() / 60).unwrap_or(u32::MAX);
        let recent = arrival
            .since_hello_min
            .is_some_and(|minutes| minutes < every_min)
            || self
                .greeted
                .is_some_and(|at| now.saturating_duration_since(at) < self.options.at_most_every);
        if talking {
            Some(Hold::Talking)
        } else if recent {
            Some(Hold::GreetedRecently)
        } else {
            None
        }
    }

    /// Take up an answer if one has come; else decide the next arrival, by
    /// a hard rule or by putting the question. At most one decision a turn.
    pub fn turn(
        &mut self,
        now: Instant,
        unix_ms: u64,
        room: Room,
        (someone_here, facing): (Option<bool>, Option<bool>),
    ) -> Option<Decided> {
        for queue in [&mut self.unasked, &mut self.notes] {
            while queue
                .front()
                .is_some_and(|at| now.saturating_duration_since(*at) > HOUR)
            {
                queue.pop_front();
            }
        }
        if self.asking.is_some() {
            return match self.answers.try_recv() {
                Ok((answer, took)) => {
                    let (context, _) = self.asking.take().expect("a question is out");
                    Some(self.decide(context, Some(answer), None, took, now, unix_ms))
                }
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.asker_gone = true;
                    let (context, sent) = self.asking.take().expect("a question is out");
                    let took = now.saturating_duration_since(sent);
                    Some(self.decide(context, None, None, took, now, unix_ms))
                }
            };
        }
        // One the voice has stopped waiting for is let go undecided.
        while self
            .waiting
            .front()
            .is_some_and(|(_, at)| now.saturating_duration_since(*at) >= VOICE_WAITS)
        {
            self.waiting.pop_front();
            self.totals.late += 1;
        }
        let (arrival, _) = self.waiting.pop_front()?;
        let context = Context {
            arrival: arrival.arrival,
            away_min: arrival.away_min,
            there_s: arrival.there_s,
            quiet_s: arrival.quiet_s,
            since_hello_min: arrival.since_hello_min,
            someone_here,
            facing,
            room,
            // In shadow the hellos chosen are counted here, but for one the
            // voice also said by its own rule, which is counted once, as a
            // turn the lamp began ([`ONE_GREETING_WITHIN`]); acting, the
            // voice says them and `voice/state` counts them.
            spoke_up_last_hour: u32::try_from(
                self.unasked.len()
                    + if self.options.act {
                        0
                    } else {
                        self.notes.len()
                    },
            )
            .unwrap_or(u32::MAX),
            hour: self
                .options
                .utc_offset_minutes
                .map(|offset| hour_of_day(unix_ms, offset)),
        };
        if let Some(hold) = self.hold(&arrival, now) {
            match hold {
                Hold::Talking => self.totals.held_talking += 1,
                Hold::GreetedRecently => self.totals.held_recent += 1,
            }
            return Some(self.decide(context, None, Some(hold), Duration::ZERO, now, unix_ms));
        }
        if self.asker_gone {
            return Some(self.decide(context, None, None, Duration::ZERO, now, unix_ms));
        }
        match self.states.try_send(state_text(&context)) {
            Ok(()) => {
                self.asking = Some((context, now));
                None
            }
            Err(_) => {
                self.asker_gone = true;
                Some(self.decide(context, None, None, Duration::ZERO, now, unix_ms))
            }
        }
    }

    /// Decide from the model's answers, by a hard rule (`held`), or by the
    /// rule when there are no answers to use.
    fn decide(
        &mut self,
        context: Context,
        answer: Option<Result<Asked, Failed>>,
        held: Option<Hold>,
        took: Duration,
        now: Instant,
        unix_ms: u64,
    ) -> Decided {
        let connection = answer
            .as_ref()
            .and_then(|a| a.as_ref().ok())
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
        // The hard rules again, as the answer comes: someone began to talk
        // while the model was asked.
        let guard = held.or_else(|| {
            (judgement.choice == Choice::SayNow)
                .then(|| {
                    let arrival = VoiceArrival {
                        arrival: context.arrival,
                        away_min: context.away_min,
                        there_s: context.there_s,
                        quiet_s: context.quiet_s,
                        since_hello_min: context.since_hello_min,
                    };
                    self.hold(&arrival, now)
                })
                .flatten()
        });
        if guard.is_some() {
            judgement.choice = Choice::Wait;
        }
        if judgement.choice == Choice::SayNow {
            self.notes.push_back(now);
            self.totals.said += 1;
            if self.options.act {
                self.greeted = Some(now);
            }
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
            shadow: !self.options.act,
            request: None,
            task: None,
            arrival: Some(context.arrival),
        };
        let line = Line {
            at_unix_ms: unix_ms,
            about: ABOUT,
            shadow: !self.options.act,
            state: state(&context),
            context,
            asked: self.judge_name.clone(),
            answers,
            judgement,
            by,
            took_ms,
            guard,
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

    fn context() -> Context {
        Context {
            arrival: 98_123_456,
            away_min: 42,
            there_s: 3,
            quiet_s: 310,
            since_hello_min: Some(95),
            someone_here: Some(true),
            facing: Some(true),
            room: Room::Quiet,
            spoke_up_last_hour: 0,
            hour: Some(9),
        }
    }

    #[test]
    fn the_state_is_this_text_to_the_byte_and_holds_no_picture_or_name() {
        assert_eq!(
            state_text(&context()),
            "{\"lamp\":\"A small desk lamp robot on the desk of the person it works for, like a friend at the next desk: when someone comes back and sits down it may greet them with a few warm words, and otherwise it keeps quiet.\",\
             \"arrival\":{\"minutes_away\":42,\"seconds_there\":3,\"minutes_since_the_lamp_last_said_hello\":95},\
             \"room\":{\"someone_at_the_lamp\":true,\"facing_the_lamp\":true,\"seconds_since_anything_was_said\":310,\
             \"room\":\"quiet: nobody else has been talking\",\"times_the_lamp_spoke_up_unasked_in_the_last_hour\":0,\"hour_of_day\":9}}"
        );
        let unknown = Context {
            since_hello_min: None,
            someone_here: None,
            facing: None,
            hour: None,
            ..context()
        };
        let value = state(&unknown);
        for (part, field) in [
            ("arrival", "minutes_since_the_lamp_last_said_hello"),
            ("room", "someone_at_the_lamp"),
            ("room", "facing_the_lamp"),
            ("room", "hour_of_day"),
        ] {
            assert!(value[part][field].is_null(), "{field}");
        }
    }

    fn answers(two: [f32; 2]) -> BTreeMap<String, Answer> {
        questions()
            .into_iter()
            .zip(two)
            .map(|((name, _), yes)| (name, Answer::Noul(yes)))
            .collect()
    }

    struct Never;
    impl Decider for Never {
        fn ask(&mut self, _: &str, _: &[(String, Question)], _: Duration) -> Result<Asked, Failed> {
            Err(Failed::TimedOut)
        }
        fn name(&self) -> &str {
            "never"
        }
    }

    /// As for the progress question (#751 review, 15:26): `resting` after
    /// speech starts the quiet then, not at the last heartbeat before it;
    /// a `resting` said again moves nothing.
    #[test]
    fn the_quiet_before_a_hello_begins_when_the_voice_stops() {
        let mut hello = Hello::start(Never, Options::new(Duration::from_secs(1), None)).unwrap();
        let arrival = VoiceArrival {
            arrival: 1,
            away_min: 60,
            there_s: 3,
            quiet_s: 300,
            since_hello_min: None,
        };
        let t = Instant::now();
        hello.voice(Voice::Speaking, t);
        let stopped = t + Duration::from_millis(900);
        hello.voice(Voice::Resting, stopped);
        let quiet = hello.options.quiet_for;
        assert_eq!(hello.hold(&arrival, t + quiet), Some(Hold::Talking));
        hello.voice(Voice::Resting, stopped + Duration::from_secs(5));
        assert_eq!(
            hello.hold(&arrival, stopped + quiet - Duration::from_millis(1)),
            Some(Hold::Talking)
        );
        assert_eq!(hello.hold(&arrival, stopped + quiet), None);
    }

    #[test]
    fn welcome_and_not_intrusive_is_a_hello() {
        let (yes, named) = combine(&answers([0.9, 0.2])).unwrap();
        assert!((yes.probability - 0.72).abs() < 1e-6);
        assert_eq!((yes.choice, yes.from_rule), (Choice::SayNow, false));
        assert_eq!(named.len(), 2);
        assert_eq!(
            combine(&answers([0.9, 0.5])).unwrap().0.choice,
            Choice::Wait
        );
        assert_eq!(
            combine(&answers([0.4, 0.0])).unwrap().0.choice,
            Choice::Wait
        );
        let mut missing = answers([1.0, 0.0]);
        missing.remove("intrusive");
        assert_eq!(combine(&missing).unwrap_err(), "intrusive");
        let asked: Vec<String> = questions().into_iter().map(|(name, _)| name).collect();
        assert_eq!(asked, ["welcome", "intrusive"]);
    }

    #[test]
    fn the_rule_says_hello_unless_busy_late_or_after_two_turns_unasked() {
        assert_eq!(fallback(&context()).choice, Choice::SayNow);
        for changed in [
            Context {
                room: Room::Busy,
                ..context()
            },
            Context {
                spoke_up_last_hour: 2,
                ..context()
            },
            Context {
                hour: Some(23),
                ..context()
            },
            Context {
                hour: Some(6),
                ..context()
            },
        ] {
            let judged = fallback(&changed);
            assert_eq!((judged.choice, judged.probability), (Choice::Wait, RULE_NO));
        }
        // Asking never takes longer than the voice can wait.
        assert_eq!(
            Options::new(Duration::from_secs(3), None).deadline,
            MOST_ASKING
        );
        assert!(MOST_ASKING < VOICE_WAITS);
    }

    /// The decision for `arrival` made at `now`, by the rule (the model
    /// never answers), waited for on the clock the test gives.
    fn decided(hello: &mut Hello, arrival: u64, now: Instant) -> Decided {
        hello.arrival(
            VoiceArrival {
                arrival,
                away_min: 60,
                there_s: 3,
                quiet_s: 300,
                since_hello_min: None,
            },
            now,
        );
        for _ in 0..500 {
            if let Some(decided) = hello.turn(now, 1_000_000, Room::Quiet, (Some(true), Some(true)))
            {
                return decided;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("no decision");
    }

    /// The reviewer on #883: in shadow, with the voice saying its own rule's
    /// hello, one greeting was counted twice, as the hello chosen and as the
    /// turn the lamp began, and the next arrival's rule answered `wait`. Now
    /// it is counted once, whichever came first; acting, and in shadow with
    /// nothing said, it is counted once as before.
    #[test]
    fn one_greeting_is_counted_once_in_shadow_or_acting() {
        let spoke = |act: bool, voice_says: Option<bool>| {
            let options = Options {
                act,
                ..Options::new(Duration::from_millis(50), None)
            };
            let mut hello = Hello::start(Never, options).unwrap();
            let t = Instant::now();
            // `Some(true)`: the voice says its own rule's hello after the
            // decision; `None`: nothing is said. (Said before the decision
            // came, the lamp is talking and the hard rule answers `wait`:
            // nothing is chosen to count twice.)
            let first = decided(&mut hello, 1, t + Duration::from_millis(500));
            assert_eq!(first.line.judgement.choice, Choice::SayNow);
            if voice_says == Some(true) {
                hello.voice(Voice::Speaking, t + Duration::from_millis(1_500));
                hello.voice(Voice::Resting, t + Duration::from_secs(3));
            }
            let later = t + AT_MOST_EVERY + Duration::from_secs(1);
            let second = decided(&mut hello, 2, later);
            (
                second.line.context.spoke_up_last_hour,
                second.line.judgement.choice,
            )
        };
        assert_eq!(spoke(false, Some(true)), (1, Choice::SayNow));
        assert_eq!(spoke(false, None), (1, Choice::SayNow));
        assert_eq!(spoke(true, Some(true)), (1, Choice::SayNow));
    }

    /// The reviewer on #883 (01:33): the voice's greeting begins with
    /// `thinking` and may make its first sound much later (the model may
    /// take up to 30 s). Followed from its beginning, it is still one
    /// greeting; a turn that ends without a sound leaves the hello chosen
    /// counted once, as chosen.
    #[test]
    fn a_greeting_whose_first_sound_comes_late_is_still_counted_once() {
        let run = |speaks_at: Option<Duration>| {
            let options = Options::new(Duration::from_millis(50), None);
            let mut hello = Hello::start(Never, options).unwrap();
            let t = Instant::now();
            let first = decided(&mut hello, 1, t + Duration::from_millis(500));
            assert_eq!(first.line.judgement.choice, Choice::SayNow);
            // Waiting for the model is said as thinking, once a second.
            hello.voice(Voice::Thinking, t + Duration::from_millis(600));
            hello.voice(Voice::Thinking, t + Duration::from_millis(1_600));
            match speaks_at {
                Some(at) => {
                    hello.voice(Voice::Speaking, t + at);
                    hello.voice(Voice::Resting, t + at + Duration::from_secs(2));
                }
                None => hello.voice(Voice::Resting, t + Duration::from_secs(3)),
            }
            let later = t + AT_MOST_EVERY + Duration::from_secs(1);
            let second = decided(&mut hello, 2, later);
            (
                second.line.context.spoke_up_last_hour,
                second.line.judgement.choice,
            )
        };
        assert_eq!(run(Some(Duration::from_secs(6))), (1, Choice::SayNow));
        assert_eq!(run(Some(Duration::from_secs(30))), (1, Choice::SayNow));
        // Cancelled before its first sound: the hello chosen, once.
        assert_eq!(run(None), (1, Choice::SayNow));
    }
}
