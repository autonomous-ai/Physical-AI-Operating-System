//! Opt-in end-of-turn confirmation for the directed session.
//!
//! The model runs only in the separate `lamp-endpoint-worker` process. The
//! coordinator holds a non-blocking client: loading, a missing model, failure,
//! recovery and shutdown are observed states, and every pause without a usable
//! verdict ends on the unchanged silence window. Nothing here waits for the
//! worker; its process lifetime is supervised by the client, not by `Rig`.
use crate::{
    activity::{ConfirmPolicy, TurnDetector},
    options::{DirectedOptions, Endpoint, TurnTaking},
};
use lamp_endpoint_ipc::{
    Availability, Classifier, Limits, Monitor, ProcessClassifier, Report, SMART_TURN_MODEL_FILE,
    SMART_TURN_MODEL_SHA256, WORKER_EXECUTABLE, WorkerCommand,
};
use lamp_ipc::monotonic_us;
use serde_json::{Value, json};
use std::{io, path::Path, time::Duration};

pub(super) struct EndpointRig {
    /// Moved into the session's detector when the conversation starts.
    classifier: Option<Box<dyn Classifier>>,
    /// Absent for in-process test doubles.
    pub(super) monitor: Option<Monitor>,
    command: Option<WorkerCommand>,
}

impl EndpointRig {
    /// Starts loading at once; the session never waits for it.
    pub(super) fn start(options: &DirectedOptions, executable: &Path) -> Option<Self> {
        if options.endpoint != Endpoint::SmartTurn {
            return None;
        }
        let directory = executable.parent().unwrap_or(Path::new("/"));
        let command = WorkerCommand::smart_turn(
            directory.join(WORKER_EXECUTABLE),
            directory.join(SMART_TURN_MODEL_FILE),
        );
        Some(Self::process(command, Limits::default()))
    }

    pub(super) fn process(command: WorkerCommand, limits: Limits) -> Self {
        let classifier = ProcessClassifier::spawn(command.clone(), limits);
        Self {
            monitor: Some(classifier.monitor()),
            classifier: Some(Box::new(classifier)),
            command: Some(command),
        }
    }

    #[cfg(test)]
    pub(super) fn double(classifier: Box<dyn Classifier>) -> Self {
        Self {
            classifier: Some(classifier),
            monitor: None,
            command: None,
        }
    }

    /// Moves the classifier into the detector and describes the policy.
    pub(super) fn attach(
        endpoint: Option<&mut Self>,
        turn_taking: TurnTaking,
        mut detector: TurnDetector,
    ) -> io::Result<(TurnDetector, Value)> {
        let Some(endpoint) = endpoint else {
            let silence = detector.endpoint_silence_ms();
            return Ok((detector, policy_event(turn_taking, silence, None, None)));
        };
        let classifier = endpoint
            .classifier
            .take()
            .ok_or_else(|| io::Error::other("endpoint classifier already in use"))?;
        detector = detector
            .with_confirmation(classifier, ConfirmPolicy::default())
            .map_err(io::Error::other)?;
        let event = policy_event(
            turn_taking,
            detector.endpoint_silence_ms(),
            detector.confirmation_policy(),
            endpoint.command.as_ref(),
        );
        Ok((detector, event))
    }
}

fn policy_event(
    turn_taking: TurnTaking,
    silence_ms: u64,
    confirmation: Option<ConfirmPolicy>,
    command: Option<&WorkerCommand>,
) -> Value {
    json!({"kind":"endpoint_policy","at_us":monotonic_us(),"turn_taking":turn_taking,"silence_wait_ms":silence_ms,
        "basis":if confirmation.is_some() {"smart_turn_with_silence_fallback"} else {"consecutive_low_vad_scores"},
        "semantic_confirmation":confirmation.is_some(),"confirmation":confirmation,
        "worker":command.map(|command| json!({"program":command.program,
            "args":command.args.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>()})),
        "model":confirmation.map(|_| json!({"file":SMART_TURN_MODEL_FILE,"sha256":SMART_TURN_MODEL_SHA256}))})
}

pub(super) fn availability_json(availability: &Availability) -> Value {
    let (state, detail) = match availability {
        Availability::Ready => ("ready", None),
        Availability::Loading => ("loading", None),
        Availability::Recovering(reason) => ("recovering", Some(reason)),
        Availability::Failed(reason) => ("failed", Some(reason)),
        Availability::Closed => ("closed", None),
    };
    json!({"state":state,"detail":detail})
}

pub(super) fn report_json(report: &Report) -> Value {
    json!({"worker":report.worker,"pid":report.pid,"spawned":report.spawned,"ready":report.ready,
        "load_us":report.load_us,"cold_us":report.cold_us,"ready_after_us":report.ready_after_us,
        "model_sha256":report.model_sha256,"requests":report.requests,"verdicts":report.verdicts,
        "errors":report.errors,"discarded":report.discarded,"lost":report.lost,"kills":report.kills,
        "last_exit":report.last_exit,"last_failure":report.last_failure})
}

/// Records each change of classifier availability with the worker report.
#[derive(Default)]
pub(super) struct Watch {
    last: Option<Availability>,
}

impl Watch {
    pub(super) fn observe(
        &mut self,
        detector: &TurnDetector,
        monitor: Option<&Monitor>,
    ) -> Option<Value> {
        let availability = detector.classifier_availability()?;
        if self.last.as_ref() == Some(&availability) {
            return None;
        }
        let event = json!({"kind":"endpoint_worker","at_us":monotonic_us(),
            "availability":availability_json(&availability),
            "report":monitor.map(|monitor| report_json(&monitor.report()))});
        self.last = Some(availability);
        Some(event)
    }
}

impl EndpointRig {
    /// Runtime teardown, never the live loop: closes a client the session did
    /// not take, then waits at most `limit` for its last worker to be reaped,
    /// so a later runtime cannot overlap it. Returns the final record.
    pub(super) fn finish(mut self, limit: Duration) -> Option<Value> {
        drop(self.classifier.take());
        let monitor = self.monitor?;
        let reaped = monitor.wait_finished(limit);
        Some(
            json!({"kind":"endpoint_worker_final","at_us":monotonic_us(),
            "reaped":reaped,"availability":availability_json(&monitor.availability()),
            "report":report_json(&monitor.report())}),
        )
    }
}
