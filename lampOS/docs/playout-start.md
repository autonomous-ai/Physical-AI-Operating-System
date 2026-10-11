# Configurable reply startup buffering

This is an opt-in experiment on the finite directed runtime. The default remains
`--playout-start-ms 0`. It is not a selected production setting, a V1 latency
match, or a demonstrated acoustic improvement.

## Evidence and reuse

V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94` requests a persistent
sounddevice output stream with at least 120 ms output latency
(`hal/drivers/voice/tts/service.py:62–70,319–383`). The effective latency can be
larger when the device high-latency setting requires it, and is logged. Native
Gemini audio writes through that same stream (`service.py:1303–1387`), preserving
the streaming resampler. The live producer writes available frames immediately;
its pending list is for unavailable speaker ownership, capped at 30 seconds
(`hal/drivers/voice/voice_service.py:1711–1763`). The turn-based producer does the
same (`hal/drivers/voice/_internal/realtime_turn.py:739–774`). These inspected
paths do not implement a deliberate 200 ms provider startup buffer. V1's driver
latency request and the Rust coordinator horizon are different boundaries.

The Rust `StartGate` and focused ownership/dispatch tests reuse experiment
`7f24ea81d69a8d66ba6f1753274ac4ba6adb5a39`. That earlier experiment did not qualify
conversation quality. This change makes its horizon explicit and bounded,
keeps the current V1-style turn-taking path, and retains default zero.

A completed two-turn Gemini run on runtime `7dde135af`,
`lamp-rust-v1-gemini-evidence-20261011T062138Z`, provided a new timing case.
Turn 2 received 2,160 frames at 24 kHz: 90 ms total. The existing final-block
holdback leaves 1,920 dispatchable frames, or 80 ms. The next provider burst's
first enqueue arrived 162,784 microseconds after the first enqueue. The speaker
reported 1,920 accepted zero frames while that reply remained unfinished,
beginning 80,328 microseconds after its first accepted speech write. Both replies
completed and this trace contains no self-cancellation. These are accepted-PCM
and coordinator observations, not listened or acoustic gap boundaries.

Source: `trial-01-ns-off-target/runtime/events.jsonl`, SHA-256
`e397fb7025baf51c104698044a649cfd38470bdfcc0aab1ba67f747ba4d3427c`.

## Runtime contract

Both `directed` and `directed-fixture` accept `--playout-start-ms N`, where N is
an integer from 0 through 200. Missing, duplicate, negative, fractional or
out-of-range values are rejected before worker creation. The typed
`PlayoutStart` also bounds programmatic callers. It is orthogonal to diagnostics,
noise suppression, ring output and `--turn-taking v1-baseline|vad-only`.

For each newly admitted reply, the coordinator owns one small `StartGate`.
After matching provider `Started` and the actual local input endpoint, it asks
whether the first dispatchable PCM block may leave the existing reply queue:

- Empty PCM and a lone held final block remain unavailable as before.
- Generation completion or terminal failure releases any retained PCM
  immediately, including short replies. Provider failure stays a failed outcome.
- Otherwise, first-audio age plus queued audio duration must reach N ms.
- Once opened, the gate never closes for later chunks or later generations of
  the same reply. A new owner starts with a new gate.

The first-audio age is **not** added UX latency: input completion, provider
Started and real dispatchability can independently delay eligibility. The gate
adds no wait call, timer worker, queue or lock. With dispatchable PCM and all
other gates satisfied, time eligibility is reached no later than the configured
horizon after first enqueue; scheduling can add delay. This is not a kernel or
acoustic deadline. A lone final block may legitimately await completion longer.

The 100 ms output permit is minted only after opening. Cancellation, privacy,
current ownership, the finite session deadline, the 30-second bounded reply
queue, capacity credits, one outstanding speaker block, 10 ms final holdback,
and the small physical ALSA queue are unchanged. Held PCM has not been played:
it does not create a playback token or own-playback suppression. The ordinary
control/capture loop continues while it is held. Audio in later generations
still waits for the preceding final cursor's real retirement.

The trace records the configured horizon and the gate's queue observation at
first accepted speech and terminal outcome. `added_latency_us` remains null.
`StartNeed` is an arrival-only counterfactual estimate, explicitly separate from
actual playback and the held final block; it is never used for control. An
unfinished/cancelled arrival history is labeled censored. The same trace may
repeat the first gate observation for a later playback occurrence.

## Offline checks and next device comparison

Unit tests cover immediate completion, bounded time eligibility, no rebuffering,
reset and default zero. Private Unix-channel tests use the production dispatch,
provider capacity, current owner and V1-baseline mask: holding emits no speaker
packet, permit, accepted-write cue or input mask; later data can open the gate;
priority provider control, cancellation and privacy remain effective.

`playout_current_arrivals.rs` replays the 89 exact enqueue-time/count pairs from
turn 2, with its observed completion time and the real gate. It models a 10 ms
consumer clock at 20 phases. It stores no PCM or transcript. For that fixed
schedule, zero horizon produces 80–90 ms of modeled zeros; a 200 ms horizon
produces none and delays the first modeled write by 110 ms. This does not model
cloud feedback, IPC dispatch phase, ALSA delay, AEC or acoustic timing. It is one
held-out schedule, not evidence that 200 ms is generally optimal.

After the unchanged baseline, compare explicit zero and candidate horizons on
the same finite protocol. Retain every failed/cancelled turn and measure both
speech-end to first audible word and accepted-zero intervals. Report short
answers separately. A later network stall is intentionally not repaired by
this startup gate. No new device or cloud experiment is claimed by this patch.


The combined candidate on endpoint checkpoint `5a2f0076b` passed host workspace
formatting, strict all-target audio/live Clippy and 508 serial audio/live tests
(zero failed, two existing ignored). A 286-file standalone export including
all new sources passed Linux ARM64 release build and strict audio/live Clippy.
Sixty focused QEMU tests passed: production playout-start, turn taking, endpoint
core, gate unit tests and the retained arrival schedule. Native loader/CLI
staging also passed, without opening audio. Source manifest SHA256:
`c6d6edb5a94f69a4de597fa00c094e91d28bfc53d645aef27375b70ab57e36bf`;
ARM binary SHA256:
`a30055ba007d1e425205a771cfd022520ac702dea405adf8c654cbc53a75cf3c`.
These checks do not establish a physical playback improvement.


## Physical comparison on 2026-10-11

The first explicit 200 ms attempt ended after eight completed replies because
the private test runner rejected a legitimate second playback occurrence in
turn eight. Its forced termination left unsealed diagnostics. That attempt is
retained as a runner failure, not a dropped eighth answer or a qualified run.

After fixing only that matcher, `20261011T075239Z` completed ten repeated
cached English greetings on the exact `a30055ba...` binary. Both target
completion certificates and both source WAV identities verify. There were no
extra inputs or cancellations. Seven replies had no accepted-zero starvation
gap; the other three had 320, 50 and 70 ms, totaling 440 ms. The earlier zero
horizon run had 15 gaps totaling 1390 ms, but generated content and duration
changed. This is a descriptive comparison, not a causal improvement estimate.

Quality still fails: two output transcripts are Spanish and another reply is
irrelevant. One turn waited 10.755097 s from local endpoint to first accepted
write; 10.655587 s elapsed before provider first audio. A separate model
comparison addresses that uncertainty. The 200 ms horizon is still an opt-in
candidate; the default remains zero.

All ten gate queue/age observations match the actual enqueue trace. One turn
initially received one sample, then waited 389.623 ms for another packet, so
its approximately 390 ms gate age is not all buffer-added latency. No actual
added-start latency or acoustic first-word boundary is claimed. Source and
target settings were restored. Detailed evidence identities are in the
[coordination result](../coordination/main.md#completed-buffered-run-and-next-model-comparison).
