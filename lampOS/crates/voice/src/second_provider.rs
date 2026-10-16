//! The second voice route: speech-to-text, an OpenAI-compatible chat model
//! and text-to-speech in place of one Live session, configured by
//! [`crate::second_config`].
//!
//! [`Engine`] speaks the provider protocol ([`ProviderInput`] in,
//! [`ProviderOutput`] out) over three services behind [`Services`]; the
//! worker loop in [`worker::run`] carries it over the ordinary provider IPC,
//! as [`crate::fixture_provider`] does; [`worker::serve`] is the process
//! `lamp-voice worker second` (role `second`).
//!
//! A spoken turn: `Started` at once and speech-to-text opened while the
//! person is still talking; at the end, their words (`Transcript` with no
//! request), then the chat answer cut into sentences, each said as a
//! `Transcript` for the request and its audio. Speech of a sentence starts
//! while the chat is still writing the next one; audio keeps sentence order.
//! A written turn is the same from the chat on.
//!
//! With `--sounds-checked` (`LAMP_SHORT_SOUNDS=quiet-room-checked`, #800),
//! a spoken turn whose end came marked as a short sound let through is
//! answered only when what was heard is, whole, a short word said to the
//! lamp ([`crate::not_speech::short_word`]); otherwise it ends quietly
//! after its words, as the Live route holds it. This route has no ears and
//! no eyes, so the heard words are all that can show it was said to the
//! lamp: a sound said close or facing the lamp but not heard as a short
//! word is held here where the Live route would answer it.
//!
//! With a [`Heed`] (`--attention`, `worker::Heeding`), a spoken turn's words
//! are judged by the Live route's rule for whom it answers before the chat
//! is asked; words it holds end the turn unanswered (#1014).
//!
//! Failures are `TurnFailed` with the stage the coordinator accepts
//! (`coordinator/cloud.rs`): `InputOpen` while the request's input is still
//! coming (speech-to-text could not be opened or fed); `AwaitingResponse`
//! after its end and before any answer audio (final transcript, chat or
//! speech failed); `Responding` once some answer audio was sent.
//! `NotDelivered` is never used: `Started` always comes first. `Interrupted`
//! and `Ended` are never sent.
use crate::provider_flow::PACKET_SAMPLES;
use crate::provider_worker::{FailedStage, Fault, ProviderInput, ProviderOutput, ReminderDeferral};
use futures_util::{StreamExt, future::BoxFuture};
use lamp_ipc::monotonic_us;
use net::{ChatEvent, Message, Sentences};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};

pub mod net;
mod services;
pub mod worker;

pub use services::{Listener, Net, Services};

#[cfg(test)]
mod tests;

/// Said once before the first answer after a quiet spell, so the person
/// knows the different voice is the lamp's.
pub const ANNOUNCEMENT: &str = "My voice sounds a little different just now.";
/// How long without an answer before the announcement is said again.
pub const ANNOUNCE_AFTER: Duration = Duration::from_secs(120);
/// Chat requests in one turn, tool rounds included.
pub const MAX_ROUNDS: usize = 4;
/// A turn's first chat round that has not begun (no word, no call) this
/// long after it was asked, which is as soon as speech to text gave the
/// words, is given up and the turn ends unanswered: an answer said 12 s
/// after the question is worse than none (lamp-4ace on 2026-10-16: first
/// events 1.6 to 20.5 s). A round after a tool ran is not bounded: what was
/// done has to be said, or the person asks again and it is done twice
/// (Second on #1049).
pub const BEGIN_WITHIN: Duration = Duration::from_secs(3);
/// The tool whose call means the words were not for the lamp.
const NOT_FOR_ME: &str = "not_for_me";
/// Microphone packets (10 ms each) held while speech-to-text connects:
/// 30 s, more than its 10 s connect bound and as much as the coordinator
/// replays at once when a line moves to this route (`coordinator/second.rs`).
const AUDIO_QUEUE: usize = 3_000;
/// Output events waiting for the worker loop; the rest wait in their task.
const OUTPUT_QUEUE: usize = 4;
/// Sentences queued ahead of the one being said.
const SENTENCES_AHEAD: usize = 2;
/// Packets of one sentence held while an earlier one is said: a whole
/// reply's worth, so a waiting download is never left unread for long.
const SENTENCE_PACKETS: usize = crate::provider_flow::MAX_REPLY_SAMPLES / PACKET_SAMPLES;

