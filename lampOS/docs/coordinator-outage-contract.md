# Coordinator behavior during provider outages

The directed provider CLI explicitly selects `Reporting::Coordinated`, paired
with this coordinator and the [provider link contract](provider-link-channel.md).
The library's `provider_worker::run()` retains Legacy for existing explicit
callers. A Legacy data Ready cannot open this coordinator's cloud-submission
gate; worker and coordinator must be upgraded together. Require remains the
cancellation policy. Images and prerecorded notice assets are not activated.
This integration has host and Linux ARM64 verification as described below. It
has not yet been deployed or acoustically qualified.

## Independent control and delivery

Provider control is serviced after privacy, before queued model packets, again
before a new input owner is admitted, and before queued microphone commands are
sent. These checks share a maximum of 16 control packets per 2 ms coordinator
iteration. A full control slice defers cloud submission/model data until the
next iteration; it does not defer local capture, privacy, speaker feedback, or
already-owned speaker dispatch. The model slice remains two packets/2 ms,
input dispatch remains eight commands per iteration, and all existing socket,
30-second PCM, and authority bounds remain unchanged. This is a cooperative
scheduling target, not a measured worst-case deadline.

Only a validated priority Ready opens cloud submission. Revision, session, and
source time cannot regress. Recovery can skip the intermediate Unavailable
notification because the worker has one latest-only unsent status slot: a new
Ready session still closes any unfinished input belonging to the old session.
A data-channel Ready is counted for flow control and traced as ignored; it
cannot open or reopen availability. Source observation and coordinator receipt
are separate trace fields. As with the producer, a remote Close still hidden
behind deliberate transport PCM backpressure is not an observed outage.

Microphone permission depends on fresh physical privacy evidence, not cloud
availability. Capture/admission retention leases continue during an outage so
local listening and previously authorized playback remain valid. Initial
`listening_ready` additionally requires a usable cloud session and retained
capture; no such event is emitted while discarding an unfinished input.

## Request and utterance boundaries

Each admitted request records the provider session that was available then.
Its IPC `Start` carries that exact required, nonzero `session` value. The
coordinator validates it against the request before sending, and the worker
calls `Supervisor::try_start_for_session` with it. A newer priority observation,
authority heartbeat or capacity update cannot replace the selected session in
a queued/retained Start. Missing session is invalid wire data, and zero is
rejected before provider submission. This is an internal wire change: worker,
coordinator and fixture must be upgraded together.

Only successful IPC Start/End sends advance submission evidence; queued commands
or WouldBlock do not. `Started` carries the immutable source session from
Gemini's `InputStarted.lineage.session`, or session 1 for the fixed fixture.
Missing session is invalid wire data. A request/session mismatch fails closed
before its PCM can acquire a speaker permit. A delayed Started from the expected
old session may still confirm already-submitted work after recovery; it never
reopens that input or grants ownership to the new session.

If the selected connection is gone or has been replaced, checked submission
refuses before enqueueing input or superseding an existing provider answer.
Typed and Coordinated modes retain exactly one pending local
`TurnFailed(NotDelivered, Local)` until it fits the existing bounded output
queue. This is a worker receipt that Gemini input was not submitted, not a
receipt from the remote service. Its duplicate Start and remaining Audio/End
are discarded even after recovery; only a fresh admitted request may use the
new session. The Legacy mode still fails the worker instead of accepting a
typed failure. Other Start errors remain fatal; an arbitrary StaleRequest is
not relabeled as a proven session replacement. Independently received owner
cancellation/privacy controls retain their existing immediate authority.

`local_not_submitted` in the coordinator outcome specifically means the Start
never left the coordinator's IPC queue. It is false for a matching worker
NotDelivered receipt even though that receipt proves no model input submission.

If an outage or session change precedes successfully sending End, unsent input
commands are discarded. The accepted stream is drained through the detector's
actual End without resetting the detector or replaying any prefix/tail. A
recovered connection accepts a new utterance only after that boundary. If Start
never left the coordinator, the trace records an explicit local NotDelivered
failure, distinct from a provider receipt. If Start was sent, the coordinator
waits for the exact ordered provider outcome or an explicit local cancellation.
An already-submitted End requires no utterance drain, and its received answer
can finish while cloud availability remains closed.

A decision accepted by the existing local admission policy still interrupts an
older answer during an outage. Its old owner/permits are cancelled immediately;
the new input records a local NotDelivered outcome, with no invented provider
request/session, and is discarded through its endpoint. Rejected or ignored
candidates never acquire this cancellation authority. The current finite
harness still uses DirectedSessionVadOnly; this integration does not add an
addressee classifier or claim an outage speech announcement/acoustic recovery UX.

## Ordered outcomes

Only the matching Started admits PCM and model completion. GenerationComplete
is still distinct from final TurnComplete; the earlier multiple-generation
playback/retirement semantics are preserved. A TurnFailed retains prior PCM,
flushes its held final block under the same owner, and ends with `provider_failed`
and the exact stage/fault. It cannot become `audio_written_unscored`. Completion
also requires the real local input endpoint, the exact request outcome, and
retirement of all retained playback. OwnershipAssumed is a fault under Require.

