use crate::*;
use lamp_turn_owner::{BootId, BoundaryGuard, CameraGrant, MonoTime, Snapshot};
use std::num::NonZeroU64;

struct Slot {
    bytes: Vec<u8>,
    len: usize,
    observation: Option<FrameObservation>,
}
impl Slot {
    fn new(size: usize) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| Error::new(ErrorKind::AllocationFailed))?;
        bytes.resize(size, 0);
        Ok(Self {
            bytes,
            len: 0,
            observation: None,
        })
    }
    fn clear(&mut self) {
        self.bytes.fill(0);
        self.len = 0;
        self.observation = None;
    }
}

#[derive(Clone, Copy)]
struct Active {
    id: CaptureId,
    grant: CameraGrant,
    mode: NegotiatedMode,
    started_at: MonoTime,
    last_frame_at: Option<MonoTime>,
    last_driver_sequence: Option<u32>,
    last_driver_timestamp: Option<DriverTimestamp>,
}

/// One owner, one port, three preallocated bounded buffers. No frame-sized
/// allocation or cloning occurs in poll/delivery. Faults require a new worker
/// incarnation; privacy and ordinary stop require an explicit subsequent start.
/// A caller must not publish readiness from start or an open handle alone.
pub struct Capture<P: PortIo, C: Clock> {
    port: P,
    clock: C,
    guard: BoundaryGuard,
    config: CaptureConfig,
    worker: BootId,
    last_now: MonoTime,
    epoch: u64,
    sequence: u64,
    active: Option<Active>,
    port_open: bool,
    deferred_teardown: bool,
    teardown_started: Option<MonoTime>,
    faulted: Option<ErrorKind>,
    timestamp_rejection: Option<TimestampRejection>,
    startup_discards: Option<StartupDiscards>,
    scratch: Slot,
    pending: Slot,
    in_flight: Slot,
    counters: Counters,
}

impl<P: PortIo, C: Clock> Capture<P, C> {
    /// Borrow port metadata without granting mutable port access. The Linux
    /// port exposes no descriptor or mapping; its cancellation handle can only
    /// revoke access, never authorize or reopen capture.
    pub fn port(&self) -> &P {
        &self.port
    }

    /// Allocates bounded buffers only. It does not call the port or open capture.
    pub fn new(
        port: P,
        mut clock: C,
        controller: BootId,
        worker: BootId,
        config: CaptureConfig,
    ) -> Result<Self, Error> {
        config.validate()?;
        let now = clock
            .now()
            .map_err(|_| Error::new(ErrorKind::ClockUnavailable))?;
        Ok(Self {
            port,
            clock,
            guard: BoundaryGuard::new(controller, now),
            config,
            worker,
            last_now: now,
            epoch: 0,
            sequence: 0,
            active: None,
            port_open: false,
            deferred_teardown: false,
            teardown_started: None,
            faulted: None,
            timestamp_rejection: None,
            startup_discards: None,
            scratch: Slot::new(config.max_frame_bytes)?,
            pending: Slot::new(config.max_frame_bytes)?,
            in_flight: Slot::new(config.max_frame_bytes)?,
            counters: Counters::default(),
        })
    }

    pub fn counters(&self) -> Counters {
        self.counters
    }
    pub fn worker(&self) -> BootId {
        self.worker
    }
    /// A dedicated supervised worker can publish local revocation before a
    /// potentially blocking driver teardown. Errors and privacy changes still
    /// erase every local slot immediately. The caller must close its downstream
    /// copies, publish that boundary, then call `finish_teardown` exactly once.
    /// No start is allowed while teardown remains pending.
    pub fn with_deferred_teardown(mut self) -> Self {
        self.deferred_teardown = true;
        self
    }
    pub fn teardown_deadline(&self) -> Option<MonoTime> {
        self.teardown_started
            .and_then(|at| at.checked_add(DRIVER_TEARDOWN_BUDGET_US))
    }
    /// Finish the already sealed port, even after a fault. The deadline remains
    /// anchored at the first local seal; delayed calls never renew it.
    pub fn finish_teardown(&mut self) -> Result<StopReport, Error> {
        if self.active.is_some() || !self.deferred_teardown {
            return Err(Error::new(ErrorKind::AlreadyStarted));
        }
        let anchor = self.teardown_started.unwrap_or(self.last_now);
        self.finish_close(anchor, DRIVER_TEARDOWN_BUDGET_US, 0)
    }
    pub fn fault(&self) -> Option<ErrorKind> {
        self.faulted
    }
    /// Read-only first rejection evidence. It grants no capture/delivery rights
    /// and survives slot erasure and driver cleanup without retaining pixels.
    pub fn timestamp_rejection(&self) -> Option<TimestampRejection> {
        self.timestamp_rejection
    }