/// Runs one tool call: `(request, heard, name, arguments)` to the result the
/// model reads. `heard` is false for a request the lamp wrote itself (a
/// `Written` turn): nothing anyone said can confirm work in it, as on the
/// Live route (`tools::run_call`).
pub type ToolRunner =
    Arc<dyn Fn(u64, bool, String, Value) -> BoxFuture<'static, Value> + Send + Sync>;
/// The system message for a turn, built when the turn's chat begins, and
/// whose turn it is.
pub type System = Arc<dyn Fn() -> Told + Send + Sync>;

/// What a turn's chat is told: the system message, and the exchanges just
/// before it as messages (user, then assistant), oldest first. Both come
/// from the thread, so they are the person's at the lamp only and nothing
/// that was forgotten (`crate::thread::Thread::forget`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Told {
    pub text: String,
    pub history: Vec<Message>,
}
impl From<String> for Told {
    /// With no exchanges before it.
    fn from(text: String) -> Self {
        Self {
            text,
            history: Vec::new(),
        }
    }
}
/// Whether a spoken turn's words were said to the lamp, judged before they
/// are answered: `(request, (began_us, ended_us), heard)` to why it is not
/// answered, or nothing. The window is the microphone's read time of the
/// request's first and last packets, so a line replayed to this route keeps
/// the time it was said. The worker's is the Live route's own rule
/// (`worker::Heeding`, `crate::attention`).
pub type Heed = Arc<dyn Fn(u64, (u64, u64), &str) -> Option<&'static str> + Send + Sync>;
/// The engine's clock, for the announcement's spell.
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// What every turn task shares.
struct Shared {
    services: Arc<dyn Services>,
    tools: ToolRunner,
    system: System,
    clock: Clock,
    /// When answer audio was last sent.
    answered_at: Mutex<Option<Instant>>,
    /// Audio packets sent by this process.
    sequence: AtomicU64,
    out: mpsc::Sender<ProviderOutput>,
    heed: Option<Heed>,
}

struct Turn {
    request: u64,
    /// Open until the request's End; `None` after it.
    audio: Option<mpsc::Sender<Vec<i16>>>,
    /// The End came marked as a short sound let through; set before the
    /// audio queue closes, read by the turn's task after.
    sound: Arc<AtomicBool>,
    /// When its first and last packets were read from the microphone.
    said: Arc<Mutex<(u64, u64)>>,
    task: JoinHandle<()>,
}
impl Drop for Turn {
    /// A turn given up stops its chat and speech at once.
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The second route's protocol state: at most one turn at a time.
pub struct Engine {
    shared: Arc<Shared>,
    output: mpsc::Receiver<ProviderOutput>,
    /// Events the engine itself says (`Started`, refusals); before any of
    /// the turn task's, so `Started` always precedes the turn's output.
    urgent: VecDeque<ProviderOutput>,
    turn: Option<Turn>,
    retired_through: u64,
    /// `--sounds-checked`: a short sound is answered only when heard as a
    /// short word.
    sounds_checked: bool,
}

impl Engine {
    /// `system` is asked for the system message at the start of each
    /// turn's chat (character, memory note and conversation so far).
    pub fn new(
        services: Arc<dyn Services>,
        system: impl Fn() -> Told + Send + Sync + 'static,
        tools: ToolRunner,
    ) -> Self {
        Self::with_clock(services, Arc::new(system), tools, Arc::new(Instant::now))
    }

    pub fn with_clock(
        services: Arc<dyn Services>,
        system: System,
        tools: ToolRunner,
        clock: Clock,
    ) -> Self {
        let (out, output) = mpsc::channel(OUTPUT_QUEUE);
        Self {
            shared: Arc::new(Shared {
                services,
                tools,
                system,
                clock,
                answered_at: Mutex::new(None),
                sequence: AtomicU64::new(0),
                out,
                heed: None,
            }),
            output,
            urgent: VecDeque::new(),
            turn: None,
            retired_through: 0,
            sounds_checked: false,
        }
    }

    /// Short sounds let through are checked (`--sounds-checked`, #800).
    pub fn checking_sounds(mut self, on: bool) -> Self {
        self.sounds_checked = on;
        self
    }

