//! The policy as it runs: it listens on the bus, judges each thing that is
//! heard, and says what it would do about it.
//!
//! This version works in shadow mode. It decides, publishes the decision
//! on `policy/decisions` and writes it down, and nothing acts on it: what
//! it would have done can then be laid beside what the lamp did.
//!
//! The decision model is asked on a thread of its own, because it can take
//! seconds and the bus has to be read meanwhile: what the lamp says and
//! does while a judgement is awaited belongs to the next one. One question
//! is out at a time, and what is heard meanwhile waits its turn in a short
//! queue.
//!
//! Nothing that goes wrong around it stops it. A voice that is not there
//! yet, a model that cannot be reached, a log that cannot be written and a
//! bus that cannot be published to are each counted, said once when they
//! begin and once when they end ([`Note`]), and the loop goes on.
use crate::{
    attention::{self, Answers, Choice, Context, Judgement, Room},
    decide::{Asked, Decider, Failed, Usage},
    gather::{Gather, Lamp, Utterance},
    record::Record,
};
use lamp_bus::{Bus, Notice, Publisher, Subscriber};
use lamp_msgs::{Decision, Faces, RoomState, Speech, Voice, VoiceState};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt, io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The name the policy subscribes and publishes under.
pub const NAME: &str = "policy";
/// Nothing acts on what this version decides.
pub const SHADOW: bool = true;
/// No more utterances than this wait to be judged. When one more ends, the
/// one that has waited longest is let go: a judgement of something said
/// half a minute ago is of no use to anyone.
pub const MOST_WAITING: usize = 8;
/// Who decided, when no model did.
pub const RULE: &str = "rule";

/// What the voice heard, and what the lamp said.
const HEARD: &str = Speech::TOPICS[0];
const SAID: &str = Speech::TOPICS[1];
/// How long the loop waits for something heard before it looks at
/// everything else. An utterance is found over at most this long after its
/// pause.
const TICK: Duration = Duration::from_millis(20);
/// The same wait while a question is out: the answer is taken up at most
/// this long after it came.
const TICK_WHILE_ASKING: Duration = Duration::from_millis(5);
const EVERY_SECOND: Duration = Duration::from_secs(1);
/// No more messages than this are taken from one topic in one turn of the
/// loop, so that a flood on one cannot keep the others from being read.
const MOST_AT_ONCE: usize = 64;

/// How the policy is to run.
#[derive(Clone, Debug)]
pub struct Options {
    /// Where the decisions log is kept: a directory of the owner's alone.
    pub log_dir: PathBuf,
    /// How long the decision model has for one asking.
    pub deadline: Duration,
    /// How long after its last piece an utterance is over.
    pub pause: Duration,
}

/// What was counted from start to stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Utterances heard to their end.
    pub utterances: u64,
    /// Decisions made, and how many of them by the rule.
    pub decided: u64,
    pub by_rule: u64,
    /// Utterances let go unjudged because eight later ones were waiting.
    pub dropped: u64,
    /// Decisions that could not be published, and that are not in the log.
    pub unpublished: u64,
    pub unlogged: u64,
    /// Messages on the bus that could not be read as their type.
    pub unreadable: u64,
}