    /// First/latest known-age startup discards survive local slot erasure and
    /// cleanup. Reset only on an explicit new capture attempt; no readiness is
    /// inferred from this diagnostic record.
    pub fn startup_discards(&self) -> Option<StartupDiscards> {
        self.startup_discards
    }

    /// Trusted control only. A rejected stale/duplicate snapshot cannot close
    /// newer permitted capture. Current authority expiry still closes capture.
    pub fn install_authority(&mut self, snapshot: Snapshot) -> Result<Status, Error> {
        let now = self.time()?;
        // A newly arrived heartbeat cannot conceal an already expired active
        // lease. The priority maintenance loop must not revive capture after a
        // control gap merely because the replacement snapshot is fresh.
        if self.active.is_some() && self.guard.state().is_some_and(|s| now >= s.expires_at()) {
            return Err(self.fail(
                ErrorKind::Authority(lamp_turn_owner::Error::ExpiredState),
                now,
            ));
        }
        if let Err(error) = self.guard.install(now, snapshot) {
            if self.guard.is_faulted() {
                return Err(self.fail(ErrorKind::Authority(error), now));
            }
            self.check_active(now)?;
            return Err(Error::new(ErrorKind::Authority(error)));
        }
        self.check_active(now)?;
        self.expire_slots(now);
        Ok(self.status())
    }

    pub fn start(&mut self) -> Result<StartReport, Error> {
        let before = self.time()?;
        if self.active.is_some() || self.port_open {
            return Err(Error::new(ErrorKind::AlreadyStarted));
        }
        let grant = self
            .guard
            .current_camera_grant(before)
            .map_err(|e| Error::new(ErrorKind::Authority(e)))?;
        let Some(epoch) = self.epoch.checked_add(1).and_then(NonZeroU64::new) else {
            return Err(self.fail(ErrorKind::CounterExhausted, before));
        };
        self.epoch = epoch.get();
        let id = CaptureId {
            worker: self.worker,
            epoch,
        };
        let budget = self.authorized_budget(before, START_BUDGET_US)?;
        self.startup_discards = None;
        // A failed start may have acquired resources; exactly one cleanup attempt
        // follows, with all retained buffers invalidated before that call.
        self.port_open = true;
        let result = self.port.start(self.config, id, budget);
        let after = self.time()?;
        let timing = CallTiming {
            started_at: before,
            completed_at: after,
        };
        let mode = match result {
            Ok(mode) => mode,
            Err(error) => return Err(self.fail(ErrorKind::Port(error), after)),
        };
        if after >= budget.deadline {
            return Err(self.fail(ErrorKind::StartBudgetExceeded, after));
        }
        self.recheck_grant(grant, after)?;
        if let Err(error) = mode.validate(self.config) {
            return Err(self.fail(error.kind, after));
        }
        self.active = Some(Active {
            id,
            grant,
            mode,
            started_at: before,
            last_frame_at: None,
            last_driver_sequence: None,
            last_driver_timestamp: None,
        });
        Ok(StartReport {
            capture: id,
            mode,
            timing,
        })
    }

    /// Finite maintenance is required even with no frame demand. Camera-ready
    /// means a current retained frame under fresh authority, never just open.
    pub fn maintain(&mut self) -> Result<Status, Error> {
        let now = self.time()?;
        self.check_active(now)?;
        self.expire_slots(now);
        Ok(self.status())
    }

