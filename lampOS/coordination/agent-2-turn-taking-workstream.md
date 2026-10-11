# MAIN-051 — Agent 2 owns fast and reliable end-of-turn processing

The owner explicitly requested one substantial autonomous workstream. Complete
the isolated Rust endpoint subsystem, its full conversational policy, integration
proposal and qualification together. MAIN-047 and MAIN-049 are requirements
inside this workstream, not separate permission gates. Do not stop after moving
the existing model into a process or after another classification report.

## Product outcome

Lamp begins a useful reply promptly when a person has finished, but continues
listening through hesitation, an unfinished sentence, a correction or resumed
speech. Target a 200–400 ms end decision for clear completed short turns, with
the existing safe fallback for uncertainty. Overall audible reply targets are
p50 300–500 ms and p95 <=1 s for ordinary conversation; end-of-turn detection
alone cannot reach them while provider startup is slow. Do not obtain a faster
number by discarding slow, interrupted or failed turns.

## Ownership and working base

Work on `lamp-a2-endpoint-runtime`, preserving `lamp-a2-endpoint` and all its
evidence. Fetch the current integration; reconcile your useful endpoint work
without importing unrelated older camera/evaluator branches. Main is currently
integrating continuous service, mute/reopen, ring isolation and camera teardown;
do not wait for those changes before working on your independent worker/policy.

Own `crates/endpoint/**`, `crates/live/src/activity.rs`, a separate endpoint worker
binary/crate, new endpoint IPC/client modules, focused tests/fixtures and
`docs/endpoint-detection.md`. Dependency/lock changes and minimal coordinator,
wire or CLI integration are authorized proposals in your own branch, in separate
commits for Main to compose. Do not link tract/model inference into the core.
Do not edit Agent 1's audio, AEC, reference, playout or interruption admission.
Routine implementation choices within this scope do not need another approval.

## Deliver the whole path

1. Finish the independent inference process: bounded input/result channels,
   one in-flight inference, original utterance/worker IDs, expiry, privacy and
   resumed-speech invalidation. Loading, missing model, malformed data, crash,
   timeout or a hung inference must not delay capture, interruption or shutdown.
   Reap an old worker before any replacement. Keep startup, model assets, pinned
   hashes, licenses, build and installation self-contained in lampOS.
2. Correct and evaluate policy, not only the model port. Current candidate adds
   seven synthetic false splits; keep it experimental until a better tradeoff is
   demonstrated. Reference parity means the port agrees with the reference; it
   does not establish that the deployed policy is suitable for people.
3. Build full-utterance tests with actual human internal pauses and continuations:
   short complete answers, yes followed by more detail, hesitation, correction,
   sentence fragments, 0.3–1.5 s pauses, quiet starts, noise and language variety.
   Reuse licensed upstream human fixtures within existing permissions. Keep
   current macOS/synthetic stress cases, human loudspeaker replay and direct-human
   cohorts separate. Preserve inputs, labels, hashes and all outcomes; hold out
   a validation partition instead of selecting thresholds on the test shard.
4. Reduce latency through preprocessing reuse and bounded scheduling where the
   measurements justify it. Consider inference while speech is still arriving
   without committing an early response or making stale verdicts authoritative.
   Measure snapshot, feature, inference, IPC and decision costs individually.
   Preserve audio and fresh capture throughout. Do not merely lower a silence
   timeout again or impose an ever-longer wait on unfinished speech.
5. Exercise the actual worker plus coordinator seam: missing-model full session,
   late results after resume, back-to-back turns, mute/unmute, shutdown during
   inference, busy queue, restart and sustained resource bounds. Return explicit
   decision provenance and fallback reason. No silent regression when the model
   is absent. Keep a working no-model configuration.

## Completion and coordination

Deliver a complete opt-in implementation and integration proposal, model fetch
and worker startup, production-path regressions, full-policy replay results,
CPU/RSS/binary-size/startup bounds, host fmt/strict Clippy/tests and Linux/ARM64
build/evaluation. Explicitly distinguish emulation, classification, replay and
physical evidence. Main measures cold/warm inference and real conversation on
A733. Do not state that its performance is known from an M2 VM.

Continue autonomously through these steps. Publish meaningful milestones rather
than wait for the next small assignment. Exact-file commits/pushes to your work
branch are authorized; Main alone merges and deploys. Put status and exact tested
SHA in `coordination/agent-2.md`, request concrete missing interfaces directly,
and continue independent work during review. No owner relay is needed. No cloud
upload of office recordings, device action or shared ARM job without Main's
reservation. No new memory, Harness or long-running agent subsystem in this task.
