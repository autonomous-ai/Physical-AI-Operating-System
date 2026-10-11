//! Event-triggered scenario execution shared by the fake and physical backends.
//!
//! Each step waits for its triggering runtime event within a bounded deadline,
//! then starts its stimulus at `event + delay`. A missing trigger, a lost overlap
//! precondition or an ended session withholds the remaining steps; the runner
//! never falls back to a fixed delay and never drops an attempt from the ledger.
use crate::{
    Result, Rng, derive_seed,
    events::{ClockDomain, EventKind, RuntimeEvent},
    ledger::{Ledger, unix_ms},
    plan::{LoadedPlan, Scenario, Step, TriggerEvent},
    playback::PlaybackHistory,
    record::{
        AttemptRecord, AttemptStatus, Attribution, ClockMapping, StepRecord, StepStatus,
        StimulusSource, Stratum, TriggerObservation,
    },
    stimulus::{SceneTiming, StimulusCatalog},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub struct AttemptContext<'a> {
    pub attempt_id: String,
    pub run_id: String,
    pub scenario: &'a Scenario,
    pub repetition: u32,
    pub order: u32,
    pub seed: u64,
    pub run_directory: PathBuf,
}

pub enum Begin {
    Ready,
    Withheld(String),
}

pub struct Injection {
    /// Best evidence of the actual start in the runner domain.
    pub started_us: u64,
    pub receipt: Value,
    /// The backend's final dispatch check refused to start the stimulus.
    pub withheld: Option<Withheld>,
}

/// Why a backend's final dispatch check refused to start a stimulus. The
/// first terminal cause seen on the attempt is latched (MAIN-009, MAIN-023).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Withheld {
    /// A turn ended with a terminal provider failure; there is no
    /// authoritative re-ready, so the provider stays unavailable.
    ProviderFailed(String),
    /// The runtime session ended: relayed `session_end`, a `run_end` cue, or
    /// the transport closed.
    SessionEnded(String),
    /// The cue or relay protocol failed; nothing trustworthy remains to
    /// schedule against.
    ProtocolFailed(String),
    /// The planned start passed its lateness bound at the backend's final
    /// side-effect boundary; nothing started (MAIN-030).
    Stale(String),
    /// The backend's final drain took events the runner has not observed;
    /// nothing started. The runner observes them, rechecks eligibility and
    /// may dispatch again. Never recorded as a step outcome by itself.
    Revalidate,
    /// The prepared player for this stimulus is missing, failed or cannot
    /// receive its start; nothing started.
    PlayerFailed(String),
}

impl Withheld {
    pub fn cause(&self) -> &'static str {
        match self {
            Self::ProviderFailed(_) => "provider_failed",
            Self::SessionEnded(_) => "session_ended",
            Self::ProtocolFailed(_) => "protocol_failed",
            Self::Stale(_) => "trigger_stale",
            Self::Revalidate => "revalidate",
            Self::PlayerFailed(_) => "player_failed",
        }
    }
    pub fn reason(&self) -> &str {
        match self {
            Self::ProviderFailed(reason)
            | Self::SessionEnded(reason)
            | Self::ProtocolFailed(reason)
            | Self::Stale(reason)
            | Self::PlayerFailed(reason) => reason,
            Self::Revalidate => {
                "events arrived during the final dispatch drain and must be observed first"
            }
        }
    }
    /// Only a provider failure is provider unavailability; a protocol
    /// failure already ends the live schedule as a run end.
    pub fn status(&self) -> StepStatus {
        match self {
            Self::ProviderFailed(_) => StepStatus::ProviderUnavailable,
            Self::SessionEnded(_) | Self::ProtocolFailed(_) => StepStatus::SessionEnded,
            Self::Stale(_) | Self::Revalidate => StepStatus::TriggerStale,
            Self::PlayerFailed(_) => StepStatus::DeliveryFailed,
        }
    }
}

pub struct Finished {
    pub events: Vec<RuntimeEvent>,
    pub clock: Option<ClockMapping>,
    pub evidence: Value,
    /// Backend failure that invalidates the attempt's evidence.
    pub aborted: Option<String>,
    pub unmapped: usize,
    pub spoken_failure_notice: Option<bool>,
    /// Later, better start evidence per step (for example the player's own
    /// start request), replacing the runner's provisional start time.
    pub corrected_starts: Vec<(String, u64)>,
    /// Steps whose stimulus the player reports as not delivered, with reason.
    pub failed_deliveries: Vec<(String, String)>,
}

/// A trigger later than this cannot start its stimulus at the planned phase;
/// the step is withheld instead of being played late.
pub const MAX_TRIGGER_LATENESS_US: u64 = 100_000;
/// Events taken in one final dispatch drain before the step is withheld.
const MAX_FINAL_DRAIN_EVENTS: usize = 256;
/// Final dispatch checks repeated because the backend's drain found events.
const MAX_DISPATCH_ATTEMPTS: u32 = 8;

pub trait Backend {
    fn describe(&self) -> Value;
    fn stratum(&self, scenario: &Scenario) -> Stratum;
    fn source(&self) -> StimulusSource;
    fn domain(&self) -> ClockDomain;
    fn profile(&self) -> Option<String> {
        None
    }
    /// Why the scenario cannot run on this backend, checked before any session.
    fn unsupported(&self, scenario: &Scenario) -> Option<String>;
    fn begin(&mut self, context: &AttemptContext) -> Result<Begin>;
    fn now_us(&mut self) -> u64;
    /// The next live event, waiting no later than `until_us` (runner domain).
    fn next_event(&mut self, until_us: u64) -> Result<Option<RuntimeEvent>>;
    /// Place a live event on the runner clock, naming the method used.
    fn runner_time(&self, event: &RuntimeEvent) -> (u64, String);
    fn timing(&mut self, catalog: &StimulusCatalog, scene: &str) -> Result<SceneTiming> {
        catalog.estimate(scene)
    }
    /// Prepare `step`'s stimulus before its trigger wait, so that no process
    /// creation or audio preparation sits between readiness and the start.
    /// Backends without a two-phase player do nothing.
    fn arm(&mut self, _step: &Step, _scene: &str, _timing: &SceneTiming) -> Result<()> {
        Ok(())
    }
    /// The trigger the next `inject` serves and the step's configured delay,
    /// so a backend can place the original deadline on its player's clock.
    fn trigger_observed(&mut self, _observation: &TriggerObservation, _delay_us: u64) {}
    fn inject(
        &mut self,
        step: &Step,
        scene: &str,
        timing: &SceneTiming,
        at_us: u64,
    ) -> Result<Injection>;
    /// Close the session within its bound and return the authoritative evidence.
    fn finish(&mut self, context: &AttemptContext) -> Result<Finished>;
    fn reproduce(&self, context: &AttemptContext) -> String;
}

#[derive(Clone, Debug)]
pub struct SuiteOptions {
    pub run_id: String,
    pub scenarios: Vec<String>,
    pub repetitions: u32,
    pub seed: u64,
    pub shuffle: bool,
    /// Reproduce one attempt exactly: use this attempt seed instead of deriving it.
    pub attempt_seed: Option<u64>,
    /// Evaluator self-test results recorded in the run's first ledger row.
    pub self_test: Value,
}

/// Runtime events seen live, in receipt order.
#[derive(Default)]
struct LiveView {
    events: Vec<(RuntimeEvent, u64, String)>,
    max_turn: u64,
    ended: bool,
    /// Latched on the first terminal provider failure (its turn, or 0).
    /// No authoritative re-ready cue exists yet, so it holds for the rest
    /// of the attempt: no later stimulus is dispatched.
    failed: Option<u64>,
    /// Latched when the provider link is reported terminal.
    link_terminal: bool,
    /// Turn lifecycle by receipt index, for next-turn readiness.
    admitted: BTreeMap<u64, usize>,
    finished: BTreeMap<u64, usize>,
    /// The first owner contradiction: a turn admitted or finished twice, or
    /// a finished turn that starts again. Evidence after it is not trusted.
    owner_fault: Option<String>,
    /// The first lost runner evidence (an undelivered stimulus or an ended
    /// room recording); every later stimulus is withheld.
    evidence_lost: Option<String>,
}

