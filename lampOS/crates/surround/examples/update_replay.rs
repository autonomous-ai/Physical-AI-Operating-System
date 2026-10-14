//! Authored, no-sleep examples of the update contract. No process, socket,
//! filesystem, calibration or hardware operation is performed here.
use lamp_surround::update::{
    Action, ActionKind, Capability, Coordinator, Evidence, Health, Inventory, Outcome, Owner, Part,
    Quiet, Receipt, ReleaseId, Request, RequestKind, State,
};
use serde_json::{Value, json};

const START_US: u64 = 100_000_000;
const STEP_US: u64 = 1_000;

fn release(letter: char) -> ReleaseId {
    ReleaseId(letter.to_string().repeat(64))
}
fn request() -> Request {
    Request {
        operation_id: "authored-update".into(),
        kind: RequestKind::Update,
        expected_current: Some(release('a')),
        target: release('b'),
    }
}
fn receipt(action: &Action, now: u64, evidence: Evidence) -> Receipt {
    Receipt {
        operation_id: action.operation_id.clone(),
        step_id: action.step_id,
        clock_id: action.clock_id.clone(),
        coordinator_session: action.coordinator_session.clone(),
        observed_us: now,
        guard: action.guard.clone(),
        evidence,
    }
}

/// This serializes an intent before handing it to the scripted owner. It tests
/// the durable-wrapper contract, not fsync, disk faults or real power loss.
fn saved(coordinator: &Coordinator) -> State {
    let bytes = serde_json::to_vec(coordinator.state()).unwrap();
    let state: State = serde_json::from_slice(&bytes).unwrap();
    state.check().unwrap();
    state
}

