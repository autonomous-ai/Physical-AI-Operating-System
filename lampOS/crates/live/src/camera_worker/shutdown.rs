//! Parent validation of local revocation versus separately bounded driver work.
use super::{CameraEvent, StopReason};
use lamp_interaction::BootId;
use std::io;

pub struct ShutdownWatch {
    worker: BootId,
    revoke_deadline_us: u64,
    revoked: Option<(u64, u64, StopReason)>,
    stopped: bool,
    failed: bool,
}
impl ShutdownWatch {
    pub fn new(worker: BootId, requested_at_us: u64) -> Self {
        Self {
            worker,
            revoke_deadline_us: requested_at_us.saturating_add(lamp_camera::REVOKE_TARGET_US),
            revoked: None,
            stopped: false,
            failed: false,
        }
    }
    pub fn deadline_us(&self) -> u64 {
        self.revoked
            .map_or(self.revoke_deadline_us, |(_, deadline, _)| deadline)
    }
    pub fn expire(&mut self, now_us: u64) -> bool {
        if now_us >= self.deadline_us() {
            self.failed = true;
            true
        } else {
            false
        }
    }
    /// Preserve transport/decoding failures that cannot produce a typed receipt.
    /// Invalidating never changes either original shutdown deadline.
    pub(crate) fn invalidate(&mut self) {
        self.failed = true;
    }
    pub fn observe(&mut self, event: &CameraEvent, now_us: u64) -> io::Result<()> {
        let result = self.observe_inner(event, now_us);
        self.failed |= result.is_err();
        result
    }
    fn observe_inner(&mut self, event: &CameraEvent, now_us: u64) -> io::Result<()> {
        match event {
            CameraEvent::Revoked {
                at_us,
                worker,
                reason,
                teardown_deadline_us,
                error,
                ..
            } => {
                if self.failed
                    || self.revoked.is_some()
                    || self.stopped
                    || *worker != self.worker
                    || *at_us > now_us
                    || now_us.saturating_sub(*at_us) >= 100_000
                    || now_us > self.revoke_deadline_us
                    || *teardown_deadline_us < *at_us
                    || teardown_deadline_us.saturating_sub(*at_us)
                        > lamp_camera::DRIVER_TEARDOWN_BUDGET_US
                {
                    return Err(io::Error::other(
                        "invalid or late camera local-revocation receipt",
                    ));
                }
                self.revoked = Some((*at_us, *teardown_deadline_us, *reason));
                if *reason == StopReason::Fault || error.is_some() {
                    return Err(io::Error::other(
                        "camera fault reported before driver teardown",
                    ));
                }
            }
            CameraEvent::Stopped {
                at_us,
                reason,
                stop,
                error,
                ..
            } => {
                let Some((revoked, deadline, expected_reason)) = self.revoked else {
                    return Err(io::Error::other(
                        "camera teardown has no local-revocation receipt",
                    ));
                };
                if self.stopped
                    || *at_us < revoked
                    || *at_us > now_us
                    || *at_us >= deadline
                    || *reason != expected_reason
                    || stop.is_some_and(|t| {
                        t.started_at_us < revoked
                            || t.completed_at_us < t.started_at_us
                            || t.completed_at_us > *at_us
                    })
                {
                    return Err(io::Error::other(
                        "invalid or late camera driver-teardown receipt",
                    ));
                }
                self.stopped = true;
                if *reason == StopReason::Fault || error.is_some() {
                    return Err(io::Error::other("camera driver teardown reported failure"));
                }
            }
            CameraEvent::FrameLink { .. } => {}
            _ if self.revoked.is_some() => {
                return Err(io::Error::other(
                    "camera progress/frame after local revocation",
                ));
            }
            _ => {}
        }
        Ok(())
    }
    pub fn finish(&self) -> io::Result<()> {
        if self.failed || self.revoked.is_none() || !self.stopped {
            Err(io::Error::other(
                "camera shutdown lacks verified revocation and teardown",
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera_worker::{Counts, PortMetadata, Timing};
    fn boot(n: u8) -> BootId {
        BootId::new([n; 16]).unwrap()
    }
    fn revoked(at: u64) -> CameraEvent {
        CameraEvent::Revoked {
            at_us: at,
            worker: boot(1),
            reason: StopReason::Requested,
            teardown_deadline_us: at + lamp_camera::DRIVER_TEARDOWN_BUDGET_US,
            error: None,
            timestamp_rejection: None,
            startup_discards: None,
        }
    }
    fn stopped(at: u64) -> CameraEvent {
        CameraEvent::Stopped {
            at_us: at,
            reason: StopReason::Requested,
            counts: Counts::default(),
            stop: Some(Timing {
                started_at_us: 1100,
                completed_at_us: at,
            }),
            port: PortMetadata::default(),
            error: None,
        }
    }
    #[test]
    fn only_prompt_valid_revoke_unlocks_separate_driver_window() {
        let mut watch = ShutdownWatch::new(boot(1), 1000);
        assert_eq!(watch.deadline_us(), 21_000);
        watch.observe(&revoked(1100), 1200).unwrap();
        assert_eq!(watch.deadline_us(), 251_100);
        watch.observe(&stopped(41_000), 42_000).unwrap();
        watch.finish().unwrap();
    }
    #[test]
    fn late_future_wrong_worker_or_duplicate_receipts_never_certify_shutdown() {
        for variant in 0..5 {
            let mut watch = ShutdownWatch::new(boot(1), 1000);
            let mut event = revoked(1100);
            let mut receipt_at = 1200;
            match variant {
                0 => receipt_at = 21_001,
                1 => receipt_at = 1099,
                2 => {
                    if let CameraEvent::Revoked { worker, .. } = &mut event {
                        *worker = boot(2);
                    }
                }
                3 => {
                    if let CameraEvent::Revoked {
                        teardown_deadline_us,
                        ..
                    } = &mut event
                    {
                        *teardown_deadline_us += 1;
                    }
                }
                4 => {
                    watch.observe(&event, receipt_at).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(watch.observe(&event, receipt_at).is_err());
            assert!(watch.finish().is_err());
            if variant != 4 {
                assert_eq!(watch.deadline_us(), 21_000);
                assert!(watch.observe(&revoked(1100), 1300).is_err());
                assert_eq!(
                    watch.deadline_us(),
                    21_000,
                    "fault cannot be repaired into longer permission"
                );
            }
        }
    }
    #[test]
    fn missing_revoke_late_teardown_or_post_revoke_frame_is_failure() {
        let mut watch = ShutdownWatch::new(boot(1), 1000);
        assert!(watch.observe(&stopped(2000), 2000).is_err());
        assert_eq!(watch.deadline_us(), 21_000);
        let mut watch = ShutdownWatch::new(boot(1), 1000);
        watch.observe(&revoked(1100), 1200).unwrap();
        assert!(watch.observe(&stopped(251_100), 251_100).is_err());
        let mut watch = ShutdownWatch::new(boot(1), 1000);
        watch.observe(&revoked(1100), 1200).unwrap();
        assert!(
            watch
                .observe(
                    &CameraEvent::Progress {
                        at_us: 1300,
                        counts: Counts::default(),
                        last_read: None,
                        startup_discards: None,
                    },
                    1300
                )
                .is_err()
        );
        watch.observe(&stopped(2000), 2000).unwrap();
        assert!(
            watch.finish().is_err(),
            "later valid completion cannot erase earlier fault"
        );
    }
    #[test]
    fn original_fault_can_be_received_before_blocking_cleanup_without_becoming_success() {
        let mut watch = ShutdownWatch::new(boot(1), 1000);
        let mut event = revoked(1100);
        if let CameraEvent::Revoked { error, .. } = &mut event {
            *error = Some("read failure".into());
        }
        assert!(watch.observe(&event, 1200).is_err());
        assert_eq!(
            watch.deadline_us(),
            251_100,
            "valid seal still precedes cleanup"
        );
        watch.observe(&stopped(41_000), 42_000).unwrap();
        assert!(watch.finish().is_err());
    }
    #[test]
    fn metadata_failure_is_sticky_without_renewing_driver_deadline() {
        let mut watch = ShutdownWatch::new(boot(1), 1000);
        watch.observe(&revoked(1100), 1200).unwrap();
        let deadline = watch.deadline_us();
        watch.invalidate();
        assert_eq!(watch.deadline_us(), deadline);
        assert!(!watch.expire(deadline - 1));
        watch.observe(&stopped(41_000), 42_000).unwrap();
        assert!(watch.finish().is_err());
        assert!(watch.expire(deadline));
        assert_eq!(watch.deadline_us(), deadline);
    }
}