/// One line the policy says about itself.
///
/// A note is counts, times, choices and kinds, and nothing else. It cannot
/// hold what was heard or said: it is `Copy`, which no text that was put
/// together while running can be, and the only words in one are the names
/// of topics and of kinds of failure that this program was built with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Note {
    /// A decision was made.
    Decided {
        number: u64,
        choice: Choice,
        probability: f32,
        by_rule: bool,
        took_ms: u32,
        waited_ms: u32,
        pieces: u32,
        bytes: u32,
    },
    /// The decision model failed, after answering or at the start.
    ModelFailing {
        kind: &'static str,
    },
    /// It answered, after failing this many times.
    ModelAnswering {
        failures: u64,
    },
    /// Nobody publishes this topic, for this time since the start.
    SourceGone {
        topic: &'static str,
        times: u64,
    },
    SourceBack {
        topic: &'static str,
    },
    /// A message on this topic was not of its type.
    Unreadable {
        topic: &'static str,
    },
    Readable {
        topic: &'static str,
        passed_over: u64,
    },
    /// This topic's socket could not be read.
    NotReceiving {
        topic: &'static str,
        kind: io::ErrorKind,
    },
    Receiving {
        topic: &'static str,
    },
    /// Utterances end faster than they are judged.
    Dropping,
    CaughtUp {
        dropped: u64,
    },
    /// The decisions log could not be written.
    LogFailing {
        kind: io::ErrorKind,
    },
    LogWriting {
        missing: u64,
    },
    /// The bus could not be published to.
    BusFailing {
        kind: io::ErrorKind,
    },
    BusWorking {
        unpublished: u64,
    },
    /// The thread that asks the decision model has ended.
    AskerGone,
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Decided {
                number,
                choice,
                probability,
                by_rule,
                took_ms,
                waited_ms,
                pieces,
                bytes,
            } => write!(
                f,
                "decision {number} about={} choice={} probability={probability:.3} by={} took_ms={took_ms} waited_ms={waited_ms} pieces={pieces} bytes={bytes}",
                attention::ABOUT,
                choice.as_str(),
                if by_rule { RULE } else { "model" },
            ),
            Self::ModelFailing { kind } => write!(
                f,
                "the decision model is not answering ({kind}): the rule decides until it does"
            ),
            Self::ModelAnswering { failures } => {
                write!(
                    f,
                    "the decision model answers again after failures={failures}"
                )
            }
            Self::SourceGone { topic, times } => {
                write!(f, "{topic} has no publisher (times={times})")
            }
            Self::SourceBack { topic } => write!(f, "{topic} has a publisher again"),
            Self::Unreadable { topic } => write!(
                f,
                "a message on {topic} is not of its type: such messages are passed over"
            ),
            Self::Readable { topic, passed_over } => {
                write!(f, "{topic} is read again after passed_over={passed_over}")
            }
            Self::NotReceiving { topic, kind } => {
                write!(f, "{topic} cannot be received from ({kind})")
            }
            Self::Receiving { topic } => write!(f, "{topic} is received from again"),
            Self::Dropping => write!(
                f,
                "more than {MOST_WAITING} utterances wait to be judged: the oldest are let go"
            ),
            Self::CaughtUp { dropped } => {
                write!(f, "no utterance waits any more; dropped={dropped}")
            }
            Self::LogFailing { kind } => write!(
                f,
                "the decisions log cannot be written ({kind}): decisions go on without it"
            ),
            Self::LogWriting { missing } => {
                write!(f, "the decisions log is written again; missing={missing}")
            }
            Self::BusFailing { kind } => write!(
                f,
                "decisions cannot be published on the bus ({kind}): they go on without it"
            ),
            Self::BusWorking { unpublished } => write!(
                f,
                "decisions are published again; unpublished={unpublished}"
            ),
            Self::AskerGone => f.write_str(
                "the thread that asks the decision model has ended: the rule decides from now on",
            ),
        }
    }
}

/// One kind of trouble: counted while it lasts, and known to have begun
/// and to have ended, so that each can be said once.
#[derive(Default)]
struct Trouble {
    on: bool,
    lost: u64,
}

impl Trouble {
    /// It went wrong, and `lost` things with it. True where it begins.
    fn wrong(&mut self, lost: u64) -> bool {
        self.lost += lost;
        !std::mem::replace(&mut self.on, true)
    }

    /// It went right. What was lost while it lasted, where it ends.
    fn right(&mut self) -> Option<u64> {
        std::mem::take(&mut self.on).then(|| std::mem::take(&mut self.lost))
    }
}

/// One topic the policy listens to.
struct Ear {
    topic: &'static str,
    subscriber: Subscriber,
    unreadable: Trouble,
    failing: Trouble,
    gone: u64,
}

impl Ear {
    fn new(bus: &Bus, topic: &'static str, type_name: &str) -> io::Result<Self> {
        Ok(Self {
            topic,
            subscriber: bus.subscribe_typed(topic, NAME, type_name)?,
            unreadable: Trouble::default(),
            failing: Trouble::default(),
            gone: 0,
        })
    }