    /// At most one nonblocking port read. Capture lineage is minted before the
    /// read and checked after it; returned bytes never receive a newer grant.
    pub fn poll(&mut self) -> Result<PollReport, Error> {
        let maintenance_at = self.time()?;
        self.check_active(maintenance_at)?;
        self.expire_slots(maintenance_at);
        // Clearing expired slots is bounded but not instantaneous. Never reuse
        // its earlier time as permission to begin another device operation.
        let before = self.time()?;
        self.check_active(before)?;
        let active = self.active.ok_or_else(|| Error::new(ErrorKind::Closed))?;
        let grant = self
            .guard
            .current_camera_grant(before)
            .map_err(|e| self.fail(ErrorKind::Authority(e), before))?;
        if grant != active.grant {
            self.close_at(before)?;
            return Err(Error::new(ErrorKind::Authority(
                lamp_turn_owner::Error::StaleCameraGrant,
            )));
        }
        let budget = self.authorized_budget(before, READ_BUDGET_US)?;
        let result = self
            .port
            .try_frame(&mut self.scratch.bytes[..active.mode.size_image], budget);
        let after = self.time()?;
        let timing = CallTiming {
            started_at: before,
            completed_at: after,
        };
        // Permission is checked even when the port reports WouldBlock or a fault.
        self.recheck_grant(grant, after)?;
        if after >= budget.deadline {
            return Err(self.fail(ErrorKind::ReadBudgetExceeded, after));
        }
        self.check_active(after)?;
        let frame = match result {
            Ok(frame) => frame,
            Err(error) => return Err(self.fail(ErrorKind::Port(error), after)),
        };
        let observation = if let Some(frame) = frame {
            let discard = match self.validate_frame(frame, active, timing) {
                Ok(discard) => discard,
                Err(kind) => return Err(self.fail(kind, after)),
            };
            // Sequence/timestamp continuity includes discarded startup reads.
            // Deliberate discards are not driver gaps or accepted observations.
            if let Some(previous) = active.last_driver_sequence {
                let missing = frame.driver_sequence.wrapping_sub(previous) - 1;
                self.counters.driver_sequence_gaps = self
                    .counters
                    .driver_sequence_gaps
                    .saturating_add(u64::from(missing));
            }
            let active = Active {
                last_driver_sequence: Some(frame.driver_sequence),
                last_driver_timestamp: frame.timestamp,
                ..active
            };
            self.active = Some(active);
            if let Some(comparison) = discard {
                let discarded = StartupDiscard {
                    age_us: comparison.comparison_at_us - comparison.timestamp.micros,
                    comparison,
                };
                match self.startup_discards.as_mut() {
                    Some(summary) => {
                        summary.count = summary.count.saturating_add(1);
                        summary.latest = discarded;
                    }
                    None => {
                        self.startup_discards = Some(StartupDiscards {
                            count: 1,
                            first: discarded,
                            latest: discarded,
                        });
                    }
                }
                self.scratch.clear();
                // No observation/token, accepted-frame clock or restart. The
                // common post-read authority/budget/deadline checks still run.
                None
            } else {
                let Some(sequence) = self.sequence.checked_add(1).and_then(NonZeroU64::new) else {
                    return Err(self.fail(ErrorKind::CounterExhausted, after));
                };
                self.sequence = sequence.get();
                let observation = FrameObservation {
                    id: FrameId {
                        capture: active.id,
                        sequence,
                    },
                    source: frame.source,
                    grant,
                    mode: active.mode,
                    driver_sequence: frame.driver_sequence,
                    driver_timestamp: frame.timestamp,
                    dequeue: timing,
                };
                self.scratch.len = frame.bytes_used;
                self.scratch.observation = Some(observation);
                if self.pending.observation.is_some() {
                    self.counters.pending_replaced =
                        self.counters.pending_replaced.saturating_add(1);
                }
                std::mem::swap(&mut self.scratch, &mut self.pending);
                self.scratch.clear();
                self.counters.frames_received = self.counters.frames_received.saturating_add(1);
                self.active = Some(Active {
                    last_frame_at: Some(after),
                    last_driver_sequence: Some(frame.driver_sequence),
                    last_driver_timestamp: frame.timestamp,
                    ..active
                });
                Some(observation)
            }
        } else {
            None
        };
        let completed_at = self.time()?;
        self.recheck_grant(grant, completed_at)?;
        if completed_at >= budget.deadline {
            return Err(self.fail(ErrorKind::ReadBudgetExceeded, completed_at));
        }
        self.check_active(completed_at)?;
        self.expire_slots(completed_at);
        Ok(PollReport {
            frame: observation,
            read_timing: timing,
            completed_at,
            status: self.status(),
        })
    }

    /// Moves the latest slot to a single in-flight slot without copying bytes.
    /// A slow consumer cannot stop acquisition; only the pending slot is replaced.
    pub fn begin_delivery(&mut self) -> Result<Option<DeliveryToken>, Error> {
        self.maintain()?;
        if self.in_flight.observation.is_some() {
            return Err(Error::new(ErrorKind::DeliveryBusy));
        }
        let Some(observation) = self.pending.observation else {
            return Ok(None);
        };
        let now = self.time()?;
        self.recheck_grant(observation.grant, now)?;
        if observation
            .dequeue_age_us(now)
            .is_none_or(|age| age >= MAX_DEQUEUE_AGE_US)
        {
            self.pending.clear();
            self.counters.aged_out = self.counters.aged_out.saturating_add(1);
            return Ok(None);
        }
        std::mem::swap(&mut self.pending, &mut self.in_flight);
        Ok(Some(DeliveryToken(observation.id)))
    }

