//! Optional motor ownership; no gesture producer or placement inference.
//!
//! Prepare files before audio starts. Spawn once in the pre-capture startup
//! slice, only from an original allowed snapshot. Subsequent operations are
//! nonblocking; the unchanged motor supervisor owns bus deadlines and reaping.
use crate::options::MotorOptions;
use lamp_body::command::JointSelection;
use lamp_body::runtime::{
    config::Config,
    qualification::{HeadPlan, HeadQualification, OriginalEvidence, Phase},
    supervisor::{Supervisor, new_boot},
    wire::{Command, Control, Event, Motion, Observation},
};
// Stepped fixtures drive this module, its Supervisor and its worker on scenario time.
#[cfg(test)]
use crate::scenario_clock::monotonic_us;
#[cfg(not(test))]
use lamp_ipc::monotonic_us;
use lamp_turn_owner::{
    BootId, BoundaryGuard, Error as AuthorityError, MonoTime, OutputKind, OutputPermit, Permission,
    Snapshot,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

mod head;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement_binding: Option<PlacementBinding>,
}

/// Explicit trusted provisioning. Only placement_source may change, before spawn.
#[derive(Clone, Debug, Serialize)]
pub struct PlacementBinding {
    pub original_sha256: String,
    pub original_source: BootId,
    pub assigned_source: BootId,
    pub record: [u8; 32],
    pub review_sha256: String,
}
fn bind_placement(config: &mut Config, hash: &str, binding: &PlacementBinding) -> io::Result<()> {
    if hash != binding.original_sha256
        || config.placement_source != binding.original_source
        || config.qualification.record != binding.record
        || binding.review_sha256.len() != 64
    {
        return Err(invalid(
            "head placement binding does not match original provisioned config",
        ));
    }
    let before = serde_json::to_value(&config).map_err(invalid)?;
    config.placement_source = binding.assigned_source;
    config.validate()?;
    let mut after = serde_json::to_value(&config).map_err(invalid)?;
    after["placement_source"] = before["placement_source"].clone();
    if before != after {
        return Err(invalid("head binding changed another configuration field"));
    }
    Ok(())
}
struct FrozenFile {
    path: PathBuf,
    device: u64,
    inode: u64,
    remove: bool,
}
impl FrozenFile {
    fn write(destination: PathBuf, bytes: &[u8]) -> io::Result<(Self, String)> {
        if bytes.is_empty() || bytes.len() > 16_384 {
            return Err(invalid("bound motor config size"));
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&destination)?;
        let meta = output.metadata()?;
        let frozen = Self {
            path: destination,
            device: meta.dev(),
            inode: meta.ino(),
            remove: true,
        };
        output.write_all(bytes)?;
        output.sync_all()?;
        Ok((frozen, format!("{:x}", Sha256::digest(bytes))))
    }

