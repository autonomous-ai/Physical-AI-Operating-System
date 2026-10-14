# Expression

`lamp-expression` (`crates/expression`) plans posture, gaze, small gestures
and the light on one injected clock. It opens no hardware. It starts in
**shadow mode**; only `serve --enable` publishes actuator requests. The body
and voice on `lamp-voice` do not yet subscribe to those requests. First owns
that opt-in integration and its physical qualification.

## What exists and what we take

The [reuse decisions](reuse-decisions.md#expression-authored-timing-and-bounded-requests-2026-10-14)
record the V1 and current Rust paths inspected for this change. V1's movement
bounds, playback cancellation and separation of a persistent light setting
from a transient face remain the owners' responsibilities. Today's body
already has calibrated gaze, joint protection and smoothing; Expression must
not replace those with a guessed camera-to-servo conversion.

The external evidence is published research or source inspection, not a
measurement of this lamp:

| Source | What is taken |
|---|---|
| [BML 1.0](https://projects.cs.ru.is/projects/behavior-markup-language/wiki/Wiki) | Preparation, stroke and relaxation on explicit time anchors; a missed gesture does not delay speech; report requested behavior separately from execution |
| [Reachy Mini movement API](https://huggingface.co/docs/reachy_mini/API/reachymini) | Sample an authored move at a fixed cadence, compose small offsets and make cancellation explicit |
| [ROS twist_mux source](https://raw.githubusercontent.com/ros-teleop/twist_mux/rolling/include/twist_mux/topic_handle.hpp) | Actuator-side priority and expiring command authority; an expression lease is not a privacy/safety lock |
| [Apple ELEGNT paper](https://arxiv.org/html/2501.12493v1) | Social expression and functional task motion serve different needs; keep stillness available during work |
| [Murray et al., CoRL](https://proceedings.mlr.press/v164/murray22a.html) | Listener feedback depends on timing. Use observed pauses and sparse gestures; our rule is not their learned policy |

The previous design mentioned Disney timing, doubled conversational stumbles
and specific turn-gap comparisons without sources available in this repo.
Those are not acceptance evidence here. The 100 ms light response and 350 ms
body transition below are engineering targets to measure.

## Ownership and layers

The existing voice coordinates capture, playback and the ring. The body owns
the servo bus, calibrated posture, gaze corrections, clamps, speed, load and
thermal limits. Actions owns deliberate requests and lasting settings.
Expression supplies an optional plan, with deliberate Actions taking priority
for any actuator they need. No model is asked to calculate joint positions.

The planner combines a base posture, authored offset clips, a normalized gaze
request for the body's calibrated solver, and a face/brightness envelope.
Offsets are in radians relative to the owner's calibrated safe posture and
attended heading. They are **not** absolute `JointState` positions, encoder
counts or shares of the body's configured reach. Every sum is clamped before
it leaves the planner; the body still applies its stricter physical bounds.
No autonomous room search or random idle motion is authored.

`behaviours.json` is the built-in catalogue. `--catalogue FILE` replaces it
at startup after validation. Postures, offset bounds, face levels, freshness,
clip weights, triggers, states, delays, cooldowns and keyframes are data.
A clip using an existing trigger can be added without changing the engine.
A new evidence source requires a new typed input and engine rule. There are
at most 24 behaviors and 64 keyframes per behavior; catalogues are at most
128 KiB. A repeating clip is allowed only on actual playout.

| Initial behavior | Default plan |
|---|---|
| Look away and back | A small thinking glance develops over 250 ms. A committed first-audio schedule causes return over up to 250 ms, reaching the planned speaking posture 40 ms before that schedule. A measured body lag can be supplied; the default is unknown |
| Nod in a pause | Only after true human activity followed by a pause in the same turn. Begin after 120 ms quiet, authored stroke at 150 ms into the 320 ms clip, no more than once per 4 s. Resume speech cancels the nod |
| Follow played sound | Sway and light brightness follow valid playout intervals and their 0..1 level, not transcript timing or cloud packet arrival. A supplied measured body lag advances only the body's sample into the committed schedule; light samples the current interval. No valid interval means no audio modulation |
| Yield when interrupted | Retire the old turn's queued sound. Change the requested face on the next tick and blend the body from its last requested offsets to listening over 350 ms. Listening is shown only if capture can retain input |
| Stay still for work | Explicit concentration holds the body and suppresses expressive movement. A quiet room alone is not evidence that someone is concentrating |

A requested trajectory is not observed motion. The built-in amplitudes and
signs have not been tuned on the lamp. Small offsets can fall below the
body's 20-count movement deadband; a synthetic nod can therefore be invisible.
First should inspect that before increasing any amplitude. Motor speed,
clearance, calibration and acoustic onset remain the hardware owners' checks.

## Clock and input contract

All new payload times use `lamp_ipc::monotonic_us()` (`CLOCK_MONOTONIC`) in
microseconds, shared by processes on one host. `clock_id` names that host's
boot, normally `/proc/sys/kernel/random/boot_id` on Linux. It is separate
from `session_id`, which changes every time Voice starts, and from increasing
`turn` and `request` identities. Sequence is increasing within a publisher
session and topic. Backward clocks, expired input, mismatched identities and
retired audio must not acquire authority.

The existing bus header's `at_us` is a publisher's **private** clock and must
not be compared across processes. Live adaptation of legacy inputs uses
receipt time minus the header's Unix-millisecond age. It rejects stale,
expired, untyped and more-than-50-ms-future wall timestamps. Lasting light
and concentration settings are exempt from observation-age limits; they
remain in effect until explicitly changed. Loading an old light setting
stamps its receipt, not an imaginary new physical observation. This estimates
legacy observation age; it does not convert transcripts into audio timing.

| Topic / type | Publisher and required meaning |
|---|---|
| `voice/timing` / `VoiceTiming` | Voice: heartbeat every 100 ms; current state, input readiness, voice session, turn and request. `Scheduled` includes the first committed sample time within 2 s; `Interrupted`, `Cancelled` and `Finished` retire that turn's audio |
| `voice/activity` / `PersonActivity` | Voice: actual admitted human activity edges and heartbeats at least every 100 ms, under the same session/turn/request. A pause requires a preceding true activity observation |
| `voice/playout` / `VoicePlayout` | Voice: non-overlapping committed speaker intervals, at most 200 ms each and at most 2 s ahead, with a normalized level. Revisions/cancellation must retire old scheduled intervals |
| `attention/concentration` / `Concentration` | An explicit user/attention owner: concentration on or off. Room state or missing speech is never substituted |
| `voice/state`, `voice/heard`, `voice/said` | Existing Voice context; none can establish playback timing or safe listening readiness |
| `humans/faces/tracked` | Eyes: largest fresh confident face can supply normalized gaze; the body still owns camera calibration and its stillness/observation filter |
| `body/joint_states` | Body: fresh readback is required for body requests; absolute servo-middle angles are not reused as relative offsets |
| `room/state` | Voice: fresh quiet evidence permits optional nods, sway and gaze; unknown, busy or expired evidence suppresses them |
| `name/heard` | Name listener: contextual evidence only, never proof of admission or readiness |
| `light/command` | Actions: the user's lasting on/color/level setting. No light request is emitted before a valid setting is known |

First must add the three Voice publications. The existing `voice/state` does
not carry the private BodyLink's pre-audio speaking state or duplex-yield
mapping, so it cannot substitute for them. The useful speaker boundary is
`WorkerEvent::Playback` in `voice/src/wire.rs`: owner, cursors, observed time,
accepted frames and queue observation. The coordinator labels the first write
“ALSA accepted; acoustic onset unmeasured”. Preserve that distinction when
constructing an estimated committed playout schedule; do not call it a
measured first audible word. Clock reset, gaps, drained/retired sound and
cancelled tails also need to invalidate the schedule.

`voice/timing` is a latest topic, so a terminal edge must remain in its
heartbeats until a new turn replaces it. A one-shot event followed immediately
by an ordinary state can be missed by a subscriber. Cancellation is sticky
within a turn; stale voice input also retires its audio, and refreshing the
same turn cannot revive it. Session retirement is bounded: after 32 Voice
restarts, Expression stops issuing requests and reports that it needs a
restart to renew its session fence.

Room evidence expires after five seconds by default. Today's room publisher
sends on change, so First must renew the observation or implement a checked
adapter combining its latest value with publisher liveness. Merely starting
Expression does not make an old quiet-room observation fresh. Lasting light
and explicit concentration settings are the persistent exceptions.

## Output contract and First's integration

These are new topics. `light/command` remains the existing `Light` setting
channel and does not change type or ownership.

| Topic / type | Subscriber and action |
|---|---|
| `expression/body` / `ExpressionBody` | Body: apply bounded relative offsets or hold current pose; an optional normalized gaze is passed through its calibrated gaze solver |
| `expression/light` / `ExpressionLight` | Voice's ring owner: draw the face and brightness multiplier using the included persistent `Light` setting; preserve off and explicit settings |
| `expression/state` / `ExpressionState` | Operator/model/recorder: enabled/shadow, planned behaviors, missing inputs and warnings; not proof of execution |
| `diagnostics` / `Diagnostic` | Health: readiness, accepted/refused input, transport losses/errors and measured host service-loop time |

Both actuator requests carry the boot clock, Expression process session,
Voice session/turn/request, sequence, issue/apply time and expiry. A lease is
at most 200 ms and is renewed on a 20 ms planner step. At the final output
boundary, Body and Voice must validate the message, boot identity, producer
session, sequence, turn, freshness and privacy. Deliberate Actions and local
safety take priority. Do not replay a queued old command after a newer one.

The consumer integration needs its own option, off by default. When no valid
lease remains, return to today's voice-state behavior. Kill or stall
Expression and verify that fallback on the lamp. A privacy or input-readiness
failure must never be masked by an expressive listening face. A hold is an
expression request, not permission to override a stop or safety inhibit.

The planner publishes light before body, but the bus promises no cross-topic
ordering. Compare requested apply times and actual ring/joint observations;
publish order alone cannot establish that a person saw the face change first.
The body cannot promise return before sound until its lag and achievable
trajectory are measured. Never hold speech to make an animation finish.

## Commands and replay

Run from the standalone `lampOS/` directory:

```sh
cargo run -p lamp-expression -- catalogue
cargo run -p lamp-expression -- clock --clock-id development
cargo run -p lamp-expression -- serve --clock-id development --bus-root /tmp/expression-bus --seconds 5
cargo run -p lamp-expression -- replay --input crates/expression/fixtures/five-behaviours.jsonl --clock-id written-scenario --enable --until-us 2000000 --output /tmp/expression-plan.jsonl
```

The server above is shadow-only. `serve --enable` enables planner output,
not hardware ownership. Linux uses the boot identity automatically when
`--clock-id` is omitted. Its one-owner lock prevents two Expression processes
on one bus. No deployment unit is installed by this task.

Replay input is one `{ "at_us": ARRIVAL_US, "input": INPUT }` per line.
The payload's observation stamp remains separate from injected arrival time,
so a late event can be tested honestly. Replay never sleeps or publishes to
a bus. Equal-time inputs are applied before that tick; inputs between ticks
are applied before the next 20 ms tick. Output is `{ "at_us": TICK_US,
"output": OUTPUT }` per tick, with a summary on standard error.

`lamp-expression import --clock-id ID FILE...` reads `lamp-bus record` files
and writes replay input to standard output. It needs a new typed timestamp
as a wall/shared-clock anchor. Legacy header `at_us` is ignored. Imported
arrival times are estimates: recordings lack receipt times; wall-clock steps,
transport delay and send delay cannot be reconstructed. Use explicit replay
arrival timestamps for any latency assertion. Import warnings preserve that
limit and sequence gaps/restarts.

A latched setting may predate the recorded interaction by hours. Import does
not silently move that record to the start of the clip; it can consequently
exceed the ten-minute replay bound. In that case, supply a written replay
record with the setting's known load/receipt time, or record a fresh setting
publication. Its time is configuration loading, not a physical observation.

A replay is bounded to 32 MiB, 100,000 lines, 50,000 inputs and ten minutes of
simulated time, with a 128 KiB line limit. Import accepts at most 32 files.
Malformed records fail with a location; rejected engine inputs are reported.
Live input drain is bounded per topic. `--record FILE` records requested
output through a bounded queue; a slow/full disk drops records and never
blocks the planner. Files are created new, mode 0600, capped at 64 MiB.
Pending records can be lost at process shutdown. Bus failures do not grant
new authority; commands already sent expire.

## Verification and lamp checklist

Host gate results and source identity are recorded in
[Third's log](work-log-third.md). Tests use injected clocks for behavior,
real temporary Unix sockets for transport, and separate processes for CLI
and ownership checks. A replay establishes planned timing only.

On Third's Intel i7-10700K x86_64 Mac (64 GiB, Darwin 25.6.0, Rust 1.96.0),
the full dependent gate passed 1,701 tests with five existing ignored tests.
Expression's 67 tests and eight new message-contract tests have no ignored
cases. The standalone source copy built the workspace and passed 92
Expression/message checks, including a doctest. The initial parallel run's
Voice failures and the successful serial rerun are retained in the
[host evidence](verification/expression-host-20261014/results.json).

`cargo run --offline --release -p lamp-expression --example replay_cost -- 100`
measured construction, parsing, 101 planned ticks over two simulated seconds
and serialization to a sink for the 42-record written fixture. In the final
100-run series, the first iteration took 0.284349 ms, the median 0.192103 ms
and the worst 0.296518 ms; no inputs were rejected. Catalogue parsing precedes
timing. This is host computation, excluding process startup, bus transport,
disk recording, scheduler jitter and physical response. Both 100-run series
are retained in the evidence; no lamp or before/after improvement is claimed.

First can run the written fixture with the built `lamp-expression` binary
and compare the JSONL to the host's replay. Then record the lamp's real bus,
including the new timing/activity/playout inputs, and import/replay it. Before
letting either consumer act on it, inspect the requested offsets and expired
commands with offline `replay --enable`. Live shadow mode records state,
behaviors and missing inputs, but deliberately omits actuator payloads. To
inspect live actuator requests, explicitly enable the planner while both
actuator-consumer switches remain off. Once both consumers are enabled,
check: look back by first audible word, nod only in a person's pause,
immediate cancellation when speech resumes, light leading the interruption
transition, stillness during work, and fallback after killing Expression.

## Not shown

No Expression process, new input publisher or actuator consumer has run on a
lamp in this task. No physical motion, camera calibration, optical latency,
audio onset, acceptable expressiveness or safe motor trajectory is established
by the host tests. The inherited robot studies are not lampOS measurements.
Body lag is unknown by default. No real recorded bus stretch was supplied;
the checked fixture is written. Transport age estimates cannot repair clock
steps or recover receipt timing. First owns all device wiring and measurement.
