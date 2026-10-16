//! Explicit attended qualification over the live controller's actual authority.
//! Static clearance is provisioned by an operator; fresh frames prove only
//! camera lineage/freshness. They never infer clearance or grant ownership.
#[cfg(test)]
use crate::scenario_clock::monotonic_us;
use crate::{
    camera_worker::FrameMetadata,
    coordinator::motor::{OptionalMotor, PlacementBinding},
    options::HeadOptions,
};
use lamp_body::runtime::{
    qualification::{HeadPlan, Phase},
    wire::Observation,
};
#[cfg(not(test))]
use lamp_ipc::monotonic_us;
use lamp_ipc::{Endpoint, MAX_DATAGRAM_BYTES};
use lamp_turn_owner::{BootId, Controller, MonoTime, OutputKind, TurnOwner};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
};

/// Full existing inspector validation with one retained original frame. Frame
/// link metadata is transport-only and never becomes movement evidence.
pub(crate) struct Frames {
    observer: Option<crate::camera_inspect::Observer>,
    worker: BootId,
    report: crate::camera_inspect::MetadataReport,
    pub latest: Option<FrameMetadata>,
}
impl Frames {
    pub fn new(
        worker: BootId,
        config: crate::camera_config::CameraConfig,
        digest: String,
    ) -> io::Result<Self> {
        let at = monotonic_us();
        Ok(Self {
            observer: None,
            worker,
            report: crate::camera_inspect::MetadataReport::new(config, digest, 1, at)?,
            latest: None,
        })
    }
    pub fn accept(
        &mut self,
        event: crate::camera_worker::CameraEvent,
        grant: Option<lamp_turn_owner::CameraGrant>,
        at: u64,
    ) -> io::Result<()> {
        if matches!(event, crate::camera_worker::CameraEvent::FrameLink { .. }) {
            return Ok(());
        }
        let frame = match &event {
            crate::camera_worker::CameraEvent::Frame { frame } => Some(frame.clone()),
            _ => None,
        };
        self.observer
            .get_or_insert_with(|| crate::camera_inspect::Observer::new(self.worker, at))
            .accept(event, grant, at, &mut self.report)?;
        if frame.is_some() {
            self.latest = frame;
        }
        Ok(())
    }
}