/// A next-turn wait's outcome once it is decided.
enum NextTurn {
    /// Ready at this observation and receipt index.
    Ready(TriggerObservation, usize),
    /// The wait was invalidated; the step is withheld with this reason.
    Lost(String),
}

impl LiveView {
    fn observe(&mut self, event: RuntimeEvent, runner_us: u64, method: String) {
        let index = self.events.len();
        if let Some(turn) = event.turn {
            self.max_turn = self.max_turn.max(turn);
        }
        if matches!(event.kind, EventKind::RunEnd { .. }) {
            self.ended = true;
        }
        if failed_terminal(&event.kind) {
            self.failed.get_or_insert(event.turn.unwrap_or(0));
        }
        if matches!(&event.kind, EventKind::ProviderLink { state } if state == "terminal") {
            self.link_terminal = true;
        }
        if let EventKind::EvidenceLost { source, reason } = &event.kind {
            self.evidence_lost
                .get_or_insert_with(|| format!("{source}: {reason}"));
        }
        if let Some(turn) = event.turn {
            let conflict = match event.kind {
                EventKind::InputAdmitted { .. } => self
                    .admitted
                    .insert(turn, index)
                    .map(|_| format!("turn {turn} admitted twice")),
                EventKind::TurnCompleted { .. } | EventKind::TurnCancelled { .. } => self
                    .finished
                    .insert(turn, index)
                    .map(|_| format!("turn {turn} finished twice")),
                EventKind::LocalEndpoint { .. } | EventKind::SpeakerFirstWrite
                    if self.finished.contains_key(&turn) =>
                {
                    Some(format!("turn {turn} resumed after it finished"))
                }
                _ => None,
            };
            if let Some(conflict) = conflict {
                self.owner_fault.get_or_insert(conflict);
            }
        }
        self.events.push((event, runner_us, method));
    }

    /// Whether the events carry the evidence a next-turn wait needs.
    fn reports_readiness(&self) -> bool {
        let seen = |want: fn(&EventKind) -> bool| self.events.iter().any(|(e, _, _)| want(&e.kind));
        seen(|kind| matches!(kind, EventKind::InputAvailability { .. }))
            && seen(|kind| matches!(kind, EventKind::ProviderLink { .. }))
    }

    /// Where a next-turn wait stands after `from_index` (the previous step's
    /// dispatch). Ready at the first event after which: every turn newer than
    /// `known_turn` admitted since then has finished (at least one was), the
    /// latest reported input availability is true and the latest provider
    /// link is ready. Each condition is the latest event-reported state at
    /// that point, never a value cached when the turn finished: V1-baseline
    /// reports availability again only after its playback tail. Any input
    /// candidate or admission after those turns finished ends the wait.
    fn next_turn(&self, known_turn: u64, from_index: usize) -> Option<NextTurn> {
        let (mut available, mut link) = (false, false);
        let mut last_finish: Option<usize> = None;
        let mut open = BTreeSet::new();
        for (index, (event, runner_us, method)) in self.events.iter().enumerate() {
            match &event.kind {
                EventKind::InputAvailability { available: now } => available = *now,
                EventKind::ProviderLink { state } => link = state == "ready",
                _ => {}
            }
            if index < from_index {
                continue;
            }
            let newer = event.turn.is_some_and(|turn| turn > known_turn);
            match event.kind {
                EventKind::InputAdmitted { .. } if newer && last_finish.is_none() => {
                    open.insert(event.turn);
                }
                EventKind::InputCandidate { .. } | EventKind::InputAdmitted { .. }
                    if last_finish.is_some() =>
                {
                    return Some(NextTurn::Lost(format!(
                        "unplanned input during the next-turn wait ({})",
                        event.name()
                    )));
                }
                EventKind::TurnCompleted { .. } | EventKind::TurnCancelled { .. }
                    if newer && open.remove(&event.turn) && open.is_empty() =>
                {
                    last_finish = Some(index);
                }
                _ => {}
            }
            if let Some(finish) = last_finish
                && self.owner_fault.is_none()
                && available
                && link
            {
                return Some(NextTurn::Ready(
                    TriggerObservation {
                        event: TriggerEvent::NextTurnReady,
                        turn: self.events[finish].0.turn,
                        event_at_us: event.at_us,
                        event_domain: event.domain,
                        runner_us: *runner_us,
                        runner_time_method: method.clone(),
                    },
                    index,
                ));
            }
        }
        None
    }

    /// Why a next-turn stimulus must not start now, rechecked at dispatch:
    /// input arrived since readiness, or availability or the link was lost.
    fn next_turn_lost(&self, ready_index: usize) -> Option<String> {
        let mut available = None;
        let mut link = None;
        for (index, (event, _, _)) in self.events.iter().enumerate() {
            match &event.kind {
                EventKind::InputAvailability { available: now } => available = Some(*now),
                EventKind::ProviderLink { state } => link = Some(state.as_str()),
                EventKind::InputCandidate { .. } | EventKind::InputAdmitted { .. }
                    if index > ready_index =>
                {
                    return Some(format!(
                        "unplanned input during the next-turn wait ({})",
                        event.name()
                    ));
                }
                _ => {}
            }
        }
        if available != Some(true) {
            return Some("input became unavailable before dispatch".into());
        }
        if link != Some("ready") {
            return Some(format!(
                "provider link {} at dispatch",
                link.unwrap_or("unreported")
            ));
        }
        None
    }

    /// A local-endpoint trigger bound to its owned turn: the one turn newer
    /// than `known_turn` admitted since the previous dispatch. A second
    /// admission or that turn finishing before its endpoint ends the wait.
    fn endpoint(&self, known_turn: u64, from_index: usize) -> Option<NextTurn> {
        let mut bound = None;
        for (index, (event, runner_us, method)) in self.events.iter().enumerate().skip(from_index) {
            let Some(turn) = event.turn.filter(|turn| *turn > known_turn) else {
                continue;
            };
            match event.kind {
                EventKind::InputAdmitted { .. } if bound.is_some() => {
                    return Some(NextTurn::Lost(format!(
                        "turn {turn} was admitted before the expected turn's endpoint"
                    )));
                }
                EventKind::InputAdmitted { .. } => bound = Some(turn),
                EventKind::LocalEndpoint { .. } if bound == Some(turn) => {
                    return Some(NextTurn::Ready(
                        TriggerObservation {
                            event: TriggerEvent::LocalEndpoint,
                            turn: Some(turn),
                            event_at_us: event.at_us,
                            event_domain: event.domain,
                            runner_us: *runner_us,
                            runner_time_method: method.clone(),
                        },
                        index,
                    ));
                }
                EventKind::SpeakerFirstWrite
                | EventKind::TurnCompleted { .. }
                | EventKind::TurnCancelled { .. }
                    if bound == Some(turn) =>
                {
                    return Some(NextTurn::Lost(format!(
                        "turn {turn} reached {} before its endpoint",
                        event.name()
                    )));
                }
                _ => {}
            }
        }
        None
    }

    /// Why a before-output correction must not start now: the bound turn
    /// has output or has finished, or other input arrived after its endpoint.
    fn before_output_lost(&self, turn: Option<u64>, endpoint_index: usize) -> Option<String> {
        let turn = turn?;
        for (index, (event, _, _)) in self.events.iter().enumerate() {
            match event.kind {
                EventKind::SpeakerFirstWrite
                | EventKind::TurnCompleted { .. }
                | EventKind::TurnCancelled { .. }
                    if event.turn == Some(turn) =>
                {
                    return Some(format!(
                        "turn {turn} reached {} before the correction could start",
                        event.name()
                    ));
                }
                EventKind::InputCandidate { .. } | EventKind::InputAdmitted { .. }
                    if index > endpoint_index =>
                {
                    return Some(format!(
                        "other input arrived after turn {turn}'s endpoint ({})",
                        event.name()
                    ));
                }
                _ => {}
            }
        }
        None
    }

