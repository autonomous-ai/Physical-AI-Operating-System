//! Prolonged scripted conversations with injected failures.
//!
//! One person, one supervised provider link, a speaker draining at 24,000
//! samples per second, and a scripted service that answers, stalls, closes,
//! announces closes and stays silent. The runtime clock is paused, so tens of
//! minutes of conversation run in seconds and every run with the same seed is
//! identical.
//!
//! Each request's audio carries that request's own sample value. The ledger
//! checks, for every accepted request, that only its own audio was ever
//! delivered under its name, that it ended in exactly one explicit outcome,
//! and that the service never received its input twice.
use super::*;
use crate::{BarrierPolicy, Discard, testing::FakeService};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::BTreeMap;

struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
    fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.below(high - low + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// The service completed it and every sample was delivered.
    Completed,
    /// Generation completed, the connection closed, every sample was delivered.
    Settled,
    Lost(Stage),
    /// The person moved on before it finished.
    Cancelled,
    /// Spoken while no connection was ready: refused at once, never sent.
    Refused,
}
#[derive(Default)]
struct Record {
    /// Samples the service put on the wire for this request before any failure.
    sent: u64,
    delivered: u64,
    /// Input blocks any session of the service received for it.
    input_blocks: u64,
    outcome: Option<Outcome>,
}
#[derive(Default)]
struct Tally {
    recoveries: u64,
    assumed: u64,
    discarded: u64,
    go_away: u64,
    lost: u64,
    settled: u64,
    completed: u64,
    cancelled: u64,
    /// Requests spoken into an outage and refused.
    refused: u64,
    /// Times the speaker stopped taking output for a while.
    held: u64,
    /// Replacement connections that died before anything was said on them.
    flapped: u64,
    /// Replacement attempts the service refused during setup.
    refused_dials: u64,
    elapsed: Duration,
}

/// How the supervisor is consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Consumer {
    /// `next()`: link changes and output through one call.
    Combined,
    /// `poll_link()` on every pass, `next_output()` only while the speaker is
    /// taking audio: the shape of a consumer paced by output capacity.
    Split,
}

/// What the service does with the next replacement connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Replacement {
    Clean,
    /// The first attempt is refused during setup; the next is accepted.
    RefusedOnce,
    /// Accepted, then closed before anything is said on it.
    DiesUnused,
    /// Accepted without a new handle: only the inherited point remains.
    NoHandle,
    /// Setup is acknowledged late, inside the outage budget.
    Late(Duration),
}
impl Replacement {
    /// Connections the service accepts before one is usable.
    fn reconnects(self) -> u64 {
        if self == Self::DiesUnused { 2 } else { 1 }
    }
}

const BLOCKS: u64 = 4;
fn marker(request: u64) -> i16 {
    (request % 30_000) as i16 + 1
}
fn confirm_idle() -> Value {
    json!({"serverContent":{"turnComplete":true,"interactionStatus":"IDLE"}})
}
fn confirm_interrupted() -> Value {
    json!({"serverContent":{"interrupted":true}})
}
fn generation_complete() -> Value {
    json!({"serverContent":{"generationComplete":true}})
}

struct Conversation {
    supervisor: Supervisor<FakeConnector>,
    dials: UnboundedReceiver<Dial>,
    service: FakeService,
    handles: u64,
    ledger: BTreeMap<u64, Record>,
    tally: Tally,
    started: Instant,
    /// The request whose answer is still owed, if any.
    open: Option<u64>,
    /// An activity the service has open with no End sent.
    activity_open: bool,
    /// The request this session of the service has been asked to answer and
    /// has not completed. Only that can be interrupted, and only on the
    /// session that received it: a new session knows nothing of it.
    service_owes: Option<u64>,
    /// An outage was announced and no connection has been ready since.
    link_down: bool,
    next_request: u64,
    consumer: Consumer,
    /// Session the service is serving now, and the last one meant to be used.
    session: u64,
    usable: u64,
    /// The newest handle: the session that issued it and whether the service
    /// still calls it current.
    point: (u64, bool),
    /// Session of the last accepted request.
    input_session: u64,
    /// Context each replacement must be reported with, by session.
    expected: BTreeMap<u64, Context>,
    plans: VecDeque<Replacement>,
    /// The person speaks during the next outage; `true` also cancels what
    /// was still owed, as a local interruption does.
    interject: Option<bool>,
    hold_until: Option<Instant>,
    revision: u64,
    /// When the outage in progress was observed.
    outage: Option<Instant>,
    /// Attempts are answered as they are made, not when the outage is read.
    eager: bool,
    /// Context the retained point can claim for the outage in progress.
    context: Context,
    /// The outage's first attempt was refused; the next one is its retry.
    refused_once: bool,
    /// An attempt whose setup is acknowledged at the given time.
    late: Option<(Instant, Dial)>,
    trace: String,
}

