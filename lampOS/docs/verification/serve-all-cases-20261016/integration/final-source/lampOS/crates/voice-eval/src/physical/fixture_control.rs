//! Explicit local coordination for the evaluator's fake-runtime fixtures.
//!
//! A parent owns fixture time. Participants perform one complete poll, announce
//! its counters, then park until the next numbered step. Reading `FixtureClock`
//! never advances time. Counters describe completed work, not pipe emptiness or
//! process exit; even a terminal acknowledgement requires the caller's existing
//! trace, EOF, `try_wait` and owned-process cleanup paths.
use lamp_turn_owner::MonoTime;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const VERSION: u32 = 1;
/// Includes the terminating LF. A larger frame is rejected without draining it.
pub const MAX_FRAME_BYTES: usize = 4096;
pub const DEFAULT_WATCHDOG: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Runtime,
    Relay,
    ServeRuntime,
    ServeRelay,
    Recorder,
    SlowTransport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Consecutive per-peer sequence, starting at one, including same-time steps.
    pub round: u64,
    /// Recorder Steps must also fit in u64 nanoseconds for the shared recorder.
    /// Slow transport uses that same domain before handing time to the relay.
    pub at_us: u64,
    /// Complete runtime stdout frames the relay must consume before acknowledging.
    /// Both targets must be zero for a runtime or recorder participant.
    /// Slow transport also leaves both streams untouched and requires zero.
    pub stdout_target: u64,
    /// Complete runner input frames the relay must consume before acknowledging.
    pub runner_input_target: u64,
    /// Only a serve runtime may wait for its real signal handler to see SIGTERM.
    /// This target never sends or fabricates a signal.
    #[serde(default, skip_serializing_if = "is_false")]
    pub wait_for_sigterm: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
pub enum Progress {
    Runtime {
        /// Consecutive completed-poll sequence, starting at one.
        poll_seq: u64,
        /// Complete stdout frames flushed by this producer.
        stdout_frames: u64,
        /// Room datagrams consumed by the runtime, not merely sent by the parent.
        room_inputs: u64,
        /// Logical execution ended; trace writing and actual exit may follow.
        logical_end: bool,
    },
    Relay {
        poll_seq: u64,
        /// Complete runtime stdout frames consumed by the relay engine.
        stdout_frames: u64,
        /// Complete runner input frames consumed by the relay engine.
        runner_inputs: u64,
        /// Complete output frames flushed to the backend, including final output.
        output_frames: u64,
        /// The relay engine returned after flushing final output; not OS exit.
        logical_end: bool,
    },
    ServeRuntime {
        poll_seq: u64,
        /// Complete stdout frames successfully flushed by the serve runtime.
        stdout_frames: u64,
        /// Decoded room datagrams consumed, including those discarded while muted.
        room_consumed: u64,
        /// Consumed room datagrams actually injected into the active runtime.
        room_injected: u64,
        cycle: u64,
        cycle_base_us: u64,
        /// May reset only when the service starts a later cycle.
        runtime_us: u64,
        /// The existing signal-handler atomic, not a send receipt or an exit claim.
        observed_signal: usize,
        /// Final metadata finish/fsync and stdout flush completed; not OS exit.
        logical_end: bool,
    },
    ServeRelay {
        poll_seq: u64,
        stdout_frames: u64,
        runner_inputs: u64,
        output_frames: u64,
        /// Timestamp of a successful real SIGTERM send. It does not prove that
        /// the runtime observed the signal or that either process has exited.
        stop_signal_sent_us: Option<u64>,
        /// The relay engine returned after flushing final output; not OS exit.
        logical_end: bool,
    },
    Recorder {
        poll_seq: u64,
        /// The original ready-file write returned successfully; not fsync.
        ready_written: bool,
        /// A fresh clock sample after the ready write, fixed for this recorder.
        wait_started_us: u64,
        /// The complete original minimum-duration wait ends at this instant.
        exit_due_us: u64,
        /// The configured wait completed; the real child exit still follows.
        logical_end: bool,
    },
    SlowTransport {
        poll_seq: u64,
        /// The first held Step sampled before the complete transport delay.
        wait_started_us: u64,
        /// Earliest instant at which the original exec may be attempted.
        exec_due_us: u64,
        /// Only the wait ended. Actual exec, a new relay Hello, and exit follow.
        logical_end: bool,
    },
}

