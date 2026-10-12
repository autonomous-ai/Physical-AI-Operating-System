//! Optional motor ownership; no gesture producer or placement inference.
//!
//! Prepare files before audio starts. Spawn once in the pre-capture startup
//! slice, only from an original allowed snapshot. Subsequent operations are
//! nonblocking; the unchanged motor supervisor owns bus deadlines and reaping.
use crate::options::MotorOptions;
use lamp_interaction::{
    BootId, BoundaryGuard, Error as AuthorityError, MonoTime, OutputKind, OutputPermit, Permission,
    Snapshot,
};
use lamp_ipc::monotonic_us;
use lamp_motor::command::JointSelection;
use lamp_motor::runtime::{
    config::Config,
    supervisor::{Supervisor, new_boot},
    wire::{Command, Control, Event, Motion, Observation},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const MAX_EXECUTABLE_BYTES: u64 = 64 * 1024 * 1024;
/// A per-instance resource bound, not a gesture frequency or safety policy.
pub const MAX_OFFERS: u64 = 128;
const POLL_BUDGET: usize = 8;

pub(super) fn bounded(value: impl ToString) -> String {
    let value = value.to_string();
    let mut end = value.len().min(192);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
fn invalid(message: impl ToString) -> io::Error {
    io::Error::other(message.to_string())
}
fn time() -> MonoTime {
    MonoTime::from_micros(monotonic_us())
}

#[derive(Clone, Debug, Serialize)]
pub struct Identity {
    pub worker: BootId,
    pub config_sha256: String,
    pub executable_sha256: String,
    pub qualification_record: [u8; 32],
    pub placement_source: BootId,
}

struct FrozenFile {
    path: PathBuf,
    device: u64,
    inode: u64,
    remove: bool,
}
impl FrozenFile {
    fn copy(
        source: &Path,
        destination: PathBuf,
        maximum: u64,
        executable: bool,
    ) -> io::Result<(Self, String)> {
        lamp_motor::runtime::config::bounded_path(source)?;
        let fd = rustix::fs::open(
            source,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        let mut input = File::from(fd);
        let before = input.metadata()?;
        if !before.is_file()
            || before.uid() != rustix::process::geteuid().as_raw()
            || before.nlink() != 1
            || before.len() == 0
            || before.len() > maximum
            || before.mode() & if executable { 0o022 } else { 0o077 } != 0
            || (executable && before.mode() & 0o100 == 0)
        {
            return Err(invalid(
                "motor source must be a bounded owned regular file with safe permissions",
            ));
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(if executable { 0o700 } else { 0o600 })
            .open(&destination)?;
        let meta = output.metadata()?;
        let frozen = Self {
            path: destination,
            device: meta.dev(),
            inode: meta.ino(),
            remove: true,
        };
        let mut hash = Sha256::new();
        let mut bytes = [0u8; 16_384];
        let mut total = 0u64;
        loop {
            let n = input.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            total = total
                .checked_add(n as u64)
                .ok_or_else(|| invalid("motor file length overflow"))?;
            if total > maximum {
                return Err(invalid("motor source grew past its bound"));
            }
            output.write_all(&bytes[..n])?;
            hash.update(&bytes[..n]);
        }
        let after = input.metadata()?;
        if total != before.len()
            || after.len() != before.len()
            || after.mtime() != before.mtime()
            || after.mtime_nsec() != before.mtime_nsec()
            || after.ctime() != before.ctime()
            || after.ctime_nsec() != before.ctime_nsec()
        {
            return Err(invalid("motor provisioning changed while frozen"));
        }
        output.sync_all()?;
        Ok((frozen, format!("{:x}", hash.finalize())))
    }
}
impl Drop for FrozenFile {
    fn drop(&mut self) {
        if self.remove
            && fs::symlink_metadata(&self.path)
                .is_ok_and(|m| m.is_file() && m.dev() == self.device && m.ino() == self.inode)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Immutable, private launch inputs. The caller selects the exact executable;
/// neither PATH search nor a legacy fallback is used. No device is opened here.
pub struct PreparedMotor {
    executable: FrozenFile,
    config_file: FrozenFile,
    config: Config,
    directory: PathBuf,
    identity: Identity,
}
impl PreparedMotor {
    pub fn load(options: &MotorOptions, directory: &Path) -> io::Result<Self> {
        let meta = fs::symlink_metadata(directory)?;
        if !meta.is_dir()
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o077 != 0
        {
            return Err(invalid("motor launch directory must be private and owned"));
        }
        let (config_file, config_sha256) =
            FrozenFile::copy(&options.config, directory.join("motor.json"), 16_384, false)?;
        let config = Config::read(&config_file.path)?;
        let at = monotonic_us();
        if !(config.qualification.valid_from_us <= at && at < config.qualification.expires_at_us) {
            return Err(invalid("motor qualification is not currently valid"));
        }
        let (executable, executable_sha256) = FrozenFile::copy(
            &options.worker,
            directory.join("motor-worker"),
            MAX_EXECUTABLE_BYTES,
            true,
        )?;
        let identity = Identity {
            worker: new_boot()?,
            config_sha256,
            executable_sha256,
            qualification_record: config.qualification.record,
            placement_source: config.placement_source,
        };
        Ok(Self {
            executable,
            config_file,
            config,
            directory: directory.join("motor"),
            identity,
        })
    }
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
}

/// A trusted producer must supply actual placement/clearance qualification.
/// A CameraGrant or ordinary scene description is not that qualification.
#[derive(Clone, Copy)]
pub struct Evidence {
    pub request: u64,
    pub requested_at_us: u64,
    pub permit: OutputPermit,
    pub placement: Observation,
}
#[derive(Clone, Copy)]
pub struct MovementOffer {
    pub selection: JointSelection,
    pub evidence: Evidence,
    pub target: [f64; 5],
    pub duration_us: u64,
}
#[derive(Clone, Copy)]
struct Pending {
    selection: Option<JointSelection>,
    evidence: Evidence,
    inspection: bool,
}
#[derive(Clone, Copy)]
struct Active {
    selection: JointSelection,
    motion: Motion,
    evidence: Evidence,
}

/// At most one pending request/active motion, one child, and one failure latch.
/// There is intentionally no replacement/retry/restart method.
pub struct OptionalMotor {
    prepared: Option<PreparedMotor>,
    child: Option<Supervisor>,
    parent: BootId,
    guard: BoundaryGuard,
    last_sent_revision: Option<u64>,
    expected_privacy_revision: Option<u64>,
    disabled: bool,
    stopping: bool,
    failure: Option<String>,
    failure_reported: bool,
    pending: Option<Pending>,
    active: Option<Active>,
    inspected_at_us: Option<u64>,
    last_request: u64,
    offers: u64,
    notice_count: u64,
}
impl OptionalMotor {
    pub fn new(parent: BootId, prepared: Option<PreparedMotor>) -> Self {
        Self {
            prepared,
            child: None,
            parent,
            guard: BoundaryGuard::new(parent, time()),
            last_sent_revision: None,
            expected_privacy_revision: None,
            disabled: false,
            stopping: false,
            failure: None,
            failure_reported: false,
            pending: None,
            active: None,
            inspected_at_us: None,
            last_request: 0,
            offers: 0,
            notice_count: 0,
        }
    }
    pub fn identity(&self) -> Option<&Identity> {
        self.prepared.as_ref().map(PreparedMotor::identity)
    }
    pub fn is_ready(&self) -> bool {
        !self.disabled && !self.stopping && self.child.as_ref().is_some_and(Supervisor::is_ready)
    }
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    pub fn reaped(&self) -> bool {
        self.child.as_ref().is_none_or(Supervisor::reaped)
    }
    pub fn started(&self) -> bool {
        self.child.is_some()
    }
    pub fn take_failure(&mut self) -> Option<&str> {
        if self.failure_reported {
            return None;
        }
        self.failure_reported = self.failure.is_some();
        self.failure.as_deref()
    }
    /// Exactly one startup attempt, before microphone StartCapture. No explicit
    /// denial is suppressed, and missing configured camera leaves motors absent.
    pub fn start_before_capture(
        &mut self,
        snapshot: Snapshot,
        capture_started: bool,
        camera_available: bool,
    ) {
        if self.child.is_some() || self.disabled || self.prepared.is_none() {
            return;
        }
        if capture_started
            || !camera_available
            || snapshot.microphone_permission() != Permission::Allowed
            || snapshot.camera_permission() != Permission::Allowed
        {
            self.disable("motor not started: original permissions/configured camera unavailable");
            return;
        }
        if let Err(error) = self.install(snapshot) {
            self.disable(error);
            return;
        }
        let prepared = self.prepared.as_ref().expect("prepared");
        let at = monotonic_us();
        if at < prepared.config.qualification.valid_from_us
            || at >= prepared.config.qualification.expires_at_us
        {
            self.disable("motor qualification expired before startup");
            return;
        }
        match Supervisor::spawn(
            &prepared.executable.path,
            &prepared.config_file.path,
            &prepared.directory,
            self.parent,
            prepared.identity.worker,
        ) {
            Ok(child) => {
                self.child = Some(child);
                self.publish(snapshot);
            }
            Err(error) => self.disable(format!("motor startup failed: {error}")),
        }
    }
    fn install(&mut self, snapshot: Snapshot) -> io::Result<bool> {
        if self
            .guard
            .state()
            .is_some_and(|s| s.revision() == snapshot.revision() && s != snapshot)
        {
            return Err(invalid("conflicting motor authority revision"));
        }
        match self.guard.install(time(), snapshot) {
            Ok(()) => Ok(true),
            Err(AuthorityError::StaleSnapshot) => Ok(false),
            Err(AuthorityError::ExpiredState)
                if self
                    .guard
                    .state()
                    .is_some_and(|current| snapshot.revision() <= current.revision()) =>
            {
                Ok(false)
            }
            Err(error) => Err(invalid(error)),
        }
    }
    /// Priority path, called before audio sends. Never waits for child exit.
    pub fn publish(&mut self, snapshot: Snapshot) {
        if self.child.is_none() || self.disabled || self.stopping {
            return;
        }
        match self.install(snapshot) {
            Ok(false) if self.guard.state() != Some(snapshot) => return,
            Err(error) => {
                self.disable(error);
                return;
            }
            _ => {}
        }
        let permitted = snapshot.microphone_permission() == Permission::Allowed
            && snapshot.camera_permission() == Permission::Allowed;
        // Revocation is sent unchanged before terminal Stop. A failed priority
        // send invokes the existing supervisor's exact-child kill path.
        let sent = self.send_authority(snapshot);
        if !permitted && sent.is_ok() && self.last_sent_revision == Some(snapshot.revision()) {
            self.expected_privacy_revision = Some(snapshot.revision());
        }
        if !permitted || sent.is_err() {
            self.disable(
                sent.err()
                    .map_or_else(|| "motor privacy revoked".into(), |e| e.to_string()),
            );
            return;
        }
        let evidence = self
            .pending
            .map(|p| p.evidence)
            .or(self.active.map(|a| a.evidence));
        if let Some(evidence) = evidence
            && self
                .guard
                .check(time(), evidence.permit, OutputKind::Motion)
                .is_err()
        {
            self.disable("original motor owner/permit was revoked");
        }
    }
    fn send_authority(&mut self, snapshot: Snapshot) -> io::Result<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        if child.connected() && self.last_sent_revision != Some(snapshot.revision()) {
            child.send_control(&Control::Authority { snapshot })?;
            self.last_sent_revision = Some(snapshot.revision());
        }
        Ok(())
    }
    fn disable(&mut self, reason: impl ToString) {
        self.disabled = true;
        self.pending = None;
        self.active = None;
        self.inspected_at_us = None;
        if self.failure.is_none() {
            self.failure = Some(bounded(reason.to_string()));
        }
        self.begin_shutdown();
    }
    pub fn begin_shutdown(&mut self) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        self.pending = None;
        self.active = None;
        self.inspected_at_us = None;
        if let Some(child) = self.child.as_mut()
            && let Err(error) = child.stop()
            && self.failure.is_none()
        {
            self.failure = Some(bounded(error.to_string()));
        }
    }
    /// At most eight receipt operations. Progress and per-frame host writes do
    /// not become unbounded trace traffic or grant conversational readiness.
    pub fn poll(&mut self, mut notice: impl FnMut(&Event)) {
        for _ in 0..POLL_BUDGET {
            let Some(child) = self.child.as_mut() else {
                return;
            };
            match child.poll() {
                Ok(Some(event)) => {
                    if let Err(error) = self.observe(&event) {
                        self.disable(error);
                    }
                    if !matches!(event, Event::Progress { .. } | Event::HostAccepted { .. })
                        && self.notice_count < MAX_OFFERS * 4 + 8
                    {
                        self.notice_count += 1;
                        notice(&event);
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    self.disable(error);
                    break;
                }
            }
        }
        if self.disabled || self.stopping {
            return;
        }
        if let Some(snapshot) = self.guard.state() {
            if snapshot.expires_at() <= time() {
                self.disable("motor authority expired");
                return;
            }
            if let Err(error) = self.send_authority(snapshot) {
                self.disable(error);
            }
        }
    }
    fn observe(&mut self, event: &Event) -> io::Result<()> {
        if self.disabled || self.stopping {
            return Ok(());
        }
        match *event {
            Event::ProfileStarted { .. } | Event::ProfilePrepared { .. } => {
                return Err(invalid(
                    "setup-only profile receipt in ordinary gesture worker",
                ));
            }
            Event::Ready { .. } | Event::Progress { .. } => {}
            Event::Inspected {
                request,
                at_us,
                readback,
            } => {
                self.validate_readback(readback, at_us)?;
                if !self
                    .pending
                    .is_some_and(|p| p.inspection && p.evidence.request == request)
                {
                    return Err(invalid("unsolicited motor inspection receipt"));
                }
                self.pending = None;
                self.inspected_at_us = Some(at_us);
            }
            Event::Started {
                request, motion, ..
            } => {
                let pending = self
                    .pending
                    .filter(|p| {
                        !p.inspection
                            && p.evidence.request == request
                            && p.evidence.permit.owner() == motion.owner
                    })
                    .ok_or_else(|| invalid("motor Started has no original offered owner"))?;
                self.guard
                    .check(time(), pending.evidence.permit, OutputKind::Motion)
                    .map_err(invalid)?;
                self.pending = None;
                self.active = Some(Active {
                    selection: pending
                        .selection
                        .ok_or_else(|| invalid("motion selection absent"))?,
                    motion,
                    evidence: pending.evidence,
                });
            }
            Event::HostAccepted {
                motion, selection, ..
            } => {
                if !self
                    .active
                    .is_some_and(|a| a.motion == motion && a.selection == selection)
                {
                    return Err(invalid("motor write receipt has no original active motion"));
                }
            }
            Event::ReadbackReached {
                motion,
                at_us,
                readback,
            } => {
                self.validate_readback(readback, at_us)?;
                if !self.active.is_some_and(|a| a.motion == motion) {
                    return Err(invalid(
                        "motor reached receipt has no original active motion",
                    ));
                }
                self.active = None;
                // A new explicit inspection is required for another offer.
                self.inspected_at_us = None;
            }
            Event::Rejected { request, .. } => {
                if self.pending.is_none_or(|p| p.evidence.request != request) {
                    return Err(invalid("motor rejection has no offered request"));
                }
                self.pending = None;
            }
            Event::CancelRejected { .. } => {}
            Event::Inhibited { .. } | Event::Stopped { .. } | Event::Fault { .. } => {
                return Err(invalid("motor worker ended independently"));
            }
        }
        Ok(())
    }
    fn validate_readback(
        &self,
        value: lamp_motor::control::Readback,
        at_us: u64,
    ) -> io::Result<()> {
        let age = self
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("motor config absent"))?
            .config
            .timing
            .readback_age_us;
        value
            .validate(lamp_interaction::MonoTime::from_micros(at_us), age)
            .map_err(invalid)?;
        value.validate(time(), age).map_err(invalid)
    }
    fn validate_offer(&mut self, e: Evidence) -> io::Result<()> {
        if !self.is_ready()
            || self.pending.is_some()
            || self.active.is_some()
            || self.offers >= MAX_OFFERS
            || e.request == 0
            || e.request <= self.last_request
        {
            return Err(invalid(
                "motor unavailable, busy, or request bound violated",
            ));
        }
        let at = time();
        self.guard
            .check(at, e.permit, OutputKind::Motion)
            .map_err(invalid)?;
        let config = &self.prepared.as_ref().expect("started config").config;
        let p = e.placement;
        if at
            .as_micros()
            .checked_sub(e.requested_at_us)
            .is_none_or(|age| age > lamp_motor::runtime::channel::MAX_AGE_US)
            || config.qualification.valid_from_us > at.as_micros()
            || config.qualification.expires_at_us <= at.as_micros()
            || p.source_boot != config.placement_source
            || p.qualification_record != config.qualification.record
            || p.frame_sequence == 0
            || at
                .as_micros()
                .checked_sub(p.acquired_at_us)
                .is_none_or(|age| age > config.timing.placement_age_us)
            || p.valid_until_us <= at.as_micros()
            || p.acquired_at_us >= p.valid_until_us
            || p.valid_until_us.saturating_sub(p.acquired_at_us) > config.timing.placement_age_us
            || Some(p.camera) != e.permit.camera()
            || self.guard.current_camera_grant(at).map_err(invalid)? != p.camera
        {
            return Err(invalid(
                "original motor offer/placement/qualification evidence is invalid or stale",
            ));
        }
        Ok(())
    }
    /// Explicit trusted readback request; never emitted automatically on Ready.
    pub fn offer_inspection(&mut self, evidence: Evidence) -> io::Result<()> {
        self.validate_offer(evidence)?;
        self.send_offer(
            evidence,
            true,
            Command::Inspect {
                request: evidence.request,
                requested_at_us: evidence.requested_at_us,
            },
        )
    }
    pub fn offer_segment(&mut self, offer: MovementOffer) -> io::Result<()> {
        let e = offer.evidence;
        self.validate_offer(e)?;
        let config = &self.prepared.as_ref().expect("started config").config;
        if self.inspected_at_us.is_none_or(|at| {
            monotonic_us()
                .checked_sub(at)
                .is_none_or(|age| age > config.timing.readback_age_us)
        }) {
            return Err(invalid("motor needs explicit fresh readback"));
        }
        let finish = monotonic_us()
            .checked_add(offer.duration_us)
            .and_then(|v| v.checked_add(config.timing.transaction_us))
            .ok_or_else(|| invalid("motor segment deadline overflow"))?;
        if offer.duration_us == 0
            || finish >= e.permit.expires_at().as_micros()
            || finish >= e.placement.valid_until_us
            || finish >= config.qualification.expires_at_us
            || offer
                .target
                .iter()
                .zip(config.qualification.joints)
                .any(|(value, limit)| {
                    !value.is_finite() || *value < limit.minimum || *value > limit.maximum
                })
        {
            return Err(invalid("motor segment exceeds original evidence/range"));
        }
        self.send_offer(
            e,
            false,
            Command::SelectedSegment {
                selection: offer.selection,
                request: e.request,
                requested_at_us: e.requested_at_us,
                permit: e.permit,
                placement: e.placement,
                target: offer.target,
                duration_us: offer.duration_us,
            },
        )
    }
    fn send_offer(
        &mut self,
        evidence: Evidence,
        inspection: bool,
        command: Command,
    ) -> io::Result<()> {
        if matches!(command, Command::PrepareHeadProfile { .. }) {
            return Err(invalid(
                "profile setup requires its explicit qualification controller",
            ));
        }
        // No retry queue. Final serial checks remain in the unchanged worker.
        self.guard
            .check(time(), evidence.permit, OutputKind::Motion)
            .map_err(invalid)?;
        if let Err(error) = self
            .child
            .as_mut()
            .expect("ready child")
            .send_command(&command)
        {
            self.disable(&error);
            return Err(error);
        }
        self.last_request = evidence.request;
        self.offers += 1;
        self.pending = Some(Pending {
            selection: match command {
                Command::Inspect { .. } | Command::PrepareHeadProfile { .. } => None,
                Command::Segment { .. } => Some(JointSelection::ALL),
                Command::SelectedSegment { selection, .. } => Some(selection),
            },
            evidence,
            inspection,
        });
        Ok(())
    }
    /// A stale callback cannot stop a newer owner. Pending work is cancelled by
    /// publishing revocation or Stop, not by inventing a future Motion identity.
    pub fn cancel(&mut self, motion: Motion) -> io::Result<()> {
        if !self.active.is_some_and(|active| active.motion == motion) || !self.is_ready() {
            return Err(invalid("stale motor cancellation"));
        }
        self.disabled = true;
        self.active = None;
        self.pending = None;
        self.stopping = true;
        self.child
            .as_mut()
            .expect("active child")
            .send_control(&Control::Cancel { motion })
    }
    /// Reap + explicit local inhibition/serial cleanup is mandatory. This stays
    /// an error after optional degradation; it must block service reopen.
    pub fn finish(&self) -> io::Result<()> {
        if let Some(child) = &self.child
            && (!child.reaped() || !(child.clean_exit() || self.privacy_released()))
        {
            return Err(invalid(format!(
                "motor cleanup unconfirmed: {:?}; {:?}",
                child.failure(),
                child.ipc_cleanup_error()
            )));
        }
        Ok(())
    }
    fn privacy_released(&self) -> bool {
        self.child.as_ref().is_some_and(|child| {
            self.expected_privacy_revision
                .is_some_and(|revision| child.privacy_release_confirmed(revision))
        })
    }
    /// A normal verified physical-mute cycle can reopen idle, with fresh worker
    /// identity and no copied offers. A degraded optional component cannot.
    pub fn may_rearm_after_cycle(&self, physical_mute: bool) -> bool {
        self.finish().is_ok()
            && (self.failure.is_none() || (physical_mute && self.privacy_released()))
    }
}

impl Drop for OptionalMotor {
    fn drop(&mut self) {
        // Unexpected unwinding is not a successful reap. Preserve launch files
        // and make the enclosing private-directory cleanup fail closed.
        if !self.reaped()
            && let Some(prepared) = self.prepared.as_mut()
        {
            prepared.executable.remove = false;
            prepared.config_file.remove = false;
        }
    }
}

#[cfg(test)]
mod tests;
