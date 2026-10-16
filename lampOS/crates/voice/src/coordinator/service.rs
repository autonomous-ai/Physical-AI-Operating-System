//! The outer service owns no audio. Only a verified physical/software mute may
//! create another runtime; faults and cleanup failures never become retries.
use super::*;
use crate::service_privacy::{PhysicalMute, SoftwareMute, WaitOutcome};

pub(super) fn supervise(
    trace: &mut SessionState,
    cues: &mut Option<CueSink>,
    termination: &Termination,
    mut wait: impl FnMut(&mut SessionState, &mut Option<CueSink>) -> Result<WaitOutcome>,
    mut active: impl FnMut(&mut SessionState, &mut Option<CueSink>) -> Result<()>,
) -> Result<()> {
    let mut cycle = 0u64;
    loop {
        if termination.signal_name().is_some() {
            return Ok(());
        }
        record(
            trace,
            json!({"kind":"service_waiting_for_privacy","at_us":monotonic_us(),"next_cycle":cycle.checked_add(1),"audio_workers_open":false}),
        )?;
        flush_cues(trace, cues);
        match wait(trace, cues)? {
            WaitOutcome::Stopped => return Ok(()),
            WaitOutcome::Allowed => {}
        }
        if termination.signal_name().is_some() {
            return Ok(());
        }
        cycle = cycle
            .checked_add(1)
            .ok_or_else(|| io::Error::other("service cycle overflow"))?;
        // The previous rig has been closed and reaped. New boots cannot settle
        // old commands. One logger/queue remains for the whole service invocation.
        trace.receipts = Default::default();
        record(
            trace,
            json!({"kind":"service_cycle_start","at_us":monotonic_us(),"cycle":cycle,"fresh_runtime_required":true,"context_continuity_claimed":false}),
        )?;
        match active(trace, cues) {
            Ok(()) => return Ok(()),
            Err(error)
                if error.downcast_ref::<PhysicalMute>().is_some()
                    || error.downcast_ref::<SoftwareMute>().is_some() =>
            {
                let (reason, observed_at_us) =
                    if let Some(mute) = error.downcast_ref::<SoftwareMute>() {
                        let reason = if mute.asleep {
                            "head_sleep"
                        } else {
                            "software_muted"
                        };
                        (reason, mute.observed_at_us)
                    } else {
                        (
                            "physical_muted",
                            error
                                .downcast_ref::<PhysicalMute>()
                                .expect("matched a physical or software mute")
                                .observed_at_us,
                        )
                    };
                record(
                    trace,
                    json!({"kind":"service_cycle_muted","at_us":monotonic_us(),"cycle":cycle,"reason":reason,"observed_at_us":observed_at_us,"cleanup_verified":true}),
                )?;
                flush_cues(trace, cues);
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn verified_mutes_repeat_the_privacy_wait_and_record_their_actual_source() {
        // Physical, software, and software because the head put it to
        // sleep (#711).
        for (software, asleep) in [(false, false), (true, false), (true, true)] {
            let mut trace = SessionState::default();
            let waits = Cell::new(0);
            let active = Cell::new(0);
            supervise(
                &mut trace,
                &mut None,
                &Termination::inactive(),
                |_, _| {
                    waits.set(waits.get() + 1);
                    assert_eq!(waits.get(), active.get() + 1);
                    Ok(WaitOutcome::Allowed)
                },
                |_, _| {
                    active.set(active.get() + 1);
                    if active.get() == 1 {
                        let mute: Box<dyn std::error::Error + Send + Sync> = if software {
                            Box::new(SoftwareMute {
                                observed_at_us: 71,
                                asleep,
                            })
                        } else {
                            Box::new(PhysicalMute { observed_at_us: 71 })
                        };
                        crate::service_privacy::finish(Err(mute), Ok(()))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap();
            assert_eq!((waits.get(), active.get()), (2, 2));
            let muted: Vec<_> = trace
                .iter()
                .filter(|row| row["kind"] == "service_cycle_muted")
                .collect();
            assert_eq!(muted.len(), 1);
            assert_eq!(
                muted[0]["reason"],
                match (software, asleep) {
                    (true, true) => "head_sleep",
                    (true, false) => "software_muted",
                    _ => "physical_muted",
                }
            );
            assert_eq!(muted[0]["observed_at_us"], 71);
            assert_eq!(muted[0]["cleanup_verified"], true);
        }
    }

    #[test]
    fn software_mute_with_failed_cleanup_never_starts_a_second_wait() {
        let mut trace = SessionState::default();
        let waits = Cell::new(0);
        let active = Cell::new(0);
        let error = supervise(
            &mut trace,
            &mut None,
            &Termination::inactive(),
            |_, _| {
                waits.set(waits.get() + 1);
                assert_eq!(waits.get(), 1, "failed cleanup must not be retried");
                Ok(WaitOutcome::Allowed)
            },
            |_, _| {
                active.set(active.get() + 1);
                crate::service_privacy::finish(
                    Err(Box::new(SoftwareMute {
                        observed_at_us: 73,
                        asleep: false,
                    })),
                    Err(io::Error::other("synthetic cleanup failure")),
                )
            },
        )
        .unwrap_err();
        assert_eq!((waits.get(), active.get()), (1, 1));
        assert!(error.downcast_ref::<SoftwareMute>().is_none());
        assert!(error.to_string().contains("synthetic cleanup failure"));
        assert!(!trace.iter().any(|row| row["kind"] == "service_cycle_muted"));
    }
}
