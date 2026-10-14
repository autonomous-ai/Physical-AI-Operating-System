//! Pure update ordering. Persist all of `state()` (sync file, rename, sync
//! directory) before calling `committed(revision)`. Only execute that returned
//! action. Adapters enforce deadlines and guards atomically at execution and
//! deduplicate (operation_id, step_id). PID liveness is never readiness.
//! Reloading an active transaction requires a new coordinator session and a
//! fresh inspection; ambiguous transactions recover the previous release set.
use serde::{Deserialize, Serialize};
use std::fmt;

pub const QUIET_US: u64 = 45_000_000;
pub const FRESH_US: u64 = 2_000_000;
pub const STEP_TIMEOUT_US: u64 = 30_000_000;
pub const QUIET_TIMEOUT_US: u64 = 120_000_000;
pub const MAX_RESTARTS: u8 = 4;
pub const MAX_BLOCKED: usize = 32;
pub const MAX_STEPS: u64 = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn invalid(why: &str) -> Error {
    Error(why.into())
}

/// Identifies an authenticated, compatible, completely staged release set.
/// This module checks digest syntax; artifact verification belongs to admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReleaseId(pub String);
impl ReleaseId {
    pub fn check(&self) -> Result<()> {
        if self.0.len() != 64
            || !self
                .0
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("release id must be a lower-case SHA-256 digest"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    Update,
    ManualRollback,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Globally unique transaction identity, including earlier finished runs.
    pub operation_id: String,
    pub kind: RequestKind,
    pub target: ReleaseId,
    pub expected_current: Option<ReleaseId>,
}
impl Request {
    fn check(&self) -> Result<()> {
        identity(&self.operation_id)?;
        self.target.check()?;
        if let Some(previous) = &self.expected_current {
            previous.check()?;
        }
        if self.expected_current.as_ref() == Some(&self.target) {
            return Err(invalid("target is already the expected current set"));
        }
        if self.kind == RequestKind::ManualRollback && self.expected_current.is_none() {
            return Err(invalid("manual rollback needs a current installed set"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Voice,
    Body,
    Eyes,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub session: String,
    pub release: ReleaseId,
}
impl Owner {
    fn check(&self) -> Result<()> {
        identity(&self.session)?;
        self.release.check()
    }
}

/// Fresh inventory under an exclusive supervisor fence. The fence blocks
/// competing starts, restarts and switches until explicitly released.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub current: Option<ReleaseId>,
    pub supervisor_session: String,
    pub exclusive_token: String,
    pub voice: Option<Owner>,
    pub body: Option<Owner>,
    pub eyes: Option<Owner>,
    /// No remaining workers or actuator handles for every absent owner.
    pub absent_workers_gone: bool,
    /// Fresh qualified Body/explicit operator evidence, never worker absence,
    /// old bus text, or an unrevalidated pre-crash rest receipt.
    pub body_safe_when_absent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quiet {
    pub voice_session: String,
    /// Continuous owner observation began here; missing history is not quiet.
    pub observed_since_us: u64,
    pub last_heard_us: Option<u64>,
    pub last_said_us: Option<u64>,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Ready,
    /// Healthy explicit privacy inhibition, not listening/looking readiness.
    Inhibited,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub owner: Owner,
    /// Same boot and monotonic clock as Receipt. The adapter must validate the
    /// originating capability report's clock before constructing this value.
    pub observed_us: u64,
    pub health: Health,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionKind {
    /// Reconcile any uncertain prior step with the adapter's durable journal.
    Inspect {
        uncertain_step: Option<u64>,
    },
    ObserveQuiet,
    QuiesceVoice,
    StopVoice,
    RestBody,
    StopBody,
    StopEyes,
    SwitchSet {
        to: Option<ReleaseId>,
    },
    StartBody,
    StartEyes,
    StartVoice,
    VerifyReady,
    ReleaseOwnership,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub supervisor_session: String,
    pub exclusive_token: String,
    pub expected_current: Option<ReleaseId>,
    pub owner: Option<Owner>,
    pub quiesce_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub operation_id: String,
    pub step_id: u64,
    pub clock_id: String,
    pub coordinator_session: String,
    pub issued_us: u64,
    /// Exclusive deadline, enforced again at the actual side-effect boundary.
    pub deadline_us: u64,
    pub kind: ActionKind,
    pub guard: Option<Guard>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Evidence {
    Inspected {
        inventory: Inventory,
    },
    Quiet {
        observation: Quiet,
    },
    /// Owner atomically rechecks quiet AND closes admission under this token.
    VoiceQuiesced {
        owner_session: String,
        token: String,
        observation: Quiet,
    },
    BodyRested {
        owner_session: String,
        token: String,
        at_rest: bool,
        new_goals_blocked: bool,
    },
    Stopped {
        part: Part,
        owner_session: String,
        quiesce_token: Option<String>,
        all_workers_gone: bool,
    },
    Switched {
        current: Option<ReleaseId>,
    },
    Started {
        part: Part,
        owner: Owner,
    },
    /// Per-capability reports, not process/unit liveness.
    Ready {
        body: Capability,
        eyes: Capability,
        voice: Capability,
    },
    Released,
    Failed {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub operation_id: String,
    pub step_id: u64,
    pub clock_id: String,
    pub coordinator_session: String,
    pub observed_us: u64,
    /// Echo the action's exact supervisor/owner/fence preconditions.
    pub guard: Option<Guard>,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    #[default]
    Idle,
    Updating,
    RollingBack,
    Succeeded {
        release: ReleaseId,
    },
    RolledBack {
        release: ReleaseId,
    },
    FirstInstallFailed {
        reason: String,
    },
    RecoveryFailed {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub request: Request,
    pub recovery: bool,
    pub target: Option<ReleaseId>,
    pub failure: Option<String>,
    pub restarts: u8,
    pub next_step: u64,
    /// True once an actuator-affecting intent exists, even if its side effect
    /// may not have happened before a crash.
    pub touched: bool,
    pub pending: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema: u32,
    pub revision: u64,
    pub blocked_candidates: Vec<ReleaseId>,
    pub outcome: Outcome,
    pub operation: Option<Operation>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            schema: 1,
            revision: 0,
            blocked_candidates: vec![],
            outcome: Outcome::Idle,
            operation: None,
        }
    }
}
impl State {
    pub fn active(&self) -> bool {
        matches!(self.outcome, Outcome::Updating | Outcome::RollingBack)
    }
    pub fn check(&self) -> Result<()> {
        if self.schema != 1
            || self.blocked_candidates.len() > MAX_BLOCKED
            || self.revision >= u64::MAX - MAX_STEPS
        {
            return Err(invalid("unsupported or exhausted update state"));
        }
        for (index, release) in self.blocked_candidates.iter().enumerate() {
            release.check()?;
            if self.blocked_candidates[..index].contains(release) {
                return Err(invalid("duplicate blocked candidate"));
            }
        }
        if let Some(op) = &self.operation {
            op.request.check()?;
            if let Some(target) = &op.target {
                target.check()?;
            }
            if let Some(reason) = &op.failure {
                reason_text(reason)?;
            }
            if op.next_step == 0 || op.next_step > MAX_STEPS || op.restarts > MAX_RESTARTS {
                return Err(invalid("update recovery budget exceeded"));
            }
            let target = if op.recovery {
                op.request.expected_current.clone()
            } else {
                Some(op.request.target.clone())
            };
            if op.target != target {
                return Err(invalid("target differs from transaction direction"));
            }
            if op.recovery && op.failure.is_none() {
                return Err(invalid("recovery has no recorded failure cause"));
            }
            if self.active() != op.pending.is_some() {
                return Err(invalid("pending action disagrees with update outcome"));
            }
            let consistent = match &self.outcome {
                Outcome::Idle => false,
                Outcome::Updating => !op.recovery,
                Outcome::RollingBack => op.recovery,
                Outcome::Succeeded { release } => {
                    !op.recovery
                        && op.request.kind == RequestKind::Update
                        && release == &op.request.target
                }
                Outcome::RolledBack { release } => {
                    op.target.as_ref() == Some(release)
                        && (op.recovery || op.request.kind == RequestKind::ManualRollback)
                }
                Outcome::FirstInstallFailed { reason } => {
                    reason_text(reason)?;
                    op.recovery && op.target.is_none()
                }
                Outcome::RecoveryFailed { reason } => {
                    reason_text(reason)?;
                    true
                }
            };
            if !consistent || (op.recovery && !self.blocked_candidates.contains(&op.request.target))
            {
                return Err(invalid(
                    "outcome or blocked ledger disagrees with transaction direction",
                ));
            }
            if let Some(action) = &op.pending {
                identity(&action.clock_id)?;
                identity(&action.coordinator_session)?;
                if action.operation_id != op.request.operation_id
                    || action.step_id == 0
                    || action.step_id != op.next_step
                    || action.deadline_us <= action.issued_us
                    || action.deadline_us - action.issued_us > QUIET_TIMEOUT_US
                {
                    return Err(invalid("invalid persisted update intent"));
                }
                match &action.kind {
                    ActionKind::Inspect { uncertain_step } => {
                        if action.guard.is_some()
                            || uncertain_step
                                .is_some_and(|step| step == 0 || step >= action.step_id)
                        {
                            return Err(invalid("invalid reconciliation intent"));
                        }
                    }
                    _ => {
                        let guard = action.guard.as_ref().ok_or_else(|| {
                            invalid("guarded intent has no exclusive owner fence")
                        })?;
                        identity(&guard.supervisor_session)?;
                        identity(&guard.exclusive_token)?;
                        if let Some(current) = &guard.expected_current {
                            current.check()?;
                        }
                        if let Some(owner) = &guard.owner {
                            owner.check()?;
                        }
                        if let Some(token) = &guard.quiesce_token {
                            identity(token)?;
                        }
                        if let ActionKind::SwitchSet { to } = &action.kind
                            && to != &op.target
                        {
                            return Err(invalid("switch intent differs from transaction target"));
                        }
                    }
                }
            }
        } else if self.outcome != Outcome::Idle {
            return Err(invalid("update outcome has no transaction"));
        }
        Ok(())
    }
}

#[derive(Default)]
struct Facts {
    inventory: Option<Inventory>,
    voice_token: Option<String>,
    body_token: Option<String>,
    retired: Vec<String>,
    verified_until: Option<u64>,
}

pub struct Coordinator {
    enabled: bool,
    clock_id: String,
    session: String,
    state: State,
    last_now: Option<u64>,
    reconcile: bool,
    authorized_step: Option<u64>,
    facts: Facts,
}
impl Coordinator {
    pub fn new(enabled: bool, clock_id: &str, session: &str, state: State) -> Result<Self> {
        identity(clock_id)?;
        identity(session)?;
        state.check()?;
        if state.active()
            && state
                .operation
                .as_ref()
                .and_then(|op| op.pending.as_ref())
                .is_some_and(|action| action.coordinator_session == session)
        {
            return Err(invalid("reload requires a fresh coordinator session"));
        }
        let reconcile = state.active();
        Ok(Self {
            enabled,
            clock_id: clock_id.into(),
            session: session.into(),
            state,
            last_now: None,
            reconcile,
            authorized_step: None,
            facts: Facts::default(),
        })
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    /// Review/persistence only; this does not authorize execution.
    pub fn pending(&self) -> Option<&Action> {
        self.state
            .operation
            .as_ref()
            .and_then(|op| op.pending.as_ref())
    }

    pub fn begin(&mut self, now_us: u64, request: Request) -> Result<()> {
        self.allowed()?;
        self.state.check()?;
        request.check()?;
        self.clock(now_us)?;
        if self.state.active() || matches!(self.state.outcome, Outcome::RecoveryFailed { .. }) {
            return Err(invalid("unfinished recovery blocks new update requests"));
        }
        if self
            .state
            .operation
            .as_ref()
            .is_some_and(|op| op.request.operation_id == request.operation_id)
        {
            return Err(invalid("operation id must never be reused"));
        }
        if request.kind == RequestKind::Update
            && self.state.blocked_candidates.contains(&request.target)
        {
            return Err(invalid("candidate is blocked from automatic retry"));
        }
        if self.state.blocked_candidates.len() == MAX_BLOCKED {
            return Err(invalid("blocked candidate ledger is full"));
        }
        self.state.outcome = Outcome::Updating;
        self.state.operation = Some(Operation {
            target: Some(request.target.clone()),
            request,
            recovery: false,
            failure: None,
            restarts: 0,
            next_step: 0,
            touched: false,
            pending: None,
        });
        self.facts = Facts::default();
        self.queue(
            now_us,
            ActionKind::Inspect {
                uncertain_step: None,
            },
        )
    }

    /// Call only after durably writing this exact revision, and after tick at
    /// the current time. Repeated calls retain the same adapter deduplication
    /// key. The adapter still checks deadline/guard at execution.
    pub fn committed(&mut self, revision: u64) -> Result<Option<Action>> {
        self.allowed()?;
        if self.reconcile || revision != self.state.revision {
            return Err(invalid("current intent has not been durably acknowledged"));
        }
        let action = self.pending().cloned();
        self.authorized_step = action.as_ref().map(|action| action.step_id);
        Ok(action)
    }

    pub fn tick(&mut self, now_us: u64) -> Result<()> {
        self.allowed()?;
        self.clock(now_us)?;
        if self.reconcile {
            self.reconcile = false;
            let op = self.state.operation.as_mut().unwrap();
            if op.restarts == MAX_RESTARTS {
                self.incomplete("restart recovery budget exhausted");
                return Ok(());
            }
            op.restarts += 1;
            let uncertain_step = op.pending.as_ref().map(|action| action.step_id);
            if !op.recovery {
                self.start_recovery(
                    now_us,
                    "coordinator restarted during an uncommitted transaction",
                    uncertain_step,
                )?;
            } else {
                self.facts = Facts::default();
                self.queue(now_us, ActionKind::Inspect { uncertain_step })?;
            }
        } else if self
            .pending()
            .is_some_and(|action| now_us >= action.deadline_us)
        {
            self.fail(now_us, "update step timed out")?;
        }
        Ok(())
    }

    /// Explicit operator retry after repairing missing safety protocols or
    /// failed recovery. This never retries the rejected candidate automatically.
    pub fn retry_recovery(&mut self, now_us: u64) -> Result<()> {
        self.allowed()?;
        self.clock(now_us)?;
        if !matches!(self.state.outcome, Outcome::RecoveryFailed { .. }) {
            return Err(invalid("only incomplete recovery can be retried"));
        }
        let op = self.state.operation.as_mut().unwrap();
        if op.restarts == MAX_RESTARTS {
            return Err(invalid("restart recovery budget exhausted"));
        }
        op.restarts += 1;
        let uncertain = Some(op.next_step);
        self.start_recovery(now_us, "explicit operator recovery retry", uncertain)
    }

    /// Bad/foreign evidence does not advance the transaction. Valid failure
    /// or timeout starts one safe recovery; recovery failure remains explicit.
    pub fn observe(&mut self, now_us: u64, receipt: Receipt) -> Result<()> {
        self.allowed()?;
        self.clock(now_us)?;
        let action = self
            .pending()
            .cloned()
            .ok_or_else(|| invalid("no update step is pending"))?;
        if self.reconcile || self.authorized_step != Some(action.step_id) {
            return Err(invalid(
                "receipt precedes committed intent or fresh reconciliation",
            ));
        }
        if receipt.operation_id != action.operation_id
            || receipt.step_id != action.step_id
            || receipt.clock_id != self.clock_id
            || receipt.coordinator_session != self.session
            || receipt.guard != action.guard
        {
            return Err(invalid(
                "receipt belongs to another operation, step, session or boot",
            ));
        }
        if receipt.observed_us < action.issued_us
            || receipt.observed_us > now_us
            || now_us - receipt.observed_us >= FRESH_US
        {
            return Err(invalid("receipt is pre-intent, future or stale"));
        }
        if now_us >= action.deadline_us {
            self.fail(now_us, "update step timed out")?;
            return Err(invalid("receipt arrived after the step deadline"));
        }
        if let Evidence::Failed { reason } = &receipt.evidence {
            reason_text(reason)?;
            return self.fail(now_us, reason);
        }
        match (&action.kind, receipt.evidence) {
            (ActionKind::Inspect { .. }, Evidence::Inspected { inventory }) => {
                self.inspected(now_us, inventory)
            }
            (ActionKind::ObserveQuiet, Evidence::Quiet { observation }) => {
                self.quiet(&observation, receipt.observed_us)?;
                self.queue(now_us, ActionKind::QuiesceVoice)
            }
            (
                ActionKind::QuiesceVoice,
                Evidence::VoiceQuiesced {
                    owner_session,
                    token,
                    observation,
                },
            ) => {
                self.owner(Part::Voice, &owner_session)?;
                identity(&token)?;
                self.quiet(&observation, receipt.observed_us)?;
                self.facts.voice_token = Some(token);
                self.queue(now_us, ActionKind::StopVoice)
            }
            (
                ActionKind::RestBody,
                Evidence::BodyRested {
                    owner_session,
                    token,
                    at_rest,
                    new_goals_blocked,
                },
            ) => {
                self.owner(Part::Body, &owner_session)?;
                identity(&token)?;
                if !at_rest || !new_goals_blocked {
                    return Err(invalid(
                        "Body has not confirmed resting with new goals blocked",
                    ));
                }
                self.facts.body_token = Some(token);
                self.queue(now_us, ActionKind::StopBody)
            }
            (
                ActionKind::StopVoice | ActionKind::StopBody | ActionKind::StopEyes,
                Evidence::Stopped {
                    part,
                    owner_session,
                    quiesce_token,
                    all_workers_gone,
                },
            ) => {
                let wanted = match action.kind {
                    ActionKind::StopVoice => Part::Voice,
                    ActionKind::StopBody => Part::Body,
                    _ => Part::Eyes,
                };
                if part != wanted
                    || !all_workers_gone
                    || quiesce_token != action.guard.as_ref().unwrap().quiesce_token
                {
                    return Err(invalid(
                        "stop receipt lacks all expected workers gone under the quiesce token",
                    ));
                }
                self.owner(part, &owner_session)?;
                self.facts.retired.push(owner_session);
                let inventory = self.facts.inventory.as_mut().unwrap();
                match part {
                    Part::Voice => inventory.voice = None,
                    Part::Body => {
                        inventory.body = None;
                        inventory.body_safe_when_absent = true;
                    }
                    Part::Eyes => inventory.eyes = None,
                }
                match part {
                    Part::Voice => self.rest_body(now_us),
                    Part::Body => self.stop_eyes(now_us),
                    Part::Eyes => self.switch(now_us),
                }
            }
            (ActionKind::SwitchSet { to }, Evidence::Switched { current }) => {
                if &current != to {
                    return Err(invalid(
                        "release-set switch did not select the requested set",
                    ));
                }
                self.facts.inventory.as_mut().unwrap().current = current.clone();
                if current.is_some() {
                    self.queue(now_us, ActionKind::StartBody)
                } else {
                    self.queue(now_us, ActionKind::ReleaseOwnership)
                }
            }
            (
                ActionKind::StartBody | ActionKind::StartEyes | ActionKind::StartVoice,
                Evidence::Started { part, owner },
            ) => {
                let wanted = match action.kind {
                    ActionKind::StartBody => Part::Body,
                    ActionKind::StartEyes => Part::Eyes,
                    _ => Part::Voice,
                };
                owner.check()?;
                if part != wanted
                    || Some(&owner.release)
                        != self.facts.inventory.as_ref().unwrap().current.as_ref()
                    || self.facts.retired.contains(&owner.session)
                {
                    return Err(invalid(
                        "start receipt has an old owner, wrong part or wrong release",
                    ));
                }
                let inventory = self.facts.inventory.as_mut().unwrap();
                match part {
                    Part::Body => inventory.body = Some(owner),
                    Part::Eyes => inventory.eyes = Some(owner),
                    Part::Voice => inventory.voice = Some(owner),
                }
                self.queue(
                    now_us,
                    match part {
                        Part::Body => ActionKind::StartEyes,
                        Part::Eyes => ActionKind::StartVoice,
                        Part::Voice => ActionKind::VerifyReady,
                    },
                )
            }
            (ActionKind::VerifyReady, Evidence::Ready { body, eyes, voice }) => {
                self.capability(Part::Body, &body, &action, receipt.observed_us, now_us)?;
                self.capability(Part::Eyes, &eyes, &action, receipt.observed_us, now_us)?;
                self.capability(Part::Voice, &voice, &action, receipt.observed_us, now_us)?;
                self.facts.verified_until = Some(
                    body.observed_us
                        .min(eyes.observed_us)
                        .min(voice.observed_us)
                        + FRESH_US,
                );
                self.queue(now_us, ActionKind::ReleaseOwnership)
            }
            (ActionKind::ReleaseOwnership, Evidence::Released) => {
                self.finish();
                Ok(())
            }
            _ => Err(invalid(
                "receipt evidence does not match the pending action",
            )),
        }
    }

    fn allowed(&self) -> Result<()> {
        if self.enabled {
            Ok(())
        } else {
            Err(invalid("updates are disabled"))
        }
    }
    fn clock(&mut self, now_us: u64) -> Result<()> {
        if now_us > u64::MAX - QUIET_TIMEOUT_US {
            if self.state.active() {
                self.incomplete("coordinator clock cannot represent a bounded deadline");
            }
            return Err(invalid(
                "coordinator clock cannot represent a bounded deadline",
            ));
        }
        if self.last_now.is_some_and(|last| now_us < last) {
            if self.state.active() {
                self.incomplete("coordinator monotonic clock moved backwards");
            }
            return Err(invalid("coordinator monotonic clock moved backwards"));
        }
        self.last_now = Some(now_us);
        Ok(())
    }
    fn queue(&mut self, now_us: u64, kind: ActionKind) -> Result<()> {
        let timeout = if kind == ActionKind::ObserveQuiet {
            QUIET_TIMEOUT_US
        } else {
            STEP_TIMEOUT_US
        };
        let mut deadline_us = now_us
            .checked_add(timeout)
            .ok_or_else(|| invalid("update deadline overflow"))?;
        if kind == ActionKind::ReleaseOwnership
            && let Some(until) = self.facts.verified_until
        {
            deadline_us = deadline_us.min(until);
        }
        let owner = match kind {
            ActionKind::ObserveQuiet | ActionKind::QuiesceVoice | ActionKind::StopVoice => {
                self.current_owner(Part::Voice)
            }
            ActionKind::RestBody | ActionKind::StopBody => self.current_owner(Part::Body),
            ActionKind::StopEyes => self.current_owner(Part::Eyes),
            _ => None,
        }
        .cloned();
        let guard = self.facts.inventory.as_ref().map(|inventory| Guard {
            supervisor_session: inventory.supervisor_session.clone(),
            exclusive_token: inventory.exclusive_token.clone(),
            expected_current: inventory.current.clone(),
            owner,
            quiesce_token: match kind {
                ActionKind::StopVoice => self.facts.voice_token.clone(),
                ActionKind::StopBody => self.facts.body_token.clone(),
                _ => None,
            },
        });
        let op = self.state.operation.as_mut().unwrap();
        if op.next_step == MAX_STEPS {
            self.incomplete("update step budget exhausted");
            return Ok(());
        }
        op.next_step += 1;
        if !matches!(
            kind,
            ActionKind::Inspect { .. }
                | ActionKind::ObserveQuiet
                | ActionKind::VerifyReady
                | ActionKind::ReleaseOwnership
        ) {
            op.touched = true;
        }
        op.pending = Some(Action {
            operation_id: op.request.operation_id.clone(),
            step_id: op.next_step,
            clock_id: self.clock_id.clone(),
            coordinator_session: self.session.clone(),
            issued_us: now_us,
            deadline_us,
            kind,
            guard,
        });
        self.authorized_step = None;
        self.bump();
        Ok(())
    }
    fn inspected(&mut self, now_us: u64, inventory: Inventory) -> Result<()> {
        identity(&inventory.supervisor_session)?;
        identity(&inventory.exclusive_token)?;
        if let Some(current) = &inventory.current {
            current.check()?;
        }
        for owner in [&inventory.voice, &inventory.body, &inventory.eyes]
            .into_iter()
            .flatten()
        {
            owner.check()?;
            if Some(&owner.release) != inventory.current.as_ref() {
                return Err(invalid(
                    "running owner differs from the current release set",
                ));
            }
        }
        if (inventory.voice.is_none() || inventory.body.is_none() || inventory.eyes.is_none())
            && !inventory.absent_workers_gone
        {
            return Err(invalid("absent owners have not been verified worker-free"));
        }
        if inventory.body.is_none() && !inventory.body_safe_when_absent {
            return Err(invalid(
                "absent Body has no fresh qualified resting evidence",
            ));
        }
        let op = self.state.operation.as_ref().unwrap();
        let expected = &op.request.expected_current;
        if inventory.current != *expected
            && !(op.recovery && inventory.current.as_ref() == Some(&op.request.target))
        {
            self.incomplete("actual release set changed outside this transaction");
            return Err(invalid(
                "actual release set changed outside this transaction",
            ));
        }
        let unchanged_recovery = op.recovery
            && !op.touched
            && inventory.current == *expected
            && inventory.voice.is_some()
            && inventory.body.is_some()
            && inventory.eyes.is_some();
        self.facts.inventory = Some(inventory);
        if unchanged_recovery {
            self.queue(now_us, ActionKind::VerifyReady)
        } else if self.current_owner(Part::Voice).is_some() {
            self.queue(now_us, ActionKind::ObserveQuiet)
        } else {
            self.rest_body(now_us)
        }
    }
    fn current_owner(&self, part: Part) -> Option<&Owner> {
        self.facts
            .inventory
            .as_ref()
            .and_then(|inventory| match part {
                Part::Voice => inventory.voice.as_ref(),
                Part::Body => inventory.body.as_ref(),
                Part::Eyes => inventory.eyes.as_ref(),
            })
    }
    fn owner(&self, part: Part, session: &str) -> Result<()> {
        if self
            .current_owner(part)
            .is_none_or(|owner| owner.session != session)
        {
            return Err(invalid("receipt belongs to a different owner session"));
        }
        Ok(())
    }
    fn quiet(&self, observation: &Quiet, at_us: u64) -> Result<()> {
        self.owner(Part::Voice, &observation.voice_session)?;
        let limit = at_us
            .checked_sub(QUIET_US)
            .ok_or_else(|| invalid("45 seconds of observation are not yet possible"))?;
        if !observation.complete
            || observation.observed_since_us > limit
            || observation.last_heard_us.is_some_and(|last| last > limit)
            || observation.last_said_us.is_some_and(|last| last > limit)
        {
            return Err(invalid(
                "continuous 45-second heard-and-said quiet window is not established",
            ));
        }
        Ok(())
    }
    fn rest_body(&mut self, now_us: u64) -> Result<()> {
        if self.current_owner(Part::Body).is_some() {
            self.queue(now_us, ActionKind::RestBody)
        } else if self.facts.inventory.as_ref().unwrap().body_safe_when_absent {
            self.stop_eyes(now_us)
        } else {
            self.incomplete("Body resting cannot be established");
            Ok(())
        }
    }
    fn stop_eyes(&mut self, now_us: u64) -> Result<()> {
        if self.current_owner(Part::Eyes).is_some() {
            self.queue(now_us, ActionKind::StopEyes)
        } else {
            self.switch(now_us)
        }
    }
    fn switch(&mut self, now_us: u64) -> Result<()> {
        let inventory = self.facts.inventory.as_ref().unwrap();
        if inventory.voice.is_some()
            || inventory.body.is_some()
            || inventory.eyes.is_some()
            || !inventory.body_safe_when_absent
        {
            return Err(invalid("switch requires all owners gone and Body resting"));
        }
        self.queue(
            now_us,
            ActionKind::SwitchSet {
                to: self.state.operation.as_ref().unwrap().target.clone(),
            },
        )
    }
    fn capability(
        &self,
        part: Part,
        capability: &Capability,
        action: &Action,
        observed_us: u64,
        now_us: u64,
    ) -> Result<()> {
        if Some(&capability.owner) != self.current_owner(part)
            || capability.observed_us < action.issued_us
            || capability.observed_us > observed_us
            || now_us - capability.observed_us >= FRESH_US
            || capability.health == Health::Failed
            || (part == Part::Body && capability.health != Health::Ready)
        {
            return Err(invalid(
                "capability readiness is missing, stale, failed or from another owner",
            ));
        }
        Ok(())
    }
    fn fail(&mut self, now_us: u64, reason: &str) -> Result<()> {
        if self.state.operation.as_ref().unwrap().recovery {
            self.incomplete(reason);
            Ok(())
        } else {
            self.start_recovery(now_us, reason, self.pending().map(|action| action.step_id))
        }
    }
    fn start_recovery(
        &mut self,
        now_us: u64,
        reason: &str,
        uncertain_step: Option<u64>,
    ) -> Result<()> {
        let op = self.state.operation.as_mut().unwrap();
        let failed = op.request.target.clone();
        if !self.state.blocked_candidates.contains(&failed) {
            if self.state.blocked_candidates.len() == MAX_BLOCKED {
                self.incomplete("blocked candidate ledger is full");
                return Ok(());
            }
            self.state.blocked_candidates.push(failed);
        }
        op.recovery = true;
        op.target = op.request.expected_current.clone();
        op.failure = Some(reason.into());
        self.state.outcome = Outcome::RollingBack;
        self.facts = Facts::default();
        self.queue(now_us, ActionKind::Inspect { uncertain_step })
    }
    fn finish(&mut self) {
        let op = self.state.operation.as_mut().unwrap();
        op.pending = None;
        self.state.outcome = if op.recovery {
            if let Some(release) = &op.target {
                Outcome::RolledBack {
                    release: release.clone(),
                }
            } else {
                Outcome::FirstInstallFailed {
                    reason: op
                        .failure
                        .clone()
                        .unwrap_or_else(|| "first install failed".into()),
                }
            }
        } else if op.request.kind == RequestKind::ManualRollback {
            Outcome::RolledBack {
                release: op.request.target.clone(),
            }
        } else {
            Outcome::Succeeded {
                release: op.request.target.clone(),
            }
        };
        self.authorized_step = None;
        self.bump();
    }
    fn incomplete(&mut self, reason: &str) {
        if let Some(op) = &mut self.state.operation {
            op.pending = None;
        }
        self.state.outcome = Outcome::RecoveryFailed {
            reason: reason.into(),
        };
        self.authorized_step = None;
        self.bump();
    }
    fn bump(&mut self) {
        self.state.revision = self.state.revision.saturating_add(1);
    }
}

fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.len() > 96
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
    {
        return Err(invalid(
            "identity needs 1..96 ASCII letters, digits, '_', '-', '.' or ':'",
        ));
    }
    Ok(())
}
fn reason_text(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(invalid("failure reason must be bounded plain text"));
    }
    Ok(())
}