    /// What is waiting, read as `T`, after waiting at most `wait` for the
    /// first. A message that is not a `T` is counted and passed over, and
    /// nothing of it is said: what it holds may be what was heard.
    fn take<T: DeserializeOwned>(
        &mut self,
        mut wait: Duration,
        unreadable: &mut u64,
        say: &mut dyn FnMut(Note),
    ) -> Vec<T> {
        let topic = self.topic;
        let mut taken = Vec::new();
        while taken.len() < MOST_AT_ONCE {
            match self.subscriber.recv(wait) {
                Ok(Some(message)) => {
                    if self.failing.right().is_some() {
                        say(Note::Receiving { topic });
                    }
                    match message.parse::<T>() {
                        Ok(read) => {
                            if let Some(passed_over) = self.unreadable.right() {
                                say(Note::Readable { topic, passed_over });
                            }
                            taken.push(read);
                        }
                        Err(_) => {
                            *unreadable += 1;
                            if self.unreadable.wrong(1) {
                                say(Note::Unreadable { topic });
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    if self.failing.wrong(1) {
                        let kind = error.kind();
                        say(Note::NotReceiving { topic, kind });
                    }
                    // A socket that fails at once must not spin the loop.
                    thread::sleep(wait);
                    break;
                }
            }
            wait = Duration::ZERO;
        }
        taken
    }

    /// Say what has changed about the topic's source. True if it has just
    /// gone.
    fn source(&mut self, say: &mut dyn FnMut(Note)) -> bool {
        let topic = self.topic;
        let mut went = false;
        for notice in self.subscriber.notices() {
            match notice {
                Notice::SourceGone => {
                    self.gone += 1;
                    went = true;
                    say(Note::SourceGone {
                        topic,
                        times: self.gone,
                    });
                }
                Notice::SourceBack => {
                    went = false;
                    say(Note::SourceBack { topic });
                }
                _ => {}
            }
        }
        went
    }
}

/// An utterance that is over, with what was known when it began.
struct Waiting {
    utterance: Utterance,
    context: Context,
    /// What the eyes saw as it ended, if they said anything lately.
    seen: Option<Seen>,
}

/// What the eyes saw: how many faces, how many of them near enough to be
/// somebody at the lamp, and how many of those turned to it. Written down
/// with each decision and not yet used to make it: nobody has stood in
/// front of the lamp since the eyes began to publish, so there is nothing
/// to weigh it by. Counts only: no box, no id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Seen {
    pub faces: u8,
    pub near: u8,
    pub facing: u8,
    /// How old the eyes' list was when the utterance ended.
    pub age_ms: u32,
}

/// A face this tall, as a share of the picture's height, is somebody at
/// the lamp: the line the eyes draw (`lamp-eyes`, `sight::LEAST_SIZE`).
const NEAR_HEIGHT: f32 = 0.15;
/// The eyes' list is good for this long (`lamp-eyes`, `bus_out::LASTS`).
const SEEN_FOR: Duration = Duration::from_secs(2);

fn counted(faces: &Faces) -> (u8, u8, u8) {
    let near = |face: &&lamp_msgs::Face| face.ymax - face.ymin >= NEAR_HEIGHT;
    let count = |n: usize| u8::try_from(n).unwrap_or(u8::MAX);
    (
        count(faces.faces.len()),
        count(faces.faces.iter().filter(near).count()),
        count(
            faces
                .faces
                .iter()
                .filter(near)
                .filter(|face| face.facing == Some(true))
                .count(),
        ),
    )
}

/// The thread that asks the decision model, and the way to it.
struct Asker {
    states: mpsc::SyncSender<String>,
    answers: mpsc::Receiver<(Result<Asked, Failed>, Duration)>,
    thread: thread::JoinHandle<()>,
}

impl Asker {
    fn start<D: Decider + Send + 'static>(mut decider: D, deadline: Duration) -> io::Result<Self> {
        let (states, asked) = mpsc::sync_channel::<String>(1);
        let (answered, answers) = mpsc::channel();
        let thread = thread::Builder::new().name("ask".into()).spawn(move || {
            let questions = attention::questions();
            for state in asked {
                let started = Instant::now();
                let answer = decider.ask(&state, &questions, deadline);
                if answered.send((answer, started.elapsed())).is_err() {
                    break;
                }
            }
        })?;
        Ok(Self {
            states,
            answers,
            thread,
        })
    }
}

/// One line of the decisions log.
#[derive(Serialize)]
struct Line<'a> {
    /// When the decision was made, in milliseconds since 1970.
    at_unix_ms: u64,
    about: &'static str,
    shadow: bool,
    /// What was known. `heard` is what was said, in words.
    context: &'a Context,
    in_conversation: bool,
    /// What the decision model was shown.
    state: serde_json::Value,
    /// The judge that was asked.
    asked: &'a str,
    /// What it answered to each question. Empty if it did not.
    answers: &'a BTreeMap<String, f32>,
    judgement: Judgement,
    /// Who decided: the model as its service names it, or the rule.
    by: &'a str,
    /// How long the asking took, and how long the utterance had waited
    /// for it since its last piece.
    took_ms: u32,
    waited_ms: u32,
    /// Why the model's answers were not used, as a kind and in words.
    #[serde(skip_serializing_if = "Option::is_none")]
    failed: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request: Option<u64>,
    /// What the eyes saw as the utterance ended. Not used in the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    seen: Option<Seen>,
    /// How many pieces the utterance came in, and whether it was ended by
    /// its length.
    pieces: u32,
    cut: bool,
}

