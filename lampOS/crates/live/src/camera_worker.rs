//! A finite camera worker. JPEG bytes never enter a control or data channel.
//! Without frame consumers it publishes metadata only (camera inspection).
//! With consumers it also serves frames over the separate bounded frame link
//! (`lamp_camera::frames`), copying each frame into the waiting consumers' own
//! buffers before releasing the library slot. Capture, privacy control and
//! metadata never wait for a consumer.
//! `Ready` means this closed loop exists, not that capture or sight is ready.
use crate::{
    camera_config::{Interval, UsbIdentity},
    transport::WorkerChannels,
    wire::Control,
};
use lamp_camera::{
    Capture, Clock, DeliveryToken, FrameObservation, NegotiatedMode, Phase, PortIo,
    frames::{CloseReason, FrameServer, LinkCounts, LinkEvent, Role},
};
use lamp_interaction::{BootId, CameraGrant};
use serde::{Deserialize, Serialize};
use std::io;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::run;

pub const CONTROL_BUDGET: usize = 16;
pub const PROGRESS_INTERVAL_US: u64 = 10_000;
pub const PROGRESS_DEADLINE_US: u64 = 30_000;
pub const WORKER_INITIAL_AUTHORITY_US: u64 = 250_000;
/// Frame link events published per loop iteration.
const LINK_EVENTS_PER_STEP: usize = 4;
/// The frame link socket inside the private session directory.
pub const FRAME_SOCKET: &str = "camera.frames";