impl Conversation {
    async fn begin(policy: RecoveryPolicy, consumer: Consumer, barrier: BarrierPolicy) -> Self {
        let config = config().barrier_policy(barrier);
        let (dialer, mut dials) = connector(config);
        let mut supervisor = Supervisor::new(dialer, policy).unwrap();
        let serve = async {
            let mut dial = next_dial(&mut dials).await;
            dial.service.accept().await;
            dial.service
        };
        let first = async {
            match consumer {
                Consumer::Combined => {
                    let first = timeout(WAIT, supervisor.next()).await;
                    assert!(matches!(first, Ok(Ok(Notice::Ready { .. }))));
                    0
                }
                // The two interfaces are never mixed on one supervisor.
                Consumer::Split => loop {
                    if let Some(update) = supervisor.poll_link() {
                        assert!(matches!(
                            update.kind,
                            LinkUpdateKind::Ready {
                                context: Context::Initial,
                                outage_started_at: None,
                                ..
                            }
                        ));
                        break update.revision;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                },
            }
        };
        let (revision, service) = tokio::join!(first, serve);
        let mut conversation = Self {
            supervisor,
            dials,
            service,
            handles: 0,
            ledger: BTreeMap::new(),
            tally: Tally::default(),
            started: Instant::now(),
            open: None,
            activity_open: false,
            service_owes: None,
            link_down: false,
            next_request: 0,
            consumer,
            session: 1,
            usable: 1,
            point: (0, false),
            input_session: 0,
            expected: BTreeMap::new(),
            plans: VecDeque::new(),
            interject: None,
            hold_until: None,
            revision,
            outage: None,
            eager: false,
            context: Context::Resumed,
            refused_once: false,
            late: None,
            trace: String::new(),
        };
        conversation.issue_handle().await;
        conversation
    }
    async fn issue_handle(&mut self) {
        self.handles += 1;
        self.service
            .send(handle(&format!("HANDLE-{}", self.handles)))
            .await;
        self.point = (self.session, true);
    }
    /// The service says its present state cannot be resumed. The last handle
    /// still resumes, only up to its own point.
    async fn withdraw_handle(&mut self) {
        self.service
            .send(json!({"sessionResumptionUpdate":{"newHandle":"","resumable":false}}))
            .await;
        self.point.1 = false;
    }
    /// The speaker takes no output for a while.
    ///
    /// The split consumer still reads the link on every pass. The combined
    /// one cannot: `next()` would hand it output it has no room for, so it
    /// only runs upkeep, and an outage announced meanwhile stays unread.
    /// Upkeep retries failed attempts regardless, but a replacement that
    /// completes setup and then dies is noticed only once the outage has been
    /// read. If that is later than the outage budget the link ends. The
    /// combined consumer is therefore held for less than the budget; reading
    /// the link independently of output is what removes the limit.
    fn hold(&mut self, seconds: u64) {
        const COMBINED_LIMIT: u64 = 12;
        debug_assert!(Duration::from_secs(COMBINED_LIMIT) < self.supervisor.policy.outage_budget);
        let seconds = match self.consumer {
            Consumer::Combined => seconds.min(COMBINED_LIMIT),
            Consumer::Split => seconds,
        };
        self.hold_until = Some(Instant::now() + Duration::from_secs(seconds));
        self.tally.held += 1;
    }

    /// One client message, or `None` when the service hears nothing in time.
    async fn heard(&mut self, wait: Duration) -> Option<&'static str> {
        let message = self.service.try_receive(wait).await?;
        let input = &message["realtimeInput"];
        if input.get("activityStart").is_some() {
            self.activity_open = true;
            return Some("start");
        }
        if input.get("activityEnd").is_some() {
            self.activity_open = false;
            return Some("end");
        }
        let pcm = STANDARD
            .decode(input["audio"]["data"].as_str().expect("an input block"))
            .unwrap();
        let value = i16::from_le_bytes([pcm[0], pcm[1]]);
        // Input blocks carry their request in the same way answers do.
        let request = self
            .ledger
            .iter()
            .find(|(request, _)| marker(**request) == value)
            .map(|(request, _)| *request)
            .expect("input of a known request");
        self.ledger.get_mut(&request).unwrap().input_blocks += 1;
        Some("block")
    }

