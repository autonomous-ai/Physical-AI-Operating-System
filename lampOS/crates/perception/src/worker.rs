//! The perception worker loop, independent of the runtime's wire types.
//!
//! The control loop stays responsive: inference runs on one dedicated thread
//! with a single job slot, so authority, stop and privacy changes are drained
//! every tick even while a frame is being analysed. The worker requests a
//! frame only while it holds a current camera grant, rechecks that grant when
//! the frame arrives and again before publishing, and bounds both frame and
//! result age. Without a frame there is no observation; nothing is inferred
//! from silence on the frame link.
use crate::{
    analyze::{Analysis, AnalysisError, Analyzer},
    observation::{
        AnalysisTiming, DiscardReason, FrameRef, PerceptionCounts, PerceptionEvent,
        VisualObservation, bounded_text,
    },
};
use lamp_camera::frames::{FrameClient, FrameMeta, Role};
use lamp_interaction::{
    BootId, BoundaryGuard, CameraGrant, Confidence, Error as AuthorityError, MonoTime, Observation,
    Snapshot,
};
use lamp_ipc::monotonic_us;
use std::{
    io,
    num::NonZeroU64,
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread,
    time::Duration,
};

pub const CONTROL_BUDGET: usize = 16;
pub const AUTHORITY_TIMEOUT_US: u64 = 250_000;
pub const CONNECT_RETRY_US: u64 = 100_000;
pub const PROGRESS_INTERVAL_US: u64 = 1_000_000;
pub const TICK: Duration = Duration::from_millis(2);
/// How long shutdown waits for an in-progress analysis before leaving it to
/// process exit.
pub const INFERENCE_JOIN_US: u64 = 2_000_000;

pub enum PerceptionControl {
    Authority(Snapshot),
    Stop,
}

/// Both methods must be nonblocking.
pub trait PerceptionLink {
    fn receive_control(&mut self) -> io::Result<Option<PerceptionControl>>;
    fn publish(&mut self, event: PerceptionEvent) -> io::Result<()>;
}

#[derive(Clone, Debug)]
pub struct WorkerOptions {
    pub frame_socket: PathBuf,
    pub controller: BootId,
    pub worker: BootId,
    /// Minimum time between frame requests; bounds CPU use.
    pub interval_us: u64,
    /// Request bound and receipt bound for frame age (host dequeue age).
    pub max_frame_age_us: u32,
    /// Bound from host dequeue to publication.
    pub max_result_age_us: u64,
    pub max_frame_bytes: usize,
}

impl WorkerOptions {
    pub fn new(frame_socket: PathBuf, controller: BootId, worker: BootId) -> Self {
        Self {
            frame_socket,
            controller,
            worker,
            interval_us: 250_000,
            max_frame_age_us: 200_000,
            max_result_age_us: 1_000_000,
            max_frame_bytes: lamp_camera::MAX_FRAME_BYTES,
        }
    }
}

struct Job {
    meta: FrameMeta,
    jpeg: Vec<u8>,
    received_at_us: u64,
}
struct Done {
    meta: FrameMeta,
    jpeg: Vec<u8>,
    received_at_us: u64,
    analysed_at_us: u64,
    result: Result<Analysis, AnalysisError>,
}

fn inference(mut analyzer: Analyzer, jobs: Receiver<Job>, done: SyncSender<Done>) {
    while let Ok(job) = jobs.recv() {
        let result = analyzer.analyze(&job.jpeg);
        let finished = Done {
            meta: job.meta,
            jpeg: job.jpeg,
            received_at_us: job.received_at_us,
            analysed_at_us: monotonic_us(),
            result,
        };
        if done.send(finished).is_err() {
            return;
        }
    }
}

/// How a frame's original grant compares with current authority.
fn grant_status(
    guard: &mut BoundaryGuard,
    grant: CameraGrant,
    now_us: u64,
) -> Option<DiscardReason> {
    match guard.current_camera_grant(MonoTime::from_micros(now_us)) {
        Ok(current) if current == grant => None,
        Ok(_) | Err(AuthorityError::PrivacyClosed) => Some(DiscardReason::PrivacyChanged),
        Err(_) => Some(DiscardReason::NoAuthority),
    }
}

