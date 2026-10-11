//! JSON-line protocol between `lamp-session` (Lamp) and the runner (Mac).
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA: u32 = 1;
/// Upper bound for one encoded relay line, enforced by both ends before any
/// larger allocation. The final trace is streamed one record per line.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const MAX_TRACE_LINES: usize = 25_000;
/// Upper bound for the runtime's `events.jsonl`, checked before and while reading.
pub const MAX_TRACE_BYTES: u64 = 64 * 1024 * 1024;
/// One trace record, leaving room for the `{"type":"trace","record":...}` envelope.
pub const MAX_TRACE_RECORD_BYTES: usize = MAX_LINE_BYTES - 1024;
/// One runtime stdout line; JSON escaping may still grow it, which `emit` checks.
pub const MAX_STDOUT_LINE_BYTES: usize = 16 * 1024;
/// One runner line (pings are tens of bytes).
pub const MAX_RUNNER_LINE_BYTES: usize = 1024;
/// Everything the runner may send in one session (at most 400 pings).
pub const MAX_RUNNER_TOTAL_BYTES: u64 = 4 * 1024 * 1024;
/// Everything the runtime may print on stdout in one session.
pub const MAX_STDOUT_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
/// Everything one session may relay to the runner: the trace plus cues,
/// stdout and pongs. The runner stops reading and invalidates the attempt above it.
pub const MAX_SESSION_BYTES: u64 = MAX_TRACE_BYTES + 32 * 1024 * 1024;

/// Lamp -> runner.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionLine {
    SessionStart {
        schema: u32,
        lamp_us: u64,
        mode: String,
        cue_socket: bool,
        runtime_argv: Vec<String>,
    },
    /// A cue datagram, verbatim, with the relay's receipt time.
    Cue {
        lamp_received_us: u64,
        cue: Value,
    },
    CueInvalid {
        lamp_received_us: u64,
        error: String,
    },
    /// One stdout line printed by the runtime (for example listening readiness).
    Stdout {
        lamp_received_us: u64,
        line: String,
    },
    /// A runtime stdout line that could not be relayed intact (oversized, not
    /// UTF-8 or cut off). The runner treats the session evidence as invalid.
    StdoutInvalid {
        lamp_received_us: u64,
        error: String,
    },
    Pong {
        seq: u64,
        lamp_us: u64,
    },
    /// One record of the runtime's final `events.jsonl`.
    Trace {
        record: Value,
    },
    SessionEnd {
        lamp_us: u64,
        exit_code: Option<i32>,
        killed: bool,
        events_sha256: Option<String>,
        events_lines: usize,
        error: Option<String>,
        /// The runtime exited normally but processes remained in its group
        /// (normally its workers) and the relay killed them.
        #[serde(default)]
        stray_processes_stopped: bool,
        /// Serve sessions only: how the runtime was stopped and what its
        /// rolling metadata held. Absent (and not serialized) for finite runs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        serve: Option<Box<ServeEnd>>,
    },
}

/// How a long-lived `lamp-live serve` runtime ended. Every serve session
/// ends through the same path: SIGTERM to the runtime, a bounded grace, then
/// a forced kill of its process group.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServeEnd {
    /// `runner_stop`, `transport_closed`, `deadline` or `runtime_exited`
    /// (the runtime ended before any stop was requested).
    pub stop_cause: String,
    pub stop_reason: Option<String>,
    pub stop_requested_us: Option<u64>,
    pub signal_sent_us: Option<u64>,
    pub signal_error: Option<String>,
    pub exited_us: Option<u64>,
    /// The runtime outlived the grace and its process group was killed.
    pub forced_kill: bool,
    /// The relay's own process group was confirmed gone after the end.
    pub group_reaped: bool,
    pub metadata: ServeMetadata,
}

/// The serve runtime's rolling metadata as relayed. History is best effort:
/// rotation can drop old segments and the writer can drop events, so every
/// loss is counted rather than inferred away.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ServeMetadata {
    pub segments: Vec<ServeSegment>,
    /// Records relayed as `trace` lines (segment headers excluded).
    pub records: usize,
    pub first_service_sequence: Option<u64>,
    pub last_service_sequence: Option<u64>,
    /// Missing or out-of-order `service_sequence` steps between records.
    pub sequence_gaps: u64,
    /// The oldest relayed segment does not start at the first writer event.
    pub history_truncated: bool,
    /// `metadata/health.json`, bounded; `None` when absent or invalid.
    pub health: Option<Value>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServeSegment {
    pub slot: usize,
    pub generation: u64,
    pub first_writer_event: u64,
    pub bytes: u64,
    pub sha256: String,
}

/// Runner -> Lamp.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerLine {
    Ping {
        seq: u64,
    },
    /// Serve sessions only: stop the runtime now (SIGTERM, bounded grace,
    /// then a forced group kill). Finite sessions end by themselves.
    Stop {
        reason: String,
    },
}