impl Progress {
    pub const fn role(self) -> Role {
        match self {
            Self::Runtime { .. } => Role::Runtime,
            Self::Relay { .. } => Role::Relay,
            Self::ServeRuntime { .. } => Role::ServeRuntime,
            Self::ServeRelay { .. } => Role::ServeRelay,
            Self::Recorder { .. } => Role::Recorder,
            Self::SlowTransport { .. } => Role::SlowTransport,
        }
    }

    pub const fn poll_seq(self) -> u64 {
        match self {
            Self::Runtime { poll_seq, .. }
            | Self::Relay { poll_seq, .. }
            | Self::ServeRuntime { poll_seq, .. }
            | Self::ServeRelay { poll_seq, .. }
            | Self::Recorder { poll_seq, .. }
            | Self::SlowTransport { poll_seq, .. } => poll_seq,
        }
    }

    pub const fn logical_end(self) -> bool {
        match self {
            Self::Runtime { logical_end, .. }
            | Self::Relay { logical_end, .. }
            | Self::ServeRuntime { logical_end, .. }
            | Self::ServeRelay { logical_end, .. }
            | Self::Recorder { logical_end, .. }
            | Self::SlowTransport { logical_end, .. } => logical_end,
        }
    }
}

/// The public handle exposes only a pure read. Only a validated parent Step can
/// update the participant's private atomic value.
#[derive(Clone, Debug)]
pub struct FixtureClock(Arc<AtomicU64>);

impl FixtureClock {
    pub fn now(&self) -> MonoTime {
        MonoTime::from_micros(self.0.load(Ordering::Acquire))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Frame {
    Hello {
        version: u32,
        role: Role,
    },
    Step {
        role: Role,
        step: Step,
    },
    Ack {
        role: Role,
        round: u64,
        at_us: u64,
        progress: Progress,
    },
}

/// Bind before spawning participants. `path` must be in a caller-owned private
/// directory. Existing paths are never removed or replaced. Drop removes only
/// the socket inode created by this listener, not a replacement at that path.
#[derive(Debug)]
pub struct ParentListener {
    listener: UnixListener,
    path: PathBuf,
    identity: (u64, u64),
    accepted_roles: Cell<u8>,
    watchdog: Duration,
}

impl ParentListener {
    pub fn bind(path: &Path) -> io::Result<Self> {
        Self::bind_with_watchdog(path, DEFAULT_WATCHDOG)
    }

    pub fn bind_with_watchdog(path: &Path, watchdog: Duration) -> io::Result<Self> {
        deadline(watchdog)?;
        let listener = UnixListener::bind(path).map_err(|e| context("bind listener", e))?;
        let metadata = fs::symlink_metadata(path).map_err(|e| context("stat listener", e))?;
        let result = Self {
            listener,
            path: path.to_owned(),
            identity: (metadata.dev(), metadata.ino()),
            accepted_roles: Cell::new(0),
            watchdog,
        };
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| context("restrict listener", e))?;
        result
            .listener
            .set_nonblocking(true)
            .map_err(|e| context("make listener nonblocking", e))?;
        Ok(result)
    }

    /// One cumulative host watchdog covers accept and the peer's Hello. The
    /// caller explicitly chooses the expected role; arrival order is no proof.
    pub fn accept(&self, expected: Role) -> io::Result<ParentPeer> {
        self.accept_inner(expected, None)
    }

    /// Retain an existing caller deadline across bootstrap and later resource
    /// waits. Accept and Hello share the earlier of it and this host watchdog.
    pub fn accept_until(&self, expected: Role, end: Instant) -> io::Result<ParentPeer> {
        self.accept_inner(expected, Some(end))
    }

    fn accept_inner(&self, expected: Role, end: Option<Instant>) -> io::Result<ParentPeer> {
        let role_bit = match expected {
            Role::Runtime => 1,
            Role::Relay => 2,
            Role::ServeRuntime => 4,
            Role::ServeRelay => 8,
            Role::Recorder => 16,
            Role::SlowTransport => 32,
        };
        if self.accepted_roles.get() & role_bit != 0 {
            return Err(invalid(format!("duplicate {expected:?} participant")));
        }
        let end = selected_deadline(deadline(self.watchdog)?, end);
        loop {
            remaining(end, "accept peer")?;
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let peer = ParentPeer::from_stream(stream, expected, self.watchdog, end)?;
                    self.accepted_roles
                        .set(self.accepted_roles.get() | role_bit);
                    return Ok(peer);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(
                        remaining(end, "accept peer")?.min(Duration::from_millis(1)),
                    );
                }
                Err(error) => return Err(context("accept peer", error)),
            }
        }
    }
}

