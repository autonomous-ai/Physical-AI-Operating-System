//! Durable setup progress. A receipt proves only its own stage, never general
//! lamp readiness. Callers persist begin() before dispatching an adapter request.
use crate::atomic;
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::IpAddr,
    path::{Path, PathBuf},
};

pub const JOIN_TIMEOUT_US: u64 = 45_000_000;
pub const RUNTIME_TIMEOUT_US: u64 = 300_000_000;
pub const RECEIPT_MAX_AGE_US: u64 = 5_000_000;
pub const MAX_PROGRESS_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SetupMode {
    Wired,
    Wifi,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Connecting,
    LanConnected,
    PreparingRuntime,
    Ready,
    Failed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Wired,
    Wifi,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Addressing {
    Unknown,
    DhcpPending,
    DhcpBound,
    Autoconfigured,
    Static,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Reachability {
    Unknown,
    Reachable,
    Unreachable,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Unknown,
    Preparing,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    Unsupported,
    AdapterUnavailable,
    WrongKey,
    AssociationRejected,
    NetworkNotFound,
    NoAddress,
    JoinTimeout,
    RuntimeFailed,
    RuntimeTimeout,
    Interrupted,
}

/// The adapter supplies selected-interface evidence, not global manager state.
/// In particular, an AP's own address is never a verified LAN address.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkEvidence {
    pub kind: LinkKind,
    pub link_up: bool,
    pub address: Option<IpAddr>,
    pub addressing: Addressing,
    pub setup_ap: bool,
    pub cloud: Reachability,
    /// Distinct, sequenced rejection observations are required twice. A single
    /// noisy authentication result must not be labelled a proven wrong key.
    pub authentication_rejected: bool,
}

impl NetworkEvidence {
    pub fn lan_ready(&self) -> bool {
        self.link_up
            && !self.setup_ap
            && matches!(
                self.addressing,
                Addressing::DhcpBound | Addressing::Autoconfigured | Addressing::Static
            )
            && self.address.is_some_and(usable_address)
    }
}

pub fn usable_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v) => {
            !v.is_unspecified()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_multicast()
                && !v.is_broadcast()
        }
        IpAddr::V6(v) => {
            !v.is_unspecified()
                && !v.is_loopback()
                && !v.is_multicast()
                && !v.is_unicast_link_local()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub schema_version: u32,
    pub clock_id: String,
    pub attempt_id: Option<String>,
    /// Never reused, even if the caller reuses a human-readable attempt ID.
    pub generation: u64,
    pub mode: Option<SetupMode>,
    pub phase: Phase,
    pub failure: Option<Failure>,
    pub started_us: u64,
    pub updated_us: u64,
    pub deadline_us: Option<u64>,
    pub network: Option<NetworkEvidence>,
    pub network_observed_us: Option<u64>,
    pub network_fresh: bool,
    pub runtime: RuntimeState,
    pub runtime_observed_us: Option<u64>,
    pub runtime_fresh: bool,
    pub network_sequence: u64,
    pub runtime_sequence: u64,
    pub authentication_rejections: u8,
}

impl Progress {
    /// Stage completion and fresh usability are intentionally separate.
    pub fn ready(&self) -> bool {
        self.phase == Phase::Ready
            && self.network_fresh
            && self.runtime_fresh
            && self
                .network
                .as_ref()
                .is_some_and(NetworkEvidence::lan_ready)
            && self.runtime == RuntimeState::Ready
    }