    /// The first matching event received after `from_index` that belongs to a
    /// turn newer than every turn known when the previous step started.
    fn find(
        &self,
        after: TriggerEvent,
        known_turn: u64,
        from_index: usize,
    ) -> Option<TriggerObservation> {
        self.events[from_index.min(self.events.len())..]
            .iter()
            .find(|(event, _, _)| {
                let kind = matches!(
                    (&event.kind, after),
                    (EventKind::ListeningReady, TriggerEvent::ListeningReady)
                        | (
                            EventKind::SpeakerFirstWrite,
                            TriggerEvent::SpeakerFirstWrite
                        )
                        | (EventKind::SpeechRetired, TriggerEvent::SpeechRetired)
                        | (EventKind::TurnCompleted { .. }, TriggerEvent::TurnCompleted)
                        | (EventKind::TurnCancelled { .. }, TriggerEvent::TurnCancelled)
                );
                // A failed terminal is never a completion trigger.
                kind && !failed_terminal(&event.kind)
                    && (after == TriggerEvent::ListeningReady
                        || event.turn.is_none_or(|turn| turn > known_turn))
            })
            .map(|(event, runner_us, method)| TriggerObservation {
                event: after,
                turn: event.turn,
                event_at_us: event.at_us,
                event_domain: event.domain,
                runner_us: *runner_us,
                runner_time_method: method.clone(),
            })
    }

    /// The latched terminal provider failure of this attempt, if any.
    fn provider_failure(&self) -> Option<u64> {
        self.failed
    }

    /// Whether the reply has started and has no terminal event yet.
    fn speaking(&self, turn: Option<u64>) -> bool {
        let Some(turn) = turn else { return false };
        let mut history = PlaybackHistory::default();
        for (event, _, _) in &self.events {
            if event.turn != Some(turn) {
                continue;
            }
            match event.kind {
                EventKind::SpeakerFirstWrite => {
                    let _ = history.start(event.playback_sequence, event.at_us.unwrap_or(0));
                }
                EventKind::SpeechRetired => {
                    let _ = history.retire(event.playback_sequence, event.at_us.unwrap_or(0));
                }
                EventKind::TurnCancelled { .. } | EventKind::TurnCompleted { .. } => return false,
                _ => {}
            }
        }
        history.active() && history.error().is_none() && !self.ended
    }
}

/// Every condition a planned stimulus needs at the moment of dispatch. When
/// one fails, the step records why nothing was played and this returns true.
fn ineligible(
    backend: &mut dyn Backend,
    view: &LiveView,
    step: &Step,
    record: &mut StepRecord,
    target_us: u64,
    ready_index: Option<usize>,
    endpoint_index: Option<usize>,
) -> bool {
    if view.ended {
        record.status = StepStatus::SessionEnded;
        return true;
    }
    if provider_unavailable(view, record) {
        return true;
    }
    if evidence_lost(view, record) {
        return true;
    }
    if let Some(lost) =
        endpoint_index.and_then(|index| view.before_output_lost(record.bound_turn, index))
    {
        record.status = StepStatus::PreconditionLost;
        record.detail = Some(lost);
        return true;
    }
    if let Some(fault) = &view.owner_fault {
        record.status = StepStatus::SessionEnded;
        record.detail = Some(format!("runtime owner evidence inconsistent: {fault}"));
        return true;
    }
    if let Some(lost) = ready_index.and_then(|index| view.next_turn_lost(index)) {
        record.status = StepStatus::PreconditionLost;
        record.detail = Some(lost);
        return true;
    }
    let now_us = backend.now_us();
    if now_us > target_us + MAX_TRIGGER_LATENESS_US {
        record.status = StepStatus::TriggerStale;
        record.detail = Some(format!(
            "planned start passed {} ms before the trigger could act",
            (now_us - target_us) / 1000
        ));
        return true;
    }
    if step.trigger.require_lamp_speaking && !view.speaking(record.bound_turn) {
        // The overlap condition no longer holds; playing now would test
        // something else, so nothing is played and the gap is recorded.
        record.status = StepStatus::PreconditionLost;
        record.detail = Some("the triggering reply was no longer playing".into());
        return true;
    }
    false
}