impl Drop for ParentListener {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| (metadata.dev(), metadata.ino()) == self.identity)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Debug)]
pub struct ParentPeer {
    channel: Channel,
    role: Role,
    last_step: Option<Step>,
    last_progress: Option<Progress>,
    failed: bool,
}

impl ParentPeer {
    fn from_stream(
        stream: UnixStream,
        role: Role,
        watchdog: Duration,
        end: Instant,
    ) -> io::Result<Self> {
        let mut channel = Channel::new(stream, watchdog)?;
        match channel.read_frame(end)? {
            Frame::Hello {
                version: VERSION,
                role: received,
            } if received == role => Ok(Self {
                channel,
                role,
                last_step: None,
                last_progress: None,
                failed: false,
            }),
            frame => Err(invalid(format!(
                "expected version {VERSION} {role:?} Hello, received {frame:?}"
            ))),
        }
    }

    /// Send exactly one Step and await its exact Ack. On any error the endpoint
    /// closes and cannot be reused; there is no retry or skipped-round recovery.
    pub fn step(&mut self, step: Step) -> io::Result<Progress> {
        self.exchange(step, None)
    }

    /// Use the earlier of this caller-owned host deadline and the endpoint's
    /// watchdog. Repeated same-time terminal-drain rounds can share one budget;
    /// neither a new round nor a longer caller deadline resets that budget.
    pub fn step_until(&mut self, step: Step, end: Instant) -> io::Result<Progress> {
        self.exchange(step, Some(end))
    }

    fn exchange(&mut self, step: Step, end: Option<Instant>) -> io::Result<Progress> {
        let result = self.step_inner(step, end);
        if result.is_err() {
            self.failed = true;
            self.channel.close();
        }
        result
    }

    fn step_inner(&mut self, step: Step, end: Option<Instant>) -> io::Result<Progress> {
        if self.failed || self.last_progress.is_some_and(Progress::logical_end) {
            return Err(invalid(
                "peer is closed after failure or terminal acknowledgement",
            ));
        }
        validate_step(self.role, self.last_step, step)?;
        let own_end = deadline(self.channel.watchdog)?;
        let end = selected_deadline(own_end, end);
        self.channel.write_frame(
            &Frame::Step {
                role: self.role,
                step,
            },
            end,
        )?;
        match self.channel.read_frame(end)? {
            Frame::Ack {
                role,
                round,
                at_us,
                progress,
            } if role == self.role && round == step.round && at_us == step.at_us => {
                validate_progress(self.role, self.last_progress, step, progress)?;
                self.last_step = Some(step);
                self.last_progress = Some(progress);
                Ok(progress)
            }
            frame => Err(invalid(format!(
                "expected {:?} Ack for round {} at {}us, received {frame:?}",
                self.role, step.round, step.at_us
            ))),
        }
    }
}

#[derive(Debug)]
pub struct Participant {
    channel: Channel,
    role: Role,
    clock: FixtureClock,
    current: Step,
    last_progress: Option<Progress>,
    closed: bool,
}

impl Participant {
    /// The parent must bind before spawning this participant. Connect sends
    /// Hello and waits for the first validated Step before the engine may start.
    pub fn connect(path: &Path, role: Role) -> io::Result<Self> {
        Self::connect_with_watchdog(path, role, DEFAULT_WATCHDOG)
    }

