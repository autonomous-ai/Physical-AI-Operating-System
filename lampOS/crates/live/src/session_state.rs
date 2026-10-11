//! Explicit live bookkeeping plus replaceable observation storage. Removing all
//! metadata cannot cancel a turn, authorize output or erase a receipt obligation.
use crate::{
    fixture_provider::CueSink,
    receipt_history::ReceiptHistory,
    service_trace::{self, ServiceTrace},
};
use lamp_interaction::TurnOwner;
use serde_json::{Value, json};
use std::{
    io,
    ops::{Deref, DerefMut},
    path::Path,
};

pub(crate) const MAX_FINITE_EVENTS: usize = 20_000;
const MAX_BATCH_EVENTS: usize = 256;
/// Coordinator withholding evidence only: no admission/output authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct YieldInterval {
    pub owner: Option<TurnOwner>,
    pub started_at_us: u64,
    pub first_frame_sequence: u64,
    pub first_read_completed_at_us: u64,
    pub last_frame_sequence: u64,
    pub missing_evidence_since: Option<u64>,
}
#[derive(Default)]
pub(crate) struct SessionState {
    events: Vec<Value>,
    pub(crate) receipts: ReceiptHistory,
    pub(crate) yield_interval: Option<YieldInterval>,
    continuous: bool,
    writer: Option<ServiceTrace>,
    dropped: u64,
    cue_history_lost: bool,
    sequence: u64,
}
impl SessionState {
    pub(crate) fn finite() -> Self {
        Self {
            events: Vec::with_capacity(MAX_FINITE_EVENTS),
            ..Self::default()
        }
    }
    pub(crate) fn service(output: &Path) -> Self {
        let writer = ServiceTrace::start(output);
        if writer.is_err() {
            eprintln!(
                "lamp-live serve: metadata unavailable at startup; recording health degraded"
            );
        }
        Self {
            events: Vec::with_capacity(MAX_BATCH_EVENTS),
            continuous: true,
            writer: writer.ok(),
            ..Self::default()
        }
    }
    pub(crate) fn continuous(&self) -> bool {
        self.continuous
    }
    pub(crate) fn record(&mut self, mut event: Value) -> io::Result<()> {
        if !self.continuous {
            if self.events.len() >= MAX_FINITE_EVENTS - 1 {
                return Err(io::Error::other("trace capacity reached"));
            }
            self.events.push(event);
            return Ok(());
        }
        let sequence = self.sequence.checked_add(1);
        if let Some(sequence) = sequence {
            self.sequence = sequence;
            event["service_sequence"] = json!(sequence);
        }
        let oversized = !service_trace::event_fits(&event);
        if sequence.is_none() || oversized || self.events.len() >= MAX_BATCH_EVENTS {
            self.dropped = self.dropped.saturating_add(1);
            self.cue_history_lost = true;
            if let Some(writer) = &self.writer {
                writer.lost(oversized);
            }
        } else {
            self.events.push(event);
        }
        Ok(())
    }
    /// Metadata only, called after control/capture service. Queue pressure never
    /// becomes a Result error and cannot block an audio/privacy tick.
    pub(crate) fn flush(&mut self, cues: &mut Option<CueSink>, at_us: u64) {
        let error = if let Some(cues) = cues.as_mut() {
            if self.continuous {
                if self.cue_history_lost {
                    cues.invalidate("service_cue_history_lost");
                }
                cues.scan_batch(&self.events, at_us)
            } else {
                cues.scan(&self.events, at_us)
            }
        } else {
            None
        };
        if let Some(error) = error {
            let _ = self.record(json!({"kind":"fixture_cue_invalid","at_us":at_us,"reason":error}));
        }
        if self.continuous {
            if let Some(writer) = self.writer.as_mut() {
                for event in &self.events {
                    writer.submit(event);
                }
            } else {
                self.dropped = self.dropped.saturating_add(self.events.len() as u64);
            }
            self.events.clear();
        }
    }
    pub(crate) fn finish_logging(&mut self) -> Option<service_trace::Report> {
        if let Some(writer) = self.writer.as_mut() {
            Some(writer.finish(service_trace::FINISH_WAIT))
        } else if self.continuous {
            Some(service_trace::Report {
                acknowledged: false,
                finish_attempted: true,
                degraded: true,
                writer_failed: true,
                initialization_failed: true,
                dropped_events: self.dropped,
                oversized_events: 0,
                written_events: 0,
                unconfirmed_events: 0,
                rotations: 0,
            })
        } else {
            None
        }
    }
}
// The slice view is observation-only. Mutable access is used by the existing
// finite ring summary and final row; neither path runs in serve mode.
impl Deref for SessionState {
    type Target = Vec<Value>;
    fn deref(&self) -> &Self::Target {
        &self.events
    }
}
impl DerefMut for SessionState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.events
    }
}
#[cfg(test)]
impl Clone for SessionState {
    fn clone(&self) -> Self {
        assert!(!self.continuous);
        Self {
            events: self.events.clone(),
            receipts: self.receipts.clone(),
            yield_interval: self.yield_interval,
            ..Self::default()
        }
    }
}
#[cfg(test)]
impl std::fmt::Debug for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionState")
            .field("events", &self.events)
            .field("receipts", &self.receipts)
            .finish()
    }
}
#[cfg(test)]
impl PartialEq for SessionState {
    fn eq(&self, other: &Self) -> bool {
        self.events == other.events
            && self.receipts == other.receipts
            && self.yield_interval == other.yield_interval
    }
}
#[cfg(test)]
impl From<Vec<Value>> for SessionState {
    fn from(events: Vec<Value>) -> Self {
        Self {
            events,
            ..Self::default()
        }
    }
}

#[cfg(test)]
impl SessionState {
    pub(crate) fn with_writer(writer: ServiceTrace) -> Self {
        Self {
            continuous: true,
            writer: Some(writer),
            ..Self::default()
        }
    }
}