    /// A spoken turn's words are judged with `heed` before they are answered.
    /// Before any turn only: the turns share it.
    pub fn heeding(mut self, heed: Heed) -> Self {
        if let Some(shared) = Arc::get_mut(&mut self.shared) {
            shared.heed = Some(heed);
        }
        self
    }

    /// Take one input. Must run inside a tokio runtime: a turn is a task.
    /// Input for a retired request, or for a request other than the
    /// current one, is dropped; the worker loop has checked authority.
    pub fn submit(&mut self, input: ProviderInput) {
        match input {
            ProviderInput::Start {
                request, session, ..
            } => {
                if !self.begin(request, session) {
                    return;
                }
                let (audio, heard) = mpsc::channel(AUDIO_QUEUE);
                let sound = Arc::new(AtomicBool::new(false));
                let checked = self.sounds_checked.then(|| sound.clone());
                let said = Arc::new(Mutex::new((0, 0)));
                let task = tokio::spawn(spoken(
                    self.shared.clone(),
                    request,
                    heard,
                    checked,
                    said.clone(),
                ));
                self.turn = Some(Turn {
                    request,
                    audio: Some(audio),
                    sound,
                    said,
                    task,
                });
            }
            ProviderInput::Audio {
                request,
                samples,
                read_completed_at_us,
                ..
            } => {
                // No age check: replayed audio is older than live audio.
                let Some(turn) = self.turn.as_mut().filter(|turn| turn.request == request) else {
                    return;
                };
                let Some(audio) = turn.audio.as_ref() else {
                    return;
                };
                if let Ok(mut said) = turn.said.lock() {
                    if said.0 == 0 {
                        said.0 = read_completed_at_us;
                    }
                    said.1 = read_completed_at_us;
                }
                match audio.try_send(samples) {
                    Ok(()) => {}
                    // The task already failed the turn itself.
                    Err(mpsc::error::TrySendError::Closed(_)) => turn.audio = None,
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        self.turn = None;
                        self.urgent.push_back(ProviderOutput::TurnFailed {
                            request,
                            stage: FailedStage::InputOpen,
                            reason: Fault::Local,
                        });
                    }
                }
            }
            // A note for Gemini's model; this voice keeps none.
            ProviderInput::Called { .. } => {}
            ProviderInput::End { request, sound, .. } => {
                if let Some(turn) = self.turn.as_mut().filter(|turn| turn.request == request) {
                    turn.sound.store(sound, Ordering::Release);
                    // Closing the queue is the end of input for the task.
                    turn.audio = None;
                }
            }
            ProviderInput::Written {
                request,
                session,
                text,
                ..
            } => {
                // `look` is ignored: this route has no eyes yet.
                if !self.begin(request, session) {
                    return;
                }
                let shared = self.shared.clone();
                let task = tokio::spawn(async move { answer(&shared, request, text, false).await });
                self.turn = Some(Turn {
                    request,
                    audio: None,
                    sound: Arc::default(),
                    said: Arc::default(),
                    task,
                });
            }
            ProviderInput::Reminder {
                request, session, ..
            } => self.urgent.push_back(ProviderOutput::ReminderDeferred {
                request,
                session,
                reason: ReminderDeferral::Disabled,
            }),
        }
    }

    /// A new turn replaces any older one. False for a retired request.
    fn begin(&mut self, request: u64, session: u64) -> bool {
        if request <= self.retired_through {
            return false;
        }
        self.turn = None;
        self.urgent.push_back(ProviderOutput::Started {
            request,
            session,
            waiting_for_barrier: false,
        });
        true
    }

    /// Requests up to `through` are over (spoken over, completed or
    /// abandoned): their task stops and their unsent output is dropped.
    pub fn retire_through(&mut self, through: u64) {
        self.retired_through = self.retired_through.max(through);
        if self
            .turn
            .as_ref()
            .is_some_and(|turn| turn.request <= self.retired_through)
        {
            self.turn = None;
        }
        let retired = self.retired_through;
        self.urgent
            .retain(|event| output_request(event).is_none_or(|r| r > retired));
    }

    /// The next output ready now, if any.
    pub fn try_output(&mut self) -> Option<ProviderOutput> {
        loop {
            let event = match self.urgent.pop_front() {
                Some(event) => event,
                None => self.output.try_recv().ok()?,
            };
            if output_request(&event).is_none_or(|r| r > self.retired_through) {
                return Some(event);
            }
        }
    }