struct State {
    options: WorkerOptions,
    guard: BoundaryGuard,
    client: FrameClient,
    last_authority_us: u64,
    next_connect_us: u64,
    next_request_us: u64,
    busy: bool,
    sequence: u64,
    counts: PerceptionCounts,
    last_progress_us: u64,
    spare: Vec<u8>,
}

impl State {
    fn discard(
        &mut self,
        link: &mut impl PerceptionLink,
        meta: &FrameMeta,
        reason: DiscardReason,
    ) -> io::Result<()> {
        let counter = match reason {
            DiscardReason::Stale => &mut self.counts.stale,
            DiscardReason::PrivacyChanged | DiscardReason::NoAuthority => {
                &mut self.counts.privacy_changed
            }
            DiscardReason::DecodeFailed => &mut self.counts.decode_failed,
            DiscardReason::InferenceFailed => &mut self.counts.inference_failed,
        };
        *counter = counter.saturating_add(1);
        link.publish(PerceptionEvent::Discarded {
            frame: FrameRef::from(meta),
            reason,
            at_us: monotonic_us(),
        })
    }

    fn drain(&mut self, link: &mut impl PerceptionLink) -> io::Result<bool> {
        for _ in 0..CONTROL_BUDGET {
            match link.receive_control()? {
                None => break,
                Some(PerceptionControl::Stop) => return Ok(false),
                Some(PerceptionControl::Authority(snapshot)) => {
                    let now = monotonic_us();
                    match self.guard.install(MonoTime::from_micros(now), snapshot) {
                        Ok(()) => self.last_authority_us = now,
                        // An old or duplicate snapshot cannot replace newer authority.
                        Err(AuthorityError::StaleSnapshot) => {}
                        Err(error) => {
                            return Err(io::Error::other(format!(
                                "perception authority rejected: {error:?}"
                            )));
                        }
                    }
                }
            }
        }
        Ok(true)
    }

    fn link_lost(&mut self, link: &mut impl PerceptionLink, error: &io::Error) -> io::Result<()> {
        self.counts.link_errors = self.counts.link_errors.saturating_add(1);
        self.next_connect_us = monotonic_us() + CONNECT_RETRY_US;
        link.publish(PerceptionEvent::CameraLink {
            connected: false,
            at_us: monotonic_us(),
            error: Some(bounded_text(&error.to_string())),
        })
    }

    fn frames(&mut self, link: &mut impl PerceptionLink, jobs: &SyncSender<Job>) -> io::Result<()> {
        let now = monotonic_us();
        if !self.client.is_connected() && now >= self.next_connect_us {
            match self.client.connect() {
                Ok(true) => link.publish(PerceptionEvent::CameraLink {
                    connected: true,
                    at_us: now,
                    error: None,
                })?,
                Ok(false) => self.next_connect_us = now + CONNECT_RETRY_US,
                Err(error) => self.link_lost(link, &error)?,
            }
        }
        if !self.client.is_connected() {
            return Ok(());
        }
        if self.client.outstanding().is_none()
            && !self.busy
            && now >= self.next_request_us
            && self
                .guard
                .current_camera_grant(MonoTime::from_micros(now))
                .is_ok()
        {
            match self.client.request(self.options.max_frame_age_us) {
                Ok(_) => {
                    self.counts.requested = self.counts.requested.saturating_add(1);
                    self.next_request_us = now + self.options.interval_us;
                }
                Err(error) => return self.link_lost(link, &error),
            }
        }
        let meta = match self.client.poll(&mut self.spare) {
            Ok(Some(meta)) => meta,
            Ok(None) => return Ok(()),
            Err(error) => return self.link_lost(link, &error),
        };
        let received_at_us = monotonic_us();
        self.counts.received = self.counts.received.saturating_add(1);
        if let Some(reason) = grant_status(&mut self.guard, meta.grant, received_at_us) {
            return self.discard(link, &meta, reason);
        }
        if received_at_us.saturating_sub(meta.dequeue_completed_at_us)
            > u64::from(self.options.max_frame_age_us)
        {
            return self.discard(link, &meta, DiscardReason::Stale);
        }
        let jpeg = std::mem::take(&mut self.spare);
        jobs.try_send(Job {
            meta,
            jpeg,
            received_at_us,
        })
        .map_err(|_| io::Error::other("perception inference slot unexpectedly busy"))?;
        self.busy = true;
        Ok(())
    }

