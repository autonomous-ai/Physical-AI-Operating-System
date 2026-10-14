//! Explicit network ownership and bounded recovery policy. Nothing in this
//! module discovers authority, installs a manager or invokes a shell command.
use crate::{
    config::Secret,
    setup::{self, NetworkEvidence},
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, fmt};

pub const LOSS_GRACE_US: u64 = 90_000_000;
pub const RETRY_INTERVAL_US: u64 = 120_000_000;
pub const JOIN_TIMEOUT_US: u64 = 45_000_000;
pub const MAX_SCRIPT_STEPS: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkError {
    Unsupported,
    Disabled,
    InvalidRequest,
    Busy,
    Unavailable,
    PermissionDenied,
    Unmanaged,
    WrongDevice,
    Superseded,
    Protocol,
    NotReady,
    RollbackFailed,
    /// Profile persistence and checkpoint destruction are separate manager
    /// operations. A private saved profile may survive this incomplete commit.
    CommitUncertain,
    CheckpointExpired,
    ClockReversed,
    Stale,
}
impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "network operation: {self:?}")
    }
}
impl std::error::Error for NetworkError {}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OperationId {
    pub attempt_id: String,
    pub generation: u64,
    pub sequence: u64,
}
impl OperationId {
    pub fn validate(&self) -> Result<(), NetworkError> {
        if !setup::valid_id(&self.attempt_id) || self.generation == 0 || self.sequence == 0 {
            Err(NetworkError::InvalidRequest)
        } else {
            Ok(())
        }
    }
    fn same_attempt(&self, other: &Self) -> bool {
        self.attempt_id == other.attempt_id && self.generation == other.generation
    }
}

pub enum WifiSecurity {
    Open,
    WpaPsk(Secret),
}