    fn validate(&self) -> io::Result<()> {
        if self.schema_version != 1
            || !valid_id(&self.clock_id)
            || self.attempt_id.as_deref().is_some_and(|v| !valid_id(v))
            || self.authentication_rejections > 2
            || self.updated_us < self.started_us
            || (self.phase == Phase::Idle) != self.attempt_id.is_none()
            || self.mode.is_some() != self.attempt_id.is_some()
            || (self.generation == 0) != (self.phase == Phase::Idle)
            || self.failure.is_some() != matches!(self.phase, Phase::Failed | Phase::Interrupted)
            || self.deadline_us.is_some()
                != matches!(
                    self.phase,
                    Phase::Connecting | Phase::LanConnected | Phase::PreparingRuntime
                )
            || self.deadline_us.is_some_and(|at| at < self.started_us)
            || (self.phase == Phase::Ready
                && self.runtime_fresh
                && self.runtime != RuntimeState::Ready)
            || (self.network_fresh && self.network.is_none())
            || self
                .network_observed_us
                .is_some_and(|at| at > self.updated_us || at < self.started_us)
            || self
                .runtime_observed_us
                .is_some_and(|at| at > self.updated_us || at < self.started_us)
            || (self.network_fresh && self.network_observed_us.is_none())
            || (self.runtime_fresh && self.runtime_observed_us.is_none())
        {
            return Err(invalid("invalid setup record"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiptValue {
    Network(NetworkEvidence),
    Runtime(RuntimeState),
    Failure(Failure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    pub attempt_id: String,
    pub generation: u64,
    pub clock_id: String,
    pub sequence: u64,
    pub observed_us: u64,
    pub value: ReceiptValue,
}

pub struct SetupStore {
    path: PathBuf,
    progress: Progress,
    last_now_us: u64,
}

impl SetupStore {
    /// A DirectoryLock held by the owner must serialize this store's writer.
    /// Restart never revives an in-flight action or earlier readiness evidence.
    pub fn open(path: &Path, clock_id: &str, now_us: u64) -> io::Result<Self> {
        if !valid_id(clock_id) {
            return Err(invalid("invalid setup clock"));
        }
        let mut progress = match atomic::read(path, MAX_PROGRESS_BYTES)? {
            Some(bytes) => {
                let mut record: Progress =
                    serde_json::from_slice(&bytes).map_err(|_| invalid("invalid setup record"))?;
                record.validate()?;
                if matches!(
                    record.phase,
                    Phase::Connecting
                        | Phase::LanConnected
                        | Phase::PreparingRuntime
                        | Phase::Ready
                ) {
                    record.phase = Phase::Interrupted;
                    record.failure = Some(Failure::Interrupted);
                }
                record
            }
            None => Progress {
                schema_version: 1,
                clock_id: clock_id.into(),
                attempt_id: None,
                generation: 0,
                mode: None,
                phase: Phase::Idle,
                failure: None,
                started_us: now_us,
                updated_us: now_us,
                deadline_us: None,
                network: None,
                network_observed_us: None,
                network_fresh: false,
                runtime: RuntimeState::Unknown,
                runtime_observed_us: None,
                runtime_fresh: false,
                network_sequence: 0,
                runtime_sequence: 0,
                authentication_rejections: 0,
            },
        };
        progress.clock_id = clock_id.into();
        progress.started_us = now_us;
        progress.updated_us = now_us;
        progress.deadline_us = None;
        progress.network_fresh = false;
        progress.network_observed_us = None;
        progress.runtime = RuntimeState::Unknown;
        progress.runtime_fresh = false;
        progress.runtime_observed_us = None;
        progress.network_sequence = 0;
        progress.runtime_sequence = 0;
        persist(path, &progress)?;
        Ok(Self {
            path: path.into(),
            progress,
            last_now_us: now_us,
        })
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    pub fn begin(
        &mut self,
        attempt_id: &str,
        now_us: u64,
        mode: SetupMode,
    ) -> io::Result<&Progress> {
        self.check_time(now_us)?;
        if !valid_id(attempt_id) {
            return Err(invalid("invalid setup attempt"));
        }
        let generation = self
            .progress
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("setup generation exhausted"))?;
        let next = Progress {
            schema_version: 1,
            clock_id: self.progress.clock_id.clone(),
            attempt_id: Some(attempt_id.into()),
            generation,
            mode: Some(mode),
            phase: Phase::Connecting,
            failure: None,
            started_us: now_us,
            updated_us: now_us,
            deadline_us: Some(deadline(now_us, JOIN_TIMEOUT_US)?),
            network: None,
            network_observed_us: None,
            network_fresh: false,
            runtime: RuntimeState::Unknown,
            runtime_observed_us: None,
            runtime_fresh: false,
            network_sequence: 0,
            runtime_sequence: 0,
            authentication_rejections: 0,
        };
        self.commit(next, now_us)?;
        Ok(&self.progress)
    }

    /// False means stale, superseded or terminal; it never advances a sequence.
    pub fn observe(&mut self, now_us: u64, receipt: Receipt) -> io::Result<bool> {
        self.check_time(now_us)?;
        self.tick(now_us)?;
        let old = &self.progress;
        if old.attempt_id.as_deref() != Some(&receipt.attempt_id)
            || old.generation != receipt.generation
            || old.clock_id != receipt.clock_id
            || receipt.sequence == 0
            || receipt.observed_us > now_us
            || now_us - receipt.observed_us > RECEIPT_MAX_AGE_US
            || receipt.observed_us < old.started_us
            || matches!(old.phase, Phase::Idle | Phase::Interrupted)
        {
            return Ok(false);
        }
        let network_receipt = matches!(receipt.value, ReceiptValue::Network(_));
        if (old.phase == Phase::Failed && !network_receipt)
            || (old.phase == Phase::Failed
                && old.failure != Some(Failure::RuntimeFailed)
                && old.failure != Some(Failure::RuntimeTimeout))
            || (network_receipt
                && (receipt.sequence <= old.network_sequence
                    || old
                        .network_observed_us
                        .is_some_and(|at| receipt.observed_us < at)))
            || (!network_receipt
                && (receipt.sequence <= old.runtime_sequence
                    || old
                        .runtime_observed_us
                        .is_some_and(|at| receipt.observed_us < at)))
        {
            return Ok(false);
        }
        let mut next = old.clone();
        if network_receipt {
            next.network_sequence = receipt.sequence;
        } else {
            next.runtime_sequence = receipt.sequence;
        }
        match receipt.value {
            ReceiptValue::Network(evidence) => {
                let expected = match next.mode {
                    Some(SetupMode::Wired) => LinkKind::Wired,
                    _ => LinkKind::Wifi,
                };
                if evidence.kind != expected {
                    return Ok(false);
                }
                if evidence.authentication_rejected && next.phase == Phase::Connecting {
                    next.authentication_rejections = (next.authentication_rejections + 1).min(2);
                }
                let lan = evidence.lan_ready();
                next.network = Some(evidence);
                next.network_observed_us = Some(receipt.observed_us);
                next.network_fresh = true;
                if next.authentication_rejections == 2 && !lan {
                    fail(&mut next, Failure::WrongKey);
                } else if lan && next.phase == Phase::Connecting {
                    next.phase = Phase::LanConnected;
                    next.deadline_us = Some(deadline(now_us, RUNTIME_TIMEOUT_US)?);
                }
            }
            ReceiptValue::Runtime(state) => {
                let withdraw_completed = next.phase == Phase::Ready
                    && matches!(state, RuntimeState::Preparing | RuntimeState::Unknown);
                let lan = next.network_fresh
                    && next
                        .network
                        .as_ref()
                        .is_some_and(NetworkEvidence::lan_ready);
                if matches!(state, RuntimeState::Preparing | RuntimeState::Ready)
                    && !withdraw_completed
                    && !lan
                {
                    return Ok(false);
                }
                next.runtime = state;
                next.runtime_observed_us = Some(receipt.observed_us);
                next.runtime_fresh = !withdraw_completed;
                match state {
                    RuntimeState::Ready => {
                        next.phase = Phase::Ready;
                        next.deadline_us = None;
                    }
                    RuntimeState::Preparing if !withdraw_completed => {
                        next.phase = Phase::PreparingRuntime;
                    }
                    RuntimeState::Preparing => {}
                    RuntimeState::Failed => fail(&mut next, Failure::RuntimeFailed),
                    RuntimeState::Unknown => {
                        next.runtime_fresh = false;
                    }
                }
            }
            ReceiptValue::Failure(reason) => fail(&mut next, reason),
        }
        next.updated_us = now_us;
        self.commit(next, now_us)?;
        Ok(true)
    }

    /// A failed owner probe withdraws current runtime evidence without erasing
    /// the last completed setup stage or changing the verified LAN/deadline.
    pub fn withdraw_runtime(&mut self, now_us: u64) -> io::Result<()> {
        self.withdraw(now_us, false)
    }

    /// An unavailable network probe cannot reuse an earlier fresh LAN claim.
    /// The last address remains useful for reconnection as historical data.
    pub fn withdraw_network(&mut self, now_us: u64) -> io::Result<()> {
        self.withdraw(now_us, true)
    }

    fn withdraw(&mut self, now_us: u64, network: bool) -> io::Result<()> {
        self.check_time(now_us)?;
        let mut next = self.progress.clone();
        if network {
            next.network_fresh = false;
        } else {
            next.runtime_fresh = false;
        }
        if next != self.progress {
            next.updated_us = now_us;
            self.commit(next, now_us)
        } else {
            self.last_now_us = now_us;
            Ok(())
        }
    }

    /// Call before serving progress as current. Timer comparisons use elapsed
    /// monotonic time in this boot; no unsynchronized wall clock is consulted.
    pub fn tick(&mut self, now_us: u64) -> io::Result<()> {
        self.check_time(now_us)?;
        let mut next = self.progress.clone();
        if next
            .network_observed_us
            .is_none_or(|at| now_us - at > RECEIPT_MAX_AGE_US)
        {
            next.network_fresh = false;
        }
        if next
            .runtime_observed_us
            .is_none_or(|at| now_us - at > RECEIPT_MAX_AGE_US)
        {
            next.runtime_fresh = false;
        }
        if next.deadline_us.is_some_and(|at| now_us >= at) {
            let reason = if next.phase == Phase::Connecting {
                Failure::JoinTimeout
            } else {
                Failure::RuntimeTimeout
            };
            fail(&mut next, reason);
        }
        if next != self.progress {
            next.updated_us = now_us;
            self.commit(next, now_us)?;
        } else {
            self.last_now_us = now_us;
        }
        Ok(())
    }

    fn check_time(&self, now_us: u64) -> io::Result<()> {
        if now_us < self.last_now_us {
            Err(invalid("setup clock moved backwards"))
        } else {
            Ok(())
        }
    }

    fn commit(&mut self, next: Progress, now_us: u64) -> io::Result<()> {
        persist(&self.path, &next)?;
        self.progress = next;
        self.last_now_us = now_us;
        Ok(())
    }
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn deadline(now: u64, wait: u64) -> io::Result<u64> {
    now.checked_add(wait)
        .ok_or_else(|| invalid("setup clock overflow"))
}
fn fail(progress: &mut Progress, reason: Failure) {
    progress.phase = Phase::Failed;
    progress.failure = Some(reason);
    progress.deadline_us = None;
    progress.runtime_fresh = false;
    if matches!(reason, Failure::RuntimeFailed | Failure::RuntimeTimeout) {
        progress.runtime = RuntimeState::Failed;
    }
}
fn persist(path: &Path, progress: &Progress) -> io::Result<()> {
    progress.validate()?;
    let bytes = serde_json::to_vec(progress).map_err(|_| invalid("invalid setup record"))?;
    atomic::write(path, &bytes, MAX_PROGRESS_BYTES)
}
