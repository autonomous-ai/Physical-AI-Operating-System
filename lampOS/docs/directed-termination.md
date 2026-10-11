# Graceful termination of a directed run

`directed` and `directed-fixture` install SIGINT/SIGTERM actions before their
initial source/cue preparation. The actions only store the signal number in an
atomic flag. They perform no I/O, allocation, locking, hardware operations or
shutdown work. `signal-hook = 0.4.4`, with default features disabled, supplies
that safe interface; `signal-hook-registry = 1.4.8` is pinned in Cargo.lock.
The local audited flag implementation uses an atomic store. No new signal
thread, channel, periodic disk write or unsafe Lamp code is introduced.

The inspected V1 main (`d5efe9d7`, `hal/server.py:80` and
`hal/runtime.py:866–922`) delegates shutdown to the web server's normal lifespan
cleanup. Keep that separation between the signal and ordinary cleanup work.
Reuse Rust's existing cancellation, privacy and diagnostic shutdown path; do
not port V1's spoken shutdown/park hooks into this voice-only experiment.

## Active conversation

The coordinator checks the flag at the next existing cooperative tick. The
latency target remains the existing approximately 2 ms loop cadence; this is
not a hard scheduling or kernel deadline. A signal makes that run return an
`Interrupted` error through its ordinary failure cleanup:

1. Cancel the active reply and invalidate its output ownership.
2. Deny microphone permission and publish the new authority.
3. Send Stop to every worker, then use the existing shared shutdown deadline.
4. Collect and validate the unchanged diagnostic finish receipts and completion
   markers. Missing/invalid ACKs and forced termination remain failures.
5. Write the retained coordinator trace with `run_end.status = "failed"` and
   `termination_signal`. The executable's existing error path exits nonzero.

Diagnostic shutdown still has a shared 750 ms cooperative grace (300 ms when
recording is disabled), with the existing bounded finish waits. Cancellation
and hard-stop delivery happen before those waits. A successfully finalized
recording describes the samples retained before the stop; it does not make the
interrupted conversation a completed answer or a valid benchmark. Repeated
SIGINT/SIGTERM requests stay cooperative rather than forcing an early exit.
The trace names the most recently stored signal, not a measured arrival time.

The complete Rig now also uses this cleanup if its reference-connection setup
fails. A signal observed after ordinary cleanup but before terminal trace
selection changes an otherwise successful result to failed. Final trace
writing remains outside the active interaction loop. Signals after that
terminal selection do not revise a conversation that has already ended.

## Startup and limits

The initial provider startup and existing later-worker callbacks check the
same flag at their existing 2 ms/5 ms wait points. A stop observed after trace
initialization retains a failed trace. Preflight validation, fixture loading, cue
connection and local filesystem/systemctl calls are not preemptible by this
flag; no hard deadline is claimed for those operations. If termination happens
before trace initialization, there may be no trace artifact.

Before a complete Rig exists, the existing partial-worker Drop path kills and
reaps children. It does not guarantee diagnostic ACKs for partially initialized
workers. A partial startup recording remains uncertified. A complete Rig gets
the ordinary privacy/Stop/diagnostic shutdown path even if conversation
activation has not started. This patch deliberately does not redesign partial
startup ownership.

**For a graceful stop, signal the coordinator PID first and allow its shutdown
grace before escalation.** Its workers keep their existing signal behavior.
A process-group-wide signal, an early service-manager kill, SIGKILL, power loss,
a crashed process, blocked kernel/filesystem operation, or exhausted storage
can prevent diagnostic completion or trace persistence. Never score missing or
unacknowledged artifacts as a clean run. No device stop behavior is qualified
by this source change.

Registration actions are removed when the finite call returns. The pinned
registry explicitly does not restore the previous default signal disposition
when its last action is removed. The production CLI exits after that call;
embedding these entry points in a long-lived unrelated process is outside this
contract. No other process's signal policy is changed.

## Offline regression

Isolated child tests receive real SIGINT/SIGTERM while executing the production
conversation, cancellation, privacy, shutdown and trace-finalization functions.
Private Unix peers finish actual Recorder instances containing synthetic zero
PCM and send their actual acknowledgement-derived receipts. The ordinary
certifier verifies their markers. Dummy sleep processes intentionally require
forced reaping, and that failure remains explicit in the same failed trace.
The tests do not impersonate physical capture, ALSA acceptance or cooperative
hardware-worker exit. A separate child covers a startup callback interruption.
All child waits have finite test deadlines and owned children are reaped.

The tests send real signals through the existing safe Rustix syscall wrapper,
so no external `kill` utility is required. Logs separately report signal-syscall
invocation to cooperative
observation and total cleanup time. Those host process measurements are not
microphone, actuator, acoustic, Linux-device or end-to-end interaction latency.

## Integration verification

The combined source on playback checkpoint `ef42c4611` passed workspace
formatting, strict all-target live Clippy, and the complete serial host live
suite: 415 passed, zero failed, two existing ignored. A standalone Linux ARM64
release build and strict all-target audio/live Clippy passed. Seventy-three
focused release-QEMU tests passed, including actual SIGINT/SIGTERM, missing
shutdown acknowledgement, diagnostic certification, playback-start, turn-taking
and endpoint cases. These checks do not qualify physical shutdown.

Frozen source manifest SHA256:
`b760780cae263aa6e1cd6aeafbe5bab7d4725b6e367b7d9a98489394d8005179`.
ARM binary SHA256:
`0aa2387a4c382ea25568f5099e11427f5a05b8caa93ea01beff23836aaccb822`.
Only this verification note was added after that source export. Initial offline
build failure from a missing cached dependency and the test-only reliance on an
absent external `kill` utility are retained in the qualification receipt. The
final tests use the existing safe Rustix signal wrapper, with no runtime change
from that test correction. The ongoing physical playback comparison retains
its prior `a30055ba...` binary and does not include this shutdown change.
