//! Optional direct camera context. One attempt per submitted input, one frame
//! buffer, no waiting for vision on the audio path, and no reconnect replay.
use crate::{camera_config::CameraConfig, camera_worker::FRAME_SOCKET, process::parse_boot};
use lamp_camera::frames::{FrameClient, FrameMeta, Role};
use lamp_gemini::{
    Connect, ImageOffer, ImageRefusal, ImageRevocation, RequestId, SessionId, Supervisor,
};
use lamp_turn_owner::{BootId, BoundaryGuard, CameraGrant, MonoTime};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Host dequeue age, not an assertion about sensor exposure or model perception.
pub const MAX_FRAME_AGE_US: u64 = 250_000;
const MAX_ATTEMPT_US: u64 = 250_000;
const MAX_CLOCK_BRACKET_US: u64 = 10_000;

#[derive(Clone, Debug)]
pub struct Source {
    pub socket: PathBuf,
    pub camera: BootId,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub max_frame_bytes: usize,
}
impl Source {
    pub fn configured(directory: &Path, camera: BootId, config: &CameraConfig) -> io::Result<Self> {
        config.capture()?;
        if config.width > 640 || config.height > 480 || config.max_frame_bytes > 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "chat camera requires explicit MJPEG at most 640x480 and a buffer at most 1 MiB; 320x240 is recommended",
            ));
        }
        Ok(Self {
            socket: directory.join(FRAME_SOCKET),
            camera,
            source: config.source.clone(),
            width: config.width,
            height: config.height,
            max_frame_bytes: config.max_frame_bytes,
        })
    }
    pub fn arguments(&self) -> Vec<String> {
        vec![
            "--camera-context".into(),
            crate::process::boot_text(self.camera),
            self.source.clone(),
            self.width.to_string(),
            self.height.to_string(),
            self.max_frame_bytes.to_string(),
        ]
    }
    pub fn parse(directory: &Path, arguments: &[String]) -> io::Result<Option<Self>> {
        if arguments.is_empty() {
            return Ok(None);
        }
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid camera context arguments",
            )
        };
        let [flag, camera, source, width, height, size] = arguments else {
            return Err(invalid());
        };
        if flag != "--camera-context" {
            return Err(invalid());
        }
        let (width, height, max_frame_bytes) = (
            width.parse().map_err(|_| invalid())?,
            height.parse().map_err(|_| invalid())?,
            size.parse().map_err(|_| invalid())?,
        );
        lamp_camera::SourceId::new(source).map_err(io::Error::other)?;
        if !(1..=640).contains(&width)
            || !(1..=480).contains(&height)
            || !(4..=1024 * 1024).contains(&max_frame_bytes)
        {
            return Err(invalid());
        }
        Ok(Some(Self {
            socket: directory.join(FRAME_SOCKET),
            camera: parse_boot(camera)?,
            source: source.clone(),
            width,
            height,
            max_frame_bytes,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Turn {
    pub request: RequestId,
    pub session: SessionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Accepted into Gemini's bounded image slot, not delivered/understood proof.
    Queued,
    CameraUnavailable,
    PrivacyClosed,
    InputClosed,
    ConnectionChanged,
    Expired,
    Oversize,
    InvalidFrame,
    ClockUncertain,
    ProviderRefused,
    RevokedUnsent,
    RevokedInFlight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub request: u64,
    pub session: u64,
    pub camera_worker: BootId,
    pub camera_grant: Option<CameraGrant>,
    /// Provider-local monotonic token for an observed CameraGrant. It is not
    /// the controller's private camera-generation representation.
    pub export_generation: u64,
    pub frame_sequence: Option<u64>,
    pub capture_epoch: Option<u64>,
    pub dequeue_started_at_us: Option<u64>,
    pub dequeue_completed_at_us: Option<u64>,
    pub copied_at_us: Option<u64>,
    pub bytes: Option<usize>,
    pub at_us: u64,
    pub outcome: Outcome,
    pub coalesced_receipts: u64,
}

pub(crate) trait ImageSink {
    fn connected_for(&self, request: RequestId) -> bool;
    fn offer(
        &self,
        request: RequestId,
        frame: u64,
        grant: u64,
        captured: Instant,
        jpeg: &[u8],
    ) -> Result<ImageOffer, ImageRefusal>;
    fn revoke(&self, through: u64) -> ImageRevocation;
}
impl<C: Connect> ImageSink for Supervisor<C> {
    fn connected_for(&self, request: RequestId) -> bool {
        self.is_connected() && !self.input_belongs_to_replaced_connection(request)
    }
    fn offer(
        &self,
        request: RequestId,
        frame: u64,
        grant: u64,
        captured: Instant,
        jpeg: &[u8],
    ) -> Result<ImageOffer, ImageRefusal> {
        self.try_image(request, frame, grant, captured, jpeg)
    }
    fn revoke(&self, through: u64) -> ImageRevocation {
        self.revoke_images(through)
    }
}

struct Attempt {
    turn: Turn,
    started_at_us: u64,
    requested: bool,
}

pub(crate) struct ContextDelivery {
    source: Source,
    client: FrameClient,
    jpeg: Vec<u8>,
    frame: Option<FrameMeta>,
    attempt: Option<Attempt>,
    highest_request: u64,
    last_frame: Option<(u64, u64)>,
    grant: Option<CameraGrant>,
    export_generation: u64,
    last_offered: Option<Receipt>,
    receipt: Option<Receipt>,
    coalesced: u64,
}
impl ContextDelivery {
    pub fn new(source: Source, parent: BootId, worker: BootId) -> io::Result<Self> {
        let client = FrameClient::new(
            &source.socket,
            Role::Context,
            worker,
            parent,
            source.max_frame_bytes,
        );
        Ok(Self {
            source,
            client,
            jpeg: Vec::new(),
            frame: None,
            attempt: None,
            highest_request: 0,
            last_frame: None,
            grant: None,
            export_generation: 0,
            last_offered: None,
            receipt: None,
            coalesced: 0,
        })
    }

    /// Run even when PCM credit is exhausted or the priority slice is full.
    pub fn authority(&mut self, guard: &mut BoundaryGuard, sink: &impl ImageSink, at_us: u64) {
        let grant = guard
            .current_camera_grant(MonoTime::from_micros(at_us))
            .ok();
        if self.grant == grant {
            return;
        }
        if self.grant.is_some() {
            let outcome = match sink.revoke(self.export_generation) {
                ImageRevocation::Nothing => None,
                ImageRevocation::DroppedUnsent => Some(Outcome::RevokedUnsent),
                ImageRevocation::AbandonedInFlight => Some(Outcome::RevokedInFlight),
            };
            if let (Some(outcome), Some(mut receipt)) = (outcome, self.last_offered.take()) {
                receipt.outcome = outcome;
                receipt.at_us = at_us;
                self.record(receipt);
            }
            self.finish(Outcome::PrivacyClosed, at_us);
        }
        self.grant = grant;
        if grant.is_some() {
            match self.export_generation.checked_add(1) {
                Some(next) => self.export_generation = next,
                None => self.grant = None, // Never reuse an export token.
            }
        }
    }

    /// Reads only into the sole bounded buffer. The caller MUST drain priority
    /// control and recheck authority before calling export(). Empty IPC never
    /// implies camera permission or a successful image delivery.
    pub fn poll(&mut self, turn: Option<Turn>, sink: &impl ImageSink, at_us: u64) {
        if self.attempt.as_ref().is_some_and(|a| Some(a.turn) != turn) {
            self.finish(Outcome::InputClosed, at_us);
        }
        if self.attempt.is_none()
            && let Some(turn) = turn
        {
            if turn.request.get() <= self.highest_request {
                return;
            }
            self.highest_request = turn.request.get();
            self.attempt = Some(Attempt {
                turn,
                started_at_us: at_us,
                requested: false,
            });
        }
        let Some(attempt) = &mut self.attempt else {
            return;
        };
        if self.grant.is_none() {
            self.finish(Outcome::PrivacyClosed, at_us);
            return;
        }
        if !sink.connected_for(attempt.turn.request) {
            self.finish(Outcome::ConnectionChanged, at_us);
            return;
        }
        if at_us
            .checked_sub(attempt.started_at_us)
            .is_none_or(|age| age >= MAX_ATTEMPT_US)
        {
            self.finish(Outcome::Expired, at_us);
            return;
        }
        if self.frame.is_some() {
            return;
        }
        let result = (|| -> io::Result<()> {
            if !attempt.requested {
                if !self.client.connect()? {
                    return Ok(());
                }
                self.client.request(MAX_FRAME_AGE_US as u32)?;
                attempt.requested = true;
            }
            self.frame = self.client.poll(&mut self.jpeg)?;
            Ok(())
        })();
        if result.is_err() {
            self.finish(Outcome::CameraUnavailable, at_us);
        }
    }

    pub fn export(&mut self, turn: Option<Turn>, guard: &mut BoundaryGuard, sink: &impl ImageSink) {
        // Sampling Instant between host samples permits a conservative mapping:
        // subtract age from the *later* host sample, never make the frame newer.
        let before = lamp_ipc::monotonic_us();
        let instant = Instant::now();
        let at = lamp_ipc::monotonic_us();
        self.export_at(turn, guard, sink, before, at, instant);
    }

    fn export_at(
        &mut self,
        turn: Option<Turn>,
        guard: &mut BoundaryGuard,
        sink: &impl ImageSink,
        before: u64,
        at: u64,
        instant: Instant,
    ) {
        self.authority(guard, sink, at);
        let (Some(attempt), Some(frame)) = (&self.attempt, &self.frame) else {
            return;
        };
        let outcome = if Some(attempt.turn) != turn {
            Some(Outcome::InputClosed)
        } else if at
            .checked_sub(attempt.started_at_us)
            .is_none_or(|age| age >= MAX_ATTEMPT_US)
        {
            // Reading a fresh frame and draining control do not renew the
            // original attempt. Recheck after either operation can yield.
            Some(Outcome::Expired)
        } else if !sink.connected_for(attempt.turn.request) {
            Some(Outcome::ConnectionChanged)
        } else if self.jpeg.len() > lamp_gemini::MAX_IMAGE_BYTES {
            Some(Outcome::Oversize)
        } else if self.grant != Some(frame.grant)
            || frame.camera_worker != self.source.camera
            || frame.source != self.source.source
            || frame.capture_epoch == 0
            || frame.frame_sequence == 0
            || frame.format != *b"MJPG"
            || frame.width != self.source.width
            || frame.height != self.source.height
            || frame.derived.is_some()
            || self.last_frame.is_some_and(|(epoch, sequence)| {
                frame.capture_epoch != epoch || frame.frame_sequence <= sequence
            })
            || frame.dequeue_started_at_us == 0
            || frame.dequeue_started_at_us > frame.dequeue_completed_at_us
            || frame.dequeue_completed_at_us > frame.copied_at_us
            || frame.copied_at_us > before
        {
            Some(Outcome::InvalidFrame)
        } else if at
            .checked_sub(frame.dequeue_started_at_us)
            .is_none_or(|age| age >= MAX_FRAME_AGE_US)
        {
            Some(Outcome::Expired)
        } else {
            None
        };
        if let Some(outcome) = outcome {
            self.finish(outcome, at);
            return;
        }
        let Some(captured) =
            conservative_capture_time(before, at, instant, frame.dequeue_started_at_us)
        else {
            self.finish(Outcome::ClockUncertain, at);
            return;
        };
        self.last_frame = Some((frame.capture_epoch, frame.frame_sequence));
        let outcome = match sink.offer(
            attempt.turn.request,
            frame.frame_sequence,
            self.export_generation,
            captured,
            &self.jpeg,
        ) {
            Ok(ImageOffer::Queued | ImageOffer::ReplacedUnsent) => Outcome::Queued,
            Err(ImageRefusal::NotConnected) => Outcome::ConnectionChanged,
            Err(ImageRefusal::NotCurrent) => Outcome::InputClosed,
            Err(ImageRefusal::TooLarge) => Outcome::Oversize,
            Err(ImageRefusal::NotJpeg) => Outcome::InvalidFrame,
            Err(ImageRefusal::TooOld) => Outcome::Expired,
            Err(ImageRefusal::Revoked) => Outcome::PrivacyClosed,
            Err(ImageRefusal::LimitReached) => Outcome::ProviderRefused,
        };
        self.finish(outcome, at);
    }

    fn finish(&mut self, outcome: Outcome, at_us: u64) {
        if let Some(attempt) = self.attempt.take() {
            let frame = self.frame.as_ref();
            let receipt = Receipt {
                request: attempt.turn.request.get(),
                session: attempt.turn.session.get(),
                camera_worker: self.source.camera,
                camera_grant: frame.map(|f| f.grant).or(self.grant),
                export_generation: self.export_generation,
                frame_sequence: frame.map(|f| f.frame_sequence),
                capture_epoch: frame.map(|f| f.capture_epoch),
                dequeue_started_at_us: frame.map(|f| f.dequeue_started_at_us),
                dequeue_completed_at_us: frame.map(|f| f.dequeue_completed_at_us),
                copied_at_us: frame.map(|f| f.copied_at_us),
                bytes: frame.map(|_| self.jpeg.len()),
                at_us,
                outcome,
                coalesced_receipts: 0,
            };
            if outcome == Outcome::Queued {
                self.last_offered = Some(receipt.clone());
            }
            self.record(receipt);
        }
        self.client.disconnect();
        self.frame = None;
        self.jpeg.clear();
    }
    fn record(&mut self, mut receipt: Receipt) {
        if self.receipt.is_some() {
            self.coalesced = self.coalesced.saturating_add(1);
        }
        receipt.coalesced_receipts = self.coalesced;
        self.receipt = Some(receipt);
    }
    pub fn send_receipt(
        &mut self,
        send: impl FnOnce(&Receipt) -> io::Result<()>,
    ) -> io::Result<()> {
        if let Some(receipt) = &self.receipt {
            match send(receipt) {
                Ok(()) => {
                    self.receipt = None;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

fn conservative_capture_time(
    before: u64,
    after: u64,
    instant: Instant,
    captured: u64,
) -> Option<Instant> {
    if after.checked_sub(before)? > MAX_CLOCK_BRACKET_US || captured > before {
        return None;
    }
    instant.checked_sub(Duration::from_micros(after.checked_sub(captured)?))
}

#[cfg(test)]
mod tests;
