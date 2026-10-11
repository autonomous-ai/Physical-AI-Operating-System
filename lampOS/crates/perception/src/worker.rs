//! The perception worker loop, independent of the runtime's wire types.
//!
//! The control loop stays responsive: inference runs on one dedicated thread
//! with a single job slot, so authority, stop and privacy changes are drained
//! every tick even while a frame is being analysed. The worker requests a
//! frame only while it holds a current camera grant, rechecks that grant when
//! the frame arrives and again before publishing, and bounds both frame and
//! result age. Without a frame there is no observation; nothing is inferred
//! from silence on the frame link.
//!
//! With a `context` consumer (the provider), the worker also serves small
//! visual-context images: the newest analysed frame reduced to at most
//! [`CONTEXT_MAX_WIDTH`] columns and re-encoded to at most
//! [`CONTEXT_MAX_BYTES`], keeping the original frame identity and grant. The
//! reduced copy is retained only while its grant is current and it is younger
//! than [`CONTEXT_MAX_AGE_US`]; encoding happens on request, on the inference
//! thread.
use crate::{
    analyze::{Analysis, AnalysisError, Analyzer},
    image::{Rgb, encode_bounded},
    observation::{
        AnalysisTiming, DiscardReason, FrameRef, PerceptionCounts, PerceptionEvent,
        VisualObservation, bounded_text,
    },
};
use lamp_camera::frames::{DerivedImage, FrameClient, FrameMeta, FrameServer, Role};
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
/// Agent 1's provider bound for one image message (`A1-006`).
pub const CONTEXT_MAX_BYTES: usize = 64 * 1024;
pub const CONTEXT_MAX_WIDTH: u32 = 640;
/// Context images older than this (from original dequeue) are never served.
pub const CONTEXT_MAX_AGE_US: u64 = 2_000_000;
/// The context frame link inside the private session directory.
pub const CONTEXT_SOCKET: &str = "perception.frames";

/// Result of one priority control drain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Drain {
    /// Every queued control message was read.
    Empty,
    /// The slice budget ran out with messages possibly still queued: no
    /// output work may happen until a later drain reaches `Empty`.
    Exhausted,
    Stopped,
}

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
    /// The provider's boot and socket, when visual context is served.
    pub context: Option<(BootId, PathBuf)>,
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
            context: None,
        }
    }
}

enum Job {
    Analyse {
        meta: FrameMeta,
        jpeg: Vec<u8>,
        received_at_us: u64,
        /// Also return a reduced copy for visual context.
        context: bool,
    },
    Encode {
        meta: FrameMeta,
        image: Rgb,
    },
}
enum Done {
    Analysed {
        meta: FrameMeta,
        jpeg: Vec<u8>,
        received_at_us: u64,
        analysed_at_us: u64,
        result: Result<Analysis, AnalysisError>,
        reduced: Option<Rgb>,
    },
    Encoded {
        meta: FrameMeta,
        image: Rgb,
        jpeg: Option<(Vec<u8>, u8)>,
    },
}