    fn copy(
        source: &Path,
        destination: PathBuf,
        maximum: u64,
        executable: bool,
    ) -> io::Result<(Self, String)> {
        lamp_body::runtime::config::bounded_path(source)?;
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
        Self::load_bound(options, directory, None)
    }
    pub(crate) fn load_bound(
        options: &MotorOptions,
        directory: &Path,
        binding: Option<PlacementBinding>,
    ) -> io::Result<Self> {
        let meta = fs::symlink_metadata(directory)?;
        if !meta.is_dir()
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o077 != 0
        {
            return Err(invalid("motor launch directory must be private and owned"));
        }
        let (mut config_file, mut config_sha256) =
            FrozenFile::copy(&options.config, directory.join("motor.json"), 16_384, false)?;
        let mut config = Config::read(&config_file.path)?;
        if let Some(binding) = &binding {
            bind_placement(&mut config, &config_sha256, binding)?;
            let bytes = serde_json::to_vec(&config).map_err(invalid)?;
            (config_file, config_sha256) =
                FrozenFile::write(directory.join("motor-bound.json"), &bytes)?;
        }
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
            placement_binding: binding,
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
    pub(crate) head: Option<crate::head_qualification::Driver>,
    head_requested: bool,
    head_profile: Option<head::ProfileSetup>,
    head_plan: Option<HeadPlan>,
    qualification: Option<HeadQualification>,
    qualification_evidence: Option<OriginalEvidence>,
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
            head: None,
            head_requested: false,
            head_profile: None,
            head_plan: None,
            qualification: None,
            qualification_evidence: None,
        }
    }
    pub(crate) fn attach_head(
        &mut self,
        driver: crate::head_qualification::Driver,
    ) -> io::Result<()> {
        let config = &self
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("head motor absent"))?
            .config;
        driver.plan.head.validate(&config.validate()?.calibration)?;
        if driver.plan.head.qualification_record != config.qualification.record
            || driver.camera_worker != config.placement_source
        {
            return Err(invalid("head plan binding mismatch"));
        }
        self.head_requested = true;
        self.head_profile = config
            .qualification
            .selected_profiles
            .as_ref()
            .and_then(|s| s.prepare_from)
            .map(|_| head::ProfileSetup::new());
        self.head_plan = Some(driver.plan.head.clone());
        self.head = Some(driver);
        Ok(())
    }
    pub(crate) fn placement_age_us(&self) -> io::Result<u64> {
        Ok(self
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("motor config absent"))?
            .config
            .timing
            .placement_age_us)
    }
    pub(crate) fn qualification_phase(&self) -> Phase {
        self.qualification.as_ref().map_or(
            if self.disabled || self.stopping {
                Phase::Inhibited
            } else {
                Phase::New
            },
            HeadQualification::phase,
        )
    }
    pub(crate) fn qualification_action(
        &mut self,
        plan: &HeadPlan,
        action: crate::head_qualification::Action,
        snapshot: Snapshot,
        permit: OutputPermit,
        placement: Observation,
    ) -> io::Result<()> {
        let result = (|| {
            if !self.head_requested
                || !self.is_ready()
                || self.pending.is_some()
                || self.active.is_some()
                || self.qualification_stage() != action.expected()
            {
                return Err(invalid("head action unavailable or out of phase"));
            }
            self.publish(snapshot);
            if !self.is_ready() {
                return Err(invalid("head authority unavailable"));
            }
            if matches!(
                action,
                crate::head_qualification::Action::InspectProfile
                    | crate::head_qualification::Action::PrepareProfile
            ) {
                return self.profile_action(action, snapshot, permit, placement);
            }
            if self.head_profile.as_ref().is_some_and(|s| !s.complete) {
                return Err(invalid("profile setup incomplete"));
            }
            if self.qualification.is_none() {
                let prepared = self.prepared.as_ref().expect("ready config");
                let mut q = HeadQualification::new(
                    plan.clone(),
                    prepared.config.validate()?.calibration,
                    self.parent,
                    prepared.identity.worker,
                    prepared.config.placement_source,
                    time(),
                )?;
                if self.head_profile.is_some() {
                    q = q.after_profile_preparation()?;
                }
                self.qualification = Some(q);
            }
            let q = self.qualification.as_mut().expect("created above");
            let child = self
                .child
                .as_mut()
                .ok_or_else(|| invalid("head worker absent"))?;
            q.authority(time(), snapshot)?;
            let evidence = OriginalEvidence { permit, placement };
            match action {
                crate::head_qualification::Action::Inspect
                | crate::head_qualification::Action::Reinspect => {
                    q.send_inspection(child, time(), evidence)?
                }
                crate::head_qualification::Action::Outbound
                | crate::head_qualification::Action::Return => {
                    q.send_segment(child, time(), evidence)?
                }
                _ => return Err(invalid("setup action crossed qualification path")),
            }
            self.qualification_evidence = Some(evidence);
            Ok(())
        })();
        if let Err(error) = &result {
            self.disable(error);
        }
        result
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
        match Supervisor::spawn_with_clock(
            &prepared.executable.path,
            &prepared.config_file.path,
            &prepared.directory,
            self.parent,
            prepared.identity.worker,
            time,
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
        if self
            .head_profile
            .as_ref()
            .is_some_and(|s| s.owner.is_some_and(|o| snapshot.owner() != Some(o)))
        {
            self.disable("original profile owner revoked");
            return;
        }
        if let Some(q) = &mut self.qualification
            && let Err(error) = q.authority(time(), snapshot)
        {
            self.disable(error);
            return;
        }
        let evidence = self
            .pending
            .map(|p| p.evidence)
            .or(self.active.map(|a| a.evidence));
        if let Some(e) = self.head_profile.as_ref().and_then(|s| s.evidence)
            && self
                .guard
                .check(time(), e.permit, OutputKind::Motion)
                .is_err()
        {
            self.disable("original profile permit revoked/expired");
            return;
        }
        if let Some(e) = self.qualification_evidence
            && self
                .guard
                .check(time(), e.permit, OutputKind::Motion)
                .is_err()
        {
            self.disable("original head permit revoked/expired");
            return;
        }
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
        if let Some(q) = &mut self.qualification
            && q.phase() != Phase::Returned
        {
            q.stop();
        }
        self.qualification_evidence = None;
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
        if !self.disabled
            && !self.stopping
            && let Some(q) = &mut self.qualification
            && let Err(error) = q.maintain(time())
        {
            self.disable(error);
        }
        for _ in 0..POLL_BUDGET {
            let Some(child) = self.child.as_mut() else {
                return;
            };
            match child.poll() {
                Ok(Some(event)) => {
                    if let Err(error) = self.observe(&event) {
                        self.disable(error);
                    }
                    if (!matches!(event, Event::Progress { .. } | Event::HostAccepted { .. })
                        || (self.head_requested && matches!(event, Event::HostAccepted { .. })))
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
        // The supervisor may discover an exit, deadline or cleanup fault after
        // reading its last receipt and still return Ok(None). Adopt that stored
        // failure even during teardown; a fresh authority send is not required
        // to inhibit this optional output or report its degradation once.
        if self.failure.is_none()
            && let Some(reason) = self
                .child
                .as_ref()
                .and_then(Supervisor::failure)
                .map(bounded)
        {
            self.disable(reason);
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
        if self.qualification.is_none() && self.head_profile.as_ref().is_some_and(|s| !s.complete) {
            return self.profile_observe(event);
        }
        if let Some(q) = &mut self.qualification {
            q.observe(time(), event)?;
            if matches!(
                q.phase(),
                Phase::Initial | Phase::AtOutbound | Phase::ReturnReady | Phase::Returned
            ) {
                self.qualification_evidence = None;
            }
            if q.phase() == Phase::Returned {
                self.begin_shutdown();
            }
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
    fn validate_readback(&self, value: lamp_body::control::Readback, at_us: u64) -> io::Result<()> {
        let age = self
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("motor config absent"))?
            .config
            .timing
            .readback_age_us;
        value
            .validate(lamp_turn_owner::MonoTime::from_micros(at_us), age)
            .map_err(invalid)?;
        value.validate(time(), age).map_err(invalid)
    }
    fn validate_offer(&mut self, e: Evidence) -> io::Result<()> {
        if self.head_requested
            || !self.is_ready()
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
            .is_none_or(|age| age > lamp_body::runtime::channel::MAX_AGE_US)
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
    /// A verified physical/software mute cycle can reopen idle, with fresh worker
    /// identity and no copied offers. A degraded optional component cannot.
    pub fn may_rearm_after_cycle(&self, privacy_mute: bool) -> bool {
        self.finish().is_ok()
            && (self.failure.is_none() || (privacy_mute && self.privacy_released()))
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