    /// The person speaks a new request to the end. Whatever was still owed is
    /// cancelled first, exactly as the provider worker does on a new owner.
    /// `confirm` is how the service reacts to being interrupted, if it is.
    async fn say(&mut self, confirm: Confirm) -> u64 {
        self.next_request += 1;
        let request = self.next_request;
        self.ledger.insert(request, Record::default());
        let interrupting = self.open.take();
        if let Some(old) = interrupting {
            self.supervisor.retire(RequestId::new(old).unwrap());
            let record = self.ledger.get_mut(&old).unwrap();
            if record.outcome.is_none() {
                record.outcome = Some(Outcome::Cancelled);
                self.tally.cancelled += 1;
            }
        }
        let id = RequestId::new(request).unwrap();
        self.supervisor.try_start(id).unwrap();
        self.input_session = self.session;
        for sequence in 0..BLOCKS {
            self.supervisor
                .try_audio(
                    id,
                    sequence,
                    std::time::Instant::now(),
                    &[marker(request); 160],
                )
                .unwrap();
        }
        self.supervisor.try_end(id).unwrap();
        // The opening words arrive whole and in order before any confirmation.
        if !self.activity_open {
            assert_eq!(self.heard(WAIT).await, Some("start"), "request {request}");
        }
        for _ in 0..BLOCKS {
            assert_eq!(self.heard(WAIT).await, Some("block"), "request {request}");
        }
        if self.service_owes.take().is_some() {
            match confirm {
                Confirm::Both => {
                    self.service.send(confirm_interrupted()).await;
                    self.service.send(confirm_idle()).await;
                }
                Confirm::Completion => self.service.send(confirm_idle()).await,
                // Nothing: the transport must assume, and say so.
                Confirm::Silent => {}
            }
        }
        // Committed once the earlier response has settled, by word or by bound.
        assert_eq!(
            self.heard(Duration::from_secs(5)).await,
            Some("end"),
            "request {request} was never committed"
        );
        self.open = Some(request);
        self.service_owes = Some(request);
        request
    }
    /// The service finishes the answer it owes.
    async fn complete(&mut self) {
        self.service.send(generation_complete()).await;
        self.service.send(confirm_idle()).await;
        self.service_owes = None;
    }

    /// The service writes `seconds` of the answer to `request` in `chunk`
    /// sample messages. Everything is on the wire at once: generation outruns
    /// playback, and the transport holds it.
    async fn answer(&mut self, request: u64, seconds: u64, chunk: usize) {
        let mut left = seconds * RATE;
        while left > 0 {
            let samples = (chunk as u64).min(left) as usize;
            self.service.send(audio(marker(request), samples)).await;
            left -= samples as u64;
        }
        self.ledger.get_mut(&request).unwrap().sent += seconds * RATE;
    }

