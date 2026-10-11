//! The outer service owns no audio. Only a verified physical-mute lifecycle may
//! create another runtime; faults and cleanup failures never become retries.
use super::*;
use crate::service_privacy::{PhysicalMute, WaitOutcome};

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
            Err(error) if error.downcast_ref::<PhysicalMute>().is_some() => {
                record(
                    trace,
                    json!({"kind":"service_cycle_muted","at_us":monotonic_us(),"cycle":cycle,"cleanup_verified":true}),
                )?;
                flush_cues(trace, cues);
            }
            Err(error) => return Err(error),
        }
    }
}