    fn finish(&mut self, link: &mut impl PerceptionLink, done: Done) -> io::Result<()> {
        self.busy = false;
        self.spare = done.jpeg;
        let now = monotonic_us();
        // Privacy may have closed while the frame was analysed.
        if let Some(reason) = grant_status(&mut self.guard, done.meta.grant, now) {
            return self.discard(link, &done.meta, reason);
        }
        if now.saturating_sub(done.meta.dequeue_completed_at_us) > self.options.max_result_age_us {
            return self.discard(link, &done.meta, DiscardReason::Stale);
        }
        let analysis = match done.result {
            Ok(analysis) => analysis,
            Err(AnalysisError::Decode(_)) => {
                return self.discard(link, &done.meta, DiscardReason::DecodeFailed);
            }
            Err(AnalysisError::Inference(_)) => {
                return self.discard(link, &done.meta, DiscardReason::InferenceFailed);
            }
        };
        self.sequence += 1;
        let confidence = analysis
            .best_score
            .map(|s| Confidence::per_mille((s * 1000.0).round().clamp(0.0, 1000.0) as u16))
            .transpose()
            .map_err(|e| io::Error::other(format!("{e:?}")))?
            .unwrap_or(Confidence::UNKNOWN);
        let provenance = Observation::new(
            self.options.worker,
            NonZeroU64::new(self.sequence).expect("starts at one"),
            MonoTime::from_micros(done.meta.dequeue_completed_at_us),
            confidence,
        );
        self.counts.published = self.counts.published.saturating_add(1);
        link.publish(PerceptionEvent::Observation {
            observation: VisualObservation {
                provenance,
                frame: FrameRef::from(&done.meta),
                timing: AnalysisTiming {
                    received_at_us: done.received_at_us,
                    decode_us: analysis.decode_us,
                    inference_us: analysis.inference_us,
                    analysed_at_us: done.analysed_at_us,
                    published_at_us: monotonic_us(),
                },
                exposure: analysis.exposure,
                presence: analysis.presence,
                faces: analysis.faces,
                attention: analysis.attention,
                model: analysis.model,
            },
        })
    }
}

/// Run until `Stop`, a fault, or controller heartbeat loss. The caller sends
/// its readiness after the analyzer has loaded, before calling this.
pub fn run(
    link: &mut impl PerceptionLink,
    analyzer: Analyzer,
    options: WorkerOptions,
) -> io::Result<()> {
    let (jobs, job_receiver) = mpsc::sync_channel::<Job>(1);
    let (done_sender, done) = mpsc::sync_channel::<Done>(1);
    let handle = thread::Builder::new()
        .name("perception-inference".into())
        .spawn(move || inference(analyzer, job_receiver, done_sender))?;
    let now = monotonic_us();
    let mut state = State {
        guard: BoundaryGuard::new(options.controller, MonoTime::from_micros(now)),
        client: FrameClient::new(
            &options.frame_socket,
            Role::Perception,
            options.worker,
            options.controller,
            options.max_frame_bytes,
        ),
        last_authority_us: now,
        next_connect_us: now,
        next_request_us: now,
        busy: false,
        sequence: 0,
        counts: PerceptionCounts::default(),
        last_progress_us: now,
        spare: Vec::new(),
        options,
    };
    let result = (|| -> io::Result<()> {
        loop {
            if !state.drain(link)? {
                return Ok(());
            }
            let now = monotonic_us();
            if now.saturating_sub(state.last_authority_us) >= AUTHORITY_TIMEOUT_US {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "perception controller heartbeat expired",
                ));
            }
            state.frames(link, &jobs)?;
            match done.try_recv() {
                Ok(finished) => state.finish(link, finished)?,
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::other("perception inference thread ended"));
                }
            }
            if now.saturating_sub(state.last_progress_us) >= PROGRESS_INTERVAL_US {
                link.publish(PerceptionEvent::Progress {
                    at_us: now,
                    counts: state.counts,
                })?;
                state.last_progress_us = now;
            }
            thread::sleep(TICK);
        }
    })();
    state.client.disconnect();
    drop(jobs);
    drop(done);
    let deadline = monotonic_us() + INFERENCE_JOIN_US;
    while !handle.is_finished() && monotonic_us() < deadline {
        thread::sleep(TICK);
    }
    if handle.is_finished() {
        let _ = handle.join();
    }
    result
}