    /// The next output, waiting for it.
    #[cfg(test)]
    async fn next(&mut self) -> ProviderOutput {
        loop {
            let event = match self.urgent.pop_front() {
                Some(event) => event,
                None => self.output.recv().await.expect("the engine holds a sender"),
            };
            if output_request(&event).is_none_or(|r| r > self.retired_through) {
                return event;
            }
        }
    }
}

/// The request an output belongs to, as the fixture has it: failures and
/// refusals are owed even for a retired request.
pub(crate) fn output_request(event: &ProviderOutput) -> Option<u64> {
    match event {
        ProviderOutput::Ready { .. }
        | ProviderOutput::TurnFailed { .. }
        | ProviderOutput::ReminderDeferred { .. }
        | ProviderOutput::InputDiscarded { .. }
        | ProviderOutput::OwnershipAssumed { .. }
        | ProviderOutput::Ended { .. } => None,
        ProviderOutput::Started { request, .. }
        | ProviderOutput::Audio { request, .. }
        | ProviderOutput::GenerationComplete { request }
        | ProviderOutput::Interrupted { request }
        | ProviderOutput::TurnComplete { request, .. }
        | ProviderOutput::Unclear { request, .. } => Some(*request),
        ProviderOutput::Transcript { request, .. } => *request,
    }
}

/// The closest provider fault for a client error. No text is carried.
pub fn fault(error: &net::Error) -> Fault {
    match error {
        net::Error::Url => Fault::Local,
        net::Error::Transport(_) => Fault::Transport,
        net::Error::Timeout(step) if step.contains("connect") => Fault::ConnectTimeout,
        net::Error::Timeout(_) => Fault::ResponseTimeout,
        net::Error::Status(401 | 403) => Fault::Authentication,
        net::Error::Status(402) => Fault::QuotaExceeded,
        net::Error::Status(429) => Fault::RateLimited,
        net::Error::Status(500..) => Fault::ServerUnavailable,
        net::Error::Status(_) => Fault::ServerRejected,
        net::Error::Protocol(_) => Fault::Protocol,
    }
}

impl Shared {
    /// False once the engine is gone; the task then just ends.
    async fn emit(&self, event: ProviderOutput) -> bool {
        self.out.send(event).await.is_ok()
    }

    async fn fail(&self, request: u64, stage: FailedStage, error: &net::Error) {
        self.emit(ProviderOutput::TurnFailed {
            request,
            stage,
            reason: fault(error),
        })
        .await;
    }

    fn announcement_due(&self) -> bool {
        let now = (self.clock)();
        self.answered_at
            .lock()
            .map(|at| at.is_none_or(|at| now.saturating_duration_since(at) >= ANNOUNCE_AFTER))
            .unwrap_or(true)
    }
}

/// A spoken turn: listen until the queue closes, then answer the words.
/// With `checked` (the sound mark, when sounds are checked), a short sound
/// not heard as a short word is not answered; with a [`Heed`], words it
/// judges not said to the lamp are not either.
async fn spoken(
    shared: Arc<Shared>,
    request: u64,
    mut heard: mpsc::Receiver<Vec<i16>>,
    checked: Option<Arc<AtomicBool>>,
    said: Arc<Mutex<(u64, u64)>>,
) {
    let listener = match shared.services.listen().await {
        Ok(listener) => listener,
        Err(error) => return shared.fail(request, FailedStage::InputOpen, &error).await,
    };
    while let Some(samples) = heard.recv().await {
        if let Err(error) = listener.send(&samples) {
            return shared.fail(request, FailedStage::InputOpen, &error).await;
        }
    }
    let finishing = Instant::now();
    let text = match listener.finish(request).await {
        Ok(text) => {
            eprintln!(
                "lamp-voice second: request {request} heard {} words, the last {} ms after the line ended",
                text.split_whitespace().count(),
                finishing.elapsed().as_millis()
            );
            text
        }
        Err(error) => {
            eprintln!("lamp-voice second: request {request} speech to text failed: {error}");
            return shared
                .fail(request, FailedStage::AwaitingResponse, &error)
                .await;
        }
    };
    let empty = text.trim().is_empty();
    if !shared
        .emit(ProviderOutput::Transcript {
            request: None,
            text: text.clone(),
            finished: true,
        })
        .await
    {
        return;
    }
    let held = checked.is_some_and(|sound| sound.load(Ordering::Acquire))
        && !crate::not_speech::short_word(&text);
    if held && !empty {
        eprintln!(
            "lamp-voice second: request {request}: its answer is held back: a short sound, and nothing shows it was said to the lamp: not heard as a short word (this route has no ears or eyes)"
        );
    }
    let heeded = match &shared.heed {
        Some(heed) if !empty && !held => {
            let window = said.lock().map_or((0, 0), |said| *said);
            heed(request, window, &text)
        }
        _ => None,
    };
    if let Some(why) = heeded {
        eprintln!("lamp-voice second: request {request}: its answer is held back: {why}");
    }
    if empty || held || heeded.is_some() {
        shared
            .emit(ProviderOutput::TurnComplete {
                request,
                idle: true,
            })
            .await;
        return;
    }
    answer(&shared, request, text, true).await;
}