    /// Borrow only for a bounded nonblocking copy/handoff. Release the borrow
    /// before polling control, waiting, inference, or any network operation.
    pub fn delivery_view(&mut self, token: &DeliveryToken) -> Result<FrameView<'_>, Error> {
        self.maintain()?;
        let observation = self
            .in_flight
            .observation
            .filter(|o| o.id == token.0)
            .ok_or_else(|| Error::new(ErrorKind::StaleDelivery))?;
        let now = self.time()?;
        self.recheck_grant(observation.grant, now)?;
        if observation
            .dequeue_age_us(now)
            .is_none_or(|age| age >= MAX_DEQUEUE_AGE_US)
        {
            self.in_flight.clear();
            self.counters.aged_out = self.counters.aged_out.saturating_add(1);
            return Err(Error::new(ErrorKind::StaleDelivery));
        }
        Ok(FrameView {
            observation,
            bytes: &self.in_flight.bytes[..self.in_flight.len],
        })
    }

    /// Completes one nonblocking handoff, not provider processing or inference.
    /// Old completion tokens never release a newer in-flight frame.
    pub fn finish_delivery(&mut self, token: DeliveryToken) -> Result<(), Error> {
        self.maintain()?;
        if self.in_flight.observation.is_none_or(|o| o.id != token.0) {
            return Err(Error::new(ErrorKind::StaleDelivery));
        }
        self.in_flight.clear();
        self.counters.delivery_completed = self.counters.delivery_completed.saturating_add(1);
        Ok(())
    }

    /// Erase frames/tokens before asking the port to close. A later allowed
    /// snapshot does not automatically restart capture.
    pub fn stop(&mut self) -> Result<StopReport, Error> {
        let now = self.time()?;
        self.close_at(now)
    }

    /// A malformed control message, disconnected control stream or supervised
    /// local failure requires a new worker; snapshots cannot recover this one.
    pub fn control_lost(&mut self) -> Error {
        let now = match self.read_clock() {
            Ok(now) => now,
            Err(_) => self.last_now,
        };
        self.fail(
            ErrorKind::Authority(lamp_turn_owner::Error::NoAuthority),
            now,
        )
    }

    fn time(&mut self) -> Result<MonoTime, Error> {
        if self.faulted.is_some() {
            return Err(Error::new(ErrorKind::Faulted));
        }
        self.read_clock()
            .map_err(|kind| self.fail(kind, self.last_now))
    }
    fn read_clock(&mut self) -> Result<MonoTime, ErrorKind> {
        let now = self.clock.now().map_err(|_| ErrorKind::ClockUnavailable)?;
        if now < self.last_now {
            return Err(ErrorKind::ClockRegression);
        }
        self.last_now = now;
        Ok(now)
    }
    fn authorized_budget(&mut self, now: MonoTime, micros: u64) -> Result<OperationBudget, Error> {
        let deadline = now
            .checked_add(micros)
            .ok_or_else(|| self.fail(ErrorKind::CounterExhausted, now))?;
        let expires = self
            .guard
            .state()
            .ok_or_else(|| Error::new(ErrorKind::Authority(lamp_turn_owner::Error::NoAuthority)))?
            .expires_at();
        Ok(OperationBudget {
            started_at: now,
            deadline: deadline.min(expires),
        })
    }
    fn recheck_grant(&mut self, original: CameraGrant, now: MonoTime) -> Result<(), Error> {
        match self.guard.current_camera_grant(now) {
            Ok(current) if current == original => Ok(()),
            Ok(_) | Err(lamp_turn_owner::Error::PrivacyClosed) => {
                self.close_at(now)?;
                Err(Error::new(ErrorKind::Authority(
                    lamp_turn_owner::Error::StaleCameraGrant,
                )))
            }
            Err(error) => Err(self.fail(ErrorKind::Authority(error), now)),
        }
    }
    fn check_active(&mut self, now: MonoTime) -> Result<(), Error> {
        let Some(active) = self.active else {
            return Ok(());
        };
        match self.guard.current_camera_grant(now) {
            Ok(grant) if grant == active.grant => {}
            Ok(_) | Err(lamp_turn_owner::Error::PrivacyClosed) => {
                self.close_at(now)?;
                return Ok(());
            }
            Err(error) => return Err(self.fail(ErrorKind::Authority(error), now)),
        }
        let (anchor, timeout, kind) = match active.last_frame_at {
            Some(last) => (
                last,
                FRAME_SILENCE_DEADLINE_US,
                ErrorKind::FrameSilenceTimeout,
            ),
            None => (
                active.started_at,
                FIRST_FRAME_DEADLINE_US,
                ErrorKind::FirstFrameTimeout,
            ),
        };
        if now.as_micros() - anchor.as_micros() >= timeout {
            return Err(self.fail(kind, now));
        }
        Ok(())
    }
    fn validate_frame(
        &mut self,
        frame: PortFrame,
        active: Active,
        timing: CallTiming,
    ) -> Result<Option<TimestampRejection>, ErrorKind> {
        let now = timing.completed_at;
        if frame.capture != active.id {
            return Err(ErrorKind::OldCapture);
        }
        if frame.source != self.config.source {
            return Err(ErrorKind::WrongSource);
        }
        if frame.bytes_used < 4 || frame.bytes_used > active.mode.size_image {
            return Err(ErrorKind::InvalidFrameSize);
        }
        let bytes = &self.scratch.bytes[..frame.bytes_used];
        // Envelope only: no JPEG decoder or claim of a valid/visible scene.
        if bytes[..2] != [0xff, 0xd8] || bytes[bytes.len() - 2..] != [0xff, 0xd9] {
            return Err(ErrorKind::InvalidJpegEnvelope);
        }
        if let Some(previous) = active.last_driver_sequence {
            let delta = frame.driver_sequence.wrapping_sub(previous);
            if delta == 0 || delta >= (1 << 31) {
                return Err(ErrorKind::OldDriverSequence);
            }
        }
        if let Some(timestamp) = frame.timestamp
            && timestamp.domain == TimestampDomain::HostMonotonic
        {
            let regressed = active.last_driver_timestamp.is_some_and(|previous| {
                timestamp.domain == previous.domain
                    && timestamp.point == previous.point
                    && timestamp.micros < previous.micros
            });
            // Keep the original first-failed comparison order for fatal evidence.
            // Startup discards additionally require no hidden regression.
            let predicate = if timestamp.micros > now.as_micros() {
                Some(TimestampPredicate::Future)
            } else if timestamp.micros < active.started_at.as_micros() {
                Some(TimestampPredicate::BeforeCaptureStart)
            } else if timestamp.point != TimestampPoint::Unknown
                && now.as_micros() - timestamp.micros >= MAX_DEQUEUE_AGE_US
            {
                Some(TimestampPredicate::TooOld)
            } else if regressed {
                Some(TimestampPredicate::Regressed)
            } else {
                None
            };
            if let Some(predicate) = predicate {
                let rejection = TimestampRejection {
                    capture_epoch: active.id.epoch.get(),
                    driver_sequence: frame.driver_sequence,
                    timestamp,
                    capture_started_at_us: active.started_at.as_micros(),
                    dequeue_started_at_us: timing.started_at.as_micros(),
                    comparison_at_us: now.as_micros(),
                    maximum_age_us: MAX_DEQUEUE_AGE_US,
                    previous_timestamp: active.last_driver_timestamp,
                    predicate,
                };
                if predicate == TimestampPredicate::TooOld
                    && active.last_frame_at.is_none()
                    && !regressed
                {
                    return Ok(Some(rejection));
                }
                self.timestamp_rejection.get_or_insert(rejection);
                return Err(ErrorKind::InvalidDriverTimestamp);
            }
        }
        Ok(None)
    }
    fn expire_slots(&mut self, now: MonoTime) {
        for slot in [&mut self.pending, &mut self.in_flight] {
            if slot.observation.is_some_and(|o| {
                o.dequeue_age_us(now)
                    .is_none_or(|age| age >= MAX_DEQUEUE_AGE_US)
            }) {
                slot.clear();
                self.counters.aged_out = self.counters.aged_out.saturating_add(1);
            }
        }
    }
    fn status(&self) -> Status {
        let latest_pending = self.pending.observation.map(|o| o.id);
        let delivery_in_flight = self.in_flight.observation.map(|o| o.id);
        let phase = if self.faulted.is_some() {
            Phase::Faulted
        } else if self.active.is_none() {
            Phase::Closed
        } else if latest_pending.is_some() || delivery_in_flight.is_some() {
            Phase::Ready
        } else {
            Phase::Acquiring
        };
        Status {
            phase,
            capture: self.active.map(|a| a.id),
            latest_pending,
            delivery_in_flight,
        }
    }
    fn close_at(&mut self, now: MonoTime) -> Result<StopReport, Error> {
        let result = self.cleanup(now);
        self.close_result(result)
    }
    fn close_result(
        &mut self,
        result: Result<StopReport, CleanupFailure>,
    ) -> Result<StopReport, Error> {
        match result {
            Ok(report) => Ok(report),
            Err(cleanup) => {
                let kind = cleanup
                    .clock
                    .or(cleanup.port.map(ErrorKind::Port))
                    .unwrap_or(ErrorKind::StopBudgetExceeded);
                self.faulted = Some(kind);
                self.guard.invalidate();
                Err(Error {
                    kind,
                    cleanup: Some(cleanup),
                })
            }
        }
    }
    fn fail(&mut self, kind: ErrorKind, now: MonoTime) -> Error {
        self.faulted = Some(kind);
        self.guard.invalidate();
        Error {
            kind,
            cleanup: self.cleanup(now).err(),
        }
    }
    fn cleanup(&mut self, now: MonoTime) -> Result<StopReport, CleanupFailure> {
        let invalidated_frames = u64::from(self.pending.observation.is_some())
            + u64::from(self.in_flight.observation.is_some());
        self.counters.invalidated = self.counters.invalidated.saturating_add(invalidated_frames);
        self.scratch.clear();
        self.pending.clear();
        self.in_flight.clear();
        self.active = None;
        if self.deferred_teardown && self.port_open {
            self.teardown_started.get_or_insert(now);
            return Ok(StopReport {
                timing: None,
                invalidated_frames,
                teardown_pending: true,
            });
        }
        self.stop_port(now, STOP_BUDGET_US, invalidated_frames)
    }
    fn finish_close(
        &mut self,
        anchor: MonoTime,
        duration_us: u64,
        invalidated_frames: u64,
    ) -> Result<StopReport, Error> {
        let result = self.stop_port(anchor, duration_us, invalidated_frames);
        self.close_result(result)
    }
    fn stop_port(
        &mut self,
        anchor: MonoTime,
        duration_us: u64,
        invalidated_frames: u64,
    ) -> Result<StopReport, CleanupFailure> {
        if !self.port_open {
            return Ok(StopReport {
                timing: None,
                invalidated_frames,
                teardown_pending: false,
            });
        }
        self.port_open = false;
        self.teardown_started = None;
        // Read actual call start independently from the original seal/deadline.
        // On clock failure still make the one mandatory cleanup attempt.
        let before = self.read_clock();
        let now = before.unwrap_or(self.last_now);
        let Some(deadline) = anchor.checked_add(duration_us) else {
            // Still perform one close when clock arithmetic cannot represent a
            // later deadline. A zero remaining budget never permits a retry.
            let port = self
                .port
                .stop(OperationBudget {
                    started_at: now,
                    deadline: now,
                })
                .err();
            return Err(CleanupFailure {
                port,
                clock: Some(ErrorKind::CounterExhausted),
                timing: None,
                budget_exceeded: true,
            });
        };
        let port = self
            .port
            .stop(OperationBudget {
                started_at: now,
                deadline,
            })
            .err();
        let (timing, mut clock) = match self.read_clock() {
            Ok(after) => (
                Some(CallTiming {
                    started_at: now,
                    completed_at: after,
                }),
                None,
            ),
            Err(error) => (None, Some(error)),
        };
        clock = before.err().or(clock);
        let budget_exceeded = timing.is_some_and(|t| t.completed_at >= deadline);
        if port.is_some() || clock.is_some() || budget_exceeded {
            Err(CleanupFailure {
                port,
                clock,
                timing,
                budget_exceeded,
            })
        } else {
            Ok(StopReport {
                timing,
                invalidated_frames,
                teardown_pending: false,
            })
        }
    }
}

impl<P: PortIo, C: Clock> Drop for Capture<P, C> {
    fn drop(&mut self) {
        // No hidden PortIo operation or retry on Drop. A physical backend must
        // release its owned resources on Drop; explicit stop reports failures.
        self.scratch.clear();
        self.pending.clear();
        self.in_flight.clear();
    }
}