    /// The supervisor's next connection attempt, whenever it makes one.
    async fn dialed(&mut self) -> Dial {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.supervisor.maintain();
            if let Ok(dial) = self.dials.try_recv() {
                return dial;
            }
            assert!(
                Instant::now() < deadline,
                "no reconnect attempt: {} pending={:?} request={} turn={}",
                match &self.supervisor.link {
                    Link::Connecting { number, .. } => format!("connecting attempt {number}"),
                    Link::Waiting { number, .. } => format!("waiting before attempt {number}"),
                    Link::Connected(_) => "connected".to_owned(),
                    Link::Failed(failure) => format!("failed: {failure}"),
                },
                self.supervisor.pending_link,
                self.next_request,
                self.trace,
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    /// Every attempt presents the newest handle, and nothing else.
    async fn presented(&self, dial: &mut Dial) {
        assert!(dial.resumed, "context is presented on every reconnect");
        let setup = dial.service.receive().await;
        assert_eq!(
            setup["setup"]["sessionResumption"]["handle"],
            format!("HANDLE-{}", self.handles)
        );
    }
    /// The person speaks while no connection is ready. Nothing is buffered
    /// for later: the request is refused at once and its number is spent.
    fn speak_into_outage(&mut self, cancel_owed: bool) {
        self.next_request += 1;
        let refused = self.next_request;
        self.ledger.insert(
            refused,
            Record {
                outcome: Some(Outcome::Refused),
                ..Record::default()
            },
        );
        self.tally.refused += 1;
        if cancel_owed && let Some(old) = self.open.take() {
            self.supervisor.retire(RequestId::new(old).unwrap());
            let record = self.ledger.get_mut(&old).unwrap();
            if record.outcome.is_none() {
                record.outcome = Some(Outcome::Cancelled);
                self.tally.cancelled += 1;
            }
        }
        let id = RequestId::new(refused).unwrap();
        assert_eq!(self.supervisor.try_start(id), Err(Error::NotConnected));
        assert_eq!(
            self.supervisor
                .try_audio(id, 0, std::time::Instant::now(), &[marker(refused); 160]),
            Err(Error::NotConnected)
        );
        assert_eq!(self.supervisor.try_end(id), Err(Error::NotConnected));
    }

    /// The original script: serve the replacement once the outage is announced.
    async fn reconnect(&mut self) {
        let dial = self.dialed().await;
        self.serve(dial).await;
    }
    /// Answer one connection attempt as the plan for this outage says.
    async fn serve(&mut self, mut dial: Dial) {
        self.presented(&mut dial).await;
        let plan = if std::mem::take(&mut self.refused_once) {
            // The attempt after a refused one belongs to the same outage.
            Replacement::Clean
        } else {
            // What the retained point can claim, as of the failure: current
            // only if the service still says so and nothing was accepted on
            // a later session than the one that issued it.
            self.context = if self.point.1 && self.input_session <= self.point.0 {
                Context::Resumed
            } else {
                Context::ResumedBeforeLatest
            };
            self.plans.pop_front().unwrap_or(Replacement::Clean)
        };
        match plan {
            Replacement::RefusedOnce => {
                dial.service.close(1011, "internal").await;
                self.tally.refused_dials += 1;
                self.refused_once = true;
            }
            Replacement::Late(wait) => self.late = Some((Instant::now() + wait, dial)),
            _ => self.accept(dial, plan).await,
        }
    }
    async fn accept(&mut self, dial: Dial, plan: Replacement) {
        self.service = dial.service;
        self.session = dial.session.get();
        self.activity_open = false;
        self.service_owes = None;
        self.service.send(json!({"setupComplete": {}})).await;
        self.expected.insert(self.session, self.context);
        if plan != Replacement::NoHandle {
            self.issue_handle().await;
        }
        self.tally.recoveries += 1;
        if plan == Replacement::DiesUnused {
            self.service.close(1011, "internal").await;
            self.tally.flapped += 1;
        } else {
            self.usable = self.session;
        }
    }
    /// A live service answers a connection attempt whenever one is made, not
    /// when its client gets round to reading that the old one failed.
    async fn serve_attempts(&mut self) {
        if let Ok(dial) = self.dials.try_recv() {
            self.serve(dial).await;
        }
        if self
            .late
            .as_ref()
            .is_some_and(|(due, _)| Instant::now() >= *due)
        {
            let (_, dial) = self.late.take().unwrap();
            self.accept(dial, Replacement::Clean).await;
        }
    }

    /// One notice, or `None` when there is nothing to take right now. The
    /// split consumer checks the link first, always, and takes output only
    /// while the speaker is not held.
    async fn take(&mut self) -> Option<Notice> {
        const PASS: Duration = Duration::from_millis(20);
        let held = self.hold_until.is_some_and(|until| Instant::now() < until);
        match self.consumer {
            Consumer::Combined if held => None,
            Consumer::Combined => timeout(PASS, self.supervisor.next())
                .await
                .ok()
                .map(|notice| notice.unwrap_or_else(|failure| panic!("{failure}: {}", self.trace))),
            Consumer::Split => {
                if let Some(update) = self.supervisor.poll_link() {
                    assert!(update.revision > self.revision, "{update:?}");
                    self.revision = update.revision;
                    assert!(update.observed_at <= Instant::now(), "{update:?}");
                    return Some(match update.kind {
                        LinkUpdateKind::Ready {
                            context,
                            attempts,
                            started_at,
                            completed_at,
                            outage_started_at,
                            after,
                        } => {
                            assert_eq!(update.observed_at, completed_at);
                            assert!(started_at <= completed_at);
                            assert_eq!(outage_started_at, self.outage, "{update:?}");
                            Notice::Ready {
                                session: update.session,
                                context,
                                attempts,
                                elapsed: completed_at
                                    .duration_since(outage_started_at.unwrap_or(started_at)),
                                after,
                            }
                        }
                        LinkUpdateKind::Unavailable { error } => {
                            self.outage = Some(update.observed_at);
                            Notice::Unavailable { error }
                        }
                        LinkUpdateKind::Terminal { failure } => {
                            panic!("terminal failure in a recoverable scenario: {failure}")
                        }
                    });
                }
                if held {
                    return None;
                }
                timeout(PASS, self.supervisor.next_output())
                    .await
                    .ok()
                    .map(|output| {
                        output
                            .unwrap_or_else(|failure| panic!("{failure}: {}", self.trace))
                            .into()
                    })
            }
        }
    }

    /// Play at speaking speed until `stop` holds, keeping the ledger. Pings
    /// are answered while the speaker is busy, as a live service would.
    async fn play(&mut self, stop: Stop) {
        let mut heard = 0u64;
        loop {
            let ready = !self.link_down && self.supervisor.is_connected();
            let done = match stop {
                Stop::AtOutcome => self.open.is_none() && ready,
                Stop::AfterSeconds(seconds) => heard >= seconds * RATE,
                Stop::Recoveries(count) => self.tally.recoveries >= count && ready,
            };
            if done {
                return;
            }
            self.supervisor.maintain();
            if self.eager {
                self.serve_attempts().await;
            }
            let Some(notice) = self.take().await else {
                // Nothing to deliver right now. Time still passes.
                if let Some(message) = self.service.rest(Duration::from_millis(20)).await {
                    panic!("unexpected client message during playback: {message}");
                }
                continue;
            };
            match notice {
                Notice::Event(Event::Audio { lineage, pcm, .. }) => {
                    let request = lineage.request.get();
                    assert!(
                        pcm.iter().all(|sample| *sample == marker(request)),
                        "audio delivered under request {request} belongs to another request"
                    );
                    let record = self.ledger.get_mut(&request).unwrap();
                    assert!(record.outcome.is_none(), "audio after {:?}", record.outcome);
                    record.delivered += pcm.len() as u64;
                    assert!(record.delivered <= record.sent);
                    heard += pcm.len() as u64;
                    let time = Duration::from_micros(pcm.len() as u64 * 1_000_000 / RATE);
                    if let Some(message) = self.service.rest(time).await {
                        panic!("unexpected client message during playback: {message}");
                    }
                }
                Notice::Event(Event::TurnComplete {
                    lineage,
                    idle: true,
                }) => {
                    let request = lineage.request.get();
                    let record = self.ledger.get_mut(&request).unwrap();
                    if record.outcome.is_none() {
                        assert_eq!(record.delivered, record.sent, "request {request}");
                        record.outcome = Some(Outcome::Completed);
                        self.tally.completed += 1;
                        self.open = None;
                    }
                }
                Notice::Settled { lineage, .. } => {
                    let request = lineage.request.get();
                    let record = self.ledger.get_mut(&request).unwrap();
                    assert!(record.outcome.is_none());
                    assert_eq!(record.delivered, record.sent, "request {request}");
                    record.outcome = Some(Outcome::Settled);
                    self.tally.settled += 1;
                    self.open = None;
                }
                Notice::TurnLost { lineage, stage, .. } => {
                    let request = lineage.request.get();
                    let record = self.ledger.get_mut(&request).unwrap();
                    assert!(record.outcome.is_none());
                    // Everything that arrived before the loss was delivered.
                    assert_eq!(record.delivered, record.sent, "request {request}");
                    record.outcome = Some(Outcome::Lost(stage));
                    self.tally.lost += 1;
                    self.open = None;
                }
                Notice::Unavailable { .. } => {
                    self.link_down = true;
                    // The outage is always read before the fate of the request
                    // it interrupted, and never after its replacement is used.
                    if let Some(cancel_owed) = self.interject.take() {
                        self.speak_into_outage(cancel_owed);
                    }
                    if !self.eager {
                        self.reconnect().await;
                    }
                }
                Notice::Ready {
                    session,
                    context,
                    attempts,
                    after,
                    ..
                } => {
                    assert!(after.is_some());
                    assert!(attempts <= self.supervisor.policy.max_attempts);
                    let expected = self.expected.remove(&session.get());
                    assert_eq!(Some(context), expected, "session {}", session.get());
                    // A connection the service has already closed is not one
                    // the person can speak on, whatever was announced.
                    if session.get() == self.usable {
                        self.link_down = false;
                    }
                }
                Notice::Event(Event::BarrierAssumed { .. }) => self.tally.assumed += 1,
                Notice::Event(Event::Discarded { reason, .. }) => {
                    assert_ne!(
                        reason,
                        Discard::LateOutput,
                        "no scenario here sends late audio"
                    );
                    self.tally.discarded += 1;
                }
                Notice::Event(Event::GoAway { .. }) => self.tally.go_away += 1,
                Notice::Event(_) => {}
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Confirm {
    Both,
    Completion,
    Silent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// The open request reached an outcome and a connection is ready.
    AtOutcome,
    AfterSeconds(u64),
    /// At least this many recoveries have happened and a connection is ready.
    Recoveries(u64),
}

async fn converse(turns: u64, seed: u64) -> Tally {
    let policy = RecoveryPolicy {
        // Failures are injected far more often than any real link would fail.
        max_recoveries: 64,
        window: Duration::from_secs(60),
        ..RecoveryPolicy::default()
    };
    let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut talk =
        Conversation::begin(policy, Consumer::Combined, BarrierPolicy::AssumeAfterQuiet).await;
    for _ in 0..turns {
        // The person speaks only when a connection is ready. Speaking into an
        // outage is the provider worker's case, covered by its own tests.
        talk.play(Stop::Recoveries(0)).await;
        let confirm = match random.below(3) {
            0 => Confirm::Both,
            1 => Confirm::Completion,
            _ => Confirm::Silent,
        };
        let request = talk.say(confirm).await;
        let seconds = match random.below(100) {
            0..=79 => random.between(1, 4),
            80..=94 => random.between(10, 30),
            _ => random.between(60, 180),
        };
        let chunk = [960, 4_800, 24_000, LARGEST][random.below(4) as usize];
        match random.below(100) {
            // A complete answer, played to the end.
            0..=44 => {
                talk.answer(request, seconds, chunk).await;
                talk.complete().await;
                talk.play(Stop::AtOutcome).await;
            }
            // The person interrupts part-way; the next turn cancels it.
            45..=59 => {
                talk.answer(request, seconds, chunk).await;
                talk.play(Stop::AfterSeconds(random.between(1, seconds)))
                    .await;
            }
            // The person continues before any answer: nothing is sent at all.
            60..=69 => {}
            // The connection dies before generation completes.
            70..=77 => {
                talk.answer(request, seconds, chunk).await;
                talk.service.close(1011, "internal").await;
                talk.play(Stop::AtOutcome).await;
                assert_eq!(
                    talk.ledger[&request].outcome,
                    Some(Outcome::Lost(Stage::Responding))
                );
            }
            // The connection dies with the whole answer already received.
            78..=85 => {
                talk.answer(request, seconds, chunk).await;
                talk.service.send(generation_complete()).await;
                talk.service.close(1008, "The operation was aborted").await;
                talk.play(Stop::AtOutcome).await;
                assert_eq!(talk.ledger[&request].outcome, Some(Outcome::Settled));
            }
            // The answer completes, then the idle session is closed.
            86..=90 => {
                talk.answer(request, seconds, chunk).await;
                talk.complete().await;
                talk.play(Stop::AtOutcome).await;
                let recoveries = talk.tally.recoveries;
                talk.service.close(1008, "The operation was aborted").await;
                talk.play(Stop::Recoveries(recoveries + 1)).await;
            }
            // The service goes quiet mid-answer without closing.
            91..=94 => {
                talk.answer(request, seconds.min(20), chunk).await;
                talk.play(Stop::AtOutcome).await;
                assert_eq!(
                    talk.ledger[&request].outcome,
                    Some(Outcome::Lost(Stage::Responding))
                );
            }
            // The question gets no answer at all.
            95..=96 => {
                talk.play(Stop::AtOutcome).await;
                assert_eq!(
                    talk.ledger[&request].outcome,
                    Some(Outcome::Lost(Stage::AwaitingResponse))
                );
            }
            // The service announces a close mid-answer, then finishes it.
            _ => {
                let recoveries = talk.tally.recoveries;
                talk.answer(request, seconds, chunk).await;
                talk.service
                    .send(json!({"goAway":{"timeLeft":"10s"}}))
                    .await;
                talk.complete().await;
                talk.play(Stop::AtOutcome).await;
                // The announced close is taken at the clean boundary.
                talk.play(Stop::Recoveries(recoveries + 1)).await;
            }
        }
    }
    // The last request may still be open; the ledger must be whole otherwise.
    let open = talk.open;
    for (request, record) in &talk.ledger {
        if Some(*request) == open {
            continue;
        }
        assert!(record.outcome.is_some(), "request {request} has no outcome");
        // Its input reached the service exactly once across every session.
        assert_eq!(record.input_blocks, BLOCKS, "request {request}");
    }
    let mut tally = talk.tally;
    tally.elapsed = talk.started.elapsed();
    tally
}

/// One request's row of the ledger, for comparing two runs.
type Row = (u64, u64, Option<Outcome>);

/// The production barrier policy, and failures chosen to break recovery:
/// replacements that are refused, late, without a handle or dead on arrival;
/// a speaker that stops taking output; speech into an outage; interruption of
/// an answer whose connection is already gone; and a withdrawn handle.
async fn adversarial(turns: u64, seed: u64, consumer: Consumer) -> (Tally, BTreeMap<u64, Row>) {
    let policy = RecoveryPolicy {
        max_recoveries: 64,
        window: Duration::from_secs(60),
        ..RecoveryPolicy::default()
    };
    let mut random = Random(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
    let mut talk = Conversation::begin(policy, consumer, BarrierPolicy::Require).await;
    talk.eager = true;
    for _ in 0..turns {
        talk.play(Stop::Recoveries(0)).await;
        // Under `Require` the service always says the earlier response ended.
        let confirm = if random.below(2) == 0 {
            Confirm::Both
        } else {
            Confirm::Completion
        };
        let request = talk.say(confirm).await;
        let seconds = match random.below(100) {
            0..=69 => random.between(1, 4),
            70..=94 => random.between(10, 30),
            _ => random.between(60, 120),
        };
        let chunk = [960, 4_800, 24_000, LARGEST][random.below(4) as usize];
        let plan = match random.below(5) {
            0 => Replacement::Clean,
            1 => Replacement::RefusedOnce,
            2 => Replacement::DiesUnused,
            3 => Replacement::NoHandle,
            _ => Replacement::Late(Duration::from_millis(random.between(100, 3_000))),
        };
        let interject = match random.below(3) {
            0 => None,
            1 => Some(false),
            _ => Some(true),
        };
        let recoveries = talk.tally.recoveries;
        let recovered = Stop::Recoveries(recoveries + plan.reconnects());
        let scenario = random.below(100);
        talk.trace = format!(
            "{consumer:?} request {request} scenario {scenario} plan {plan:?} interject {interject:?} seconds {seconds} chunk {chunk}"
        );
        match scenario {
            // A complete answer. The speaker stops part-way for less than the
            // delivery bound, then plays the rest.
            0..=14 => {
                talk.answer(request, seconds, chunk).await;
                talk.complete().await;
                talk.play(Stop::AfterSeconds(random.between(1, seconds)))
                    .await;
                talk.hold(random.between(1, 20));
                talk.play(Stop::AtOutcome).await;
                assert_eq!(talk.ledger[&request].outcome, Some(Outcome::Completed));
            }
            // The person interrupts part-way; the next turn cancels it.
            15..=24 => {
                talk.answer(request, seconds, chunk).await;
                talk.play(Stop::AfterSeconds(random.between(1, seconds)))
                    .await;
            }
            // The connection dies before generation completes. The person
            // may speak into the outage, with or without cancelling.
            25..=39 => {
                talk.plans.push_back(plan);
                talk.interject = interject;
                talk.answer(request, seconds, chunk).await;
                talk.service.close(1011, "internal").await;
                talk.play(Stop::AtOutcome).await;
                let expected = if interject == Some(true) {
                    Outcome::Cancelled
                } else {
                    Outcome::Lost(Stage::Responding)
                };
                assert_eq!(talk.ledger[&request].outcome, Some(expected));
            }
            // The connection dies with the whole answer received. It plays to
            // the end whatever happens to the replacement, held or not.
            40..=54 => {
                talk.plans.push_back(plan);
                talk.interject = interject.filter(|cancel_owed| !cancel_owed);
                talk.answer(request, seconds, chunk).await;
                talk.service.send(generation_complete()).await;
                talk.service.close(1008, "The operation was aborted").await;
                if random.below(2) == 0 {
                    talk.play(Stop::AfterSeconds(1)).await;
                    talk.hold(random.between(1, 20));
                }
                talk.play(Stop::AtOutcome).await;
                assert_eq!(talk.ledger[&request].outcome, Some(Outcome::Settled));
            }
            // The same, and the person interrupts on the replacement while the
            // dead connection's answer is still playing: the next turn does it.
            55..=64 => {
                talk.plans.push_back(plan);
                talk.answer(request, seconds.max(15), chunk).await;
                talk.service.send(generation_complete()).await;
                talk.service.close(1008, "The operation was aborted").await;
                talk.play(recovered).await;
                assert_eq!(talk.ledger[&request].outcome, None, "still playing");
            }
            // The answer completes, then the idle session is closed.
            65..=76 => {
                talk.answer(request, seconds, chunk).await;
                talk.complete().await;
                talk.play(Stop::AtOutcome).await;
                talk.plans.push_back(plan);
                talk.service.close(1008, "The operation was aborted").await;
                talk.play(recovered).await;
            }
            // The same after the service withdrew its resumable state.
            77..=84 => {
                talk.answer(request, seconds, chunk).await;
                talk.complete().await;
                talk.play(Stop::AtOutcome).await;
                talk.withdraw_handle().await;
                talk.plans.push_back(plan);
                talk.service.close(1008, "The operation was aborted").await;
                talk.play(recovered).await;
            }
            // The service goes quiet mid-answer without closing.
            85..=90 => {
                talk.plans.push_back(plan);
                talk.answer(request, seconds.min(20), chunk).await;
                talk.play(Stop::AtOutcome).await;
                assert_eq!(
                    talk.ledger[&request].outcome,
                    Some(Outcome::Lost(Stage::Responding))
                );
            }
            // The question gets no answer at all.
            91..=94 => {
                talk.plans.push_back(plan);
                talk.play(Stop::AtOutcome).await;
                assert_eq!(
                    talk.ledger[&request].outcome,
                    Some(Outcome::Lost(Stage::AwaitingResponse))
                );
            }
            // The service announces a close mid-answer, then finishes it.
            _ => {
                talk.plans.push_back(plan);
                talk.answer(request, seconds, chunk).await;
                talk.service
                    .send(json!({"goAway":{"timeLeft":"10s"}}))
                    .await;
                talk.complete().await;
                talk.play(Stop::AtOutcome).await;
                talk.play(recovered).await;
            }
        }
        assert!(talk.plans.is_empty() && talk.interject.is_none() && talk.late.is_none());
    }
    let open = talk.open;
    let mut rows = BTreeMap::new();
    for (request, record) in &talk.ledger {
        rows.insert(*request, (record.sent, record.delivered, record.outcome));
        if Some(*request) == open {
            continue;
        }
        let outcome = record
            .outcome
            .unwrap_or_else(|| panic!("request {request} has no outcome"));
        // Accepted input reached the service exactly once across every
        // session. Refused input reached it never.
        let heard = if outcome == Outcome::Refused {
            0
        } else {
            BLOCKS
        };
        assert_eq!(record.input_blocks, heard, "request {request}");
    }
    let mut tally = talk.tally;
    tally.elapsed = talk.started.elapsed();
    (tally, rows)
}

fn report_adversarial(name: &str, turns: u64, seed: u64, consumer: Consumer, tally: &Tally) {
    println!(
        "MEASURED {name} consumer={consumer:?} turns={turns} seed={seed} virtual_minutes={:.1} completed={} cancelled={} settled={} lost={} refused={} recoveries={} refused_dials={} dead_on_arrival={} speaker_holds={} go_away={} dropped_events={}",
        tally.elapsed.as_secs_f64() / 60.0,
        tally.completed,
        tally.cancelled,
        tally.settled,
        tally.lost,
        tally.refused,
        tally.recoveries,
        tally.refused_dials,
        tally.flapped,
        tally.held,
        tally.go_away,
        tally.discarded
    );
}

/// Both ways of consuming the supervisor must tell the same story about
/// every request of the same conversation.
async fn adversarial_both(name: &str, turns: u64, seed: u64) -> Tally {
    let (combined, by_next) = adversarial(turns, seed, Consumer::Combined).await;
    report_adversarial(name, turns, seed, Consumer::Combined, &combined);
    let (split, by_split) = adversarial(turns, seed, Consumer::Split).await;
    report_adversarial(name, turns, seed, Consumer::Split, &split);
    assert_eq!(by_next, by_split, "seed {seed}");
    assert_eq!(split.assumed, 0, "the production policy never assumes");
    split
}

fn report(name: &str, turns: u64, seed: u64, tally: &Tally) {
    println!(
        "MEASURED {name} turns={turns} seed={seed} virtual_minutes={:.1} completed={} cancelled={} settled={} lost={} recoveries={} assumed_barriers={} go_away={} dropped_events={}",
        tally.elapsed.as_secs_f64() / 60.0,
        tally.completed,
        tally.cancelled,
        tally.settled,
        tally.lost,
        tally.recoveries,
        tally.assumed,
        tally.go_away,
        tally.discarded
    );
}

#[tokio::test(start_paused = true)]
async fn sixty_turn_conversation_with_injected_failures_keeps_every_request_accounted_for() {
    let tally = converse(60, 1).await;
    report("scripted_conversation", 60, 1, &tally);
    assert!(tally.recoveries > 0 && tally.lost > 0 && tally.settled > 0 && tally.cancelled > 0);
}

#[tokio::test(start_paused = true)]
async fn adversarial_recoveries_account_for_every_request_through_both_interfaces() {
    let tally = adversarial_both("adversarial_conversation", 60, 1).await;
    assert!(
        tally.lost > 0
            && tally.settled > 0
            && tally.cancelled > 0
            && tally.refused > 0
            && tally.flapped > 0
            && tally.refused_dials > 0
            && tally.held > 0
    );
}

#[tokio::test(start_paused = true)]
#[ignore = "several hundred turns per seed and interface; run explicitly"]
async fn prolonged_adversarial_recoveries_account_for_every_request() {
    for seed in [2, 3, 5, 8] {
        adversarial_both("prolonged_adversarial_conversation", 400, seed).await;
    }
}

#[tokio::test(start_paused = true)]
#[ignore = "several hundred turns per seed; run explicitly"]
async fn prolonged_conversations_with_injected_failures_keep_every_request_accounted_for() {
    for seed in [2, 3, 5, 8] {
        let tally = converse(400, seed).await;
        report("prolonged_scripted_conversation", 400, seed, &tally);
    }
}