    pub fn connect_with_watchdog(path: &Path, role: Role, watchdog: Duration) -> io::Result<Self> {
        deadline(watchdog)?;
        let stream = UnixStream::connect(path).map_err(|e| context("connect participant", e))?;
        Self::from_stream(stream, role, watchdog)
    }

    fn from_stream(stream: UnixStream, role: Role, watchdog: Duration) -> io::Result<Self> {
        let mut channel = Channel::new(stream, watchdog)?;
        let end = deadline(watchdog)?;
        channel.write_frame(
            &Frame::Hello {
                version: VERSION,
                role,
            },
            end,
        )?;
        let first = read_step(&mut channel, role, None, end)?;
        Ok(Self {
            channel,
            role,
            clock: FixtureClock(Arc::new(AtomicU64::new(first.at_us))),
            current: first,
            last_progress: None,
            closed: false,
        })
    }

    pub fn clock(&self) -> FixtureClock {
        self.clock.clone()
    }

    pub const fn current_step(&self) -> Step {
        self.current
    }

    /// Call only after the complete engine poll and all counted writes/reads.
    /// Sends its Ack and parks until the next Step. The next Step may have the
    /// same time; the caller must not repeat a runtime tick merely for a round.
    pub fn complete(&mut self, progress: Progress) -> io::Result<Step> {
        let result = self.complete_inner(progress);
        if result.is_err() {
            self.closed = true;
            self.channel.close();
        }
        result
    }

    fn complete_inner(&mut self, progress: Progress) -> io::Result<Step> {
        if progress.logical_end() {
            return Err(invalid("terminal progress requires final_ack"));
        }
        let end = deadline(self.channel.watchdog)?;
        self.acknowledge(progress, end)?;
        let next = read_step(&mut self.channel, self.role, Some(self.current), end)?;
        self.clock.0.store(next.at_us, Ordering::Release);
        self.current = next;
        Ok(next)
    }

    /// Announce logical completion without waiting for another Step. This is
    /// never evidence of OS exit. Finite runtime trace writes/fsync and exit
    /// follow; a serve runtime first finishes metadata/fsync and stdout, and
    /// a relay wrapper first finishes final trace/SessionEnd flushes.
    /// Slow transport completes only its delay, before attempting the real exec.
    pub fn final_ack(&mut self, progress: Progress) -> io::Result<()> {
        let result = (|| {
            if !progress.logical_end() {
                return Err(invalid("final_ack requires terminal progress"));
            }
            self.acknowledge(progress, deadline(self.channel.watchdog)?)
        })();
        self.closed = true;
        if result.is_err() {
            self.channel.close();
        }
        result
    }

    fn acknowledge(&mut self, progress: Progress, end: Instant) -> io::Result<()> {
        if self.closed {
            return Err(invalid(
                "participant is closed after failure or terminal acknowledgement",
            ));
        }
        validate_progress(self.role, self.last_progress, self.current, progress)?;
        self.channel.write_frame(
            &Frame::Ack {
                role: self.role,
                round: self.current.round,
                at_us: self.current.at_us,
                progress,
            },
            end,
        )?;
        self.last_progress = Some(progress);
        Ok(())
    }
}

fn read_step(
    channel: &mut Channel,
    role: Role,
    previous: Option<Step>,
    end: Instant,
) -> io::Result<Step> {
    match channel.read_frame(end)? {
        Frame::Step {
            role: received,
            step,
        } if received == role => {
            validate_step(role, previous, step)?;
            Ok(step)
        }
        frame => Err(invalid(format!(
            "expected {role:?} Step, received {frame:?}"
        ))),
    }
}