fn millis(time: Duration) -> u32 {
    u32::try_from(time.as_millis()).unwrap_or(u32::MAX)
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// Everything the loop holds between two turns.
struct Policy<'a> {
    bus: &'a Bus,
    say: &'a mut dyn FnMut(Note),
    started: Instant,
    /// What the judge that is asked is called.
    judge_name: String,
    asker: Asker,
    asker_gone: bool,
    /// The one question that is out, and when it went.
    asking: Option<(Waiting, Instant)>,
    waiting: VecDeque<Waiting>,
    gather: Gather,
    lamp: Lamp,
    /// What the room sounds like, as the voice last published it on
    /// `room/state`: not known until it has, or once it has gone.
    room: Room,
    /// The eyes' newest list, counted, and when it came.
    seen: Option<((u8, u8, u8), Instant)>,
    record: Record,
    out: Option<Publisher>,
    model: Trouble,
    log: Trouble,
    publishing: Trouble,
    dropping: Trouble,
    totals: Totals,
}

impl Policy<'_> {
    /// An utterance is over: it waits to be judged, with what was known
    /// when it began.
    fn heard(&mut self, utterance: Utterance) {
        self.totals.utterances += 1;
        let context = Context {
            heard: utterance.text.clone(),
            room: self.room,
            lamp_is_speaking: utterance.lamp.speaking,
            lamp_last_said: attention::last_words(&utterance.lamp.said),
            seconds_since_lamp_spoke: utterance
                .lamp
                .spoke
                .map(|at| utterance.began.saturating_duration_since(at).as_secs_f32()),
        };
        if self.waiting.len() == MOST_WAITING {
            self.waiting.pop_front();
            self.totals.dropped += 1;
            if self.dropping.wrong(1) {
                (self.say)(Note::Dropping);
            }
        }
        let seen = self.seen.and_then(|((faces, near, facing), at)| {
            let age = utterance.ended.saturating_duration_since(at);
            (age <= SEEN_FOR).then_some(Seen {
                faces,
                near,
                facing,
                age_ms: millis(age),
            })
        });
        self.waiting.push_back(Waiting {
            utterance,
            context,
            seen,
        });
    }

    /// Take up an answer if one has come, and ask the next question if
    /// none is out.
    fn judge(&mut self, now: Instant) {
        match self.asker.answers.try_recv() {
            Ok((answer, took)) => {
                if let Some((waiting, sent)) = self.asking.take() {
                    self.decide(waiting, answer, took, sent);
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.asker_ended(now),
        }
        while self.asking.is_none() {
            let Some(waiting) = self.waiting.pop_front() else {
                break;
            };
            let sent = if self.asker_gone {
                Err(mpsc::TrySendError::Disconnected(String::new()))
            } else {
                let state = attention::state_text(&waiting.context);
                self.asker.states.try_send(state)
            };
            match sent {
                Ok(()) => self.asking = Some((waiting, now)),
                // The thread has not come back for its next question yet.
                Err(mpsc::TrySendError::Full(_)) => {
                    self.waiting.push_front(waiting);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    self.asker_ended(now);
                    self.decide(waiting, Err(Self::no_asker()), Duration::ZERO, now);
                }
            }
        }
        if self.waiting.is_empty()
            && let Some(dropped) = self.dropping.right()
        {
            (self.say)(Note::CaughtUp { dropped });
        }
    }

    fn no_asker() -> Failed {
        Failed::Unreachable {
            during: "asking",
            kind: io::ErrorKind::BrokenPipe,
        }
    }

    /// The asking thread is no more. What it was asked is decided by the
    /// rule, and so is everything after it.
    fn asker_ended(&mut self, now: Instant) {
        if !std::mem::replace(&mut self.asker_gone, true) {
            (self.say)(Note::AskerGone);
        }
        if let Some((waiting, sent)) = self.asking.take() {
            let took = now.saturating_duration_since(sent);
            self.decide(waiting, Err(Self::no_asker()), took, sent);
        }
    }

    /// Make the decision from the model's answers, or by the rule if there
    /// are none that can be used, and publish it and write it down.
    fn decide(
        &mut self,
        waiting: Waiting,
        answer: Result<Asked, Failed>,
        took: Duration,
        sent: Instant,
    ) {
        let Waiting {
            utterance,
            context,
            seen,
        } = waiting;
        let (judgement, answers, by, failed, usage) = match answer {
            Ok(asked) => match Answers::of(&asked.answers) {
                Ok(answers) => (
                    attention::combine(&answers, &context),
                    answers.by_name(),
                    asked.model,
                    None,
                    asked.usage,
                ),
                // A judge that returned without all it was asked.
                Err(question) => (
                    attention::fallback(&context),
                    BTreeMap::new(),
                    RULE.to_owned(),
                    Some(Failed::Missing(question.to_owned())),
                    asked.usage,
                ),
            },
            Err(failed) => (
                attention::fallback(&context),
                BTreeMap::new(),
                RULE.to_owned(),
                Some(failed),
                None,
            ),
        };
        self.totals.decided += 1;
        self.totals.by_rule += u64::from(judgement.from_rule);
        match &failed {
            Some(failed) => {
                if self.model.wrong(1) {
                    (self.say)(Note::ModelFailing {
                        kind: failed.kind(),
                    });
                }
            }
            None => {
                if let Some(failures) = self.model.right() {
                    (self.say)(Note::ModelAnswering { failures });
                }
            }
        }
        let took_ms = millis(took);
        let waited_ms = millis(sent.saturating_duration_since(utterance.ended));
        // Published before it is written down: whoever comes to act on a
        // decision must not wait for a disk.
        self.publish(&Decision {
            about: attention::ABOUT.to_owned(),
            choice: judgement.choice.as_str().to_owned(),
            probability: judgement.probability,
            answers: answers.clone(),
            by: by.clone(),
            took_ms,
            shadow: SHADOW,
            request: utterance.request,
        });
        let at_unix_ms = unix_ms();
        let line = Line {
            at_unix_ms,
            about: attention::ABOUT,
            shadow: SHADOW,
            context: &context,
            in_conversation: context.in_conversation(),
            state: attention::state(&context),
            asked: &self.judge_name,
            answers: &answers,
            judgement,
            by: &by,
            took_ms,
            waited_ms,
            failed: failed.as_ref().map(Failed::kind),
            failed_why: failed.as_ref().map(Failed::to_string),
            usage,
            request: utterance.request,
            seen,
            pieces: utterance.pieces,
            cut: utterance.cut,
        };
        match self.record.write(at_unix_ms, &line) {
            Ok(()) => {
                if let Some(missing) = self.log.right() {
                    (self.say)(Note::LogWriting { missing });
                }
            }
            Err(error) => {
                self.totals.unlogged += 1;
                if self.log.wrong(1) {
                    (self.say)(Note::LogFailing { kind: error.kind() });
                }
            }
        }
        (self.say)(Note::Decided {
            number: self.totals.decided,
            choice: judgement.choice,
            probability: judgement.probability,
            by_rule: judgement.from_rule,
            took_ms,
            waited_ms,
            pieces: utterance.pieces,
            bytes: u32::try_from(utterance.text.len()).unwrap_or(u32::MAX),
        });
    }

    fn publish(&mut self, decision: &Decision) {
        let at_us = self.started.elapsed().as_micros() as u64;
        let sent = match self.out.as_mut() {
            Some(out) => out.publish_json(at_us, decision).map(|_| ()),
            None => Err(io::ErrorKind::NotConnected.into()),
        };
        // A publisher says of a root it cannot use once, and then takes
        // each message as before: while that trouble lasts, what it takes
        // goes nowhere.
        if let Err(error) = &sent {
            self.totals.unpublished += 1;
            if self.publishing.wrong(1) {
                (self.say)(Note::BusFailing { kind: error.kind() });
            }
        } else if self.publishing.on {
            self.totals.unpublished += 1;
            self.publishing.wrong(1);
        }
    }

    /// Once a second: show the bus that the policy is alive though it has
    /// decided nothing, which is also how it learns that the bus can be
    /// published to again.
    fn second(&mut self) {
        if self.out.is_none() {
            let made = self
                .bus
                .publisher_declared(Decision::TOPIC, &Decision::declaration(), NAME);
            match made {
                Ok(out) => self.out = Some(out),
                Err(error) => {
                    if self.publishing.wrong(0) {
                        (self.say)(Note::BusFailing { kind: error.kind() });
                    }
                    return;
                }
            }
        }
        let Some(out) = self.out.as_mut() else {
            return;
        };
        match out.alive() {
            Ok(()) => {
                if let Some(unpublished) = self.publishing.right() {
                    (self.say)(Note::BusWorking { unpublished });
                }
            }
            Err(error) => {
                if self.publishing.wrong(0) {
                    (self.say)(Note::BusFailing { kind: error.kind() });
                }
            }
        }
    }
}

/// Run the policy on `bus` until `stop` is set: subscribe to what the
/// voice hears, says and is doing, judge each utterance with `decider`,
/// and publish and write down each decision.
///
/// It fails only at the start, if it cannot subscribe or start its thread.
/// After that nothing ends it but `stop`. `say` is given each [`Note`] as
/// it happens.
pub fn run<D: Decider + Send + 'static>(
    bus: &Bus,
    decider: D,
    options: &Options,
    stop: &AtomicBool,
    say: &mut dyn FnMut(Note),
) -> io::Result<Totals> {
    let mut heard = Ear::new(bus, HEARD, Speech::TYPE)?;
    let mut said = Ear::new(bus, SAID, Speech::TYPE)?;
    let mut voice = Ear::new(bus, VoiceState::TOPIC, VoiceState::TYPE)?;
    let mut room = Ear::new(bus, RoomState::TOPIC, RoomState::TYPE)?;
    let mut eyes = Ear::new(bus, Faces::TOPIC, Faces::TYPE)?;
    let judge_name = decider.name().to_owned();
    let mut policy = Policy {
        bus,
        say,
        started: Instant::now(),
        judge_name,
        asker: Asker::start(decider, options.deadline)?,
        asker_gone: false,
        asking: None,
        waiting: VecDeque::new(),
        gather: Gather::new(options.pause),
        lamp: Lamp::default(),
        room: Room::Unknown,
        seen: None,
        record: Record::new(&options.log_dir),
        out: None,
        model: Trouble::default(),
        log: Trouble::default(),
        publishing: Trouble::default(),
        dropping: Trouble::default(),
        totals: Totals::default(),
    };
    policy.second();
    let mut shown = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        // What the lamp is doing and saying first, so that a piece heard
        // in the same turn finds it as it was. What the lamp says while
        // this turn waits for a piece is taken in with the next turn: an
        // utterance may miss the lamp's last 20 ms, and never has any of
        // what came after its own beginning.
        let now = Instant::now();
        let unreadable = &mut policy.totals.unreadable;
        for state in voice.take::<VoiceState>(Duration::ZERO, unreadable, policy.say) {
            policy.lamp.state(state.state == Voice::Speaking, now);
        }
        if voice.source(policy.say) {
            policy.lamp.gone();
        }
        for piece in said.take::<Speech>(Duration::ZERO, unreadable, policy.say) {
            policy.lamp.said(&piece.text, piece.request, now);
        }
        said.source(policy.say);
        // How the room is and who the eyes see, as each was last said. A
        // source that has gone leaves its fact not known, never as it was.
        for state in room.take::<RoomState>(Duration::ZERO, unreadable, policy.say) {
            policy.room = match state.state {
                lamp_msgs::Room::Quiet => Room::Quiet,
                lamp_msgs::Room::Busy => Room::Busy,
                lamp_msgs::Room::Unknown => Room::Unknown,
            };
        }
        if room.source(policy.say) {
            policy.room = Room::Unknown;
        }
        for faces in eyes.take::<Faces>(Duration::ZERO, unreadable, policy.say) {
            policy.seen = Some((counted(&faces), now));
        }
        if eyes.source(policy.say) {
            policy.seen = None;
        }
        // The wait of the loop is here: for something to be heard.
        let wait = if policy.asking.is_some() {
            TICK_WHILE_ASKING
        } else {
            TICK
        };
        let pieces = heard.take::<Speech>(wait, unreadable, policy.say);
        heard.source(policy.say);
        let now = Instant::now();
        for piece in pieces {
            policy.gather.piece(
                &piece.text,
                piece.finished,
                piece.request,
                &policy.lamp,
                now,
            );
        }
        policy.gather.tick(now);
        while let Some(utterance) = policy.gather.over() {
            policy.heard(utterance);
        }
        policy.judge(now);
        if now.saturating_duration_since(shown) >= EVERY_SECOND {
            shown = now;
            policy.second();
        }
    }
    // The thread ends with its question, which is its deadline at most.
    let Policy { asker, totals, .. } = policy;
    drop(asker.states);
    let _ = asker.thread.join();
    Ok(totals)
}