/// Parse `--frame-consumer ROLE=BOOT_HEX` flags (at most one per role).
pub fn parse_frame_consumers(flags: &[String]) -> io::Result<Vec<(Role, BootId)>> {
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidInput, message.to_owned());
    let mut consumers: Vec<(Role, BootId)> = Vec::new();
    let mut iter = flags.iter();
    while let Some(flag) = iter.next() {
        if flag != "--frame-consumer" {
            return Err(invalid("unknown camera worker option"));
        }
        let value = iter
            .next()
            .ok_or_else(|| invalid("--frame-consumer needs ROLE=BOOT"))?;
        let (role, boot) = value
            .split_once('=')
            .ok_or_else(|| invalid("--frame-consumer needs ROLE=BOOT"))?;
        let role = Role::parse(role).ok_or_else(|| invalid("unknown frame consumer role"))?;
        let boot = crate::process::parse_boot(boot)?;
        if consumers.iter().any(|(r, _)| *r == role) || consumers.len() >= Role::ALL.len() {
            return Err(invalid("duplicate frame consumer role"));
        }
        consumers.push((role, boot));
    }
    Ok(consumers)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub started_at_us: u64,
    pub completed_at_us: u64,
}
impl From<lamp_camera::CallTiming> for Timing {
    fn from(t: lamp_camera::CallTiming) -> Self {
        Self {
            started_at_us: t.started_at.as_micros(),
            completed_at_us: t.completed_at.as_micros(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mode {
    pub source: String,
    pub format: [u8; 4],
    pub width: u32,
    pub height: u32,
    pub interval: Option<Interval>,
    pub size_image: usize,
    pub buffers: u8,
}
impl From<NegotiatedMode> for Mode {
    fn from(mode: NegotiatedMode) -> Self {
        Self {
            source: mode.source.as_str().to_owned(),
            format: mode.format.0,
            width: mode.width,
            height: mode.height,
            interval: mode.interval.map(Into::into),
            size_image: mode.size_image,
            buffers: mode.buffers,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockDomain {
    HostMonotonic,
    Realtime,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampPoint {
    StartOfExposure,
    EndOfFrame,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverTimestamp {
    pub micros: u64,
    pub domain: ClockDomain,
    pub point: TimestampPoint,
}
impl From<lamp_camera::DriverTimestamp> for DriverTimestamp {
    fn from(t: lamp_camera::DriverTimestamp) -> Self {
        Self {
            micros: t.micros,
            domain: match t.domain {
                lamp_camera::TimestampDomain::HostMonotonic => ClockDomain::HostMonotonic,
                lamp_camera::TimestampDomain::Realtime => ClockDomain::Realtime,
                lamp_camera::TimestampDomain::Unknown => ClockDomain::Unknown,
            },
            point: match t.point {
                lamp_camera::TimestampPoint::StartOfExposure => TimestampPoint::StartOfExposure,
                lamp_camera::TimestampPoint::EndOfFrame => TimestampPoint::EndOfFrame,
                lamp_camera::TimestampPoint::Unknown => TimestampPoint::Unknown,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub received: u64,
    pub pending_replaced: u64,
    pub aged_out: u64,
    pub invalidated: u64,
    pub completed: u64,
    pub driver_sequence_gaps: u64,
}
impl From<lamp_camera::Counters> for Counts {
    fn from(c: lamp_camera::Counters) -> Self {
        Self {
            received: c.frames_received,
            pending_replaced: c.pending_replaced,
            aged_out: c.aged_out,
            invalidated: c.invalidated,
            completed: c.delivery_completed,
            driver_sequence_gaps: c.driver_sequence_gaps,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameMetadata {
    pub worker: BootId,
    pub capture_epoch: u64,
    pub sequence: u64,
    pub grant: CameraGrant,
    pub mode: Mode,
    pub bytes_used: usize,
    pub driver_sequence: u32,
    pub driver_timestamp: Option<DriverTimestamp>,
    pub dequeue: Timing,
    pub published_at_us: u64,
}
impl FrameMetadata {
    fn new(o: FrameObservation, bytes_used: usize, published_at_us: u64) -> Self {
        Self {
            worker: o.id.capture.worker,
            capture_epoch: o.id.capture.epoch.get(),
            sequence: o.id.sequence.get(),
            grant: o.grant,
            mode: o.mode.into(),
            bytes_used,
            driver_sequence: o.driver_sequence,
            driver_timestamp: o.driver_timestamp.map(Into::into),
            dequeue: o.dequeue.into(),
            published_at_us,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityMetadata {
    pub canonical_node: String,
    pub major: u32,
    pub minor: u32,
    pub inode: u64,
    pub usb: UsbIdentity,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortMetadata {
    pub identity: Option<IdentityMetadata>,
    // These bounded values are produced only from fixed V4L2 structures. They
    // contain metadata, never a JPEG, base64 image, descriptor or mapping.
    pub capabilities: Option<serde_json::Value>,
    pub interval_query: Option<serde_json::Value>,
    pub controls: Option<[serde_json::Value; 5]>,
    pub buffer_lengths: [Option<usize>; 4],
    pub last_raw_buffer_flags: Option<u32>,
    pub stop: Option<Timing>,
    pub fault: Option<String>,
    pub cleanup: Option<String>,
}
/// Only immutable bounded diagnostics; this grants no mutable port/device access.
pub trait InspectPort: PortIo {
    fn metadata(&self) -> PortMetadata;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Requested,
    PrivacyChanged,
    Fault,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CameraEvent {
    Opening {
        at_us: u64,
        deadline_us: u64,
    },
    Started {
        at_us: u64,
        worker: BootId,
        capture_epoch: u64,
        mode: Mode,
        timing: Timing,
        port: PortMetadata,
    },
    Frame {
        frame: FrameMetadata,
    },
    Progress {
        at_us: u64,
        counts: Counts,
        last_read: Option<Timing>,
    },
    Stopped {
        at_us: u64,
        reason: StopReason,
        counts: Counts,
        stop: Option<Timing>,
        port: PortMetadata,
        error: Option<String>,
    },
    /// A frame consumer connected or was closed. Never sent without consumers.
    FrameLink {
        event: LinkEvent,
        counts: LinkCounts,
    },
}
/// Both methods must be nonblocking. Full/error channels terminate inspection;
/// backpressure cannot silently hide frame loss or postpone urgent control.
pub trait CameraLink {
    fn receive_control(&mut self) -> io::Result<Option<Control>>;
    fn publish(&mut self, event: CameraEvent) -> io::Result<()>;
}
impl CameraLink for WorkerChannels {
    fn receive_control(&mut self) -> io::Result<Option<Control>> {
        self.control.receive()
    }
    fn publish(&mut self, event: CameraEvent) -> io::Result<()> {
        self.data.send(event)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Continue,
    Stopped,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Drain {
    Empty,
    Full,
    Stopped,
}

pub struct CameraLoop<P: InspectPort, C: Clock> {
    capture: Capture<P, C>,
    start_pending: bool,
    started: bool,
    terminal: bool,
    in_flight: Option<DeliveryToken>,
    last_authority_us: u64,
    last_progress_us: u64,
    last_read: Option<Timing>,
    frames: Option<FrameServer>,
}
impl<P: InspectPort, C: Clock> CameraLoop<P, C> {
    pub fn new(capture: Capture<P, C>, now_us: u64) -> Self {
        Self {
            capture,
            start_pending: false,
            started: false,
            terminal: false,
            in_flight: None,
            last_authority_us: now_us,
            last_progress_us: now_us,
            last_read: None,
            frames: None,
        }
    }
    /// Serve frames to the permitted consumers. Frames are then taken from the
    /// library only while a consumer is waiting for one.
    pub fn with_frames(mut self, frames: FrameServer) -> Self {
        self.frames = Some(frames);
        self
    }
    pub fn frame_link(&self) -> Option<&FrameServer> {
        self.frames.as_ref()
    }
    /// Continue frame writes, accept consumers and publish a bounded number
    /// of link events.
    fn service_frames(
        &mut self,
        link: &mut impl CameraLink,
        now: &mut impl FnMut() -> u64,
    ) -> io::Result<()> {
        let Some(frames) = self.frames.as_mut() else {
            return Ok(());
        };
        frames.service(now());
        for _ in 0..LINK_EVENTS_PER_STEP {
            let Some(event) = frames.take_event() else {
                break;
            };
            link.publish(CameraEvent::FrameLink {
                event,
                counts: frames.counts(),
            })?;
        }
        Ok(())
    }
    fn close_frames(&mut self, reason: CloseReason, now_us: u64) {
        if let Some(frames) = self.frames.as_mut() {
            frames.close_all(reason, now_us);
        }
    }
    /// One bounded control slice and at most one port read. A full control slice
    /// defers port work and delivery, retaining at most the library's one token.
    pub fn step(
        &mut self,
        link: &mut impl CameraLink,
        now: &mut impl FnMut() -> u64,
    ) -> io::Result<Step> {
        if self.terminal {
            return Err(io::Error::other("camera worker already stopped"));
        }
        match self.inner(link, now) {
            Ok(result) => Ok(result),
            Err(error) => {
                let cleanup = self.capture.control_lost();
                self.in_flight = None;
                self.terminal = true;
                // Partly sent frames are abandoned; consumers see the link close.
                self.close_frames(CloseReason::Fault, now());
                let message = format!("{error}; local cleanup: {cleanup}");
                // A failed report is never mistaken for successful cleanup; the
                // parent observes abnormal exit or its finite progress deadline.
                let _ = link.publish(CameraEvent::Stopped {
                    at_us: now(),
                    reason: StopReason::Fault,
                    counts: self.capture.counters().into(),
                    stop: cleanup.cleanup.and_then(|c| c.timing).map(Into::into),
                    port: self.capture.port().metadata(),
                    error: Some(message.clone()),
                });
                Err(io::Error::other(message))
            }
        }
    }
    fn inner(
        &mut self,
        link: &mut impl CameraLink,
        now: &mut impl FnMut() -> u64,
    ) -> io::Result<Step> {
        let mut remaining = CONTROL_BUDGET;
        match self.drain(link, now, &mut remaining)? {
            Drain::Stopped => return Ok(Step::Stopped),
            Drain::Full => {
                self.maintain(now)?;
                return Ok(Step::Continue);
            }
            Drain::Empty => {}
        }
        self.maintain(now)?;
        self.service_frames(link, now)?;
        if self.start_pending {
            self.start_pending = false;
            let at_us = now();
            let deadline_us = at_us
                .checked_add(lamp_camera::START_BUDGET_US)
                .ok_or_else(|| io::Error::other("camera start deadline overflow"))?;
            link.publish(CameraEvent::Opening { at_us, deadline_us })?;
            let started = self.capture.start().map_err(io::Error::other)?;
            self.started = true;
            match self.drain(link, now, &mut remaining)? {
                Drain::Stopped => return Ok(Step::Stopped),
                Drain::Full => {
                    return Err(io::Error::other(
                        "control saturated during camera start; no readiness published",
                    ));
                }
                Drain::Empty => {}
            }
            self.maintain(now)?;
            link.publish(CameraEvent::Started {
                at_us: now(),
                worker: started.capture.worker,
                capture_epoch: started.capture.epoch.get(),
                mode: started.mode.into(),
                timing: started.timing.into(),
                port: self.capture.port().metadata(),
            })?;
        } else if self.started {
            let polled = self.capture.poll().map_err(io::Error::other)?;
            self.last_read = Some(polled.read_timing.into());
            match self.drain(link, now, &mut remaining)? {
                Drain::Stopped => return Ok(Step::Stopped),
                Drain::Full => return Ok(Step::Continue),
                Drain::Empty => {}
            }
            // With consumers, a frame leaves the library only for a waiting
            // consumer; otherwise newer frames keep replacing the pending slot.
            let wanted = self.frames.as_ref().is_none_or(FrameServer::wants_frame);
            if self.in_flight.is_none() && wanted {
                self.in_flight = self.capture.begin_delivery().map_err(io::Error::other)?;
            }
            if self.in_flight.is_some() {
                // Another priority drain is required after dequeue and before
                // handing off the original observation. Never mint a new grant.
                match self.drain(link, now, &mut remaining)? {
                    Drain::Stopped => return Ok(Step::Stopped),
                    Drain::Full => return Ok(Step::Continue),
                    Drain::Empty => {}
                }
                let token = self.in_flight.as_ref().expect("checked token");
                let frame = self
                    .capture
                    .delivery_view(token)
                    .map_err(io::Error::other)?;
                let at_us = now();
                let metadata = FrameMetadata::new(frame.observation, frame.bytes.len(), at_us);
                if let Some(frames) = self.frames.as_mut() {
                    // One bounded copy into each waiting consumer's own buffer;
                    // writing continues nonblockingly in later iterations.
                    frames.offer(&frame.observation, frame.bytes, at_us)?;
                }
                // The borrow ends here. Only small metadata enters the data
                // channel; image bytes travel only over the frame link.
                link.publish(CameraEvent::Frame { frame: metadata })?;
                let token = self.in_flight.take().expect("owned token");
                self.capture
                    .finish_delivery(token)
                    .map_err(io::Error::other)?;
            }
        }
        let at_us = now();
        if at_us
            .checked_sub(self.last_progress_us)
            .is_none_or(|age| age >= PROGRESS_INTERVAL_US)
        {
            link.publish(CameraEvent::Progress {
                at_us,
                counts: self.capture.counters().into(),
                last_read: self.last_read,
            })?;
            self.last_progress_us = at_us;
        }
        Ok(Step::Continue)
    }
    fn maintain(&mut self, now: &mut impl FnMut() -> u64) -> io::Result<()> {
        if now()
            .checked_sub(self.last_authority_us)
            .is_none_or(|age| age >= WORKER_INITIAL_AUTHORITY_US)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "camera controller heartbeat expired",
            ));
        }
        self.capture.maintain().map_err(io::Error::other)?;
        Ok(())
    }
    fn drain(
        &mut self,
        link: &mut impl CameraLink,
        now: &mut impl FnMut() -> u64,
        remaining: &mut usize,
    ) -> io::Result<Drain> {
        while *remaining > 0 {
            let Some(command) = link.receive_control()? else {
                return Ok(Drain::Empty);
            };
            *remaining -= 1;
            match command {
                Control::Stop => {
                    self.stop(link, now, StopReason::Requested)?;
                    return Ok(Drain::Stopped);
                }
                Control::StartCapture => {
                    if self.start_pending || self.started {
                        return Err(io::Error::other("duplicate camera start"));
                    }
                    self.start_pending = true;
                }
                Control::Authority { snapshot } => match self.capture.install_authority(snapshot) {
                    Ok(status) => {
                        self.last_authority_us = now();
                        if self.started && status.phase == Phase::Closed {
                            self.stop(link, now, StopReason::PrivacyChanged)?;
                            return Ok(Drain::Stopped);
                        }
                    }
                    Err(lamp_camera::Error {
                        kind:
                            lamp_camera::ErrorKind::Authority(
                                lamp_interaction::Error::StaleSnapshot
                                | lamp_interaction::Error::WrongBoot
                                | lamp_interaction::Error::FutureState,
                            ),
                        ..
                    }) if self.capture.fault().is_none() => {}
                    Err(error) => return Err(io::Error::other(error)),
                },
                Control::ConnectReference { .. } | Control::ProviderOutputCapacity { .. } => {
                    return Err(io::Error::other(
                        "camera worker rejects audio reference control",
                    ));
                }
            }
        }
        Ok(Drain::Full)
    }
    fn stop(
        &mut self,
        link: &mut impl CameraLink,
        now: &mut impl FnMut() -> u64,
        reason: StopReason,
    ) -> io::Result<()> {
        self.in_flight = None;
        self.start_pending = false;
        // Close consumers before the port stops: no partial frame completes.
        self.close_frames(
            match reason {
                StopReason::Requested => CloseReason::Stopped,
                StopReason::PrivacyChanged => CloseReason::PrivacyClosed,
                StopReason::Fault => CloseReason::Fault,
            },
            now(),
        );
        self.service_frames(link, now)?;
        let report = self.capture.stop().map_err(io::Error::other)?;
        self.terminal = true;
        link.publish(CameraEvent::Stopped {
            at_us: now(),
            reason,
            counts: self.capture.counters().into(),
            stop: report.timing.map(Into::into),
            port: self.capture.port().metadata(),
            error: None,
        })
    }
}