struct Script {
    coordinator: Coordinator,
    inventory: Inventory,
    now: u64,
    steps: Vec<Value>,
    starts: u64,
    inspections: u64,
    rest_available: bool,
    body_at_rest: bool,
}
impl Script {
    fn new() -> Self {
        let current = release('a');
        let old = |name| {
            Some(Owner {
                session: format!("old-{name}"),
                release: current.clone(),
            })
        };
        let mut coordinator =
            Coordinator::new(true, "authored-clock", "before-crash", State::default()).unwrap();
        coordinator.begin(START_US, request()).unwrap();
        Self {
            coordinator,
            inventory: Inventory {
                current: Some(current.clone()),
                supervisor_session: "scripted-supervisor".into(),
                exclusive_token: "initial".into(),
                voice: old("voice"),
                body: old("body"),
                eyes: old("eyes"),
                absent_workers_gone: true,
                body_safe_when_absent: false,
            },
            now: START_US,
            steps: vec![],
            starts: 0,
            inspections: 0,
            rest_available: true,
            body_at_rest: false,
        }
    }
    fn action(&mut self) -> Action {
        self.coordinator.tick(self.now).unwrap();
        let checkpoint = saved(&self.coordinator);
        self.coordinator
            .committed(checkpoint.revision)
            .unwrap()
            .unwrap()
    }
    fn quiet(&self) -> Quiet {
        Quiet {
            voice_session: self.inventory.voice.as_ref().unwrap().session.clone(),
            observed_since_us: 0,
            last_heard_us: None,
            last_said_us: None,
            complete: true,
        }
    }
    fn effect(&mut self, action: &Action) -> Evidence {
        assert!(self.now < action.deadline_us);
        if let Some(guard) = &action.guard {
            assert_eq!(guard.expected_current, self.inventory.current);
            assert_eq!(guard.supervisor_session, self.inventory.supervisor_session);
            assert_eq!(guard.exclusive_token, self.inventory.exclusive_token);
        }
        match &action.kind {
            ActionKind::Inspect { .. } => {
                self.inspections += 1;
                self.inventory.exclusive_token = format!("inspection-{}", self.inspections);
                // An authored qualified observer supplies fresh actual rest
                // evidence here. A real adapter cannot derive it from absence.
                self.inventory.body_safe_when_absent =
                    self.inventory.body.is_none() && self.body_at_rest;
                Evidence::Inspected {
                    inventory: self.inventory.clone(),
                }
            }
            ActionKind::ObserveQuiet => Evidence::Quiet {
                observation: self.quiet(),
            },
            ActionKind::QuiesceVoice => Evidence::VoiceQuiesced {
                owner_session: self.inventory.voice.as_ref().unwrap().session.clone(),
                token: "closed-voice".into(),
                observation: self.quiet(),
            },
            ActionKind::RestBody if !self.rest_available => Evidence::Failed {
                reason: "authored missing Body rest protocol".into(),
            },
            ActionKind::RestBody => {
                self.body_at_rest = true;
                Evidence::BodyRested {
                    owner_session: self.inventory.body.as_ref().unwrap().session.clone(),
                    token: "rested-body".into(),
                    at_rest: true,
                    new_goals_blocked: true,
                }
            }
            ActionKind::StopVoice | ActionKind::StopBody | ActionKind::StopEyes => {
                let (part, slot) = match action.kind {
                    ActionKind::StopVoice => (Part::Voice, &mut self.inventory.voice),
                    ActionKind::StopBody => {
                        assert!(self.body_at_rest);
                        self.inventory.body_safe_when_absent = true;
                        (Part::Body, &mut self.inventory.body)
                    }
                    _ => (Part::Eyes, &mut self.inventory.eyes),
                };
                let owner = slot.take().unwrap();
                let guard = action.guard.as_ref().unwrap();
                assert_eq!(guard.owner.as_ref(), Some(&owner));
                Evidence::Stopped {
                    part,
                    owner_session: owner.session,
                    quiesce_token: guard.quiesce_token.clone(),
                    all_workers_gone: true,
                }
            }
            ActionKind::SwitchSet { to } => {
                assert!(
                    self.inventory.voice.is_none()
                        && self.inventory.body.is_none()
                        && self.inventory.eyes.is_none()
                        && self.body_at_rest
                );
                self.inventory.current = to.clone();
                Evidence::Switched {
                    current: to.clone(),
                }
            }
            ActionKind::StartBody | ActionKind::StartEyes | ActionKind::StartVoice => {
                let (part, slot) = match action.kind {
                    ActionKind::StartBody => {
                        self.body_at_rest = false;
                        self.inventory.body_safe_when_absent = false;
                        (Part::Body, &mut self.inventory.body)
                    }
                    ActionKind::StartEyes => (Part::Eyes, &mut self.inventory.eyes),
                    _ => (Part::Voice, &mut self.inventory.voice),
                };
                assert!(slot.is_none());
                self.starts += 1;
                let owner = Owner {
                    session: format!("new-owner-{}", self.starts),
                    release: self.inventory.current.clone().unwrap(),
                };
                *slot = Some(owner.clone());
                Evidence::Started { part, owner }
            }
            ActionKind::VerifyReady => {
                let fresh = |owner: &Option<Owner>, health| Capability {
                    owner: owner.clone().unwrap(),
                    observed_us: self.now,
                    health,
                };
                Evidence::Ready {
                    body: fresh(&self.inventory.body, Health::Ready),
                    eyes: fresh(&self.inventory.eyes, Health::Inhibited),
                    voice: fresh(&self.inventory.voice, Health::Inhibited),
                }
            }
            ActionKind::ReleaseOwnership => Evidence::Released,
        }
    }
    fn submit(&mut self, action: Action, evidence: Evidence) {
        self.steps.push(json!({ "at_us": self.now, "step_id": action.step_id, "action": action.kind, "evidence": evidence }));
        self.coordinator
            .observe(self.now, receipt(&action, self.now, evidence))
            .unwrap();
        saved(&self.coordinator);
        self.now += STEP_US;
    }
    fn step(&mut self) {
        let action = self.action();
        let evidence = self.effect(&action);
        self.submit(action, evidence);
    }
    fn until(&mut self, kind: ActionKind) {
        for _ in 0..40 {
            if self.coordinator.pending().unwrap().kind == kind {
                return;
            }
            self.step();
        }
        panic!("authored phase not reached: {kind:?}");
    }
    fn finish(&mut self) {
        for _ in 0..40 {
            if !self.coordinator.state().active() {
                return;
            }
            self.step();
        }
        panic!("authored transaction did not finish");
    }
    fn report(&self, name: &str) -> Value {
        json!({ "scenario": name, "assertions_passed": true, "outcome": self.coordinator.state().outcome,
            "authored_start_us": START_US, "authored_elapsed_us": self.now - START_US, "action_count": self.steps.len(),
            "actions": self.steps.iter().map(|step| &step["action"]).collect::<Vec<_>>(), "blocked_candidates": self.coordinator.state().blocked_candidates })
    }
}

