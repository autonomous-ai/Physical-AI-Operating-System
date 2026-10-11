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
use std::{collections::BTreeSet, path::PathBuf};

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
    /// The backend's final dispatch check refused to start the stimulus
    /// (for example a provider failure that arrived at dispatch time).
    pub withheld: Option<String>,
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
}

impl LiveView {
    fn observe(&mut self, event: RuntimeEvent, runner_us: u64, method: String) {
        if let Some(turn) = event.turn {
            self.max_turn = self.max_turn.max(turn);
        }
        if matches!(event.kind, EventKind::RunEnd { .. }) {
            self.ended = true;
        }
        if failed_terminal(&event.kind) {
            self.failed.get_or_insert(event.turn.unwrap_or(0));
        }
        self.events.push((event, runner_us, method));
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

/// Mark the step withheld when a terminal provider failure has been seen.
fn provider_unavailable(view: &LiveView, record: &mut StepRecord) -> bool {
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
        let observation = loop {
            // Never schedule a stimulus on a failed terminal.
            if provider_unavailable(view, record) {
                return Ok(());
            }
            if let Some(found) = view.find(step.trigger.after, known_turn, anchor_index) {
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
        record.bound_turn = observation.turn;
        record.trigger = Some(observation);
        record.target_us = Some(target_us);
        // A failure during the configured delay cancels the stimulus too.
        while !view.ended
            && backend.now_us() < target_us
            && view.provider_failure().is_none()
            && observe_next(backend, view, target_us)?
        {}
        if view.ended {
            record.status = StepStatus::SessionEnded;
            return Ok(());
        }
        if provider_unavailable(view, record) {
            return Ok(());
        }
        let now_us = backend.now_us();
        if now_us > target_us + MAX_TRIGGER_LATENESS_US {
            record.status = StepStatus::TriggerStale;
            record.detail = Some(format!(
                "planned start passed {} ms before the trigger could act",
                (now_us - target_us) / 1000
            ));
            return Ok(());
        }
        if step.trigger.require_lamp_speaking && !view.speaking(record.bound_turn) {
            // The overlap condition no longer holds; playing now would test
            // something else, so nothing is played and the gap is recorded.
            record.status = StepStatus::PreconditionLost;
            record.detail = Some("the triggering reply was no longer playing".into());
            return Ok(());
        }
        // Take anything already delivered, then check once more immediately
        // before playback: no stimulus after a failed terminal or session end.
        let now_us = backend.now_us();
        while observe_next(backend, view, now_us)? {}
        if view.ended {
            record.status = StepStatus::SessionEnded;
            return Ok(());
        }
        if provider_unavailable(view, record) {
            return Ok(());
        }
        let started_us = match (&step.scene, &timings[index]) {
            (Some(scene), Some(timing)) => {
                let injection = backend.inject(step, scene, timing, target_us)?;
                record.timing = Some(timing.clone());
                record.receipt = injection.receipt;
                if let Some(reason) = injection.withheld {
                    record.status = StepStatus::ProviderUnavailable;
                    record.detail = Some(reason);
                    return Ok(());
                }
                injection.started_us
            }
            _ => backend.now_us(),
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

    /// A virtual-clock backend that replays scripted live events.
    struct Script {
        now: u64,
        events: std::collections::VecDeque<(u64, EventKind, Option<u64>)>,
        injected: Vec<(String, u64)>,
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
}