A failed input does not always have a `Started` acknowledgement. The worker can
accept Start into its local provider queue and then fail while writing the
activity-start message, before Gemini's actor emits `InputStarted`. For the
matching IPC-submitted request, `TurnFailed(InputOpen)` is therefore valid
before `Started`. `TurnFailed(AwaitingResponse)` additionally requires a
successful local IPC End send. These stages describe accepted local submission,
not proof that the remote service heard or processed the utterance. A
`Responding` failure still requires the matching source-stamped `Started`;
`NotDelivered` is incompatible with an already-confirmed `Started`. Failure
without a submitted Start, duplicate terminal outcomes, mismatched Started
sessions, and output/completion after failure remain invalid.

Accepting either pre-Started failure does not set `started`, grant PCM authority,
or mark the answer successful. The ordinary failure path clears unsent input
and drains an unfinished utterance through its original detector End. Only
then can a new utterance be admitted on a recovered session. This is a local
state check with no I/O, new wait, retry, or queue; acoustic response/recovery
latency remains unmeasured. Runner/evaluator support for the failed terminal
outcome remains separate reviewed work.

Ended permanently closes cloud submission, but is neither process EOF nor a
request outcome. Earlier PCM and outcomes drain in order, and a Start already
in flight can still receive one NotDelivered afterward. A legitimate priority
Ready already in flight may be observed after Ended because control and data
use separate sockets; its monotonic history is retained, but it cannot reopen
the permanent latch. No model PCM/Started/completion is accepted after Ended.
The worker stays alive until Stop, and the coordinator continues local control
and retains unresolved work until its matching outcome or explicit finite-run
cancellation. No early success is inferred from upstream closure.

## Verification limits

Focused tests use actual private Unix datagrams, the production coordinator
status/submission/speaker helpers, real Controller permissions, and deterministic
TurnDetector/InputAdmission sequences. They cover stale Ready, source session
mismatch, WouldBlock on End, coalesced recovery, actual endpoint draining,
retained speech after failure, Ended followed by an in-flight Start refusal,
and the fixture worker's real priority startup path. The checked-session
regressions additionally use the real in-memory Supervisor to recover from A
to B before delivering a queued A Start, prove B receives neither that Start
nor its PCM, preserve a live B response after checked refusal, and retain the
original selected session through mixed authority/capacity controls on private
Unix sockets. Pre-Started failure regressions retain the actual detector End
and require a fresh next utterance. No provider credentials,
real microphone, speaker, camera, GPIO, or cloud service is used. These checks do
not qualify physical latency, reconnect behavior against the production proxy,
or conversational interruption quality.

The paired integration regression runs the real Coordinated worker/Supervisor,
private Unix datagrams, and production consumer helpers against an in-memory
service. It covers a normal Started/answer/completion; outage and recovery while
the two output credits are spent, with prior PCM before failure; an A-selected
Start refused after B recovers, followed by a new B utterance only after the old
endpoint; and blocked activityStart writes producing both InputOpen and
AwaitingResponse failures before Started. The 16-byte scripted transport in the
last case blocks an actual protocol write; no failure event is synthesized.
These tests do not run the OS process supervisor or real audio/GPIO devices.

Run the paired cases with:

```sh
cargo test --locked --offline -p lamp-voice --lib coordinator::tests::provider_pair -- --test-threads=1
```

Unix datagram access is required even though no network service is contacted.
Clock correlation records conservative source intervals, not exact latency;
see the provider link contract before interpreting availability timestamps.

## Integrated-source checks: 2026-10-11

The combined source includes Agent 1's `ae4f05db4` audio/Gemini checkpoint and
Main's coordinated worker, consumer, session-bound Start, and conservative clock
correlation. The frozen candidate manifest is SHA256
`205510be7bbbed18f62365510c93796b5c14877fd1cb421df379c2687d18e5a4`.
Its code bytes match the integrated tree; subsequent documentation changes do
not change the executable.

Run from the integration tree on macOS, sequentially:

```sh
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1
```

All passed: 960 tests including one doctest, zero failures, four ignored. The
ignored cases are the two prolonged Gemini recovery soaks and the provider
worker's real-time 60-second and 180-second tests; they were not rerun for this
checkpoint. An earlier focused combined run passed 607 tests with the same four
ignored. These counts describe different scopes and must not be added together.

A source-only Linux ARM64 container passed the release `lamp-voice` build and
strict all-target Clippy for `lamp-audio`, `lamp-gemini`, and `lamp-voice`. The five
paired coordinator/provider tests passed under QEMU in release mode. The ELF
uses the expected ARM64 interpreter and requires no GLIBC symbol newer than
2.34. Binary SHA256:
`fb7e11911b7b84ef7a0af40a0c0c8ec26e7fc852dcd1b4228d3b173a2b979371`.
An independent source review found no blocking defect in this slice. None of
these checks is a native device or room-audio measurement.