fn main() {
    assert_eq!(
        std::env::args().len(),
        1,
        "update_replay accepts no arguments"
    );
    let mut reports = Vec::new();
    let mut happy = Script::new();
    happy.finish();
    assert_eq!(
        happy.coordinator.state().outcome,
        Outcome::Succeeded {
            release: release('b')
        }
    );
    assert_eq!(happy.steps.len(), 13);
    reports.push(happy.report("happy_update_with_privacy_inhibited_capabilities"));

    let mut failed = Script::new();
    failed.until(ActionKind::StartVoice);
    let action = failed.action();
    failed.submit(
        action,
        Evidence::Failed {
            reason: "authored candidate Voice start failure".into(),
        },
    );
    failed.finish();
    assert_eq!(
        failed.coordinator.state().outcome,
        Outcome::RolledBack {
            release: release('a')
        }
    );
    assert!(
        failed
            .coordinator
            .state()
            .blocked_candidates
            .contains(&release('b'))
    );
    reports.push(failed.report("candidate_start_failure_and_safe_rollback"));

    let mut absent = Script::new();
    absent.rest_available = false;
    absent.finish();
    assert!(matches!(
        absent.coordinator.state().outcome,
        Outcome::RecoveryFailed { .. }
    ));
    assert!(
        !absent
            .steps
            .iter()
            .any(|step| step["action"]["kind"] == "stop_body")
    );
    assert_eq!(absent.inventory.current, Some(release('a')));
    reports.push(absent.report("missing_body_rest_blocks_stop_and_leaves_recovery_incomplete"));

    let mut disabled =
        Coordinator::new(false, "authored-clock", "disabled", State::default()).unwrap();
    assert!(disabled.begin(START_US, request()).is_err());
    assert!(disabled.tick(START_US).is_err());
    assert!(disabled.committed(0).is_err());
    assert_eq!(disabled.state(), &State::default());
    reports.push(json!({ "scenario": "disabled", "assertions_passed": true, "action_count": 0, "outcome": "idle", "authored_elapsed_us": 0 }));

    let mut crash = Script::new();
    crash.until(ActionKind::StartBody);
    let uncertain = crash.action();
    crash.coordinator = Coordinator::new(
        true,
        "authored-clock",
        "after-crash",
        saved(&crash.coordinator),
    )
    .unwrap();
    let resumed = crash.action();
    assert_eq!(
        resumed.kind,
        ActionKind::Inspect {
            uncertain_step: Some(uncertain.step_id)
        }
    );
    assert!(
        crash
            .coordinator
            .observe(
                crash.now,
                receipt(&uncertain, crash.now, Evidence::Released)
            )
            .is_err()
    );
    crash.finish();
    assert_eq!(
        crash.coordinator.state().outcome,
        Outcome::RolledBack {
            release: release('a')
        }
    );
    reports.push(crash.report("power_cut_after_set_switch_requires_new_inspection_and_recovery"));

    println!("{}", serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "source": "authored scripted owners; no lamp, network, shell or filesystem effects",
        "timing": "injected monotonic microseconds; 1000us per accepted authored action, not measured latency",
        "persistence": "JSON round trip before dispatch; real fsync and crash durability require the separate atomic/worker tests",
        "scenario_count": reports.len(), "all_assertions_passed": true, "scenarios": reports,
    })).unwrap());
}
