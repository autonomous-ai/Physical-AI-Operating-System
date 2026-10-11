//! Disable failed presentation immediately; observe cleanup without waiting on it.
use super::*;

const STOP_GRACE_US: u64 = 300_000;
const CLEANUP_EVENT_BUDGET: usize = 16;

pub(super) struct OptionalRing {
    worker: Option<Worker>,
    degraded: Option<Degraded>,
}

struct Degraded {
    at_us: u64,
    cause: String,
    notice_pending: bool,
    stop_error: Option<String>,
    cleanup_error: Option<String>,
    blank_confirmed: bool,
    exit_observed: bool,
    kill_requested: bool,
}

impl From<Option<Worker>> for OptionalRing {
    fn from(worker: Option<Worker>) -> Self {
        Self {
            worker,
            degraded: None,
        }
    }
}

impl OptionalRing {
    pub(super) fn active_mut(&mut self) -> Option<&mut Worker> {
        if self.degraded.is_none() {
            self.worker.as_mut()
        } else {
            None
        }
    }

    /// Includes the disabled child: losing availability never loses ownership.
    pub(super) fn iter_mut(&mut self) -> std::option::IterMut<'_, Worker> {
        self.worker.iter_mut()
    }

    pub(super) fn is_some(&self) -> bool {
        self.worker.is_some()
    }
    pub(super) fn is_active(&self) -> bool {
        self.worker.is_some() && self.degraded.is_none()
    }

    pub(super) fn blank_confirmed(&self) -> bool {
        self.worker.is_none() || self.degraded.as_ref().is_some_and(|d| d.blank_confirmed)
    }

    pub(super) fn stop_requested_at(&self, normal_shutdown_at: u64) -> u64 {
        self.degraded
            .as_ref()
            .map_or(normal_shutdown_at, |d| d.at_us)
    }

    pub(super) fn cleanup_error(&self) -> Option<io::Error> {
        self.degraded
            .as_ref()
            .and_then(|d| d.cleanup_error.as_ref())
            .map(|message| io::Error::other(message.clone()))
    }

    pub(super) fn disable(&mut self, cause: impl std::fmt::Display) {
        if self.degraded.is_some() || self.worker.is_none() {
            return;
        }
        // Latch before attempting any I/O. No subsequent authority or frame can
        // reach this child, even when its Stop socket is full or disconnected.
        self.degraded = Some(Degraded {
            at_us: monotonic_us(),
            cause: cause.to_string(),
            notice_pending: true,
            stop_error: None,
            cleanup_error: None,
            blank_confirmed: false,
            exit_observed: false,
            kill_requested: false,
        });
        if let Err(error) = self
            .worker
            .as_mut()
            .unwrap()
            .channels
            .control
            .send(Control::Stop)
        {
            let state = self.degraded.as_mut().unwrap();
            state.stop_error = Some(error.to_string());
            state.cleanup_error = Some(format!("ring stop delivery failed: {error}"));
        }
    }

    pub(super) fn publish(&mut self, snapshot: Snapshot) {
        if let Some(worker) = self.active_mut()
            && let Err(error) = worker
                .channels
                .control
                .send(Control::Authority { snapshot })
        {
            self.disable(format!("ring authority transport failed: {error}"));
        }
    }

    pub(super) fn check_running(&mut self) {
        if let Some(worker) = self.active_mut()
            && let Err(error) = worker.check_running()
        {
            self.disable(error);
        }
    }

    /// Pre-readiness progress shares the existing startup cadence, not a wait.
    pub(super) fn startup_tick(
        &mut self,
        startup: &mut StartupEvents,
        snapshot: Option<Snapshot>,
    ) -> io::Result<()> {
        if let Some(worker) = self.active_mut()
            && let Err(error) = startup.poll(&mut worker.channels, "ring")
        {
            self.disable(error);
        }
        self.check_running();
        if let Some(snapshot) = snapshot {
            self.publish(snapshot);
        }
        self.poll_cleanup_with_clock(|event| Ok(startup.push(event)?), monotonic_us)
            .map_err(io::Error::other)
    }

    /// At most sixteen receives, one waitpid(WNOHANG), and one signal attempt.
    /// This never joins, sleeps, restarts or performs hardware I/O. A kernel
    /// signal/waitpid call has no userspace hard-deadline guarantee.
    pub(super) fn poll_cleanup(&mut self, trace: &mut SessionState) -> Result<()> {
        self.poll_cleanup_with_clock(|event| record(trace, event), monotonic_us)
    }

    fn poll_cleanup_with_clock(
        &mut self,
        mut emit: impl FnMut(Value) -> Result<()>,
        mut clock: impl FnMut() -> u64,
    ) -> Result<()> {
        let (Some(worker), Some(state)) = (&mut self.worker, &mut self.degraded) else {
            return Ok(());
        };
        if state.notice_pending {
            emit(json!({"kind":"ring_degraded", "at_us":state.at_us,
                "worker_boot":worker.boot, "reason":state.cause,
                "stop_delivery_error":state.stop_error, "audio_continues":true,
                "light_available":false, "cleanup_confirmed":false}))?;
            state.notice_pending = false;
        }
        for _ in 0..CLEANUP_EVENT_BUDGET {
            match worker.channels.control.receive::<WorkerEvent>() {
                Ok(Some(WorkerEvent::Ring {
                    report:
                        report @ RingFeedback::Blanked {
                            reason: RingBlankReason::Shutdown,
                            ..
                        },
                })) => {
                    let at_us = clock();
                    let valid =
                        !state.blank_confirmed && report.confirms_shutdown(state.at_us, at_us);
                    if valid {
                        state.blank_confirmed = true;
                    } else {
                        state
                            .cleanup_error
                            .get_or_insert_with(|| "invalid degraded ring shutdown receipt".into());
                    }
                    emit(json!({"kind":"ring_degraded_blank", "at_us":at_us,
                        "details":report, "valid":valid,
                        "boundary":"SPI write return; optical darkness unmeasured"}))?;
                }
                Ok(Some(event)) => {
                    let at_us = clock();
                    if !matches!(event, WorkerEvent::Ring { .. }) {
                        state.cleanup_error.get_or_insert_with(|| {
                            format!("unexpected degraded ring event: {event:?}")
                        });
                    }
                    // Old presentation receipts remain historical. They cannot
                    // reach choreography or signal listening readiness.
                    emit(json!({"kind":"ring_degraded_event", "at_us":at_us, "details":event}))?;
                }
                Ok(None) => break,
                Err(error) => {
                    state
                        .cleanup_error
                        .get_or_insert_with(|| format!("ring cleanup transport failed: {error}"));
                    break;
                }
            }
        }
        if !state.exit_observed {
            match worker.try_exit() {
                Ok(Some(status)) => {
                    let at_us = clock();
                    state.exit_observed = true;
                    if let Err(error) = worker.exit_result(status) {
                        state.cleanup_error.get_or_insert_with(|| error.to_string());
                    }
                    emit(json!({"kind":"ring_degraded_exit", "at_us":at_us,
                        "success":status.success(), "status":status.to_string(),
                        "blank_write_confirmed":state.blank_confirmed}))?;
                }
                Ok(None) => {}
                Err(error) => {
                    state
                        .cleanup_error
                        .get_or_insert_with(|| format!("ring cleanup status failed: {error}"));
                }
            }
        }
        let at_us = clock();
        if !state.exit_observed
            && !state.kill_requested
            && at_us.saturating_sub(state.at_us) >= STOP_GRACE_US
        {
            state.kill_requested = true;
            let result = worker.request_termination();
            state.cleanup_error.get_or_insert_with(|| {
                "ring exceeded cooperative stop grace; forced termination requested".into()
            });
            emit(json!({"kind":"ring_degraded_kill_requested", "at_us":at_us,
                "error":result.err().map(|e| e.to_string()), "exit_confirmed":false}))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hung_ring_gets_one_non_waiting_signal_without_a_cleanup_success_claim() {
        let directory = SessionDirectory::create().unwrap();
        let (worker, mut peer) = Worker::sleeping_fixture(&directory.path, "ring");
        let mut ring = OptionalRing::from(Some(worker));
        ring.disable("test hung SPI");
        assert!(matches!(
            peer.control.receive().unwrap(),
            Some(Control::Stop)
        ));
        let at = ring.stop_requested_at(0);
        let mut trace = SessionState::default();
        ring.poll_cleanup_with_clock(|event| record(&mut trace, event), || at + STOP_GRACE_US - 1)
            .unwrap();
        assert!(!ring.degraded.as_ref().unwrap().kill_requested);
        ring.poll_cleanup_with_clock(|event| record(&mut trace, event), || at + STOP_GRACE_US)
            .unwrap();
        assert!(ring.degraded.as_ref().unwrap().kill_requested);
        assert!(!ring.degraded.as_ref().unwrap().exit_observed);
        assert!(!ring.blank_confirmed());
        assert!(ring.cleanup_error().is_some());
        ring.poll_cleanup_with_clock(|event| record(&mut trace, event), || at + STOP_GRACE_US + 1)
            .unwrap();
        assert_eq!(
            trace
                .iter()
                .filter(|e| e["kind"] == "ring_degraded_kill_requested")
                .count(),
            1
        );
        assert!(ring.iter_mut().next().unwrap().shutdown().is_err());
        assert!(
            ring.iter_mut()
                .next()
                .unwrap()
                .try_exit()
                .unwrap()
                .is_some()
        );
    }
}