fn inference(mut analyzer: Analyzer, jobs: Receiver<Job>, done: SyncSender<Done>) {
    while let Ok(job) = jobs.recv() {
        let finished = match job {
            Job::Analyse {
                meta,
                jpeg,
                received_at_us,
                context,
            } => {
                let (result, reduced) = if context {
                    match analyzer.analyze_for_context(&jpeg, CONTEXT_MAX_WIDTH) {
                        Ok((analysis, reduced)) => (Ok(analysis), Some(reduced)),
                        Err(error) => (Err(error), None),
                    }
                } else {
                    (analyzer.analyze(&jpeg), None)
                };
                Done::Analysed {
                    meta,
                    jpeg,
                    received_at_us,
                    analysed_at_us: monotonic_us(),
                    result,
                    reduced,
                }
            }
            Job::Encode { meta, image } => Done::Encoded {
                jpeg: encode_bounded(&image, CONTEXT_MAX_BYTES),
                meta,
                image,
            },
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
    /// A received frame waiting for a complete drain before submission.
    received: Option<(FrameMeta, Vec<u8>, u64)>,
    /// A finished job waiting for a complete drain before publication.
    ready: Option<Done>,
    context: Option<FrameServer>,
    /// The newest analysed frame, reduced, while its grant is current.
    context_source: Option<(FrameMeta, Rgb)>,
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

    /// Read control messages until the queue is empty, a stop, or the shared
    /// per-iteration budget is spent. Exhaustion is reported, never treated
    /// as an empty queue.
    fn drain(&mut self, link: &mut impl PerceptionLink, budget: &mut usize) -> io::Result<Drain> {
        while *budget > 0 {
            *budget -= 1;
            match link.receive_control()? {
                None => return Ok(Drain::Empty),
                Some(PerceptionControl::Stop) => return Ok(Drain::Stopped),
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
        Ok(Drain::Exhausted)
    }

    fn heartbeat(&self) -> io::Result<()> {
        if monotonic_us().saturating_sub(self.last_authority_us) >= AUTHORITY_TIMEOUT_US {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "perception controller heartbeat expired",
            ));
        }
        Ok(())
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

    /// Connect, request and receive. A received frame is held for `submit`.
    fn fetch(&mut self, link: &mut impl PerceptionLink) -> io::Result<()> {
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
            && self.received.is_none()
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
        self.counts.received = self.counts.received.saturating_add(1);
        let jpeg = std::mem::take(&mut self.spare);
        self.received = Some((meta, jpeg, monotonic_us()));
        Ok(())
    }

    /// Check the held frame's lineage and age (after a complete drain), then
    /// hand it to inference.
    fn submit(&mut self, link: &mut impl PerceptionLink, jobs: &SyncSender<Job>) -> io::Result<()> {
        let Some((meta, jpeg, received_at_us)) = self.received.take() else {
            return Ok(());
        };
        let now = monotonic_us();
        if let Some(reason) = grant_status(&mut self.guard, meta.grant, now) {
            self.spare = jpeg;
            return self.discard(link, &meta, reason);
        }
        if now.saturating_sub(meta.dequeue_completed_at_us)
            > u64::from(self.options.max_frame_age_us)
        {
            self.spare = jpeg;
            return self.discard(link, &meta, DiscardReason::Stale);
        }
        jobs.try_send(Job::Analyse {
            meta,
            jpeg,
            received_at_us,
            context: self.context.is_some(),
        })
        .map_err(|_| io::Error::other("perception inference slot unexpectedly busy"))?;
        self.busy = true;
        Ok(())
    }

    /// Drop the retained context image once its grant or age no longer holds
    /// and keep the context link serviced. Returns whether an encode should
    /// start for a waiting consumer.
    fn service_context(&mut self, link: &mut impl PerceptionLink) -> io::Result<bool> {
        let now = monotonic_us();
        if let Some((meta, _)) = &self.context_source
            && (grant_status(&mut self.guard, meta.grant, now).is_some()
                || now.saturating_sub(meta.dequeue_completed_at_us) > CONTEXT_MAX_AGE_US)
        {
            self.context_source = None;
        }
        let Some(server) = self.context.as_mut() else {
            return Ok(false);
        };
        server.service(now);
        while let Some(event) = server.take_event() {
            link.publish(PerceptionEvent::ContextLink { event })?;
        }
        Ok(!self.busy && server.wants_frame() && self.context_source.is_some())
    }

    fn submit_encode(&mut self, jobs: &SyncSender<Job>) -> io::Result<()> {
        if let Some((meta, image)) = self.context_source.take() {
            jobs.try_send(Job::Encode { meta, image })
                .map_err(|_| io::Error::other("perception inference slot unexpectedly busy"))?;
            self.busy = true;
        }
        Ok(())
    }

    fn finish(&mut self, link: &mut impl PerceptionLink, done: Done) -> io::Result<()> {
        self.busy = false;
        let (meta, jpeg, received_at_us, analysed_at_us, result, reduced) = match done {
            Done::Analysed {
                meta,
                jpeg,
                received_at_us,
                analysed_at_us,
                result,
                reduced,
            } => (meta, jpeg, received_at_us, analysed_at_us, result, reduced),
            Done::Encoded { meta, image, jpeg } => return self.encoded(meta, image, jpeg),
        };
        self.spare = jpeg;
        let now = monotonic_us();
        // Privacy may have closed while the frame was analysed.
        if let Some(reason) = grant_status(&mut self.guard, meta.grant, now) {
            return self.discard(link, &meta, reason);
        }
        if now.saturating_sub(meta.dequeue_completed_at_us) > self.options.max_result_age_us {
            return self.discard(link, &meta, DiscardReason::Stale);
        }
        let analysis = match result {
            Ok(analysis) => analysis,
            Err(AnalysisError::Decode(_)) => {
                return self.discard(link, &meta, DiscardReason::DecodeFailed);
            }
            Err(AnalysisError::Inference(_)) => {
                return self.discard(link, &meta, DiscardReason::InferenceFailed);
            }
        };
        if let Some(reduced) = reduced {
            self.context_source = Some((meta.clone(), reduced));
        }
        let done_meta = meta;
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
            MonoTime::from_micros(done_meta.dequeue_completed_at_us),
            confidence,
        );
        self.counts.published = self.counts.published.saturating_add(1);
        link.publish(PerceptionEvent::Observation {
            observation: VisualObservation {
                provenance,
                frame: FrameRef::from(&done_meta),
                timing: AnalysisTiming {
                    received_at_us,
                    decode_us: analysis.decode_us,
                    inference_us: analysis.inference_us,
                    analysed_at_us,
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

    /// A context encode finished: recheck lineage and age, then offer it.
    fn encoded(
        &mut self,
        meta: FrameMeta,
        image: Rgb,
        jpeg: Option<(Vec<u8>, u8)>,
    ) -> io::Result<()> {
        let now = monotonic_us();
        if grant_status(&mut self.guard, meta.grant, now).is_some()
            || now.saturating_sub(meta.dequeue_completed_at_us) > CONTEXT_MAX_AGE_US
        {
            return Ok(());
        }
        let Some((bytes, quality)) = jpeg else {
            self.counts.context_oversize = self.counts.context_oversize.saturating_add(1);
            return Ok(());
        };
        let mut described = meta.clone();
        described.derived = Some(DerivedImage {
            source_width: meta.width,
            source_height: meta.height,
            quality,
        });
        described.width = image.width;
        described.height = image.height;
        if let Some(server) = self.context.as_mut() {
            let served = server.offer_meta(&described, &bytes, now)?;
            self.counts.context_served = self.counts.context_served.saturating_add(served as u64);
        }
        // A newer analysis may already have replaced the retained source.
        if self.context_source.is_none() {
            self.context_source = Some((meta, image));
        }
        Ok(())
    }
}

/// Run until `Stop`, a fault, or controller heartbeat loss. The caller sends
/// its readiness after the analyzer has loaded, before calling this.
pub fn run(
    link: &mut impl PerceptionLink,
    analyzer: Analyzer,
    options: WorkerOptions,
) -> io::Result<()> {
    let context = options
        .context
        .as_ref()
        .map(|(boot, socket)| {
            FrameServer::bind(
                socket,
                options.controller,
                &[(Role::Context, *boot)],
                CONTEXT_MAX_BYTES,
            )
        })
        .transpose()?;
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
        received: None,
        ready: None,
        context,
        context_source: None,
        options,
    };
    let result = (|| -> io::Result<()> {
        // Output work (submitting a frame, publishing a result, offering a
        // context image) happens only after a drain that reached an empty
        // control queue, and each action drains again immediately before it.
        // Work interrupted by an exhausted slice is held, not dropped, and
        // its lineage is rechecked when it resumes.
        loop {
            let mut budget = CONTROL_BUDGET;
            match state.drain(link, &mut budget)? {
                Drain::Stopped => return Ok(()),
                Drain::Exhausted => {
                    state.heartbeat()?;
                    thread::sleep(TICK);
                    continue;
                }
                Drain::Empty => {}
            }
            state.heartbeat()?;
            if state.ready.is_none() {
                match done.try_recv() {
                    Ok(finished) => state.ready = Some(finished),
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        return Err(io::Error::other("perception inference thread ended"));
                    }
                }
            }
            if state.ready.is_some() {
                match state.drain(link, &mut budget)? {
                    Drain::Stopped => return Ok(()),
                    Drain::Exhausted => {
                        thread::sleep(TICK);
                        continue;
                    }
                    Drain::Empty => {
                        let finished = state.ready.take().expect("checked");
                        state.finish(link, finished)?;
                    }
                }
            }
            state.fetch(link)?;
            if state.received.is_some() && !state.busy {
                match state.drain(link, &mut budget)? {
                    Drain::Stopped => return Ok(()),
                    Drain::Exhausted => {
                        thread::sleep(TICK);
                        continue;
                    }
                    Drain::Empty => state.submit(link, &jobs)?,
                }
            }
            if state.service_context(link)? {
                match state.drain(link, &mut budget)? {
                    Drain::Stopped => return Ok(()),
                    Drain::Exhausted => {
                        thread::sleep(TICK);
                        continue;
                    }
                    Drain::Empty => state.submit_encode(&jobs)?,
                }
            }
            let now = monotonic_us();
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
    if let Some(server) = state.context.as_mut() {
        server.close_all(lamp_camera::frames::CloseReason::Stopped, monotonic_us());
    }
    state.context_source = None;
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