/// SSID and password are input-only and have no Debug or Serialize route.
pub struct WifiCredentials {
    ssid: Vec<u8>,
    security: WifiSecurity,
}
impl WifiCredentials {
    pub fn new(ssid: Vec<u8>, security: WifiSecurity) -> Result<Self, NetworkError> {
        if ssid.is_empty() || ssid.len() > 32 {
            return Err(NetworkError::InvalidRequest);
        }
        if let WifiSecurity::WpaPsk(secret) = &security {
            let p = secret.expose();
            if !(((8..=63).contains(&p.len()) && p.iter().all(|v| (32..=126).contains(v)))
                || (p.len() == 64 && p.iter().all(u8::is_ascii_hexdigit)))
            {
                return Err(NetworkError::InvalidRequest);
            }
        }
        Ok(Self { ssid, security })
    }
    pub fn ssid(&self) -> &[u8] {
        &self.ssid
    }
    pub fn security(&self) -> &WifiSecurity {
        &self.security
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKind {
    Probe,
    JoinWifi,
    Commit,
    Rollback,
    ActivateRecovery,
    RetrySaved,
}
pub enum NetworkCommand {
    Probe,
    JoinWifi(WifiCredentials),
    Commit,
    Rollback,
    ActivateRecovery,
    RetrySaved,
}
impl NetworkCommand {
    pub fn kind(&self) -> CommandKind {
        match self {
            Self::Probe => CommandKind::Probe,
            Self::JoinWifi(_) => CommandKind::JoinWifi,
            Self::Commit => CommandKind::Commit,
            Self::Rollback => CommandKind::Rollback,
            Self::ActivateRecovery => CommandKind::ActivateRecovery,
            Self::RetrySaved => CommandKind::RetrySaved,
        }
    }
}
pub struct NetworkRequest {
    pub id: OperationId,
    pub command: NetworkCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkResult {
    Accepted,
    Committed,
    RolledBack,
    NoActiveTransaction,
    Observation(NetworkEvidence),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    pub id: OperationId,
    /// Supplied by the same boot clock as SetupStore, at the start of a probe.
    /// Slow calls therefore produce stale evidence rather than refreshed facts.
    pub observed_us: u64,
    pub result: Result<NetworkResult, NetworkError>,
}

/// submit/poll must not perform blocking I/O. There is at most one outstanding
/// operation, including a completed response the owner has not collected yet.
pub trait NetworkAdapter {
    fn submit(&mut self, request: NetworkRequest) -> Result<(), NetworkError>;
    fn poll(&mut self) -> Option<Completion>;
}

#[derive(Default)]
pub struct UnsupportedAdapter {
    pending: Option<Completion>,
}
impl NetworkAdapter for UnsupportedAdapter {
    fn submit(&mut self, request: NetworkRequest) -> Result<(), NetworkError> {
        request.id.validate()?;
        if self.pending.is_some() {
            return Err(NetworkError::Busy);
        }
        self.pending = Some(Completion {
            id: request.id,
            observed_us: 0,
            result: Err(NetworkError::Unsupported),
        });
        Ok(())
    }
    fn poll(&mut self) -> Option<Completion> {
        self.pending.take()
    }
}

pub struct ScriptStep {
    pub expected: CommandKind,
    pub observed_us: u64,
    pub result: Result<NetworkResult, NetworkError>,
}
pub struct ScriptedAdapter {
    steps: VecDeque<ScriptStep>,
    pending: Option<Completion>,
}
impl ScriptedAdapter {
    pub fn new(steps: Vec<ScriptStep>) -> Result<Self, NetworkError> {
        if steps.len() > MAX_SCRIPT_STEPS {
            return Err(NetworkError::InvalidRequest);
        }
        Ok(Self {
            steps: steps.into(),
            pending: None,
        })
    }
    pub fn remaining(&self) -> usize {
        self.steps.len()
    }
}
impl NetworkAdapter for ScriptedAdapter {
    fn submit(&mut self, request: NetworkRequest) -> Result<(), NetworkError> {
        request.id.validate()?;
        if self.pending.is_some() {
            return Err(NetworkError::Busy);
        }
        let step = self.steps.pop_front().ok_or(NetworkError::Protocol)?;
        let result = if step.expected == request.command.kind() {
            step.result
        } else {
            Err(NetworkError::Protocol)
        };
        self.pending = Some(Completion {
            id: request.id,
            observed_us: step.observed_us,
            result,
        });
        Ok(())
    }
    fn poll(&mut self) -> Option<Completion> {
        self.pending.take()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkStatus {
    Unknown,
    Lost,
    LanReady,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryObservation {
    pub link: LinkStatus,
    pub ap_active: bool,
    /// Unknown clients suppress an AP-breaking retry, just as a nonzero count.
    pub ap_clients: Option<u16>,
    pub saved_station: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    StartAp,
    RetrySaved,
    RestoreAp,
}

/// Pure V1 recovery policy. Actions are requests; only new observations prove
/// success. The owner dispatches through an explicitly selected adapter.
pub struct Recovery {
    clock_id: String,
    last_now: u64,
    sequence: u64,
    observed: Option<(u64, RecoveryObservation)>,
    loss_since: Option<u64>,
    retry_at: Option<u64>,
    join_until: Option<u64>,
}
impl Recovery {
    pub fn new(clock_id: &str, now_us: u64) -> Result<Self, NetworkError> {
        if !setup::valid_id(clock_id) {
            return Err(NetworkError::InvalidRequest);
        }
        Ok(Self {
            clock_id: clock_id.into(),
            last_now: now_us,
            sequence: 0,
            observed: None,
            loss_since: None,
            retry_at: None,
            join_until: None,
        })
    }
    pub fn observe(
        &mut self,
        clock_id: &str,
        sequence: u64,
        observed_us: u64,
        now_us: u64,
        value: RecoveryObservation,
    ) -> Result<Option<RecoveryAction>, NetworkError> {
        self.check_time(now_us)?;
        if clock_id != self.clock_id
            || sequence <= self.sequence
            || observed_us > now_us
            || now_us - observed_us > setup::RECEIPT_MAX_AGE_US
            || self.observed.is_some_and(|(at, _)| observed_us < at)
        {
            return Err(NetworkError::Stale);
        }
        self.sequence = sequence;
        // A gap cannot prove that a link was continuously lost for 90 seconds.
        if self
            .observed
            .is_some_and(|(at, _)| observed_us - at > setup::RECEIPT_MAX_AGE_US)
        {
            self.loss_since = None;
        }
        if value.link == LinkStatus::LanReady {
            self.loss_since = None;
            self.join_until = None;
            self.retry_at = None;
        } else if value.link == LinkStatus::Unknown {
            self.loss_since = None;
        } else if !value.ap_active && self.loss_since.is_none() {
            self.loss_since = Some(observed_us);
        }
        if value.ap_active && self.retry_at.is_none() {
            self.retry_at = Some(add(now_us, RETRY_INTERVAL_US)?);
        }
        self.observed = Some((observed_us, value));
        self.tick(now_us)
    }
    pub fn tick(&mut self, now_us: u64) -> Result<Option<RecoveryAction>, NetworkError> {
        self.check_time(now_us)?;
        self.last_now = now_us;
        // Own join timeout is independent of failed/unknown subsequent probes.
        if self.join_until.is_some_and(|until| now_us >= until) {
            self.join_until = None;
            self.retry_at = Some(add(now_us, RETRY_INTERVAL_US)?);
            self.loss_since = None;
            return Ok(Some(RecoveryAction::RestoreAp));
        }
        if self.join_until.is_some() {
            return Ok(None);
        }
        let Some((at, value)) = self.observed else {
            return Ok(None);
        };
        if now_us - at > setup::RECEIPT_MAX_AGE_US || value.link == LinkStatus::LanReady {
            return Ok(None);
        }
        if value.ap_active {
            if value.ap_clients == Some(0)
                && value.saved_station
                && value.link != LinkStatus::Unknown
                && self.retry_at.is_some_and(|at| now_us >= at)
            {
                self.join_until = Some(add(now_us, JOIN_TIMEOUT_US)?);
                self.retry_at = None;
                return Ok(Some(RecoveryAction::RetrySaved));
            }
        } else if value.link == LinkStatus::Lost
            && self
                .loss_since
                .is_some_and(|at| now_us - at >= LOSS_GRACE_US)
            && self.retry_at.is_none_or(|at| now_us >= at)
        {
            self.retry_at = Some(add(now_us, RETRY_INTERVAL_US)?);
            return Ok(Some(RecoveryAction::StartAp));
        }
        Ok(None)
    }
    fn check_time(&self, now_us: u64) -> Result<(), NetworkError> {
        if now_us < self.last_now {
            Err(NetworkError::ClockReversed)
        } else {
            Ok(())
        }
    }
}
fn add(now: u64, wait: u64) -> Result<u64, NetworkError> {
    now.checked_add(wait).ok_or(NetworkError::InvalidRequest)
}

/// These paths are selected by the owner, not discovered from an SSID or the
/// first wireless device. Profile UUIDs name already provisioned NM profiles.
#[derive(Clone, Debug)]
pub struct Selection {
    pub enabled: bool,
    pub interface: String,
    pub kind: setup::LinkKind,
    pub recovery_profile_uuid: Option<String>,
    pub station_profile_uuid: Option<String>,
}
impl Selection {
    pub fn validate(&self) -> Result<(), NetworkError> {
        if !self.enabled {
            return Err(NetworkError::Disabled);
        }
        if self.interface.is_empty()
            || self.interface.len() > 15
            || !self
                .interface
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || self.interface == "."
            || self.interface == ".."
            || [&self.recovery_profile_uuid, &self.station_profile_uuid]
                .into_iter()
                .flatten()
                .any(|uuid| !valid_uuid(uuid))
        {
            return Err(NetworkError::InvalidRequest);
        }
        Ok(())
    }
}
fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(n, b)| {
            if matches!(n, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// A typed command layer allows deterministic manager tests without a bus.
/// Real implementations must not print raw errors or connection dictionaries.
pub trait ManagerBus {
    fn selected_device(&mut self, selection: &Selection) -> Result<String, NetworkError>;
    fn checkpoint(&mut self, device: &str, timeout_seconds: u32) -> Result<String, NetworkError>;
    fn join(
        &mut self,
        device: &str,
        credentials: &WifiCredentials,
        id: &OperationId,
    ) -> Result<String, NetworkError>;
    fn observe(
        &mut self,
        device: &str,
        selection: &Selection,
        expected_profile: Option<&str>,
    ) -> Result<NetworkEvidence, NetworkError>;
    fn save(&mut self, profile: &str) -> Result<(), NetworkError>;
    fn destroy_checkpoint(&mut self, checkpoint: &str) -> Result<(), NetworkError>;
    fn rollback(&mut self, checkpoint: &str, device: &str) -> Result<(), NetworkError>;
    fn activate_profile(
        &mut self,
        device: &str,
        uuid: &str,
        recovery_ap: bool,
    ) -> Result<String, NetworkError>;
}

struct Candidate {
    id: OperationId,
    device: String,
    checkpoint: String,
    profile: String,
    new_profile: bool,
    commit_started: bool,
}
pub struct ManagerSession<B> {
    selection: Selection,
    bus: B,
    candidate: Option<Candidate>,
    last: Option<OperationId>,
}
impl<B: ManagerBus> ManagerSession<B> {
    pub fn new(selection: Selection, bus: B) -> Result<Self, NetworkError> {
        selection.validate()?;
        Ok(Self {
            selection,
            bus,
            candidate: None,
            last: None,
        })
    }
    pub fn execute(&mut self, request: NetworkRequest) -> Result<NetworkResult, NetworkError> {
        request.id.validate()?;
        if let Some(last) = &self.last
            && (request.id.generation < last.generation
                || (request.id.generation == last.generation
                    && (!request.id.same_attempt(last) || request.id.sequence <= last.sequence)))
        {
            return Err(NetworkError::Superseded);
        }
        if matches!(request.command, NetworkCommand::JoinWifi(_))
            && self
                .candidate
                .as_ref()
                .is_some_and(|c| !c.id.same_attempt(&request.id))
        {
            self.rollback_candidate()?;
        }
        self.last = Some(request.id.clone());
        let device = self.bus.selected_device(&self.selection)?;
        if !object_path(&device) {
            return Err(NetworkError::Protocol);
        }
        if self.candidate.as_ref().is_some_and(|c| c.device != device) {
            return Err(NetworkError::WrongDevice);
        }
        match request.command {
            NetworkCommand::Probe => self
                .bus
                .observe(
                    &device,
                    &self.selection,
                    self.candidate
                        .as_ref()
                        .filter(|c| c.id.same_attempt(&request.id) && !c.commit_started)
                        .map(|c| c.profile.as_str()),
                )
                .map(NetworkResult::Observation),
            NetworkCommand::JoinWifi(credentials) => {
                if self.selection.kind != setup::LinkKind::Wifi {
                    return Err(NetworkError::WrongDevice);
                }
                self.rollback_candidate()?;
                // Never an empty list/all devices; automatic rollback survives
                // this process dying before it can send an explicit rollback.
                let checkpoint = self.bus.checkpoint(&device, 45)?;
                if !object_path(&checkpoint) {
                    return Err(NetworkError::Protocol);
                }
                let profile = match self.bus.join(&device, &credentials, &request.id) {
                    Ok(profile) if object_path(&profile) => profile,
                    result => {
                        self.bus
                            .rollback(&checkpoint, &device)
                            .map_err(|_| NetworkError::RollbackFailed)?;
                        return Err(result.err().unwrap_or(NetworkError::Protocol));
                    }
                };
                self.candidate = Some(Candidate {
                    id: request.id,
                    device,
                    checkpoint,
                    profile,
                    new_profile: true,
                    commit_started: false,
                });
                Ok(NetworkResult::Accepted)
            }
            NetworkCommand::Commit => {
                let candidate = self.candidate.as_mut().ok_or(NetworkError::NotReady)?;
                if !candidate.id.same_attempt(&request.id) {
                    return Err(NetworkError::Superseded);
                }
                if candidate.commit_started {
                    return Err(NetworkError::CommitUncertain);
                }
                let evidence =
                    self.bus
                        .observe(&device, &self.selection, Some(&candidate.profile))?;
                if !evidence.lan_ready() {
                    return Err(NetworkError::NotReady);
                }
                // A transport error cannot prove the manager did not persist
                // settings or destroy the checkpoint. Mark the boundary before
                // invoking either effect and retain that uncertainty on errors.
                candidate.commit_started = true;
                if candidate.new_profile {
                    self.bus
                        .save(&candidate.profile)
                        .map_err(|_| NetworkError::CommitUncertain)?;
                }
                self.bus
                    .destroy_checkpoint(&candidate.checkpoint)
                    .map_err(|_| NetworkError::CommitUncertain)?;
                self.candidate = None;
                Ok(NetworkResult::Committed)
            }
            NetworkCommand::Rollback => Ok(if self.rollback_candidate()? {
                NetworkResult::RolledBack
            } else {
                NetworkResult::NoActiveTransaction
            }),
            NetworkCommand::ActivateRecovery | NetworkCommand::RetrySaved => {
                if self.selection.kind != setup::LinkKind::Wifi {
                    return Err(NetworkError::WrongDevice);
                }
                // Probe the selected interface before superseding any pending
                // attempt. Recovery/retry must preserve a healthy candidate or
                // unrelated fallback even when runtime preparation failed.
                if self
                    .bus
                    .observe(&device, &self.selection, None)?
                    .lan_ready()
                {
                    return Err(NetworkError::NotReady);
                }
                if self.rollback_candidate()?
                    && self
                        .bus
                        .observe(&device, &self.selection, None)?
                        .lan_ready()
                {
                    // Rollback may itself have restored the previous LAN.
                    return Err(NetworkError::NotReady);
                }
                let recovery = matches!(request.command, NetworkCommand::ActivateRecovery);
                let profile = if recovery {
                    &self.selection.recovery_profile_uuid
                } else {
                    &self.selection.station_profile_uuid
                };
                let uuid = profile.as_deref().ok_or(NetworkError::Unsupported)?;
                if recovery {
                    self.bus.activate_profile(&device, uuid, true)?;
                } else {
                    let checkpoint = self.bus.checkpoint(&device, 45)?;
                    if !object_path(&checkpoint) {
                        return Err(NetworkError::Protocol);
                    }
                    let profile = match self.bus.activate_profile(&device, uuid, false) {
                        Ok(profile) if object_path(&profile) => profile,
                        result => {
                            self.bus
                                .rollback(&checkpoint, &device)
                                .map_err(|_| NetworkError::RollbackFailed)?;
                            return Err(result.err().unwrap_or(NetworkError::Protocol));
                        }
                    };
                    self.candidate = Some(Candidate {
                        id: request.id,
                        device,
                        checkpoint,
                        profile,
                        new_profile: false,
                        commit_started: false,
                    });
                }
                Ok(NetworkResult::Accepted)
            }
        }
    }
    fn rollback_candidate(&mut self) -> Result<bool, NetworkError> {
        if let Some(candidate) = &self.candidate {
            let uncertain = candidate.commit_started;
            let result = self.bus.rollback(&candidate.checkpoint, &candidate.device);
            if result == Err(NetworkError::CheckpointExpired) {
                // Automatic rollback removes the checkpoint. Its absence is
                // not evidence that restoration worked, but it no longer owns
                // an active transaction that should block a fresh user retry.
                self.candidate = None;
                return if uncertain {
                    Err(NetworkError::CommitUncertain)
                } else {
                    Ok(false)
                };
            }
            result.map_err(|_| {
                if uncertain {
                    NetworkError::CommitUncertain
                } else {
                    NetworkError::RollbackFailed
                }
            })?;
            self.candidate = None;
            return if uncertain {
                // Restoring the device does not prove removal of a profile
                // already persisted before a failed commit acknowledgement.
                Err(NetworkError::CommitUncertain)
            } else {
                Ok(true)
            };
        }
        Ok(false)
    }
    pub fn bus(&self) -> &B {
        &self.bus
    }
}

fn object_path(value: &str) -> bool {
    value.len() > 1
        && value.len() <= 256
        && value.starts_with('/')
        && value.split('/').skip(1).all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

/// NetworkManager is optional. The worker is created only by an explicit
/// enabled selection; tests exercise ManagerBus without connecting to a daemon.
#[cfg(feature = "network-manager")]
pub mod nm {
    use super::*;
    use crate::setup::{Addressing, LinkKind, Reachability};
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            mpsc::{self, Receiver, SyncSender, TryRecvError},
        },
        time::Duration,
    };
    use zbus::{
        blocking::{Connection, Proxy},
        zvariant::{OwnedObjectPath, OwnedValue, Value},
    };

    const DEST: &str = "org.freedesktop.NetworkManager";
    const ROOT: &str = "/org/freedesktop/NetworkManager";
    const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
    const PROFILE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
    type Settings = HashMap<String, HashMap<String, OwnedValue>>;

    pub struct NmAdapter {
        tx: SyncSender<NetworkRequest>,
        rx: Receiver<Completion>,
        pending: Option<OperationId>,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    }
    impl NmAdapter {
        /// clock must be the same monotonic clock that stamps SetupStore.
        /// One worker, one queued request and one queued completion are bounds,
        /// not a promise about D-Bus daemon or hardware response latency.
        pub fn spawn(
            selection: Selection,
            clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        ) -> Result<Self, NetworkError> {
            selection.validate()?;
            if !cfg!(target_os = "linux") {
                return Err(NetworkError::Unsupported);
            }
            let (tx, requests) = mpsc::sync_channel::<NetworkRequest>(1);
            let (completed, rx) = mpsc::sync_channel(1);
            let worker_clock = clock.clone();
            std::thread::Builder::new()
                .name("surround-network".into())
                .spawn(move || {
                    let mut session = None;
                    while let Ok(request) = requests.recv() {
                        let observed_us = worker_clock();
                        let id = request.id.clone();
                        if session.is_none() {
                            session = Dbus::connect()
                                .and_then(|bus| ManagerSession::new(selection.clone(), bus))
                                .ok();
                        }
                        let result = match &mut session {
                            Some(session) => session.execute(request),
                            None => Err(NetworkError::Unavailable),
                        };
                        if completed
                            .send(Completion {
                                id,
                                observed_us,
                                result,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    // Closing a process before commit leaves the manager's 45 s
                    // checkpoint responsible for rollback. Drop never blocks UI.
                })
                .map_err(|_| NetworkError::Unavailable)?;
            Ok(Self {
                tx,
                rx,
                pending: None,
                clock,
            })
        }
    }
    impl NetworkAdapter for NmAdapter {
        fn submit(&mut self, request: NetworkRequest) -> Result<(), NetworkError> {
            request.id.validate()?;
            if self.pending.is_some() {
                return Err(NetworkError::Busy);
            }
            let id = request.id.clone();
            self.tx.try_send(request).map_err(|e| match e {
                mpsc::TrySendError::Full(_) => NetworkError::Busy,
                mpsc::TrySendError::Disconnected(_) => NetworkError::Unavailable,
            })?;
            self.pending = Some(id);
            Ok(())
        }
        fn poll(&mut self) -> Option<Completion> {
            match self.rx.try_recv() {
                Ok(value) => {
                    self.pending = None;
                    Some(value)
                }
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => self.pending.take().map(|id| Completion {
                    id,
                    observed_us: (self.clock)(),
                    result: Err(NetworkError::Unavailable),
                }),
            }
        }
    }

    struct Dbus {
        connection: Connection,
    }
    impl Dbus {
        fn connect() -> Result<Self, NetworkError> {
            if !cfg!(target_os = "linux") {
                return Err(NetworkError::Unsupported);
            }
            // An explicit local socket avoids an environment-provided remote
            // bus address receiving credentials. No session-bus autolaunch.
            let connection = zbus::blocking::connection::Builder::address(
                "unix:path=/run/dbus/system_bus_socket",
            )
            .map_err(redact)?
            .method_timeout(Duration::from_secs(2))
            .max_queued(8)
            .build()
            .map_err(redact)?;
            Ok(Self { connection })
        }
        fn proxy<'a>(
            &'a self,
            path: &'a str,
            interface: &'a str,
        ) -> Result<Proxy<'a>, NetworkError> {
            zbus::blocking::proxy::Builder::new(&self.connection)
                .destination(DEST)
                .map_err(redact)?
                .path(path)
                .map_err(redact)?
                .interface(interface)
                .map_err(redact)?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .map_err(redact)
        }
        fn settings(&self, profile: &str) -> Result<Settings, NetworkError> {
            self.proxy(profile, PROFILE)?
                .call("GetSettings", &())
                .map_err(redact)
        }
    }
    fn redact(_: zbus::Error) -> NetworkError {
        NetworkError::Unavailable
    }
    fn path(value: &str) -> Result<OwnedObjectPath, NetworkError> {
        if !object_path(value) {
            return Err(NetworkError::Protocol);
        }
        OwnedObjectPath::try_from(value.to_owned()).map_err(|_| NetworkError::Protocol)
    }
    fn string<'a>(settings: &'a Settings, section: &str, key: &str) -> Option<&'a str> {
        settings
            .get(section)?
            .get(key)
            .and_then(|v| <&str>::try_from(v).ok())
    }

    impl ManagerBus for Dbus {
        fn selected_device(&mut self, selection: &Selection) -> Result<String, NetworkError> {
            let device: OwnedObjectPath = self
                .proxy(ROOT, DEST)?
                .call("GetDeviceByIpIface", &(selection.interface.as_str(),))
                .map_err(redact)?;
            if !object_path(device.as_str()) {
                return Err(NetworkError::WrongDevice);
            }
            let proxy = self.proxy(device.as_str(), DEVICE)?;
            let interface: String = proxy.get_property("Interface").map_err(redact)?;
            let kind: u32 = proxy.get_property("DeviceType").map_err(redact)?;
            let managed: bool = proxy.get_property("Managed").map_err(redact)?;
            let wanted = match selection.kind {
                LinkKind::Wired => 1,
                LinkKind::Wifi => 2,
            };
            if interface != selection.interface || kind != wanted {
                return Err(NetworkError::WrongDevice);
            }
            if !managed {
                return Err(NetworkError::Unmanaged);
            }
            Ok(device.to_string())
        }
        fn checkpoint(
            &mut self,
            device: &str,
            timeout_seconds: u32,
        ) -> Result<String, NetworkError> {
            if timeout_seconds != 45 {
                return Err(NetworkError::InvalidRequest);
            }
            let checkpoint: OwnedObjectPath = self
                .proxy(ROOT, DEST)?
                .call(
                    "CheckpointCreate",
                    &(vec![path(device)?], timeout_seconds, 0u32),
                )
                .map_err(redact)?;
            Ok(checkpoint.to_string())
        }
        fn join(
            &mut self,
            device: &str,
            credentials: &WifiCredentials,
            id: &OperationId,
        ) -> Result<String, NetworkError> {
            let interface: String = self
                .proxy(device, DEVICE)?
                .get_property("Interface")
                .map_err(redact)?;
            let profile_name = format!("lampOS-{}-{}", id.generation, id.attempt_id);
            let mut connection = HashMap::new();
            connection.insert("id", Value::from(profile_name.as_str()));
            connection.insert("type", Value::from("802-11-wireless"));
            connection.insert("interface-name", Value::from(interface.as_str()));
            connection.insert("autoconnect", Value::from(true));
            let mut wireless = HashMap::new();
            wireless.insert("ssid", Value::from(credentials.ssid().to_vec()));
            wireless.insert("mode", Value::from("infrastructure"));
            let mut settings = HashMap::from([
                ("connection", connection),
                ("802-11-wireless", wireless),
                ("ipv4", HashMap::from([("method", Value::from("auto"))])),
                ("ipv6", HashMap::from([("method", Value::from("auto"))])),
            ]);
            if let WifiSecurity::WpaPsk(secret) = credentials.security() {
                let psk = std::str::from_utf8(secret.expose())
                    .map_err(|_| NetworkError::InvalidRequest)?;
                settings.insert(
                    "802-11-wireless-security",
                    HashMap::from([
                        ("key-mgmt", Value::from("wpa-psk")),
                        ("psk", Value::from(psk)),
                        // NM owns the secret in its private profile only on commit.
                        ("psk-flags", Value::from(0u32)),
                    ]),
                );
            }
            let options = HashMap::from([("persist", Value::from("volatile"))]);
            let specific = OwnedObjectPath::try_from("/").map_err(|_| NetworkError::Protocol)?;
            let (profile, _active, _result): (
                OwnedObjectPath,
                OwnedObjectPath,
                HashMap<String, OwnedValue>,
            ) = self
                .proxy(ROOT, DEST)?
                .call(
                    "AddAndActivateConnection2",
                    &(settings, path(device)?, specific, options),
                )
                .map_err(redact)?;
            Ok(profile.to_string())
        }
        fn observe(
            &mut self,
            device: &str,
            selection: &Selection,
            expected_profile: Option<&str>,
        ) -> Result<NetworkEvidence, NetworkError> {
            let proxy = self.proxy(device, DEVICE)?;
            let state: u32 = proxy.get_property("State").map_err(redact)?;
            let mut evidence = NetworkEvidence {
                kind: selection.kind,
                link_up: false,
                address: None,
                addressing: Addressing::Unknown,
                setup_ap: false,
                cloud: Reachability::Unknown,
                authentication_rejected: false,
            };
            // NM's generic supplicant/no-secrets failures do not prove a wrong
            // key. A qualified adapter may supply distinct rejection receipts.
            if state != 100 {
                return Ok(evidence);
            }
            let active: OwnedObjectPath = proxy.get_property("ActiveConnection").map_err(redact)?;
            if !object_path(active.as_str()) {
                return Err(NetworkError::Protocol);
            }
            let profile: OwnedObjectPath = self
                .proxy(
                    active.as_str(),
                    "org.freedesktop.NetworkManager.Connection.Active",
                )?
                .get_property("Connection")
                .map_err(redact)?;
            if expected_profile.is_some_and(|expected| expected != profile.as_str()) {
                return Err(NetworkError::NotReady);
            }
            let settings = self.settings(profile.as_str())?;
            evidence.setup_ap = string(&settings, "802-11-wireless", "mode") == Some("ap")
                || string(&settings, "ipv4", "method") == Some("shared");
            if selection.kind == LinkKind::Wired {
                let carrier: bool = self
                    .proxy(device, "org.freedesktop.NetworkManager.Device.Wired")?
                    .get_property("Carrier")
                    .map_err(redact)?;
                if !carrier {
                    return Ok(evidence);
                }
            }
            for (family, interface) in [
                ("Ip4Config", "org.freedesktop.NetworkManager.IP4Config"),
                ("Ip6Config", "org.freedesktop.NetworkManager.IP6Config"),
            ] {
                let ip: OwnedObjectPath = proxy.get_property(family).map_err(redact)?;
                if ip.as_str() == "/" {
                    continue;
                }
                let addresses: Vec<HashMap<String, OwnedValue>> = self
                    .proxy(ip.as_str(), interface)?
                    .get_property("AddressData")
                    .map_err(redact)?;
                if addresses.len() > 64 {
                    return Err(NetworkError::Protocol);
                }
                evidence.address = addresses
                    .iter()
                    .filter_map(|entry| entry.get("address"))
                    .filter_map(|value| <&str>::try_from(value).ok())
                    .filter_map(|value| value.parse().ok())
                    .find(|address| setup::usable_address(*address));
                if evidence.address.is_some() {
                    let section = if family == "Ip4Config" {
                        "ipv4"
                    } else {
                        "ipv6"
                    };
                    evidence.addressing = match string(&settings, section, "method") {
                        Some("manual") => Addressing::Static,
                        Some("auto") => {
                            let dhcp: OwnedObjectPath = proxy
                                .get_property(if section == "ipv4" {
                                    "Dhcp4Config"
                                } else {
                                    "Dhcp6Config"
                                })
                                .map_err(redact)?;
                            if dhcp.as_str() != "/" {
                                Addressing::DhcpBound
                            } else if section == "ipv6" {
                                Addressing::Autoconfigured
                            } else {
                                Addressing::Unknown
                            }
                        }
                        _ => Addressing::Unknown,
                    };
                    break;
                }
            }
            let state_after: u32 = proxy.get_property("State").map_err(redact)?;
            let active_after: OwnedObjectPath =
                proxy.get_property("ActiveConnection").map_err(redact)?;
            if state_after != 100 || active_after != active {
                return Err(NetworkError::Stale);
            }
            evidence.link_up = true;
            // Global connectivity could belong to Ethernet instead of this
            // WiFi link. Cloud readiness requires the runtime's own probe.
            Ok(evidence)
        }
        fn save(&mut self, profile: &str) -> Result<(), NetworkError> {
            let empty: Settings = HashMap::new();
            let args: HashMap<&str, Value<'_>> = HashMap::new();
            let _: HashMap<String, OwnedValue> = self
                .proxy(profile, PROFILE)?
                .call("Update2", &(empty, 1u32, args))
                .map_err(redact)?;
            Ok(())
        }
        fn destroy_checkpoint(&mut self, checkpoint: &str) -> Result<(), NetworkError> {
            self.proxy(ROOT, DEST)?
                .call("CheckpointDestroy", &(path(checkpoint)?,))
                .map_err(redact)
        }
        fn rollback(&mut self, checkpoint: &str, device: &str) -> Result<(), NetworkError> {
            let live: Vec<OwnedObjectPath> = self
                .proxy(ROOT, DEST)?
                .get_property("Checkpoints")
                .map_err(redact)?;
            if live.len() > 128 {
                return Err(NetworkError::Protocol);
            }
            if !live.iter().any(|p| p.as_str() == checkpoint) {
                return Err(NetworkError::CheckpointExpired);
            }
            let result: HashMap<String, u32> = self
                .proxy(ROOT, DEST)?
                .call("CheckpointRollback", &(path(checkpoint)?,))
                .map_err(redact)?;
            if result.get(device) != Some(&0) {
                return Err(NetworkError::RollbackFailed);
            }
            Ok(())
        }
        fn activate_profile(
            &mut self,
            device: &str,
            uuid: &str,
            recovery_ap: bool,
        ) -> Result<String, NetworkError> {
            if !valid_uuid(uuid) {
                return Err(NetworkError::InvalidRequest);
            }
            let profile: OwnedObjectPath = self
                .proxy(
                    "/org/freedesktop/NetworkManager/Settings",
                    "org.freedesktop.NetworkManager.Settings",
                )?
                .call("GetConnectionByUuid", &(uuid,))
                .map_err(redact)?;
            let settings = self.settings(profile.as_str())?;
            let interface: String = self
                .proxy(device, DEVICE)?
                .get_property("Interface")
                .map_err(redact)?;
            if string(&settings, "connection", "type") != Some("802-11-wireless")
                || string(&settings, "connection", "interface-name") != Some(interface.as_str())
                || string(&settings, "802-11-wireless", "mode")
                    != Some(if recovery_ap { "ap" } else { "infrastructure" })
                || (recovery_ap
                    && string(&settings, "802-11-wireless-security", "key-mgmt") != Some("wpa-psk"))
            {
                return Err(NetworkError::WrongDevice);
            }
            let specific = OwnedObjectPath::try_from("/").map_err(|_| NetworkError::Protocol)?;
            let _: OwnedObjectPath = self
                .proxy(ROOT, DEST)?
                .call(
                    "ActivateConnection",
                    &(profile.clone(), path(device)?, specific),
                )
                .map_err(redact)?;
            Ok(profile.to_string())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn worker_loss_returns_one_correlated_failure_without_waiting_for_setup_timeout() {
            let (tx, _requests) = mpsc::sync_channel(1);
            let (completed, rx) = mpsc::sync_channel(1);
            let id = OperationId {
                attempt_id: "a".into(),
                generation: 1,
                sequence: 2,
            };
            let mut adapter = NmAdapter {
                tx,
                rx,
                pending: Some(id.clone()),
                clock: Arc::new(|| 99),
            };
            drop(completed);
            let failed = adapter.poll().unwrap();
            assert_eq!(failed.id, id);
            assert_eq!(failed.observed_us, 99);
            assert_eq!(failed.result, Err(NetworkError::Unavailable));
            assert!(adapter.poll().is_none());
        }

        #[test]
        fn pending_request_and_uncollected_completion_use_one_slot() {
            let (tx, requests) = mpsc::sync_channel(1);
            let (completed, rx) = mpsc::sync_channel(1);
            let id = OperationId {
                attempt_id: "a".into(),
                generation: 1,
                sequence: 2,
            };
            let mut adapter = NmAdapter {
                tx,
                rx,
                pending: None,
                clock: Arc::new(|| 99),
            };
            adapter
                .submit(NetworkRequest {
                    id: id.clone(),
                    command: NetworkCommand::Probe,
                })
                .unwrap();
            requests.recv().unwrap();
            completed
                .send(Completion {
                    id: id.clone(),
                    observed_us: 8,
                    result: Ok(NetworkResult::Accepted),
                })
                .unwrap();
            assert_eq!(
                adapter.submit(NetworkRequest {
                    id,
                    command: NetworkCommand::Probe
                }),
                Err(NetworkError::Busy)
            );
            assert_eq!(adapter.poll().unwrap().observed_us, 8);
        }
    }
}
