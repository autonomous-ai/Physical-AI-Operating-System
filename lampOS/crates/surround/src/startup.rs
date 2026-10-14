//! Power-up planning and independently fresh owner capabilities. Plans are
//! data, not systemd calls. The platform supervisor must prove absence before
//! executing a start, and must never interpret Start as RestartIfRunning.
use crate::setup::valid_id;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

pub const MAX_COMPONENTS: usize = 12;
pub const MAX_DEPENDENCIES: usize = 12;
pub const MAX_STARTS: u32 = 3;
pub const START_TIMEOUT_US: u64 = 10_000_000;
pub const REPORT_MAX_AGE_US: u64 = 2_000_000;
pub const RETRY_BACKOFF_US: u64 = 1_000_000;
pub const VOICE_QUIET_US: u64 = 45_000_000;

/// Declaration order is launch order, not a serialized readiness dependency.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Body,
    Eyes,
    Voice,
    Memory,
    Actions,
    Controls,
    Schedule,
    Name,
    Expression,
    Policy,
    Health,
    Tasks,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Dependency {
    Configuration,
    Calibration,
    Audio,
    Camera,
    Network,
    Provider,
    Memory,
    Controls,
    Schedules,
    Harness,
    Owner,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Calibration {
    NotRequired,
    Present,
    Missing,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComponentConfig {
    pub component: Component,
    pub configured: bool,
    /// A platform-verified installed executable, not an invented default path.
    pub binary_available: bool,
    pub calibration: Calibration,
    /// Feature degradation only. Never a global network-online/optional-part
    /// gate; a runtime reports whether its own remaining capability can work.
    pub missing_dependencies: Vec<Dependency>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub enabled: bool,
    pub components: Vec<ComponentConfig>,
}
impl Config {
    pub fn validate(&self) -> Result<(), Error> {
        if self.components.len() > MAX_COMPONENTS {
            return Err(Error::Invalid);
        }
        let mut components = BTreeSet::new();
        for spec in &self.components {
            if !components.insert(spec.component)
                || spec.missing_dependencies.len() > MAX_DEPENDENCIES
                || spec
                    .missing_dependencies
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    != spec.missing_dependencies.len()
            {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Capability {
    Starting,
    Ready,
    Listening,
    Inhibited,
    Unavailable { reason: Dependency },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    pub component: Component,
    pub generation: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub component: Component,
    pub generation: u64,
    pub owner_session: String,
}
impl Binding {
    pub fn validate(&self) -> Result<(), Error> {
        if self.generation == 0 || !valid_id(&self.owner_session) {
            Err(Error::Invalid)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub component: Component,
    pub generation: u64,
    pub owner_session: String,
    pub boot_id: String,
    pub clock_id: String,
    pub sequence: u64,
    pub observed_us: u64,
    pub capability: Capability,
}
impl Receipt {
    pub fn validate(&self) -> Result<(), Error> {
        if self.generation == 0
            || self.sequence == 0
            || !valid_id(&self.owner_session)
            || !valid_id(&self.boot_id)
            || !valid_id(&self.clock_id)
            || (self.capability == Capability::Listening && self.component != Component::Voice)
        {
            Err(Error::Invalid)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExitProof {
    pub component: Component,
    pub generation: u64,
    pub owner_session: String,
    pub boot_id: String,
    pub clock_id: String,
    pub observed_us: u64,
    /// All of this owner's workers and device handles are gone. A PID exit or
    /// lost health report alone is insufficient.
    pub workers_gone: bool,
    /// Required for Body replacement; absent process does not mean safe pose.
    pub body_at_rest: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QuietProof {
    pub generation: u64,
    pub owner_session: String,
    pub boot_id: String,
    pub clock_id: String,
    pub observed_us: u64,
    pub observed_since_us: u64,
    pub last_heard_us: Option<u64>,
    pub last_said_us: Option<u64>,
    pub complete: bool,
    /// The old owner rechecked quiet and closed admission atomically. The
    /// platform must preserve that fence through verified exit and replacement.
    pub admission_closed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Disabled,
    MissingConfiguration,
    MissingBinary,
    MissingCalibration,
    Pending,
    Starting,
    StartTimedOut,
    Ready,
    Inhibited,
    Unavailable,
    Stale,
    Backoff,
    AwaitingQuiet,
    AwaitingRest,
    Exhausted,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub component: Component,
    pub phase: Phase,
    pub generation: u64,
    pub starts: u32,
    pub owner_session: Option<String>,
    pub observed_us: Option<u64>,
    pub capability: Option<Capability>,
    pub healthy: bool,
    pub listening: bool,
    pub missing_dependencies: Vec<Dependency>,
}
impl Status {
    pub fn validate(&self) -> Result<(), Error> {
        if self.starts > MAX_STARTS
            || self.generation != u64::from(self.starts)
            || self.owner_session.as_deref().is_some_and(|s| !valid_id(s))
            || self.missing_dependencies.len() > MAX_DEPENDENCIES
            || self
                .missing_dependencies
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.missing_dependencies.len()
            || self.healthy != matches!(self.phase, Phase::Ready | Phase::Inhibited)
            || self.listening != (self.healthy && self.capability == Some(Capability::Listening))
            || (self.capability == Some(Capability::Listening)
                && self.component != Component::Voice)
            || (self.phase == Phase::Inhibited && self.capability != Some(Capability::Inhibited))
            || (self.phase == Phase::Ready
                && !matches!(
                    self.capability,
                    Some(Capability::Ready | Capability::Listening)
                ))
            || (self.capability.is_some()
                && (self.owner_session.is_none() || self.observed_us.is_none()))
            || (self.starts == 0 && self.owner_session.is_some())
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid,
    ClockReversed,
    UnknownComponent,
    Stale,
    NotRequested,
    AlreadyBound,
    NotAbsent,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "startup evidence: {self:?}")
    }
}
impl std::error::Error for Error {}

struct Part {
    config: ComponentConfig,
    starts: u32,
    requested_us: Option<u64>,
    owner: Option<String>,
    retired: Vec<String>,
    receipt: Option<Receipt>,
    sequence: u64,
    exited_us: Option<u64>,
    body_at_rest: bool,
    quiet: Option<QuietProof>,
}
impl Part {
    fn configured_phase(&self) -> Option<Phase> {
        if !self.config.configured {
            Some(Phase::MissingConfiguration)
        } else if !self.config.binary_available {
            Some(Phase::MissingBinary)
        } else if self.config.calibration == Calibration::Missing {
            Some(Phase::MissingCalibration)
        } else {
            None
        }
    }
    fn phase(&self, enabled: bool, now: u64) -> Phase {
        if !enabled {
            return Phase::Disabled;
        }
        if let Some(phase) = self.configured_phase() {
            return phase;
        }
        if let Some(receipt) = &self.receipt {
            if now - receipt.observed_us > REPORT_MAX_AGE_US {
                return Phase::Stale;
            }
            match receipt.capability {
                Capability::Ready | Capability::Listening => return Phase::Ready,
                Capability::Inhibited => return Phase::Inhibited,
                Capability::Unavailable { .. } => return Phase::Unavailable,
                Capability::Starting => {}
            }
        }
        if let Some(at) = self.requested_us {
            return if now - at >= START_TIMEOUT_US {
                Phase::StartTimedOut
            } else {
                Phase::Starting
            };
        }
        if self.starts >= MAX_STARTS {
            return Phase::Exhausted;
        }
        if self.exited_us.is_some_and(|at| now - at < RETRY_BACKOFF_US) {
            return Phase::Backoff;
        }
        if !self.retired.is_empty()
            && self.config.component == Component::Body
            && !self.body_at_rest
        {
            return Phase::AwaitingRest;
        }
        if self.starts > 0
            && self.config.component == Component::Voice
            && self
                .quiet
                .as_ref()
                .is_none_or(|q| now - q.observed_us > REPORT_MAX_AGE_US)
        {
            return Phase::AwaitingQuiet;
        }
        Phase::Pending
    }
}

pub struct Monitor {
    enabled: bool,
    boot_id: String,
    clock_id: String,
    last_now: u64,
    parts: Vec<Part>,
}
impl Monitor {
    pub fn new(
        mut config: Config,
        boot_id: &str,
        clock_id: &str,
        now_us: u64,
    ) -> Result<Self, Error> {
        config.validate()?;
        if !valid_id(boot_id) || !valid_id(clock_id) {
            return Err(Error::Invalid);
        }
        config.components.sort_by_key(|s| s.component);
        Ok(Self {
            enabled: config.enabled,
            boot_id: boot_id.into(),
            clock_id: clock_id.into(),
            last_now: now_us,
            parts: config
                .components
                .into_iter()
                .map(|config| Part {
                    config,
                    starts: 0,
                    requested_us: None,
                    owner: None,
                    retired: Vec::new(),
                    receipt: None,
                    sequence: 0,
                    exited_us: None,
                    body_at_rest: false,
                    quiet: None,
                })
                .collect(),
        })
    }

    /// One bounded batch, Body/Eyes/Voice first. Missing camera, memory, or
    /// network never adds a wait before an otherwise configured Voice start.
    /// The executor must use an exclusive supervisor fence and start-if-absent.
    pub fn plan(&mut self, now_us: u64) -> Result<Vec<Launch>, Error> {
        self.time(now_us)?;
        let mut plan = Vec::new();
        for part in &mut self.parts {
            if part.phase(self.enabled, now_us) != Phase::Pending {
                continue;
            }
            part.starts += 1;
            part.requested_us = Some(now_us);
            part.exited_us = None;
            part.quiet = None;
            part.receipt = None;
            part.sequence = 0;
            plan.push(Launch {
                component: part.config.component,
                generation: u64::from(part.starts),
            });
        }
        Ok(plan)
    }

    /// The private supervisor supplies this binding in a fresh acknowledgement
    /// to this launch generation. Public UI data must never bind an owner.
    pub fn bind(
        &mut self,
        component: Component,
        generation: u64,
        owner_session: &str,
        now_us: u64,
    ) -> Result<(), Error> {
        self.time(now_us)?;
        Binding {
            component,
            generation,
            owner_session: owner_session.into(),
        }
        .validate()?;
        let part = self.part(component)?;
        if part.requested_us.is_none() || generation != u64::from(part.starts) {
            return Err(Error::NotRequested);
        }
        if part.owner.as_deref() == Some(owner_session) {
            return Ok(());
        }
        if part.owner.is_some() || part.retired.iter().any(|s| s == owner_session) {
            return Err(Error::AlreadyBound);
        }
        part.owner = Some(owner_session.into());
        Ok(())
    }

    /// false means stale or superseded and never advances the accepted sequence.
    pub fn observe(&mut self, now_us: u64, receipt: Receipt) -> Result<bool, Error> {
        self.time(now_us)?;
        receipt.validate()?;
        if !self.fresh(
            &receipt.boot_id,
            &receipt.clock_id,
            receipt.observed_us,
            now_us,
        ) {
            return Ok(false);
        }
        let part = self.part(receipt.component)?;
        if part.owner.as_deref() != Some(&receipt.owner_session)
            || receipt.generation != u64::from(part.starts)
            || part.requested_us.is_none_or(|at| receipt.observed_us < at)
            || receipt.sequence <= part.sequence
            || part
                .receipt
                .as_ref()
                .is_some_and(|old| receipt.observed_us < old.observed_us)
        {
            return Ok(false);
        }
        part.sequence = receipt.sequence;
        if receipt.capability != Capability::Inhibited {
            // A fresh advertisement of ordinary readiness cannot coexist with
            // the old owner's closed-admission replacement fence.
            part.quiet = None;
        }
        part.receipt = Some(receipt);
        Ok(true)
    }

    /// Explicit failure to create any owner. A timeout or missing report must
    /// never be converted into this proof by a caller.
    pub fn launch_failed(
        &mut self,
        component: Component,
        generation: u64,
        now_us: u64,
    ) -> Result<(), Error> {
        self.time(now_us)?;
        let part = self.part(component)?;
        if part.requested_us.is_none() || generation != u64::from(part.starts) {
            return Err(Error::NotRequested);
        }
        if part.owner.is_some() {
            return Err(Error::NotAbsent);
        }
        part.requested_us = None;
        part.exited_us = Some(now_us);
        Ok(())
    }

    pub fn exited(&mut self, now_us: u64, proof: ExitProof) -> Result<(), Error> {
        self.time(now_us)?;
        if !proof.workers_gone
            || !valid_id(&proof.owner_session)
            || !self.fresh(&proof.boot_id, &proof.clock_id, proof.observed_us, now_us)
        {
            return Err(Error::Stale);
        }
        let part = self.part(proof.component)?;
        if proof.generation != u64::from(part.starts)
            || part.owner.as_deref() != Some(&proof.owner_session)
            || part.requested_us.is_none_or(|at| proof.observed_us < at)
            || part
                .receipt
                .as_ref()
                .is_some_and(|r| proof.observed_us < r.observed_us)
        {
            return Err(Error::Stale);
        }
        part.retired.push(proof.owner_session);
        part.owner = None;
        part.receipt = None;
        part.requested_us = None;
        part.exited_us = Some(now_us);
        part.body_at_rest = proof.body_at_rest;
        Ok(())
    }

    pub fn authorize_voice_restart(&mut self, now_us: u64, proof: QuietProof) -> Result<(), Error> {
        self.time(now_us)?;
        if !proof.complete
            || !proof.admission_closed
            || !valid_id(&proof.owner_session)
            || !self.fresh(&proof.boot_id, &proof.clock_id, proof.observed_us, now_us)
        {
            return Err(Error::Stale);
        }
        let Some(cutoff) = proof.observed_us.checked_sub(VOICE_QUIET_US) else {
            return Err(Error::Invalid);
        };
        if proof.observed_since_us > cutoff
            || proof.last_heard_us.is_some_and(|at| at > cutoff)
            || proof.last_said_us.is_some_and(|at| at > cutoff)
        {
            return Err(Error::Invalid);
        }
        let part = self.part(Component::Voice)?;
        if proof.generation != u64::from(part.starts)
            || part.owner.as_deref() != Some(&proof.owner_session)
            || part
                .receipt
                .as_ref()
                .is_some_and(|r| proof.observed_us < r.observed_us)
        {
            return Err(Error::Stale);
        }
        part.quiet = Some(proof);
        Ok(())
    }

    pub fn snapshot(&mut self, now_us: u64) -> Result<Vec<Status>, Error> {
        self.time(now_us)?;
        Ok(self
            .parts
            .iter()
            .map(|part| {
                let phase = part.phase(self.enabled, now_us);
                let healthy = matches!(phase, Phase::Ready | Phase::Inhibited);
                let mut missing = part.config.missing_dependencies.clone();
                if let Some(Receipt {
                    capability: Capability::Unavailable { reason },
                    ..
                }) = &part.receipt
                    && !missing.contains(reason)
                {
                    missing.push(*reason);
                }
                missing.sort();
                let capability = part.receipt.as_ref().map(|r| r.capability);
                Status {
                    component: part.config.component,
                    phase,
                    generation: u64::from(part.starts),
                    starts: part.starts,
                    owner_session: part.owner.clone(),
                    observed_us: part.receipt.as_ref().map(|r| r.observed_us),
                    capability,
                    healthy,
                    listening: healthy && capability == Some(Capability::Listening),
                    missing_dependencies: missing,
                }
            })
            .collect())
    }
    fn part(&mut self, component: Component) -> Result<&mut Part, Error> {
        self.parts
            .iter_mut()
            .find(|p| p.config.component == component)
            .ok_or(Error::UnknownComponent)
    }
    fn time(&mut self, now_us: u64) -> Result<(), Error> {
        if now_us < self.last_now {
            Err(Error::ClockReversed)
        } else {
            self.last_now = now_us;
            Ok(())
        }
    }
    fn fresh(&self, boot: &str, clock: &str, observed: u64, now: u64) -> bool {
        boot == self.boot_id
            && clock == self.clock_id
            && observed <= now
            && now - observed <= REPORT_MAX_AGE_US
    }
}