fn observe_next(backend: &mut dyn Backend, view: &mut LiveView, until_us: u64) -> Result<bool> {
    match backend.next_event(until_us)? {
        Some(mut event) => {
            if event.received_us.is_none() {
                event.received_us = Some(backend.now_us());
            }
            let (runner_us, method) = backend.runner_time(&event);
            view.observe(event, runner_us, method);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Whether an event is a terminal provider failure (MAIN-009).
fn failed_terminal(kind: &EventKind) -> bool {
    matches!(kind, EventKind::TurnCompleted { outcome, .. } if outcome == crate::events::PROVIDER_FAILED)
}

/// Mark the step withheld once runner evidence was lost: a conversation
/// whose earlier stimulus never reached the room has diverged from its plan.
fn evidence_lost(view: &LiveView, record: &mut StepRecord) -> bool {
    let Some(lost) = &view.evidence_lost else {
        return false;
    };
    record.status = StepStatus::PreconditionLost;
    record.detail = Some(format!("runner evidence lost before this step: {lost}"));
    true
}

/// Mark the step withheld when a terminal provider failure has been seen.
fn provider_unavailable(view: &LiveView, record: &mut StepRecord) -> bool {
    if view.link_terminal {
        record.status = StepStatus::ProviderUnavailable;
        record.detail =
            Some("the provider link is terminal; no stimulus after the service ended".into());
        return true;
    }
    let Some(turn) = view.provider_failure() else {
        return false;
    };
    record.status = StepStatus::ProviderUnavailable;
    record.detail = Some(format!(
        "turn {turn} ended with a provider failure; no stimulus while the provider is unavailable"
    ));
    true
}

/// Execute the steps of one attempt. Returns step records, live events and an
/// abort reason when the runner itself could not continue.
fn execute(
    backend: &mut dyn Backend,
    scenario: &Scenario,
    timings: &[Option<SceneTiming>],
    steps: &mut [StepRecord],
    view: &mut LiveView,
) -> Result<()> {
    let mut anchor_us = backend.now_us();
    let mut anchor_index = 0;
    let mut known_turn = 0;
    let mut index = 0;
    while index < scenario.steps.len() {
        let step = &scenario.steps[index];
        let record = &mut steps[index];
        let deadline_us = anchor_us + u64::from(step.trigger.deadline_ms) * 1000;
        let next_turn = step.trigger.after == TriggerEvent::NextTurnReady;
        if next_turn && !view.reports_readiness() {
            // Never wait on evidence this runtime does not send.
            record.status = StepStatus::TriggerUnavailable;
            record.detail = Some(
                "the runtime's events carry no input availability or provider link; \
                 next_turn_ready cannot be observed"
                    .into(),
            );
            return Ok(());
        }
        if let (Some(scene), Some(timing)) = (&step.scene, &timings[index]) {
            backend.arm(step, scene, timing)?;
        }
        let endpoint = step.trigger.after == TriggerEvent::LocalEndpoint;
        let mut ready_index = None;
        let mut endpoint_index = None;
        let observation = loop {
            // Never schedule a stimulus on a failed terminal.
            if provider_unavailable(view, record) || evidence_lost(view, record) {
                return Ok(());
            }
            if let Some(fault) = &view.owner_fault {
                record.status = StepStatus::SessionEnded;
                record.detail = Some(format!("runtime owner evidence inconsistent: {fault}"));
                return Ok(());
            }
            if next_turn {
                match view.next_turn(known_turn, anchor_index) {
                    Some(NextTurn::Ready(found, index)) => {
                        ready_index = Some(index);
                        break Some(found);
                    }
                    Some(NextTurn::Lost(reason)) => {
                        record.status = StepStatus::PreconditionLost;
                        record.detail = Some(reason);
                        return Ok(());
                    }
                    None => {}
                }
            } else if endpoint {
                match view.endpoint(known_turn, anchor_index) {
                    Some(NextTurn::Ready(found, index)) => {
                        endpoint_index = Some(index);
                        break Some(found);
                    }
                    Some(NextTurn::Lost(reason)) => {
                        record.status = StepStatus::PreconditionLost;
                        record.detail = Some(reason);
                        return Ok(());
                    }
                    None => {}
                }
            } else if let Some(found) = view.find(step.trigger.after, known_turn, anchor_index) {
                break Some(found);
            }
            if view.ended || !observe_next(backend, view, deadline_us)? {
                break None;
            }
        };
        let Some(observation) = observation else {
            record.status = if view.ended {
                StepStatus::SessionEnded
            } else {
                StepStatus::TriggerMissed
            };
            record.detail = Some(format!(
                "no {:?} within {} ms",
                step.trigger.after, step.trigger.deadline_ms
            ));
            return Ok(());
        };
        let target_us = observation.runner_us + u64::from(step.trigger.delay_ms) * 1000;
        backend.trigger_observed(&observation, u64::from(step.trigger.delay_ms) * 1000);
        record.bound_turn = observation.turn;
        record.trigger = Some(observation);
        record.target_us = Some(target_us);
        // A failure during the configured delay cancels the stimulus too.
        while !view.ended
            && backend.now_us() < target_us
            && view.provider_failure().is_none()
            && view.evidence_lost.is_none()
            && observe_next(backend, view, target_us)?
        {}
        // Final dispatch: take everything already delivered, then recheck
        // every condition that made this stimulus valid. When the backend's
        // own final drain consumed further events, nothing started; observe
        // them and check again (MAIN-023, MAIN-025). Both loops are bounded.
        let mut dispatched = None;
        for attempt in 0..MAX_DISPATCH_ATTEMPTS {
            let now_us = backend.now_us();
            let mut drained = 0;
            while drained < MAX_FINAL_DRAIN_EVENTS && observe_next(backend, view, now_us)? {
                drained += 1;
            }
            if drained == MAX_FINAL_DRAIN_EVENTS {
                record.status = StepStatus::TriggerStale;
                record.detail = Some(format!(
                    "events kept arriving at dispatch: {MAX_FINAL_DRAIN_EVENTS} taken without settling"
                ));
                return Ok(());
            }
            if ineligible(
                backend,
                view,
                step,
                record,
                target_us,
                ready_index,
                endpoint_index,
            ) {
                return Ok(());
            }
            let (Some(scene), Some(timing)) = (&step.scene, &timings[index]) else {
                dispatched = Some(backend.now_us());
                break;
            };
            let injection = backend.inject(step, scene, timing, target_us)?;
            record.timing = Some(timing.clone());
            record.receipt = injection.receipt;
            match injection.withheld {
                None => {
                    if attempt > 0 {
                        record.receipt["revalidations"] = json!(attempt);
                    }
                    dispatched = Some(injection.started_us);
                    break;
                }
                Some(Withheld::Revalidate) => continue,
                Some(withheld) => {
                    record.status = withheld.status();
                    record.detail = Some(format!("{}: {}", withheld.cause(), withheld.reason()));
                    return Ok(());
                }
            }
        }
        let Some(started_us) = dispatched else {
            record.status = StepStatus::TriggerStale;
            record.detail = Some(format!(
                "events kept arriving during {MAX_DISPATCH_ATTEMPTS} final dispatch checks"
            ));
            return Ok(());
        };
        record.started_us = Some(started_us);
        record.status = StepStatus::Injected;
        anchor_us = started_us.max(target_us);
        anchor_index = view.events.len();
        known_turn = view.max_turn;
        index += 1;
    }
    let until = anchor_us + u64::from(scenario.observe_ms) * 1000;
    while !view.ended && observe_next(backend, view, until)? {}
    Ok(())
}

pub fn run_attempt(
    backend: &mut dyn Backend,
    catalog: &StimulusCatalog,
    context: &AttemptContext,
) -> Result<AttemptRecord> {
    let scenario = context.scenario;
    let mut steps: Vec<StepRecord> = scenario
        .steps
        .iter()
        .map(|step| StepRecord::planned(step, StepStatus::Withheld))
        .collect();
    let mut record = AttemptRecord {
        attempt_id: context.attempt_id.clone(),
        run_id: context.run_id.clone(),
        scenario: scenario.id.clone(),
        cohort: scenario.cohort,
        capability: scenario.capability,
        provider: scenario.provider,
        stratum: backend.stratum(scenario),
        source: backend.source(),
        repetition: context.repetition,
        order: context.order,
        profile: backend.profile(),
        seed: Some(context.seed),
        step_domain: backend.domain(),
        steps: Vec::new(),
        events: Vec::new(),
        live_events: Vec::new(),
        clock: None,
        attribution: Attribution::Timed,
        status: AttemptStatus::Completed,
        spoken_failure_notice: None,
        evidence: Value::Null,
        unmapped_trace_records: 0,
        reproduce: backend.reproduce(context),
    };
    if let Some(reason) = backend.unsupported(scenario) {
        for step in &mut steps {
            step.status = StepStatus::TriggerUnavailable;
        }
        record.steps = steps;
        record.status = AttemptStatus::Withheld { reason };
        return Ok(record);
    }
    // Stimulus timing reads and verifies cached audio; do it before the session
    // so that work never sits between a trigger and its stimulus.
    let mut timings = Vec::with_capacity(scenario.steps.len());
    for step in &scenario.steps {
        match step
            .scene
            .as_deref()
            .map(|scene| backend.timing(catalog, scene))
            .transpose()
        {
            Ok(timing) => timings.push(timing),
            Err(error) => {
                record.steps = steps;
                record.status = AttemptStatus::Withheld {
                    reason: format!("stimulus timing unavailable: {error}"),
                };
                return Ok(record);
            }
        }
    }
    match backend.begin(context) {
        Ok(Begin::Ready) => {}
        Ok(Begin::Withheld(reason)) => {
            record.steps = steps;
            record.status = AttemptStatus::Withheld { reason };
            return Ok(record);
        }
        Err(error) => {
            // Release whatever the backend started; keep the attempt in the ledger.
            let _ = backend.finish(context);
            record.steps = steps;
            record.status = AttemptStatus::Aborted {
                reason: format!("session start failed: {error}"),
            };
            return Ok(record);
        }
    }
    let mut view = LiveView::default();
    let executed = execute(backend, scenario, &timings, &mut steps, &mut view);
    // Always close the session, even after a runner error, so it stays bounded.
    let finished = backend.finish(context);
    record.steps = steps;
    record.live_events = view.events.into_iter().map(|(event, _, _)| event).collect();
    match finished {
        Ok(finished) => {
            record.events = finished.events;
            record.clock = finished.clock;
            record.evidence = finished.evidence;
            record.unmapped_trace_records = finished.unmapped;
            record.spoken_failure_notice = finished.spoken_failure_notice;
            for (step_id, reason) in finished.failed_deliveries {
                if let Some(step) = record.steps.iter_mut().find(|s| s.step == step_id) {
                    step.status = StepStatus::DeliveryFailed;
                    step.detail = Some(reason);
                }
            }
            for (step_id, started) in finished.corrected_starts {
                if let Some(step) = record.steps.iter_mut().find(|s| s.step == step_id) {
                    step.receipt["runner_start_us"] = json!(step.started_us);
                    step.started_us = Some(started);
                }
            }
            if let Some(reason) = finished.aborted {
                record.status = AttemptStatus::Aborted { reason };
            }
        }
        Err(error) => {
            record.status = AttemptStatus::Aborted {
                reason: format!("session close failed: {error}"),
            };
        }
    }
    if let Err(error) = executed {
        record.status = AttemptStatus::Aborted {
            reason: format!("runner failed during steps: {error}"),
        };
    }
    Ok(record)
}

/// The deterministic attempt order for a suite: each repetition is one block,
/// optionally shuffled with a seed derived from the run seed.
pub fn attempt_order(plan: &LoadedPlan, options: &SuiteOptions) -> Result<Vec<(u32, String)>> {
    let selected: Vec<String> = if options.scenarios.is_empty() {
        plan.plan.scenarios.iter().map(|s| s.id.clone()).collect()
    } else {
        let mut seen = BTreeSet::new();
        for id in &options.scenarios {
            plan.scenario(id)?;
            if !seen.insert(id) {
                return Err(crate::invalid(&format!("scenario {id} selected twice")));
            }
        }
        options.scenarios.clone()
    };
    if !(1..=1000).contains(&options.repetitions) {
        return Err(crate::invalid("repetitions must be 1..1000"));
    }
    let mut order = Vec::new();
    for repetition in 0..options.repetitions {
        let mut block = selected.clone();
        if options.shuffle {
            Rng::new(derive_seed(options.seed, &format!("order-{repetition}"))).shuffle(&mut block);
        }
        order.extend(block.into_iter().map(|id| (repetition, id)));
    }
    Ok(order)
}

/// Run every selected attempt and retain all of them in the ledger.
pub fn run_suite(
    plan: &LoadedPlan,
    catalog: &StimulusCatalog,
    backend: &mut dyn Backend,
    options: &SuiteOptions,
    run_directory: &std::path::Path,
) -> Result<Vec<AttemptRecord>> {
    let order = attempt_order(plan, options)?;
    let mut ledger = Ledger::create(run_directory)?;
    ledger.append(
        "run_start",
        json!({
            "run_id": options.run_id,
            "plan_id": plan.plan.id,
            "plan_sha256": plan.sha256,
            "desk_catalog_file_sha256": catalog.desk_file_sha256,
            "stimulus_extension_file_sha256": catalog.extension_file_sha256,
            "merged_catalog_sha256": catalog.merged_sha256,
            "backend": backend.describe(),
            "seed": options.seed,
            "repetitions": options.repetitions,
            "shuffle": options.shuffle,
            "attempt_order": order,
            "evaluator_self_test": options.self_test,
            "tool": {"package": env!("CARGO_PKG_NAME"), "version": env!("CARGO_PKG_VERSION"),
                     "os": std::env::consts::OS, "arch": std::env::consts::ARCH},
            "started_unix_ms": unix_ms(),
        }),
        true,
    )?;
    let mut records = Vec::with_capacity(order.len());
    for (order_index, (repetition, scenario_id)) in order.iter().enumerate() {
        let scenario = plan.scenario(scenario_id)?;
        let attempt_id = format!(
            "{}-{:04}-{}-r{}",
            options.run_id, order_index, scenario.id, repetition
        );
        let context = AttemptContext {
            seed: options
                .attempt_seed
                .unwrap_or_else(|| derive_seed(options.seed, &attempt_id)),
            attempt_id,
            run_id: options.run_id.clone(),
            scenario,
            repetition: *repetition,
            order: order_index as u32,
            run_directory: run_directory.to_path_buf(),
        };
        // Written and synced before any session starts or stimulus plays.
        ledger.append(
            "attempt_planned",
            json!({
                "attempt_id": context.attempt_id,
                "scenario": scenario.id,
                "cohort": scenario.cohort,
                "capability": scenario.capability,
                "repetition": repetition,
                "order": order_index,
                "stratum": backend.stratum(scenario),
                "stimulus_source": backend.source(),
                "expected": scenario.steps.iter().map(|s| json!({
                    "step": s.id, "scene": s.scene, "expect": s.expect,
                    "trigger": s.trigger, "reference": s.reference})).collect::<Vec<_>>(),
            }),
            true,
        )?;
        let record = run_attempt(backend, catalog, &context)?;
        ledger.append(
            "attempt_finished",
            json!({"attempt_id": record.attempt_id, "record": record}),
            true,
        )?;
        records.push(record);
    }
    ledger.append(
        "run_end",
        json!({"run_id": options.run_id, "attempts": records.len(), "finished_unix_ms": unix_ms()}),
        true,
    )?;
    Ok(records)
}

#[cfg(test)]
mod occurrence_tests {
    use super::*;
    use crate::events::EventSource;
    fn observe(view: &mut LiveView, kind: EventKind, sequence: Option<u64>, at: u64) {
        let mut event =
            RuntimeEvent::new(kind, Some(1), at, ClockDomain::Virtual, EventSource::Fake);
        event.playback_sequence = sequence;
        view.observe(event, at, "identity".into());
    }
    #[test]
    fn second_occurrence_is_speaking_but_first_retirement_is_not_completion() {
        let mut view = LiveView::default();
        observe(&mut view, EventKind::SpeakerFirstWrite, Some(1), 10);
        observe(&mut view, EventKind::SpeechRetired, Some(1), 20);
        assert!(!view.speaking(Some(1)));
        assert!(view.find(TriggerEvent::TurnCompleted, 0, 0).is_none());
        observe(&mut view, EventKind::SpeakerFirstWrite, Some(2), 30);
        assert!(view.speaking(Some(1)));
        observe(&mut view, EventKind::SpeechRetired, Some(2), 40);
        assert!(!view.speaking(Some(1)));
        assert!(view.find(TriggerEvent::TurnCompleted, 0, 0).is_none());
        observe(
            &mut view,
            EventKind::TurnCompleted {
                outcome: "audio_written_unscored".into(),
                playback_gaps: 0,
            },
            None,
            50,
        );
        assert_eq!(
            view.find(TriggerEvent::TurnCompleted, 0, 0)
                .unwrap()
                .event_at_us,
            Some(50)
        );
        assert_eq!(
            view.find(TriggerEvent::SpeechRetired, 0, 0)
                .unwrap()
                .event_at_us,
            Some(20),
            "legacy occurrence trigger keeps its meaning"
        );
    }

    /// One scripted live event: time, kind and owning turn.
    type Scripted = (u64, EventKind, Option<u64>);

    /// A virtual-clock backend that replays scripted live events.
    struct Script {
        now: u64,
        events: std::collections::VecDeque<(u64, EventKind, Option<u64>)>,
        injected: Vec<(String, u64)>,
        /// Events the backend's own final drain takes when a step dispatches.
        held: Vec<(String, Scripted)>,
        /// Drained events not yet returned to the runner.
        pending: std::collections::VecDeque<Scripted>,
        /// Every dispatch drains one more harmless event.
        endless: bool,
    }
    impl Backend for Script {
        fn describe(&self) -> Value {
            json!({})
        }
        fn stratum(&self, _: &Scenario) -> Stratum {
            Stratum::FakeTiming
        }
        fn source(&self) -> StimulusSource {
            StimulusSource::DeclaredTiming
        }
        fn domain(&self) -> ClockDomain {
            ClockDomain::Virtual
        }
        fn unsupported(&self, _: &Scenario) -> Option<String> {
            None
        }
        fn begin(&mut self, _: &AttemptContext) -> Result<Begin> {
            Ok(Begin::Ready)
        }
        fn now_us(&mut self) -> u64 {
            self.now
        }
        fn next_event(&mut self, until_us: u64) -> Result<Option<RuntimeEvent>> {
            if let Some((at, kind, turn)) = self.pending.pop_front() {
                let mut event =
                    RuntimeEvent::new(kind, turn, at, ClockDomain::Virtual, EventSource::Fake);
                event.playback_sequence = turn.map(|_| 1);
                return Ok(Some(event));
            }
            match self.events.front() {
                Some((at, _, _)) if *at <= until_us => {
                    let (at, kind, turn) = self.events.pop_front().expect("checked");
                    self.now = self.now.max(at);
                    let mut event =
                        RuntimeEvent::new(kind, turn, at, ClockDomain::Virtual, EventSource::Fake);
                    event.playback_sequence = turn.map(|_| 1);
                    Ok(Some(event))
                }
                _ => {
                    self.now = self.now.max(until_us);
                    Ok(None)
                }
            }
        }
        fn runner_time(&self, event: &RuntimeEvent) -> (u64, String) {
            (event.at_us.unwrap_or(0), "virtual".into())
        }
        fn inject(
            &mut self,
            step: &Step,
            _: &str,
            _: &SceneTiming,
            at_us: u64,
        ) -> Result<Injection> {
            self.now = self.now.max(at_us);
            if self.endless {
                let text = "queued work".to_owned();
                let kind = EventKind::InputTranscript {
                    text,
                    finished: false,
                };
                self.pending.push_back((self.now, kind, None));
            }
            let (drained, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
                .into_iter()
                .partition(|(id, _)| *id == step.id);
            self.held = kept;
            self.pending
                .extend(drained.into_iter().map(|(_, event)| event));
            if !self.pending.is_empty() {
                return Ok(Injection {
                    withheld: Some(Withheld::Revalidate),
                    started_us: self.now,
                    receipt: json!({"dispatched": false}),
                });
            }
            self.injected.push((step.id.clone(), self.now));
            Ok(Injection {
                withheld: None,
                started_us: self.now,
                receipt: json!({}),
            })
        }
        fn finish(&mut self, _: &AttemptContext) -> Result<Finished> {
            Err(crate::invalid("not used"))
        }
        fn reproduce(&self, _: &AttemptContext) -> String {
            String::new()
        }
    }

    fn run_script(
        scenario_id: &str,
        events: Vec<(u64, EventKind, Option<u64>)>,
    ) -> (Vec<StepRecord>, Vec<(String, u64)>) {
        run_script_with(scenario_id, |_| {}, events)
    }

    fn run_script_with(
        scenario_id: &str,
        adjust: impl FnOnce(&mut Scenario),
        events: Vec<(u64, EventKind, Option<u64>)>,
    ) -> (Vec<StepRecord>, Vec<(String, u64)>) {
        run_script_held(scenario_id, adjust, events, Vec::new(), false)
    }

    fn run_script_held(
        scenario_id: &str,
        adjust: impl FnOnce(&mut Scenario),
        events: Vec<(u64, EventKind, Option<u64>)>,
        held: Vec<(String, Scripted)>,
        endless: bool,
    ) -> (Vec<StepRecord>, Vec<(String, u64)>) {
        let catalog = StimulusCatalog::load().unwrap();
        let plan = crate::plan::LoadedPlan::default_plan(&catalog).unwrap();
        let mut scenario = plan
            .plan
            .scenarios
            .iter()
            .find(|s| s.id == scenario_id)
            .unwrap()
            .clone();
        adjust(&mut scenario);
        let scenario = &scenario;
        let timings: Vec<Option<SceneTiming>> = scenario
            .steps
            .iter()
            .map(|s| {
                s.scene
                    .as_ref()
                    .map(|scene| catalog.estimate(scene).unwrap())
            })
            .collect();
        let mut steps: Vec<StepRecord> = scenario
            .steps
            .iter()
            .map(|s| StepRecord::planned(s, StepStatus::Withheld))
            .collect();
        let mut backend = Script {
            now: 0,
            events: events.into(),
            injected: Vec::new(),
            held,
            pending: Default::default(),
            endless,
        };
        let mut view = LiveView::default();
        execute(&mut backend, scenario, &timings, &mut steps, &mut view).unwrap();
        (steps, backend.injected)
    }

    fn completed(outcome: &str) -> EventKind {
        EventKind::TurnCompleted {
            outcome: outcome.into(),
            playback_gaps: 0,
        }
    }

    /// MAIN-014 review repro: a valid trigger, then `provider_failed` 200 ms
    /// into the configured delay. The stimulus must not be injected.
    #[test]
    fn a_failure_during_the_trigger_delay_withholds_the_stimulus() {
        let first = 1_000_000;
        let (steps, injected) = run_script(
            "follow-up-chain",
            vec![
                (0, EventKind::ListeningReady, None),
                (
                    first,
                    EventKind::InputAdmitted {
                        prefix_first_read_us: None,
                        candidate: None,
                        basis: None,
                    },
                    Some(1),
                ),
                (first + 1_000_000, EventKind::SpeakerFirstWrite, Some(1)),
                (first + 2_000_000, EventKind::SpeechRetired, Some(1)),
                // The follow-up trigger (completion), with a 500 ms delay ...
                (
                    first + 2_100_000,
                    completed("audio_written_unscored"),
                    Some(1),
                ),
                // ... and a terminal provider failure 200 ms into it.
                (
                    first + 2_300_000,
                    completed(crate::events::PROVIDER_FAILED),
                    None,
                ),
                (
                    first + 30_000_000,
                    EventKind::RunEnd {
                        status: None,
                        error: None,
                    },
                    None,
                ),
            ],
        );
        assert_eq!(
            injected.len(),
            1,
            "only the first step may play: {injected:?}"
        );
        assert_eq!(
            steps[1].status,
            StepStatus::ProviderUnavailable,
            "{:?}",
            steps[1]
        );
        assert!(
            steps[1].trigger.is_some(),
            "the valid trigger is still recorded"
        );
        assert_eq!(steps[1].started_us, None);
    }

    /// MAIN-015 exact repro: listening_ready at 1 ms, admission at 100 ms,
    /// provider_failed at 200 ms; the first stimulus was planned for 501 ms.
    #[test]
    fn main_015_failure_before_the_first_planned_start_injects_nothing() {
        let (steps, injected) = run_script_with(
            "quick-chat",
            |scenario| scenario.steps[0].trigger.delay_ms = 500,
            vec![
                (1_000, EventKind::ListeningReady, None),
                (
                    100_000,
                    EventKind::InputAdmitted {
                        prefix_first_read_us: None,
                        candidate: None,
                        basis: None,
                    },
                    Some(1),
                ),
                (200_000, completed(crate::events::PROVIDER_FAILED), Some(1)),
                (
                    30_000_000,
                    EventKind::RunEnd {
                        status: None,
                        error: None,
                    },
                    None,
                ),
            ],
        );
        assert!(injected.is_empty(), "{injected:?}");
        assert_eq!(
            steps[0].status,
            StepStatus::ProviderUnavailable,
            "{:?}",
            steps[0]
        );
        assert!(steps[0].trigger.is_some());
        assert!(
            steps
                .iter()
                .skip(1)
                .all(|s| s.status != StepStatus::Injected)
        );
    }

    /// The latch holds for the attempt: a later trigger after a failure of
    /// an earlier turn still dispatches nothing.
    #[test]
    fn a_failure_latches_for_the_rest_of_the_attempt() {
        let first = 1_000_000;
        let (steps, injected) = run_script(
            "follow-up-chain",
            vec![
                (0, EventKind::ListeningReady, None),
                (
                    first,
                    EventKind::InputAdmitted {
                        prefix_first_read_us: None,
                        candidate: None,
                        basis: None,
                    },
                    Some(1),
                ),
                (
                    first + 500_000,
                    completed(crate::events::PROVIDER_FAILED),
                    Some(1),
                ),
                // An unexpected later completion must not revive dispatch.
                (
                    first + 2_000_000,
                    EventKind::InputAdmitted {
                        prefix_first_read_us: None,
                        candidate: None,
                        basis: None,
                    },
                    Some(2),
                ),
                (
                    first + 3_000_000,
                    completed("audio_written_unscored"),
                    Some(2),
                ),
                (
                    first + 30_000_000,
                    EventKind::RunEnd {
                        status: None,
                        error: None,
                    },
                    None,
                ),
            ],
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(
            steps[1].status,
            StepStatus::ProviderUnavailable,
            "{:?}",
            steps[1]
        );
    }

    #[test]
    fn a_failure_already_delivered_at_playback_time_withholds_the_stimulus() {
        let first = 1_000_000;
        // The failure carries the same time as the planned start.
        let (steps, injected) = run_script(
            "follow-up-chain",
            vec![
                (0, EventKind::ListeningReady, None),
                (
                    first,
                    EventKind::InputAdmitted {
                        prefix_first_read_us: None,
                        candidate: None,
                        basis: None,
                    },
                    Some(1),
                ),
                (first + 1_000_000, EventKind::SpeakerFirstWrite, Some(1)),
                (first + 2_000_000, EventKind::SpeechRetired, Some(1)),
                (
                    first + 2_100_000,
                    completed("audio_written_unscored"),
                    Some(1),
                ),
                (
                    first + 2_600_000,
                    completed(crate::events::PROVIDER_FAILED),
                    None,
                ),
                (
                    first + 30_000_000,
                    EventKind::RunEnd {
                        status: None,
                        error: None,
                    },
                    None,
                ),
            ],
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::ProviderUnavailable);
    }

    #[test]
    fn a_provider_failure_is_never_a_completion_trigger_and_is_reported() {
        let mut view = LiveView::default();
        observe(&mut view, EventKind::SpeakerFirstWrite, Some(1), 10);
        observe(&mut view, EventKind::SpeechRetired, Some(1), 20);
        assert!(view.provider_failure().is_none());
        observe(
            &mut view,
            EventKind::TurnCompleted {
                outcome: crate::events::PROVIDER_FAILED.into(),
                playback_gaps: 0,
            },
            None,
            30,
        );
        assert!(view.find(TriggerEvent::TurnCompleted, 0, 0).is_none());
        assert_eq!(view.provider_failure(), Some(1));
        // Latched for the attempt: no re-ready cue exists yet.
        observe(&mut view, EventKind::SpeakerFirstWrite, Some(2), 40);
        assert_eq!(view.provider_failure(), Some(1));
    }

    fn speaking_reply() -> Vec<(u64, EventKind, Option<u64>)> {
        vec![
            (1_000, EventKind::ListeningReady, None),
            (
                1_000_000,
                EventKind::InputAdmitted {
                    prefix_first_read_us: None,
                    candidate: None,
                    basis: None,
                },
                Some(1),
            ),
            (
                1_500_000,
                EventKind::LocalEndpoint {
                    last_block_read_us: Some(1_499_000),
                },
                Some(1),
            ),
            (2_000_000, EventKind::SpeakerFirstWrite, Some(1)),
        ]
    }

    /// MAIN-025: harmless events taken by the backend's final drain are
    /// observed, eligibility is rechecked, and the stimulus starts once.
    #[test]
    fn a_backend_revalidation_with_harmless_events_dispatches_once() {
        let text = "partial".to_owned();
        let held = vec![(
            "story".to_owned(),
            (
                500_000,
                EventKind::InputTranscript {
                    text,
                    finished: false,
                },
                None,
            ),
        )];
        let (steps, injected) =
            run_script_held("topic-change", |_| {}, speaking_reply(), held, false);
        assert_eq!(injected.len(), 2, "{injected:?}");
        assert_eq!(steps[0].status, StepStatus::Injected);
        assert_eq!(steps[0].receipt["revalidations"], 1);
    }

    /// MAIN-025: a retirement first taken by the backend's own final drain
    /// ends the overlap condition; the interruption is not played.
    #[test]
    fn a_retirement_taken_by_the_backend_drain_is_precondition_lost() {
        let held = vec![(
            "change".to_owned(),
            (3_000_000, EventKind::SpeechRetired, Some(1)),
        )];
        let (steps, injected) =
            run_script_held("topic-change", |_| {}, speaking_reply(), held, false);
        assert_eq!(
            injected
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            vec!["story"]
        );
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
    }

    /// MAIN-025: dispatch that never settles is withheld after its bound,
    /// with the reason recorded.
    #[test]
    fn a_dispatch_that_never_settles_is_withheld_after_its_bound() {
        let (steps, injected) =
            run_script_held("quick-chat", |_| {}, speaking_reply(), Vec::new(), true);
        assert!(injected.is_empty(), "{injected:?}");
        assert_eq!(steps[0].status, StepStatus::TriggerStale);
        assert!(
            steps[0]
                .detail
                .as_deref()
                .is_some_and(|d| d.contains("final dispatch checks")),
            "{:?}",
            steps[0].detail
        );
    }

    // Next-turn readiness (MAIN, after native run e9/b1): completion is not
    // listening readiness, availability arrives late, extra input during the
    // wait, terminal link, and contradictory owners.

    const COMPLETED_AT: u64 = 4_000_000;
    /// The native e9/b1 reopen: availability 100.865 ms after final retirement.
    const REOPEN_AT: u64 = COMPLETED_AT + 100_865;

    fn admitted() -> EventKind {
        EventKind::InputAdmitted {
            prefix_first_read_us: None,
            candidate: None,
            basis: None,
        }
    }

    fn availability(available: bool) -> EventKind {
        EventKind::InputAvailability { available }
    }

    fn link(state: &str) -> EventKind {
        EventKind::ProviderLink {
            state: state.into(),
        }
    }

    fn candidate() -> EventKind {
        EventKind::InputCandidate {
            candidate: Value::Null,
        }
    }

    /// Startup evidence, the first question answered as turn 1 under
    /// V1-baseline (availability withdrawn while Lamp speaks), then `tail`.
    fn answered_then(tail: Vec<Scripted>) -> Vec<Scripted> {
        let mut events = vec![
            (0, link("ready"), None),
            (0, availability(true), None),
            (1_000, EventKind::ListeningReady, None),
            (1_000_000, candidate(), None),
            (1_100_000, admitted(), Some(1)),
            (
                2_000_000,
                EventKind::LocalEndpoint {
                    last_block_read_us: None,
                },
                Some(1),
            ),
            (3_000_000, availability(false), None),
            (3_000_000, EventKind::SpeakerFirstWrite, Some(1)),
            (COMPLETED_AT, EventKind::SpeechRetired, Some(1)),
            (COMPLETED_AT, completed("audio_written_unscored"), Some(1)),
        ];
        events.extend(tail);
        events.push((
            30_000_000,
            EventKind::RunEnd {
                status: None,
                error: None,
            },
            None,
        ));
        events
    }

    /// `follow-up-chain` with its second step waiting for the next turn.
    fn next_turn_run(
        delay_ms: u32,
        events: Vec<Scripted>,
    ) -> (Vec<StepRecord>, Vec<(String, u64)>) {
        run_script_with(
            "follow-up-chain",
            |scenario| {
                scenario.steps[1].trigger.after = TriggerEvent::NextTurnReady;
                scenario.steps[1].trigger.delay_ms = delay_ms;
            },
            events,
        )
    }

    #[test]
    fn completion_is_not_readiness_the_late_reopen_is_awaited_by_event() {
        let (steps, injected) = next_turn_run(
            0,
            answered_then(vec![(REOPEN_AT, availability(true), None)]),
        );
        // The cached state at completion said unavailable; the step neither
        // starts then nor gives up, it starts on the actual reopen.
        assert_eq!(injected[1], ("why".into(), REOPEN_AT), "{injected:?}");
        let trigger = steps[1].trigger.as_ref().unwrap();
        assert_eq!(trigger.event, TriggerEvent::NextTurnReady);
        assert_eq!(trigger.turn, Some(1));
        assert_eq!(trigger.event_at_us, Some(REOPEN_AT));
        assert_eq!(steps[1].status, StepStatus::Injected);
    }

    #[test]
    fn availability_never_withdrawn_is_ready_at_the_finish_not_before_admission() {
        // `vad-only` reports availability once. Ready means the previous
        // step's turn was admitted and finished, never just "available".
        let events: Vec<Scripted> = answered_then(Vec::new())
            .into_iter()
            .filter(|(_, kind, _)| *kind != availability(false))
            .collect();
        let (_, injected) = next_turn_run(0, events);
        assert_eq!(injected[1], ("why".into(), COMPLETED_AT), "{injected:?}");
    }

    #[test]
    fn a_cancelled_turn_also_finishes_the_wait() {
        let mut events = answered_then(vec![(REOPEN_AT, availability(true), None)]);
        events.retain(|(_, kind, _)| !matches!(kind, EventKind::TurnCompleted { .. }));
        events.push((
            COMPLETED_AT,
            EventKind::TurnCancelled {
                reason: Some("deadline_cancelled".into()),
                provider_audio_seen: Some(true),
            },
            Some(1),
        ));
        events.sort_by_key(|(at, _, _)| *at);
        let (_, injected) = next_turn_run(0, events);
        assert_eq!(injected[1], ("why".into(), REOPEN_AT), "{injected:?}");
    }

    #[test]
    fn an_extra_candidate_during_the_wait_withholds_the_next_turn() {
        let (steps, injected) = next_turn_run(
            0,
            answered_then(vec![
                (COMPLETED_AT + 50_000, candidate(), None),
                (REOPEN_AT, availability(true), None),
            ]),
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(
            steps[1]
                .detail
                .as_ref()
                .unwrap()
                .contains("unplanned input")
        );
        assert_eq!(steps[1].started_us, None);
    }

    #[test]
    fn an_extra_admitted_turn_during_the_wait_is_not_waited_out() {
        // A whole unplanned turn would otherwise finish and look ready again.
        let (steps, injected) = next_turn_run(
            0,
            answered_then(vec![
                (COMPLETED_AT + 50_000, admitted(), Some(2)),
                (
                    COMPLETED_AT + 900_000,
                    completed("audio_written_unscored"),
                    Some(2),
                ),
                (COMPLETED_AT + 1_000_000, availability(true), None),
            ]),
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
    }

    #[test]
    fn input_between_readiness_and_dispatch_is_rechecked_at_dispatch() {
        let (steps, injected) = next_turn_run(
            300,
            answered_then(vec![
                (REOPEN_AT, availability(true), None),
                (REOPEN_AT + 100_000, candidate(), None),
            ]),
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(steps[1].trigger.is_some(), "readiness itself was observed");
        assert!(
            steps[1]
                .detail
                .as_ref()
                .unwrap()
                .contains("unplanned input")
        );
    }

    #[test]
    fn availability_withdrawn_before_dispatch_withholds() {
        let (steps, injected) = next_turn_run(
            300,
            answered_then(vec![
                (REOPEN_AT, availability(true), None),
                (REOPEN_AT + 100_000, availability(false), None),
            ]),
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(steps[1].detail.as_ref().unwrap().contains("unavailable"));
    }

    #[test]
    fn a_terminal_link_during_the_wait_is_provider_unavailable() {
        let (steps, injected) = next_turn_run(
            0,
            answered_then(vec![
                (COMPLETED_AT + 50_000, link("terminal"), None),
                (REOPEN_AT, availability(true), None),
            ]),
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::ProviderUnavailable);
        assert!(steps[1].detail.as_ref().unwrap().contains("terminal"));
    }

    #[test]
    fn a_link_outage_is_waited_out_by_event() {
        let recovered = COMPLETED_AT + 400_000;
        let (_, injected) = next_turn_run(
            0,
            answered_then(vec![
                (COMPLETED_AT + 10_000, link("unavailable"), None),
                (REOPEN_AT, availability(true), None),
                (recovered, link("ready"), None),
            ]),
        );
        assert_eq!(injected[1], ("why".into(), recovered), "{injected:?}");
    }

    #[test]
    fn a_turn_finished_twice_is_an_owner_fault() {
        let (steps, injected) = next_turn_run(
            0,
            answered_then(vec![
                (
                    COMPLETED_AT + 1_000,
                    completed("audio_written_unscored"),
                    Some(1),
                ),
                (REOPEN_AT, availability(true), None),
            ]),
        );
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::SessionEnded);
        assert!(steps[1].detail.as_ref().unwrap().contains("finished twice"));
    }

    #[test]
    fn a_stream_without_readiness_evidence_never_waits_on_it() {
        let events: Vec<Scripted> = answered_then(vec![(REOPEN_AT, availability(true), None)])
            .into_iter()
            .filter(|(_, kind, _)| {
                !matches!(
                    kind,
                    EventKind::InputAvailability { .. } | EventKind::ProviderLink { .. }
                )
            })
            .collect();
        let (steps, injected) = next_turn_run(0, events);
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::TriggerUnavailable);
    }

    #[test]
    fn audit_a_completion_trigger_starts_inside_the_reopen_window() {
        // The class this replaces: `turn_completed` cannot see availability,
        // so with no delay the next question starts while V1-baseline still
        // withholds input. Only a planned delay hides it, and only by luck.
        let (_, injected) = run_script_with(
            "follow-up-chain",
            |scenario| scenario.steps[1].trigger.delay_ms = 0,
            answered_then(vec![(REOPEN_AT, availability(true), None)]),
        );
        assert_eq!(injected[1], ("why".into(), COMPLETED_AT), "{injected:?}");
        const { assert!(COMPLETED_AT < REOPEN_AT) };
    }

    /// `follow-up-chain` with its second step a correction on the first
    /// turn's local endpoint.
    fn endpoint_run(delay_ms: u32, events: Vec<Scripted>) -> (Vec<StepRecord>, Vec<(String, u64)>) {
        run_script_with(
            "follow-up-chain",
            |scenario| {
                scenario.steps[1].trigger.after = TriggerEvent::LocalEndpoint;
                scenario.steps[1].trigger.delay_ms = delay_ms;
            },
            events,
        )
    }

    fn endpoint() -> EventKind {
        EventKind::LocalEndpoint {
            last_block_read_us: None,
        }
    }

    #[test]
    fn a_correction_binds_to_the_turn_admitted_since_the_previous_step() {
        // Output starts at 3.0 s; the correction is due at 2.3 s.
        let (steps, injected) = endpoint_run(300, answered_then(Vec::new()));
        assert_eq!(
            steps[1].status,
            StepStatus::Injected,
            "{:?}",
            steps[1].detail
        );
        assert_eq!(injected[1], ("why".into(), 2_300_000));
        let trigger = steps[1].trigger.as_ref().unwrap();
        assert_eq!(
            (trigger.event, trigger.turn),
            (TriggerEvent::LocalEndpoint, Some(1))
        );
    }

    #[test]
    fn a_correction_due_after_output_started_is_withheld() {
        let (steps, injected) = endpoint_run(1_500, answered_then(Vec::new()));
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(
            steps[1]
                .detail
                .as_deref()
                .is_some_and(|d| d.contains("speaker_first_write")),
            "{:?}",
            steps[1].detail
        );
    }

    #[test]
    fn a_correction_after_new_input_or_a_second_owner_is_withheld() {
        let mut after_endpoint = answered_then(Vec::new());
        after_endpoint.push((2_100_000, candidate(), None));
        after_endpoint.sort_by_key(|(at, _, _)| *at);
        let (steps, injected) = endpoint_run(300, after_endpoint);
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(
            steps[1]
                .detail
                .as_deref()
                .unwrap()
                .contains("after turn 1's endpoint")
        );

        // A second admission before any endpoint: the expected owner is ambiguous.
        let second = vec![
            (0, link("ready"), None),
            (0, availability(true), None),
            (1_000, EventKind::ListeningReady, None),
            (1_100_000, admitted(), Some(1)),
            (1_500_000, admitted(), Some(2)),
            (2_000_000, endpoint(), Some(2)),
            (
                30_000_000,
                EventKind::RunEnd {
                    status: None,
                    error: None,
                },
                None,
            ),
        ];
        let (steps, injected) = endpoint_run(300, second);
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(
            steps[1]
                .detail
                .as_deref()
                .unwrap()
                .contains("turn 2 was admitted")
        );

        // The expected turn finishing before its endpoint is no correction point.
        let finished = vec![
            (0, link("ready"), None),
            (1_000, EventKind::ListeningReady, None),
            (1_100_000, admitted(), Some(1)),
            (
                1_800_000,
                EventKind::TurnCancelled {
                    reason: Some("runtime_failed".into()),
                    provider_audio_seen: Some(false),
                },
                Some(1),
            ),
            (2_000_000, endpoint(), Some(1)),
            (
                30_000_000,
                EventKind::RunEnd {
                    status: None,
                    error: None,
                },
                None,
            ),
        ];
        let (steps, _) = endpoint_run(300, finished);
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(
            steps[1]
                .detail
                .as_deref()
                .unwrap()
                .contains("before its endpoint")
        );
    }

    #[test]
    fn lost_runner_evidence_withholds_the_next_step_at_once() {
        let mut events = answered_then(vec![(REOPEN_AT, availability(true), None)]);
        events.push((
            3_500_000,
            EventKind::EvidenceLost {
                source: "player:ask".into(),
                reason: "stimulus not delivered: start_expired".into(),
            },
            None,
        ));
        events.sort_by_key(|(at, _, _)| *at);
        let (steps, injected) = next_turn_run(0, events);
        assert_eq!(injected.len(), 1, "{injected:?}");
        assert_eq!(steps[1].status, StepStatus::PreconditionLost);
        assert!(steps[1].detail.as_deref().unwrap().contains("player:ask"));
    }
}