fn validate_step(role: Role, previous: Option<Step>, step: Step) -> io::Result<()> {
    let expected = previous.map_or(Some(1), |last| last.round.checked_add(1));
    if expected != Some(step.round) {
        return Err(invalid(format!(
            "expected round {expected:?}, received {}",
            step.round
        )));
    }
    if let Some(last) = previous {
        if step.at_us < last.at_us {
            return Err(invalid("Step time regressed"));
        }
        if step.stdout_target < last.stdout_target
            || step.runner_input_target < last.runner_input_target
        {
            return Err(invalid("Step input target regressed"));
        }
    }
    if matches!(role, Role::Runtime | Role::ServeRuntime)
        && (step.stdout_target != 0 || step.runner_input_target != 0)
    {
        return Err(invalid("runtime Step contains relay-only input targets"));
    }
    if role == Role::Recorder && (step.stdout_target != 0 || step.runner_input_target != 0) {
        return Err(invalid("recorder Step contains relay-only input targets"));
    }
    if role == Role::Recorder && step.at_us > u64::MAX / 1000 {
        return Err(invalid(
            "recorder Step microseconds-to-nanoseconds overflow",
        ));
    }
    if role == Role::SlowTransport {
        if step.stdout_target != 0 || step.runner_input_target != 0 {
            return Err(invalid(
                "slow transport Step contains relay-only input targets",
            ));
        }
        if step.at_us > u64::MAX / 1000 {
            return Err(invalid(
                "slow transport Step exceeds the shared nanosecond clock domain",
            ));
        }
    }
    if step.wait_for_sigterm && role != Role::ServeRuntime {
        return Err(invalid(
            "SIGTERM observation target requires a serve runtime",
        ));
    }
    Ok(())
}

fn validate_progress(
    role: Role,
    previous: Option<Progress>,
    step: Step,
    progress: Progress,
) -> io::Result<()> {
    if progress.role() != role {
        return Err(invalid("Ack progress has the wrong role"));
    }
    let expected = previous.map_or(Some(1), |last| last.poll_seq().checked_add(1));
    if expected != Some(progress.poll_seq()) {
        return Err(invalid(format!(
            "expected completed poll {expected:?}, received {}",
            progress.poll_seq()
        )));
    }
    if let Progress::Relay {
        stdout_frames,
        runner_inputs,
        ..
    }
    | Progress::ServeRelay {
        stdout_frames,
        runner_inputs,
        ..
    } = progress
        && (stdout_frames < step.stdout_target || runner_inputs < step.runner_input_target)
    {
        return Err(invalid(
            "relay Ack has not consumed the announced input targets",
        ));
    }
    validate_serve_progress(previous, step, progress)?;
    validate_recorder_progress(previous, step, progress)?;
    validate_slow_transport_progress(previous, step, progress)?;
    let regressed = match (previous, progress) {
        (
            Some(Progress::Runtime {
                stdout_frames: old_stdout,
                room_inputs: old_room,
                ..
            }),
            Progress::Runtime {
                stdout_frames,
                room_inputs,
                ..
            },
        ) => stdout_frames < old_stdout || room_inputs < old_room,
        (
            Some(Progress::Relay {
                stdout_frames: old_stdout,
                runner_inputs: old_input,
                output_frames: old_output,
                ..
            }),
            Progress::Relay {
                stdout_frames,
                runner_inputs,
                output_frames,
                ..
            },
        ) => stdout_frames < old_stdout || runner_inputs < old_input || output_frames < old_output,
        (
            Some(Progress::ServeRuntime {
                stdout_frames: old_stdout,
                room_consumed: old_consumed,
                room_injected: old_injected,
                cycle: old_cycle,
                cycle_base_us: old_base,
                runtime_us: old_runtime,
                ..
            }),
            Progress::ServeRuntime {
                stdout_frames,
                room_consumed,
                room_injected,
                cycle,
                cycle_base_us,
                runtime_us,
                ..
            },
        ) => {
            stdout_frames < old_stdout
                || room_consumed < old_consumed
                || room_injected < old_injected
                || cycle < old_cycle
                || cycle_base_us < old_base
                || (cycle == old_cycle && runtime_us < old_runtime)
        }
        (
            Some(Progress::ServeRelay {
                stdout_frames: old_stdout,
                runner_inputs: old_input,
                output_frames: old_output,
                ..
            }),
            Progress::ServeRelay {
                stdout_frames,
                runner_inputs,
                output_frames,
                ..
            },
        ) => stdout_frames < old_stdout || runner_inputs < old_input || output_frames < old_output,
        (Some(Progress::Recorder { .. }), Progress::Recorder { .. }) => false,
        (Some(Progress::SlowTransport { .. }), Progress::SlowTransport { .. }) => false,
        (Some(_), _) => true,
        (None, _) => false,
    };
    if regressed {
        return Err(invalid("Ack progress counters regressed"));
    }
    if previous.is_some_and(Progress::logical_end) {
        return Err(invalid("Ack followed terminal progress"));
    }
    Ok(())
}