pub const CONTROL_AGE_US: u64 = 100_000;
const MAX_REQUESTS: u64 = 4096;
fn invalid(message: impl ToString) -> io::Error {
    io::Error::other(message.to_string())
}
fn time(us: u64) -> MonoTime {
    MonoTime::from_micros(us)
}
fn hash_valid(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()) && s.bytes().any(|c| c != b'0')
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub head: HeadPlan,
    pub motor_config_sha256: String,
    pub camera_config_sha256: String,
    pub original_placement_source: BootId,
    /// Provisioner's immutable review record, not automatic visual clearance.
    pub placement_review_sha256: String,
    pub kernel_boot_id: String,
    pub reviewed_at_us: u64,
    pub valid_until_us: u64,
}
impl Plan {
    fn check(&self, camera_hash: &str, host_boot: &str, now: u64) -> io::Result<()> {
        if !hash_valid(&self.motor_config_sha256)
            || !hash_valid(&self.camera_config_sha256)
            || !hash_valid(&self.placement_review_sha256)
            || self.camera_config_sha256 != camera_hash
            || self.kernel_boot_id != host_boot
            || self.reviewed_at_us > now
            || self.valid_until_us <= now
            || self.reviewed_at_us >= self.valid_until_us
        {
            return Err(invalid(
                "head placement review/config/host identity missing or expired",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    InspectProfile,
    PrepareProfile,
    Inspect,
    Outbound,
    Reinspect,
    Return,
}
impl Action {
    pub(crate) fn expected(self) -> Stage {
        match self {
            Self::InspectProfile => Stage::NeedsProfileInspection,
            Self::PrepareProfile => Stage::ProfileInspected,
            Self::Inspect => Stage::New,
            Self::Outbound => Stage::Initial,
            Self::Reinspect => Stage::AtOutbound,
            Self::Return => Stage::ReturnReady,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    NeedsProfileInspection,
    InspectingProfile,
    ProfileInspected,
    PreparingProfile,
    New,
    InspectingInitial,
    Initial,
    Outbound,
    AtOutbound,
    InspectingReturn,
    ReturnReady,
    Returning,
    Returned,
    Inhibited,
}
impl From<Phase> for Stage {
    fn from(v: Phase) -> Self {
        match v {
            Phase::New => Self::New,
            Phase::InspectingInitial => Self::InspectingInitial,
            Phase::Initial => Self::Initial,
            Phase::Outbound => Self::Outbound,
            Phase::AtOutbound => Self::AtOutbound,
            Phase::InspectingReturn => Self::InspectingReturn,
            Phase::ReturnReady => Self::ReturnReady,
            Phase::Returning => Self::Returning,
            Phase::Returned => Self::Returned,
            Phase::Inhibited => Self::Inhibited,
        }
    }
}
impl Action {
    pub fn stage(self) -> Stage {
        self.expected()
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status {
        sequence: u64,
        sent_at_us: u64,
    },
    Act {
        sequence: u64,
        sent_at_us: u64,
        ticket: u64,
        owner: TurnOwner,
        action: Action,
    },
    Stop {
        sequence: u64,
        sent_at_us: u64,
    },
}
impl Request {
    fn stamp(&self) -> (u64, u64) {
        match *self {
            Self::Status {
                sequence,
                sent_at_us,
            }
            | Self::Act {
                sequence,
                sent_at_us,
                ..
            }
            | Self::Stop {
                sequence,
                sent_at_us,
            } => (sequence, sent_at_us),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub id: u64,
    pub owner: TurnOwner,
    pub stage: Stage,
    pub expires_at_us: u64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub sequence: u64,
    pub session: BootId,
    pub at_us: u64,
    pub stage: Stage,
    pub ticket: Option<Ticket>,
    pub error: Option<String>,
    pub motor_reaped: bool,
    pub cleanup_verified: bool,
}
struct Held {
    ticket: Ticket,
    frame: FrameMetadata,
}
pub(crate) struct Driver {
    pub plan: Plan,
    pub plan_sha256: String,
    pub camera_worker: BootId,
    endpoint: Endpoint,
    session: BootId,
    last_sequence: u64,
    session_announced: bool,
    ticket_count: u64,
    last_action_frame: u64,
    first_action_at: Option<u64>,
    held: Option<Held>,
    terminal: bool,
    action_evidence: Option<serde_json::Value>,
}
impl Driver {
    pub(crate) fn load(
        options: &HeadOptions,
        camera_hash: &str,
        camera_worker: BootId,
        session: BootId,
    ) -> io::Result<Self> {
        let bytes = private_bytes(&options.plan)?;
        let plan: Plan = serde_json::from_slice(&bytes).map_err(invalid)?;
        let host_boot = kernel_boot()?;
        plan.check(camera_hash, &host_boot, monotonic_us())?;
        let mut endpoint = Endpoint::bind(options.channel.join("runtime.sock"))?;
        endpoint.connect(options.channel.join("operator.sock"))?;
        Ok(Self {
            plan,
            plan_sha256: format!("{:x}", Sha256::digest(&bytes)),
            camera_worker,
            endpoint,
            session,
            last_sequence: 0,
            session_announced: false,
            ticket_count: 0,
            last_action_frame: 0,
            first_action_at: None,
            held: None,
            terminal: false,
            action_evidence: None,
        })
    }
    #[cfg(test)]
    pub(crate) fn test_driver(
        plan: Plan,
        camera_worker: BootId,
        session: BootId,
        endpoint: Endpoint,
    ) -> Self {
        Self {
            plan,
            plan_sha256: "aa".repeat(32),
            camera_worker,
            endpoint,
            session,
            last_sequence: 0,
            session_announced: true,
            ticket_count: 0,
            last_action_frame: 0,
            first_action_at: None,
            held: None,
            terminal: false,
            action_evidence: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn test_unannounced(&mut self) {
        self.session_announced = false;
    }
    pub(crate) fn binding(&self) -> PlacementBinding {
        PlacementBinding {
            original_sha256: self.plan.motor_config_sha256.clone(),
            original_source: self.plan.original_placement_source,
            assigned_source: self.camera_worker,
            record: self.plan.head.qualification_record,
            review_sha256: self.plan.placement_review_sha256.clone(),
        }
    }
    pub(crate) fn stop(&mut self, motor: &mut OptionalMotor) {
        self.terminal = true;
        self.held = None;
        motor.begin_shutdown();
    }
    /// Two fixed datagrams per live slice. No filesystem, provider or child wait.
    pub(crate) fn tick(
        &mut self,
        controller: &mut Controller,
        motor: &mut OptionalMotor,
        frame: Option<&FrameMetadata>,
        privacy_allowed: bool,
    ) -> Vec<serde_json::Value> {
        let mut events = Vec::with_capacity(2);
        if self.terminal {
            return events;
        }
        if !self.session_announced {
            // Startup can take seconds. This first live slice announces only
            // that control requests can be serviced, never input/motor readiness.
            let hello = Reply {
                sequence: 0,
                session: self.session,
                at_us: monotonic_us(),
                stage: motor.qualification_stage(),
                ticket: None,
                error: None,
                motor_reaped: false,
                cleanup_verified: false,
            };
            let sent = serde_json::to_vec(&hello)
                .map_err(invalid)
                .and_then(|b| self.endpoint.try_send(&b));
            if sent.is_err() {
                self.stop(motor);
                return events;
            }
            self.session_announced = true;
        }
        if monotonic_us() >= self.plan.valid_until_us
            || self.first_action_at.is_some_and(|start| {
                monotonic_us().saturating_sub(start) >= self.plan.head.maximum_duration_us
            })
            || motor.failure().is_some()
            || (!privacy_allowed && (motor.started() || self.first_action_at.is_some()))
        {
            self.stop(motor);
            events.push(serde_json::json!({"kind":"head_qualification_inhibited","at_us":monotonic_us(),"reason":"review/deadline expired, motor failed, or privacy revoked"}));
            return events;
        }
        for _ in 0..2 {
            let mut bytes = [0; MAX_DATAGRAM_BYTES];
            let n = match self.endpoint.try_recv(&mut bytes) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    self.stop(motor);
                    events.push(serde_json::json!({"kind":"head_qualification_inhibited","reason":e.to_string()}));
                    break;
                }
            };
            let request = serde_json::from_slice::<Request>(&bytes[..n]);
            let at = monotonic_us();
            let seq = request.as_ref().map_or(0, |r| r.stamp().0);
            let is_action = request
                .as_ref()
                .is_ok_and(|r| !matches!(r, Request::Status { .. }));
            let outcome = request.map_err(invalid).and_then(|r| {
                if !privacy_allowed && matches!(r, Request::Act { .. }) {
                    return Err(invalid("physical privacy not established"));
                }
                self.handle(
                    r,
                    controller,
                    motor,
                    if privacy_allowed { frame } else { None },
                    at,
                )
            });
            let ticket = outcome.as_ref().ok().copied().flatten();
            let error = outcome
                .as_ref()
                .err()
                .map(|e| e.to_string().chars().take(192).collect::<String>());
            if outcome.is_err() {
                self.stop(motor);
            }
            let reply = Reply {
                sequence: seq,
                session: self.session,
                at_us: at,
                stage: motor.qualification_stage(),
                ticket,
                error: error.clone(),
                motor_reaped: motor.reaped(),
                cleanup_verified: motor.started() && motor.reaped() && motor.finish().is_ok(),
            };
            if is_action || error.is_some() {
                events.push(serde_json::json!({"kind":"head_qualification_control","at_us":at,"sequence":seq,"stage":reply.stage,"error":error,"original_action_evidence":self.action_evidence.take(),"placement_review_sha256":self.plan.placement_review_sha256,"automatic_clearance_inference":false}));
            }
            let sent = serde_json::to_vec(&reply)
                .map_err(invalid)
                .and_then(|b| self.endpoint.try_send(&b));
            if sent.is_err() {
                self.stop(motor);
                break;
            }
            if self.terminal {
                break;
            }
        }
        events
    }
    fn handle(
        &mut self,
        r: Request,
        c: &mut Controller,
        motor: &mut OptionalMotor,
        frame: Option<&FrameMetadata>,
        at: u64,
    ) -> io::Result<Option<Ticket>> {
        let (sequence, sent) = r.stamp();
        if sequence != self.last_sequence + 1
            || sequence > MAX_REQUESTS
            || at.checked_sub(sent).is_none_or(|v| v >= CONTROL_AGE_US)
        {
            return Err(invalid("operator order/freshness violated"));
        }
        self.last_sequence = sequence;
        match r {
            Request::Stop { .. } => {
                self.stop(motor);
                Ok(None)
            }
            Request::Status { .. } => {
                self.held = None;
                if !motor.is_ready()
                    || !c
                        .snapshot(time(at))
                        .map_err(invalid)?
                        .listening_ready(time(at))
                {
                    return Ok(None);
                }
                let Some(owner) = c.snapshot(time(at)).map_err(invalid)?.owner() else {
                    return Ok(None);
                };
                let Some(frame) = frame else {
                    return Ok(None);
                };
                if frame.sequence <= self.last_action_frame
                    || self.check_frame(c, frame, at).is_err()
                {
                    return Ok(None);
                }
                self.ticket_count += 1;
                let expires = at
                    .checked_add(CONTROL_AGE_US)
                    .ok_or_else(|| invalid("ticket time overflow"))?
                    .min(self.plan.valid_until_us)
                    .min(
                        frame
                            .dequeue
                            .completed_at_us
                            .saturating_add(motor.placement_age_us()?),
                    );
                let ticket = Ticket {
                    id: self.ticket_count,
                    owner,
                    stage: motor.qualification_stage(),
                    expires_at_us: expires,
                };
                self.held = Some(Held {
                    ticket,
                    frame: frame.clone(),
                });
                Ok(Some(ticket))
            }
            Request::Act {
                ticket,
                owner,
                action,
                ..
            } => {
                let held = self
                    .held
                    .take()
                    .ok_or_else(|| invalid("no one-use operator receipt"))?;
                if held.ticket.id != ticket
                    || held.ticket.owner != owner
                    || held.ticket.stage != action.stage()
                    || at >= held.ticket.expires_at_us
                    || motor.qualification_stage() != action.expected()
                    || c.snapshot(time(at)).map_err(invalid)?.owner() != Some(owner)
                {
                    return Err(invalid("operator receipt owner/phase/expiry changed"));
                }
                self.check_frame(c, &held.frame, at)?;
                let output = c
                    .plan_output(time(at), owner, OutputKind::Motion, Some(held.frame.grant))
                    .map_err(invalid)?;
                let permit = c
                    .issue_output(time(at), output, lamp_turn_owner::MAX_OUTPUT_LEASE_US)
                    .map_err(invalid)?;
                let snapshot = c.snapshot(time(at)).map_err(invalid)?;
                let placement = Observation {
                    source_boot: self.camera_worker,
                    frame_sequence: held.frame.sequence,
                    acquired_at_us: held.frame.dequeue.completed_at_us,
                    valid_until_us: held
                        .frame
                        .dequeue
                        .completed_at_us
                        .checked_add(motor.placement_age_us()?)
                        .ok_or_else(|| invalid("placement time overflow"))?
                        .min(self.plan.valid_until_us),
                    camera: held.frame.grant,
                    qualification_record: self.plan.head.qualification_record,
                };
                self.first_action_at.get_or_insert(at);
                self.last_action_frame = held.frame.sequence;
                self.action_evidence = Some(
                    serde_json::json!({"action":action,"owner":owner,"original_frame":held.frame,"placement":placement,"permit":permit,"snapshot":snapshot}),
                );
                motor.qualification_action(&self.plan.head, action, snapshot, permit, placement)?;
                Ok(None)
            }
        }
    }
    fn check_frame(&self, c: &mut Controller, f: &FrameMetadata, at: u64) -> io::Result<()> {
        if at >= self.plan.valid_until_us
            || f.worker != self.camera_worker
            || f.capture_epoch == 0
            || f.sequence == 0
            || f.grant != c.camera_grant(time(at)).map_err(invalid)?
            || f.dequeue.completed_at_us > f.published_at_us
            || at
                .checked_sub(f.dequeue.completed_at_us)
                .is_none_or(|age| age >= lamp_camera::MAX_DEQUEUE_AGE_US)
        {
            return Err(invalid(
                "original camera frame/grant is absent, changed or stale",
            ));
        }
        Ok(())
    }
}
fn private_bytes(path: &Path) -> io::Result<Vec<u8>> {
    lamp_body::runtime::config::bounded_path(path)?;
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?;
    let f = File::from(fd);
    let m = f.metadata()?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.nlink() != 1
        || m.mode() & 0o077 != 0
        || m.len() == 0
        || m.len() > 8192
    {
        return Err(invalid(
            "head plan must be a bounded private owned regular file",
        ));
    }
    let mut bytes = Vec::new();
    f.take(8193).read_to_end(&mut bytes)?;
    if bytes.len() > 8192 {
        return Err(invalid("head plan grew"));
    }
    Ok(bytes)
}
fn kernel_boot() -> io::Result<String> {
    #[cfg(target_os = "linux")]
    {
        let mut s = String::new();
        File::open("/proc/sys/kernel/random/boot_id")?
            .take(37)
            .read_to_string(&mut s)?;
        let s = s.trim();
        if s.len() != 36
            || s.chars().filter(|&c| c != '-').count() != 32
            || !s.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
        {
            return Err(invalid("kernel boot identity malformed"));
        }
        Ok(s.into())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native head qualification requires Linux",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera_config::{CameraConfig, UsbIdentity};
    use crate::camera_worker::{CameraEvent, IdentityMetadata, Mode, PortMetadata, Timing};
    use lamp_turn_owner::Permission;
    fn config() -> CameraConfig {
        CameraConfig {
            device: "/dev/inert-camera".into(),
            lock_directory: "/run/inert-camera".into(),
            source: "desk-camera".into(),
            expected_usb: UsbIdentity {
                vendor: 0x1234,
                product: 0x5678,
                topology: "1-2.3".into(),
                serial: None,
                interface_number: 0,
                capture_index: 0,
            },
            width: 640,
            height: 480,
            interval: None,
            max_frame_bytes: 256,
            buffers: 2,
        }
    }
    #[test]
    fn head_frames_retain_only_valid_original_camera_lineage() {
        let worker = BootId::new([3; 16]).unwrap();
        let at = monotonic_us();
        let mut c = Controller::new(BootId::new([1; 16]).unwrap(), time(at));
        c.set_camera_permission(time(at), Permission::Allowed)
            .unwrap();
        let grant = c.camera_grant(time(at)).unwrap();
        let mode = Mode {
            source: "desk-camera".into(),
            format: *b"MJPG",
            width: 640,
            height: 480,
            size_image: 256,
            buffers: 2,
            interval: None,
        };
        let mut frames = Frames::new(worker, config(), "aa".repeat(32)).unwrap();
        // Preparation can precede mandatory-worker startup; no unopened camera
        // progress lease is created before its actual Opening event.
        let at = at + 2_000_000;
        frames
            .accept(
                CameraEvent::Opening {
                    at_us: at,
                    deadline_us: at + lamp_camera::START_BUDGET_US,
                },
                Some(grant),
                at,
            )
            .unwrap();
        frames
            .accept(
                CameraEvent::Started {
                    at_us: at + 1,
                    worker,
                    capture_epoch: 1,
                    mode: mode.clone(),
                    timing: Timing {
                        started_at_us: at,
                        completed_at_us: at + 1,
                    },
                    port: PortMetadata {
                        identity: Some(IdentityMetadata {
                            canonical_node: "/dev/video-test".into(),
                            major: 81,
                            minor: 0,
                            inode: 10,
                            usb: config().expected_usb,
                        }),
                        ..PortMetadata::default()
                    },
                },
                Some(grant),
                at + 1,
            )
            .unwrap();
        let frame = FrameMetadata {
            worker,
            capture_epoch: 1,
            sequence: 1,
            grant,
            mode,
            bytes_used: 20,
            driver_sequence: 1,
            driver_timestamp: None,
            dequeue: Timing {
                started_at_us: at + 2,
                completed_at_us: at + 3,
            },
            published_at_us: at + 4,
        };
        frames
            .accept(
                CameraEvent::Frame {
                    frame: frame.clone(),
                },
                Some(grant),
                at + 4,
            )
            .unwrap();
        assert_eq!(frames.latest.as_ref().unwrap().sequence, 1);
        let mut bad = frame.clone();
        bad.sequence = 2;
        bad.capture_epoch = 2;
        assert!(
            frames
                .accept(CameraEvent::Frame { frame: bad }, Some(grant), at + 4)
                .is_err()
        );
        assert_eq!(frames.latest.as_ref().unwrap().capture_epoch, 1);
        assert!(
            frames
                .accept(CameraEvent::Frame { frame }, None, at + 4)
                .is_err()
        );
    }
    #[test]
    fn head_plan_rejects_wrong_boot_digest_and_expiry() {
        let plan = Plan {
            head: HeadPlan {
                unit: "test".into(),
                qualification_record: [1; 32],
                expected_initial_counts: [2000; 5],
                minimum_count: 1990,
                maximum_count: 2010,
                outbound_count: 2002,
                segment_duration_us: 20_000,
                maximum_duration_us: 1_000_000,
                readback_age_us: 100_000,
            },
            motor_config_sha256: "aa".repeat(32),
            camera_config_sha256: "bb".repeat(32),
            original_placement_source: BootId::new([3; 16]).unwrap(),
            placement_review_sha256: "cc".repeat(32),
            kernel_boot_id: "original-boot".into(),
            reviewed_at_us: 10,
            valid_until_us: 20,
        };
        assert!(
            plan.check(&plan.camera_config_sha256, "original-boot", 11)
                .is_ok()
        );
        assert!(
            plan.check(&plan.camera_config_sha256, "other-boot", 11)
                .is_err()
        );
        assert!(plan.check("cc", "original-boot", 11).is_err());
        assert!(
            plan.check(&plan.camera_config_sha256, "original-boot", 20)
                .is_err()
        );
    }
}
