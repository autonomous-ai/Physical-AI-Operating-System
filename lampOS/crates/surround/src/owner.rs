//! Bounded private Unix transport to an explicitly installed owner supervisor.
//! This is not an owner implementation. A transport reply never substitutes
//! for update/reset validation, real worker exit or calibrated Body evidence.
//! The peer must enforce request deadlines and deduplicate stable operation IDs
//! at its final side-effect boundary. Cancelling a call does not undo a command.
use crate::{atomic, configuration::Generation, reset, setup, startup, update};
use rustix::fs::{AtFlags, FileType};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fmt, io,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{OwnedSemaphorePermit, Semaphore},
};

pub const MAX_FRAME: usize = 65_536;
pub const PROBE_TIMEOUT_US: u64 = 2_000_000;
pub const ACTION_TIMEOUT_US: u64 = 30_000_000;
/// Quiet observation can include 45 seconds after the call was admitted.
/// Ordinary owner effects retain the shorter ACTION_TIMEOUT_US boundary.
pub const QUIET_TIMEOUT_US: u64 = 120_000_000;
pub const RECEIPT_AGE_US: u64 = 2_000_000;
pub const CONFIGURATION_QUIET_US: u64 = 45_000_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "command",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Command {
    Probe,
    Update(update::Action),
    Reset(reset::Action),
    /// Always requires CONFIGURATION_QUIET_US of continuously observed quiet,
    /// atomic Voice admission closure, verified exit and a durable fenced CAS.
    /// These requirements are fixed by schema 1, never caller-selected flags.
    Configure {
        operation_id: String,
        expected: Option<Generation>,
        target: Generation,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub schema_version: u32,
    pub id: String,
    pub clock_id: String,
    pub coordinator_session: String,
    pub issued_us: u64,
    pub expires_us: u64,
    pub command: Command,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplyEnvelope {
    pub schema_version: u32,
    pub id: String,
    pub clock_id: String,
    pub coordinator_session: String,
    pub observed_us: u64,
    pub reply: Reply,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "reply",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Reply {
    Probe(Probe),
    Update(update::Receipt),
    Reset(reset::Receipt),
    Configure(ConfigurationReceipt),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    /// Trusted supervisor time-sync evidence. A locally plausible date or an
    /// HTTP Date header is not permission to set time_synchronized.
    pub unix_seconds: u64,
    pub time_synchronized: bool,
    pub observed_us: u64,
    pub runtime: setup::RuntimeState,
    pub startup: Vec<startup::Receipt>,
}

impl Probe {
    /// Only validated receipts returned by Client establish fresh capability.
    pub fn voice_capability(&self) -> Option<startup::Capability> {
        self.startup
            .iter()
            .find(|r| r.component == startup::Component::Voice)
            .map(|r| r.capability)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationFailure {
    Unavailable,
    Refused,
    QuietTimeout,
    OwnerChanged,
    SelectionChanged,
    StartFailed,
    RecoveryFailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCapability {
    Listening,
    Inhibited,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationBarrier {
    /// None is an explicit supervisor proof of absent Voice and absent workers,
    /// not a missing or failed observation of an owner.
    pub owner_session: Option<String>,
    pub exclusive_token: String,
    pub observed_since_us: u64,
    pub observed_us: u64,
    pub last_heard_us: Option<u64>,
    pub last_said_us: Option<u64>,
    pub complete: bool,
    pub admission_closed: bool,
    pub workers_gone: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationSelection {
    pub from: Option<Generation>,
    pub to: Option<Generation>,
    pub exclusive_token: String,
    pub observed_us: u64,
    pub durable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationReady {
    pub owner_session: String,
    pub generation: Generation,
    pub observed_us: u64,
    /// Both are healthy; Inhibited never grants microphone/camera access.
    pub capability: VoiceCapability,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfigurationOutcome {
    Applied {
        barrier: ConfigurationBarrier,
        selection: ConfigurationSelection,
        ready: ConfigurationReady,
    },
    /// A failed candidate is stopped through the same quiet/exit fence before
    /// restoring the original selection. No previous install means `ready`
    /// is None and selection.to is None, not successful runtime readiness.
    RolledBack {
        failure: ConfigurationFailure,
        barrier: ConfigurationBarrier,
        selection: ConfigurationSelection,
        ready: Option<ConfigurationReady>,
    },
    /// No configuration or owner side effect occurred.
    Rejected { failure: ConfigurationFailure },
    /// Recovery could not be proved. `selected` is diagnostic only; it cannot
    /// authorize readiness, selection publication or an automatic retry.
    Incomplete {
        failure: ConfigurationFailure,
        selected: Option<Generation>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationReceipt {
    pub operation_id: String,
    pub expected: Option<Generation>,
    pub target: Generation,
    pub supervisor_session: String,
    pub observed_us: u64,
    pub outcome: ConfigurationOutcome,
}

impl ConfigurationReceipt {
    /// Only call after Client::call accepted the receipt's request/time binding.
    /// This still checks the entire applied proof, not a standalone success bit.
    pub fn applied(&self) -> bool {
        matches!(self.outcome, ConfigurationOutcome::Applied { .. })
            && self.validate(0, self.observed_us).is_ok()
    }

    fn validate(&self, issued_us: u64, now_us: u64) -> Result<(), Error> {
        require(identity(&self.operation_id) && identity(&self.supervisor_session))?;
        generation(&self.target)?;
        optional_generation(&self.expected)?;
        require(self.expected.as_ref() != Some(&self.target))?;
        fresh(self.observed_us, issued_us, now_us)?;
        match &self.outcome {
            ConfigurationOutcome::Applied {
                barrier,
                selection,
                ready,
            } => {
                configuration_proof(barrier, selection, issued_us, self.observed_us)?;
                require(
                    selection.from == self.expected && selection.to.as_ref() == Some(&self.target),
                )?;
                require(self.expected.is_none() || barrier.owner_session.is_some())?;
                require(ready.observed_us <= self.observed_us)?;
                ready_proof(ready, barrier, selection, &self.target, now_us)
            }
            ConfigurationOutcome::RolledBack {
                barrier,
                selection,
                ready,
                ..
            } => {
                configuration_proof(barrier, selection, issued_us, self.observed_us)?;
                require(
                    selection.from.as_ref() == Some(&self.target) && selection.to == self.expected,
                )?;
                match (&self.expected, ready) {
                    (Some(expected), Some(ready)) => {
                        require(ready.observed_us <= self.observed_us)?;
                        ready_proof(ready, barrier, selection, expected, now_us)
                    }
                    (None, None) => Ok(()),
                    _ => Err(Error::InvalidReply),
                }
            }
            ConfigurationOutcome::Rejected { .. } => Ok(()),
            ConfigurationOutcome::Incomplete { selected, .. } => optional_generation(selected),
        }
    }
}

fn configuration_proof(
    barrier: &ConfigurationBarrier,
    selection: &ConfigurationSelection,
    issued_us: u64,
    observed_us: u64,
) -> Result<(), Error> {
    require(
        barrier.owner_session.as_deref().is_none_or(identity)
            && identity(&barrier.exclusive_token)
            && barrier.complete
            && barrier.admission_closed
            && barrier.workers_gone
            && barrier.observed_us >= issued_us
            && barrier.observed_us <= observed_us
            && barrier.observed_since_us <= barrier.observed_us
            && barrier.observed_us - barrier.observed_since_us >= CONFIGURATION_QUIET_US,
    )?;
    for activity in [barrier.last_heard_us, barrier.last_said_us]
        .into_iter()
        .flatten()
    {
        require(
            activity >= barrier.observed_since_us
                && activity <= barrier.observed_us
                && barrier.observed_us - activity >= CONFIGURATION_QUIET_US,
        )?;
    }
    optional_generation(&selection.from)?;
    optional_generation(&selection.to)?;
    require(
        selection.durable
            && selection.exclusive_token == barrier.exclusive_token
            && selection.observed_us >= barrier.observed_us
            && selection.observed_us <= observed_us,
    )
}

fn ready_proof(
    ready: &ConfigurationReady,
    barrier: &ConfigurationBarrier,
    selection: &ConfigurationSelection,
    target: &Generation,
    now_us: u64,
) -> Result<(), Error> {
    require(
        identity(&ready.owner_session)
            && barrier.owner_session.as_ref() != Some(&ready.owner_session)
            && ready.generation == *target,
    )?;
    fresh(ready.observed_us, selection.observed_us, now_us)
}

/// Deliberately fixed, private diagnostics: never return peer-controlled text,
/// serde excerpts, OS paths, provider payloads or nested update failure strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Disabled,
    Busy,
    Unavailable,
    UnsafeEndpoint,
    InvalidRequest,
    InvalidReply,
    Oversized,
    Timeout,
    ClockChanged,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Disabled => "owner access is disabled",
            Self::Busy => "owner request is already active",
            Self::Unavailable => "owner supervisor is unavailable",
            Self::UnsafeEndpoint => "owner endpoint failed local ownership checks",
            Self::InvalidRequest => "owner request is invalid",
            Self::InvalidReply => "owner receipt is invalid",
            Self::Oversized => "owner frame exceeds its bound",
            Self::Timeout => "owner request timed out",
            Self::ClockChanged => "owner request clock changed",
        })
    }
}
impl std::error::Error for Error {}

pub struct Client {
    path: PathBuf,
    enabled: bool,
    clock_id: String,
    session: String,
    gate: Arc<Semaphore>,
    last_now_us: AtomicU64,
    owner_uid: u32,
}

impl Client {
    /// Configuration-only validation. Missing sockets are allowed here; there
    /// is no filesystem access, connection or implicit owner startup.
    pub fn new(
        path: PathBuf,
        enabled: bool,
        clock_id: String,
        coordinator_session: String,
    ) -> io::Result<Self> {
        if !path.is_absolute()
            || path.as_os_str().len() > 100
            || path
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
            || path
                .file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| !identity(name))
            || !identity(&clock_id)
            || !identity(&coordinator_session)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid owner client configuration",
            ));
        }
        Ok(Self {
            path,
            enabled,
            clock_id,
            session: coordinator_session,
            gate: Arc::new(Semaphore::new(1)),
            last_now_us: AtomicU64::new(0),
            owner_uid: rustix::process::geteuid().as_raw(),
        })
    }

    /// One request per connection, one in-flight request per Client, no retries.
    /// The elapsed Tokio deadline also bounds a stalled injected clock. Blocking
    /// path checks retain the permit if this future is cancelled while checking.
    pub async fn call(
        &self,
        id: &str,
        command: Command,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Result<Reply, Error> {
        if !self.enabled {
            return Err(Error::Disabled);
        }
        let permit = self
            .gate
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        let issued_us = clock();
        self.monotonic(issued_us)?;
        let expires_us = self.deadline(id, &command, issued_us)?;
        let request = RequestEnvelope {
            schema_version: 1,
            id: id.into(),
            clock_id: self.clock_id.clone(),
            coordinator_session: self.session.clone(),
            issued_us,
            expires_us,
            command,
        };
        let mut encoded = FrameWriter(Vec::new());
        serde_json::to_writer(&mut encoded, &request).map_err(|_| Error::Oversized)?;
        let mut bytes = encoded.0;
        bytes.push(b'\n');
        tokio::time::timeout(
            Duration::from_micros(expires_us - issued_us),
            self.exchange(request, bytes, clock, permit),
        )
        .await
        .map_err(|_| Error::Timeout)?
    }

    fn deadline(&self, id: &str, command: &Command, now: u64) -> Result<u64, Error> {
        if !identity(id) {
            return Err(Error::InvalidRequest);
        }
        let timeout = match command {
            Command::Probe => PROBE_TIMEOUT_US,
            Command::Configure { .. } => QUIET_TIMEOUT_US,
            Command::Reset(action) => reset::timeout_us(action.store, action.kind),
            Command::Update(action) if action.kind == update::ActionKind::ObserveQuiet => {
                QUIET_TIMEOUT_US
            }
            _ => ACTION_TIMEOUT_US,
        };
        let mut expires = now.checked_add(timeout).ok_or(Error::InvalidRequest)?;
        match command {
            Command::Probe => {}
            Command::Update(action) => {
                if !identity(&action.operation_id)
                    || action.step_id == 0
                    || action.clock_id != self.clock_id
                    || action.coordinator_session != self.session
                    || action.issued_us > now
                    || action.deadline_us <= now
                    || action
                        .deadline_us
                        .checked_sub(action.issued_us)
                        .is_none_or(|d| d > update::QUIET_TIMEOUT_US)
                {
                    return Err(Error::InvalidRequest);
                }
                expires = expires.min(action.deadline_us);
            }
            Command::Reset(action) => {
                if !identity(&action.operation_id)
                    || action.clock_id != self.clock_id
                    || action.coordinator_session != self.session
                    || action.issued_us > now
                    || action.not_after_us <= now
                    || action.not_after_us.checked_sub(action.issued_us)
                        != Some(reset::timeout_us(action.store, action.kind))
                    || action.request_id.len() > 160
                    || action.request_id.is_empty()
                    || action
                        .expected_owner_session
                        .as_deref()
                        .is_some_and(|s| !identity(s))
                {
                    return Err(Error::InvalidRequest);
                }
                expires = expires.min(action.not_after_us);
            }
            Command::Configure {
                operation_id,
                expected,
                target,
            } => {
                if !identity(operation_id)
                    || generation(target).is_err()
                    || optional_generation(expected).is_err()
                    || expected.as_ref() == Some(target)
                {
                    return Err(Error::InvalidRequest);
                }
            }
        }
        Ok(expires)
    }

    async fn exchange(
        &self,
        request: RequestEnvelope,
        bytes: Vec<u8>,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        permit: OwnedSemaphorePermit,
    ) -> Result<Reply, Error> {
        let path = self.path.clone();
        let owner_uid = self.owner_uid;
        let (endpoint, permit) = tokio::task::spawn_blocking(move || {
            let result = inspect(&path, owner_uid);
            (result, permit)
        })
        .await
        .map_err(|_| Error::Unavailable)?;
        let endpoint = endpoint?;
        self.check_clock(&request, clock())?;
        let mut stream = UnixStream::connect(&self.path)
            .await
            .map_err(connection_error)?;
        check_peer(&stream, self.owner_uid)?;
        let path = self.path.clone();
        let (current, _permit) = tokio::task::spawn_blocking(move || {
            let result = inspect(&path, owner_uid);
            (result, permit)
        })
        .await
        .map_err(|_| Error::Unavailable)?;
        if current? != endpoint {
            return Err(Error::UnsafeEndpoint);
        }
        self.check_clock(&request, clock())?;
        stream.write_all(&bytes).await.map_err(connection_error)?;
        // EOF marks the single completed response. Requiring both newline and
        // EOF rejects extra replies/trailing bytes without an unbounded buffer.
        stream.shutdown().await.map_err(connection_error)?;
        let mut bytes = Vec::new();
        stream
            .take((MAX_FRAME + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(connection_error)?;
        let now = clock();
        self.check_clock(&request, now)?;
        if bytes.len() > MAX_FRAME {
            return Err(Error::Oversized);
        }
        if bytes.last() != Some(&b'\n') || bytes[..bytes.len().saturating_sub(1)].contains(&b'\n') {
            return Err(Error::InvalidReply);
        }
        let response: ReplyEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| Error::InvalidReply)?;
        validate_reply(&request, &response, now)?;
        Ok(response.reply)
    }

    fn monotonic(&self, now_us: u64) -> Result<(), Error> {
        if now_us < self.last_now_us.fetch_max(now_us, Ordering::SeqCst) {
            Err(Error::ClockChanged)
        } else {
            Ok(())
        }
    }

    fn check_clock(&self, request: &RequestEnvelope, now_us: u64) -> Result<(), Error> {
        self.monotonic(now_us)?;
        check_clock(request, now_us)
    }
}

struct FrameWriter(Vec<u8>);
impl io::Write for FrameWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() >= MAX_FRAME - self.0.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "owner request exceeds frame bound",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
struct Endpoint {
    parent_device: u64,
    parent_inode: u64,
    device: u64,
    inode: u64,
}
fn inspect(path: &Path, owner_uid: u32) -> Result<Endpoint, Error> {
    let parent =
        atomic::directory(path.parent().ok_or(Error::UnsafeEndpoint)?).map_err(path_error)?;
    let stat = rustix::fs::statat(
        &parent,
        path.file_name().ok_or(Error::UnsafeEndpoint)?,
        AtFlags::SYMLINK_NOFOLLOW,
    )
    .map_err(|e| path_error(e.into()))?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Socket
        || stat.st_mode & 0o777 != 0o600
        || stat.st_uid != owner_uid
        || stat.st_nlink != 1
    {
        return Err(Error::UnsafeEndpoint);
    }
    let parent = rustix::fs::fstat(&parent).map_err(|_| Error::UnsafeEndpoint)?;
    Ok(Endpoint {
        parent_device: parent.st_dev as u64,
        parent_inode: parent.st_ino as u64,
        device: stat.st_dev as u64,
        inode: stat.st_ino as u64,
    })
}
fn check_peer(stream: &UnixStream, owner_uid: u32) -> Result<(), Error> {
    if stream.peer_cred().map_err(|_| Error::UnsafeEndpoint)?.uid() == owner_uid {
        Ok(())
    } else {
        Err(Error::UnsafeEndpoint)
    }
}
fn path_error(error: io::Error) -> Error {
    if error.kind() == io::ErrorKind::NotFound {
        Error::Unavailable
    } else {
        Error::UnsafeEndpoint
    }
}
fn connection_error(_: io::Error) -> Error {
    Error::Unavailable
}
fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:".contains(&b))
}
fn generation(value: &Generation) -> Result<(), Error> {
    require(
        value.id.len() == 32
            && value.document_sha256.len() == 64
            && value
                .id
                .bytes()
                .chain(value.document_sha256.bytes())
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
    )
}
fn optional_generation(value: &Option<Generation>) -> Result<(), Error> {
    value.as_ref().map_or(Ok(()), generation)
}
fn require(accepted: bool) -> Result<(), Error> {
    if accepted {
        Ok(())
    } else {
        Err(Error::InvalidReply)
    }
}
fn fresh(observed: u64, issued: u64, now: u64) -> Result<(), Error> {
    require(observed >= issued && observed <= now && now - observed < RECEIPT_AGE_US)
}
fn check_clock(request: &RequestEnvelope, now: u64) -> Result<(), Error> {
    if now < request.issued_us {
        Err(Error::ClockChanged)
    } else if now >= request.expires_us {
        Err(Error::Timeout)
    } else {
        Ok(())
    }
}

fn validate_reply(
    request: &RequestEnvelope,
    response: &ReplyEnvelope,
    now: u64,
) -> Result<(), Error> {
    require(
        response.schema_version == 1
            && response.id == request.id
            && response.clock_id == request.clock_id
            && response.coordinator_session == request.coordinator_session,
    )?;
    fresh(response.observed_us, request.issued_us, now)?;
    match (&request.command, &response.reply) {
        (Command::Probe, Reply::Probe(probe)) => {
            require(probe.observed_us <= response.observed_us)?;
            fresh(probe.observed_us, request.issued_us, now)?;
            require(!probe.time_synchronized || probe.unix_seconds > 0)?;
            require(probe.startup.len() <= startup::MAX_COMPONENTS)?;
            let mut seen = BTreeSet::new();
            let mut boot = None;
            for receipt in &probe.startup {
                receipt.validate().map_err(|_| Error::InvalidReply)?;
                require(receipt.clock_id == request.clock_id && seen.insert(receipt.component))?;
                if let Some(previous) = boot {
                    require(previous == &receipt.boot_id)?;
                }
                boot = Some(&receipt.boot_id);
                require(receipt.observed_us <= probe.observed_us)?;
                // Periodic capability samples may predate this query. Their
                // own acquisition age, not the outer probe's time, is checked.
                fresh(receipt.observed_us, 0, now)?;
            }
            require(
                probe.runtime != setup::RuntimeState::Ready
                    || matches!(
                        probe.voice_capability(),
                        Some(
                            startup::Capability::Ready
                                | startup::Capability::Listening
                                | startup::Capability::Inhibited
                        )
                    ),
            )?;
            Ok(())
        }
        (Command::Update(action), Reply::Update(receipt)) => {
            require(
                receipt.operation_id == action.operation_id
                    && receipt.step_id == action.step_id
                    && receipt.clock_id == action.clock_id
                    && receipt.coordinator_session == action.coordinator_session
                    && receipt.guard == action.guard,
            )?;
            require(receipt.observed_us <= response.observed_us)?;
            fresh(receipt.observed_us, request.issued_us, now)?;
            // Do not expose arbitrary owner errors in status/debug output. The
            // update FSM still receives an explicit failure and recovers safely.
            if let update::Evidence::Failed { reason } = &receipt.evidence {
                require(reason == "owner action failed")?;
            }
            Ok(())
        }
        (Command::Reset(action), Reply::Reset(receipt)) => {
            require(
                receipt.operation_id == action.operation_id
                    && receipt.request_id == action.request_id
                    && receipt.clock_id == action.clock_id
                    && receipt.coordinator_session == action.coordinator_session
                    && identity(&receipt.owner_session),
            )?;
            require(receipt.observed_us <= response.observed_us)?;
            fresh(receipt.observed_us, request.issued_us, now)
        }
        (
            Command::Configure {
                operation_id,
                expected,
                target,
            },
            Reply::Configure(receipt),
        ) => {
            require(
                receipt.operation_id == *operation_id
                    && receipt.expected == *expected
                    && receipt.target == *target,
            )?;
            require(receipt.observed_us <= response.observed_us)?;
            fresh(receipt.observed_us, request.issued_us, now)?;
            receipt.validate(request.issued_us, now)
        }
        _ => Err(Error::InvalidReply),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn socket_owned_by_a_different_expected_uid_is_refused_before_connecting() {
        let temp = tempfile::Builder::new()
            .prefix("owner-uid-")
            .tempdir_in("/tmp")
            .unwrap();
        let parent = temp.path().canonicalize().unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = parent.join("supervisor.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let current_uid = rustix::process::geteuid().as_raw();
        let mut client = Client::new(path, true, "boot".into(), "session".into()).unwrap();
        // Simulate the exact foreign-owner mismatch without privileged chown.
        client.owner_uid = current_uid.wrapping_add(1);
        assert_eq!(
            client.call("probe", Command::Probe, Arc::new(|| 1)).await,
            Err(Error::UnsafeEndpoint)
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(10), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn real_peer_credentials_must_match_the_expected_uid() {
        let (client, _server) = UnixStream::pair().unwrap();
        let current_uid = rustix::process::geteuid().as_raw();
        assert_eq!(check_peer(&client, current_uid), Ok(()));
        assert_eq!(
            check_peer(&client, current_uid.wrapping_add(1)),
            Err(Error::UnsafeEndpoint)
        );
    }
}