fn validate_serve_progress(
    previous: Option<Progress>,
    step: Step,
    progress: Progress,
) -> io::Result<()> {
    if step.wait_for_sigterm
        && !matches!(
            progress,
            Progress::ServeRuntime { observed_signal, .. }
                if observed_signal == signal_hook::consts::SIGTERM as usize
        )
    {
        return Err(invalid("Ack has not observed the requested real SIGTERM"));
    }
    match progress {
        Progress::ServeRuntime {
            room_consumed,
            room_injected,
            cycle,
            cycle_base_us,
            observed_signal,
            ..
        } => {
            if room_injected > room_consumed {
                return Err(invalid(
                    "serve runtime injected more room inputs than it consumed",
                ));
            }
            if observed_signal != 0
                && observed_signal != signal_hook::consts::SIGTERM as usize
                && observed_signal != signal_hook::consts::SIGINT as usize
            {
                return Err(invalid("serve runtime reported an unregistered signal"));
            }
            if let Some(Progress::ServeRuntime {
                cycle: old_cycle,
                cycle_base_us: old_base,
                observed_signal: old_signal,
                ..
            }) = previous
            {
                if cycle == old_cycle && cycle_base_us != old_base {
                    return Err(invalid(
                        "serve runtime changed the base of an existing cycle",
                    ));
                }
                if old_signal != 0 && observed_signal == 0 {
                    return Err(invalid("serve runtime cleared its observed signal"));
                }
            }
        }
        Progress::ServeRelay {
            stop_signal_sent_us,
            ..
        } => {
            if stop_signal_sent_us.is_some_and(|sent| sent > step.at_us) {
                return Err(invalid(
                    "serve relay signal receipt is later than the current Step",
                ));
            }
            if let Some(Progress::ServeRelay {
                stop_signal_sent_us: Some(sent),
                ..
            }) = previous
                && stop_signal_sent_us != Some(sent)
            {
                return Err(invalid(
                    "serve relay changed its successful signal-send receipt",
                ));
            }
        }
        Progress::Runtime { .. }
        | Progress::Relay { .. }
        | Progress::Recorder { .. }
        | Progress::SlowTransport { .. } => {}
    }
    Ok(())
}

fn validate_recorder_progress(
    previous: Option<Progress>,
    step: Step,
    progress: Progress,
) -> io::Result<()> {
    let Progress::Recorder {
        ready_written,
        wait_started_us,
        exit_due_us,
        logical_end,
        ..
    } = progress
    else {
        return Ok(());
    };
    if step.stdout_target != 0 || step.runner_input_target != 0 || step.wait_for_sigterm {
        return Err(invalid(
            "recorder Ack cannot satisfy input or signal targets",
        ));
    }
    if !ready_written {
        return Err(invalid(
            "recorder Ack precedes its successful ready-file write",
        ));
    }
    if wait_started_us > step.at_us || exit_due_us < wait_started_us {
        return Err(invalid("recorder wait has inconsistent start or due time"));
    }
    match previous {
        Some(Progress::Recorder {
            wait_started_us: old_start,
            exit_due_us: old_due,
            ..
        }) if wait_started_us != old_start || exit_due_us != old_due => {
            return Err(invalid("recorder changed its fixed wait start or due time"));
        }
        None if wait_started_us != step.at_us => {
            return Err(invalid(
                "recorder first wait start differs from its held Step",
            ));
        }
        _ => {}
    }
    if logical_end != (step.at_us >= exit_due_us) {
        return Err(invalid(
            "recorder terminal flag disagrees with its wait deadline",
        ));
    }
    Ok(())
}