/// How an answer stopped short.
enum Stop {
    /// `not_for_me`: nothing more is said.
    Silent,
    Failed(Fault),
    /// The engine is gone.
    Gone,
}

/// Chat, speech and the turn's last events, for the person's `said`.
async fn answer(shared: &Shared, request: u64, said: String, heard: bool) {
    // Started now, with the chat request, so it is ready first.
    let mut speech = JoinSet::new();
    let announcement = shared
        .announcement_due()
        .then(|| start_speech(shared, &mut speech, ANNOUNCEMENT.to_owned()));
    let (sentences, mut queued) = mpsc::channel(SENTENCES_AHEAD);
    let spoke = AtomicBool::new(false);
    let mut queue = Queue {
        shared,
        speech: &mut speech,
        announcement,
        sentences,
        started: false,
    };
    let began = Instant::now();
    let asked = Message::user(said);
    let writing = async move {
        let told = (shared.system)();
        let mut messages = vec![Message::system(told.text)];
        messages.extend(told.history);
        messages.push(asked);
        for round in 1..=MAX_ROUNDS {
            let mut stream = shared.services.chat(messages.clone());
            let mut first = true;
            let mut split = Sentences::default();
            let mut text = String::new();
            let mut calls = Vec::new();
            loop {
                let next = if first && round == 1 {
                    let by = tokio::time::Instant::from_std(began + BEGIN_WITHIN);
                    match tokio::time::timeout_at(by, stream.next()).await {
                        Ok(next) => next,
                        Err(_) => {
                            eprintln!(
                                "lamp-voice second: request {request} chat round {round} had not begun in {} ms; the turn ends unanswered",
                                began.elapsed().as_millis()
                            );
                            return Err(Stop::Silent);
                        }
                    }
                } else {
                    stream.next().await
                };
                let Some(event) = next else { break };
                let event = event.map_err(|error| {
                    eprintln!("lamp-voice second: request {request} chat round {round} failed after {} ms: {error}", began.elapsed().as_millis());
                    Stop::Failed(fault(&error))
                })?;
                if first {
                    first = false;
                    eprintln!(
                        "lamp-voice second: request {request} chat round {round} first event in {} ms",
                        began.elapsed().as_millis()
                    );
                }
                match event {
                    ChatEvent::Text(piece) => {
                        text.push_str(&piece);
                        // The model has chosen to speak: the announcement
                        // need not wait for its first sentence.
                        if piece.chars().any(char::is_alphanumeric) {
                            queue.announce().await?;
                        }
                        let mut sentences = split.push(&piece);
                        if sentences.is_empty() && !queue.started {
                            sentences.extend(split.clause());
                        }
                        for sentence in sentences {
                            queue.push(sentence).await?;
                        }
                    }
                    ChatEvent::ToolCalls(asked) => calls = asked,
                    ChatEvent::Done => break,
                }
            }
            // Before the unfinished last words are said.
            if !calls.is_empty() {
                let names: Vec<&str> = calls.iter().map(|call| call.name.as_str()).collect();
                eprintln!(
                    "lamp-voice second: request {request} chat round {round} called {}",
                    names.join(", ")
                );
            }
            if calls.iter().any(|call| call.name == NOT_FOR_ME) {
                return Err(Stop::Silent);
            }
            if let Some(rest) = split.finish() {
                queue.push(rest).await?;
            }
            if calls.is_empty() {
                return Ok(());
            }
            messages.push(Message::assistant_calls(Some(text), &calls));
            for call in &calls {
                let result =
                    (shared.tools)(request, heard, call.name.clone(), call.args.clone()).await;
                messages.push(Message::tool(call.id.clone(), result.to_string()));
            }
        }
        // ponytail: a model still calling tools after MAX_ROUNDS ends the
        // turn with what it has said; a spoken "I could not finish" if seen.
        Ok(())
    };
    let saying = async {
        let mut after = false;
        while let Some((text, mut packets)) = queued.recv().await {
            // Sentences come trimmed: each after the first is said after a
            // space, so the lamp's words read "just now. Hmm," (#1014).
            let words = ProviderOutput::Transcript {
                request: Some(request),
                text: if std::mem::replace(&mut after, true) {
                    format!(" {text}")
                } else {
                    text
                },
                finished: false,
            };
            if !shared.emit(words).await {
                return Err(Stop::Gone);
            }
            while let Some(packet) = packets.recv().await {
                let samples = packet.map_err(|error| Stop::Failed(fault(&error)))?;
                for part in samples.chunks(PACKET_SAMPLES) {
                    let audio = ProviderOutput::Audio {
                        request,
                        sequence: shared.sequence.fetch_add(1, Ordering::Relaxed) + 1,
                        provider_event_at_us: monotonic_us(),
                        source_frames: part.len(),
                        source_offset: 0,
                        samples: part.to_vec(),
                    };
                    if !shared.emit(audio).await {
                        return Err(Stop::Gone);
                    }
                    if !spoke.swap(true, Ordering::Relaxed) {
                        eprintln!(
                            "lamp-voice second: request {request} first answer audio {} ms after the words were heard",
                            began.elapsed().as_millis()
                        );
                    }
                    if let Ok(mut at) = shared.answered_at.lock() {
                        *at = Some((shared.clock)());
                    }
                }
            }
        }
        Ok(())
    };
    let last = match tokio::try_join!(writing, saying) {
        Ok(_) => {
            if !shared
                .emit(ProviderOutput::GenerationComplete { request })
                .await
            {
                return;
            }
            ProviderOutput::TurnComplete {
                request,
                idle: true,
            }
        }
        Err(Stop::Silent) => ProviderOutput::TurnComplete {
            request,
            idle: true,
        },
        Err(Stop::Failed(reason)) => ProviderOutput::TurnFailed {
            request,
            stage: if spoke.load(Ordering::Relaxed) {
                FailedStage::Responding
            } else {
                FailedStage::AwaitingResponse
            },
            reason,
        },
        Err(Stop::Gone) => return,
    };
    shared.emit(last).await;
}