fn validate_slow_transport_progress(
    previous: Option<Progress>,
    step: Step,
    progress: Progress,
) -> io::Result<()> {
    let Progress::SlowTransport {
        wait_started_us,
        exec_due_us,
        logical_end,
        ..
    } = progress
    else {
        return Ok(());
    };
    if step.stdout_target != 0 || step.runner_input_target != 0 || step.wait_for_sigterm {
        return Err(invalid(
            "slow transport Ack cannot satisfy input or signal targets",
        ));
    }
    if step.at_us > u64::MAX / 1000 || exec_due_us > u64::MAX / 1000 {
        return Err(invalid(
            "slow transport wait exceeds the shared nanosecond clock domain",
        ));
    }
    if wait_started_us > step.at_us || exec_due_us < wait_started_us {
        return Err(invalid(
            "slow transport wait has inconsistent start or due time",
        ));
    }
    match previous {
        Some(Progress::SlowTransport {
            wait_started_us: old_start,
            exec_due_us: old_due,
            ..
        }) if wait_started_us != old_start || exec_due_us != old_due => {
            return Err(invalid(
                "slow transport changed its fixed wait start or due time",
            ));
        }
        None if wait_started_us != step.at_us => {
            return Err(invalid(
                "slow transport first wait start differs from its held Step",
            ));
        }
        _ => {}
    }
    if logical_end != (step.at_us >= exec_due_us) {
        return Err(invalid(
            "slow transport terminal flag disagrees with its wait deadline",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct BoundedReader {
    stream: UnixStream,
    watchdog: Cell<Duration>,
    frame_end: Cell<Option<Instant>>,
}

impl BoundedReader {
    /// Raw reader probes in the existing tests retain bounded stream semantics.
    /// This is a local budget, never SO_RCVTIMEO on a possibly closed socket.
    #[cfg(test)]
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        let timeout = timeout.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "fixture reader requires a bounded timeout",
            )
        })?;
        deadline(timeout)?;
        self.watchdog.set(timeout);
        Ok(())
    }
}

impl Read for BoundedReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let end = match self.frame_end.get() {
            Some(end) => end,
            None => deadline(self.watchdog.get())?,
        };
        loop {
            remaining(end, "read frame")?;
            match self.stream.read(bytes) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_ready(&self.stream, PollFlags::IN, end, "read frame")?;
                }
                result => return result,
            }
        }
    }
}

/// `poll` on Darwin represents its timeout as signed 32-bit milliseconds.
/// A large caller budget is split into representable waits without replacing
/// the original absolute deadline or rounding an expired budget back to life.
fn poll_timeout(left: Duration) -> io::Result<Timespec> {
    Timespec::try_from(left.min(Duration::from_millis(i32::MAX as u64)))
        .map_err(|error| invalid(format!("poll timeout conversion: {error}")))
}

fn wait_ready(
    stream: &UnixStream,
    flags: PollFlags,
    end: Instant,
    operation: &str,
) -> io::Result<()> {
    loop {
        let timeout = poll_timeout(remaining(end, operation)?)?;
        let mut descriptors = [PollFd::new(stream, flags)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => continue,
            Ok(_) => {
                let events = descriptors[0].revents();
                if events.contains(PollFlags::NVAL) {
                    return Err(invalid(format!(
                        "{operation}: poll reported an invalid peer descriptor"
                    )));
                }
                // HUP is not EOF: complete or partial frame bytes may still be
                // queued. The next nonblocking I/O supplies data, EOF or error.
                if events.intersects(flags | PollFlags::HUP | PollFlags::ERR) {
                    return Ok(());
                }
            }
            Err(error) if error == rustix::io::Errno::INTR => continue,
            Err(error) => return Err(context("poll peer", error.into())),
        }
    }
}

#[derive(Debug)]
struct Channel {
    reader: BufReader<BoundedReader>,
    writer: UnixStream,
    watchdog: Duration,
}

impl Channel {
    fn new(stream: UnixStream, watchdog: Duration) -> io::Result<Self> {
        deadline(watchdog)?;
        stream
            .set_nonblocking(true)
            .map_err(|e| context("make peer nonblocking", e))?;
        let writer = stream.try_clone().map_err(|e| context("clone peer", e))?;
        Ok(Self {
            reader: BufReader::with_capacity(
                MAX_FRAME_BYTES,
                BoundedReader {
                    stream,
                    watchdog: Cell::new(watchdog),
                    frame_end: Cell::new(None),
                },
            ),
            writer,
            watchdog,
        })
    }

    fn close(&self) {
        let _ = self.writer.shutdown(Shutdown::Both);
    }

    fn write_frame(&mut self, frame: &impl Serialize, end: Instant) -> io::Result<()> {
        let mut encoded =
            serde_json::to_vec(frame).map_err(|e| invalid(format!("encode frame: {e}")))?;
        if encoded.len() >= MAX_FRAME_BYTES {
            return Err(invalid(format!(
                "frame exceeds {MAX_FRAME_BYTES}-byte limit including LF"
            )));
        }
        encoded.push(b'\n');
        let mut bytes = encoded.as_slice();
        while !bytes.is_empty() {
            remaining(end, "write frame")?;
            match self.writer.write(bytes) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "fixture control write frame: peer accepted zero bytes",
                    ));
                }
                Ok(written) => bytes = &bytes[written..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_ready(&self.writer, PollFlags::OUT, end, "write frame")?;
                }
                Err(error) => return Err(io_failure("write frame", error)),
            }
        }
        Ok(())
    }

    fn read_frame(&mut self, end: Instant) -> io::Result<Frame> {
        self.reader.get_ref().frame_end.set(Some(end));
        let result = self.read_frame_inner(end);
        self.reader.get_ref().frame_end.set(None);
        result
    }

    fn read_frame_inner(&mut self, end: Instant) -> io::Result<Frame> {
        let mut bytes = Vec::with_capacity(256);
        loop {
            // Check even when BufReader already holds a complete frame. Its
            // underlying reader receives this same end across all partial reads.
            remaining(end, "read frame")?;
            let available = match self.reader.fill_buf() {
                Ok(available) => available,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(io_failure("read frame", error)),
            };
            if available.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    if bytes.is_empty() {
                        "fixture control missing frame: peer closed"
                    } else {
                        "fixture control truncated frame: peer closed before LF"
                    },
                ));
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let consumed = newline.map_or(available.len(), |index| index + 1);
            if bytes.len() + consumed > MAX_FRAME_BYTES
                || (newline.is_none() && bytes.len() + consumed == MAX_FRAME_BYTES)
            {
                return Err(invalid(format!(
                    "frame exceeds {MAX_FRAME_BYTES}-byte limit including LF"
                )));
            }
            bytes.extend_from_slice(&available[..consumed]);
            self.reader.consume(consumed);
            if newline.is_some() {
                return serde_json::from_slice(&bytes)
                    .map_err(|e| invalid(format!("decode frame: {e}")));
            }
        }
    }
}

/// Selection is independent of scheduler delay: neither a later caller limit
/// nor a new exchange replaces an already earlier absolute deadline.
fn selected_deadline(endpoint: Instant, caller: Option<Instant>) -> Instant {
    caller.map_or(endpoint, |caller| caller.min(endpoint))
}

fn deadline(watchdog: Duration) -> io::Result<Instant> {
    if watchdog.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fixture control watchdog must be positive",
        ));
    }
    Instant::now().checked_add(watchdog).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "fixture control watchdog overflow",
        )
    })
}

fn remaining(end: Instant, operation: &str) -> io::Result<Duration> {
    end.checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("fixture control {operation}: host watchdog expired"),
            )
        })
}

fn invalid(message: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("fixture control: {message}"),
    )
}

fn context(operation: &str, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("fixture control {operation}: {error}"),
    )
}

fn io_failure(operation: &str, error: io::Error) -> io::Error {
    if matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ) {
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!("fixture control {operation}: host watchdog expired ({error})"),
        )
    } else {
        context(operation, error)
    }
}

#[cfg(test)]
#[path = "fixture_control_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "serve_fixture_control_tests.rs"]
mod serve_tests;

#[cfg(test)]
#[path = "channel_closed_peer_tests.rs"]
mod closed_peer_tests;

#[cfg(test)]
#[path = "recorder_fixture_control_tests.rs"]
mod recorder_tests;

#[cfg(test)]
#[path = "slow_transport_fixture_control_tests.rs"]
mod slow_transport_tests;