type Packets = mpsc::Receiver<net::Result<Vec<i16>>>;

/// Speech of `text` downloading into its own queue, in the turn's set so
/// that it stops with the turn.
fn start_speech(shared: &Shared, speech: &mut JoinSet<()>, text: String) -> Packets {
    let (packets, queued) = mpsc::channel(SENTENCE_PACKETS);
    let mut stream = shared.services.speak(text);
    speech.spawn(async move {
        while let Some(packet) = stream.next().await {
            let failed = packet.is_err();
            if packets.send(packet).await.is_err() || failed {
                break;
            }
        }
    });
    queued
}

/// Sentences on their way to being said, each with its speech started.
struct Queue<'a> {
    shared: &'a Shared,
    speech: &'a mut JoinSet<()>,
    /// Said before the first sentence of the answer, if due.
    announcement: Option<Packets>,
    sentences: mpsc::Sender<(String, Packets)>,
    /// Some of the answer itself is on its way.
    started: bool,
}
impl Queue<'_> {
    async fn announce(&mut self) -> Result<(), Stop> {
        match self.announcement.take() {
            Some(packets) => self.send(ANNOUNCEMENT.to_owned(), packets).await,
            None => Ok(()),
        }
    }

    async fn push(&mut self, text: String) -> Result<(), Stop> {
        // A "sentence" with no word in it is not said: as the first words
        // of an answer, the coordinator would take it for silence.
        if !text.chars().any(char::is_alphanumeric) {
            return Ok(());
        }
        self.announce().await?;
        self.started = true;
        let packets = start_speech(self.shared, self.speech, text.clone());
        self.send(text, packets).await
    }

    async fn send(&mut self, text: String, packets: Packets) -> Result<(), Stop> {
        // Closed only when the saying half stopped; its error is the one kept.
        self.sentences
            .send((text, packets))
            .await
            .map_err(|_| Stop::Gone)
    }
}
