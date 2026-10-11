# MAIN-050 — Agent 1 owns continuous hearing and natural interruption

The owner explicitly requested a substantial autonomous workstream, not small
follow-up tasks. This assignment supersedes the narrow next-task part of
MAIN-048. Keep the delivered playout candidate and its evidence intact. Own this
workstream through a reviewable working implementation, integration proposal,
stress evidence and English documentation; do not stop after an audit or wait
for another small assignment.

## Product outcome

Lamp keeps hearing while speaking. Its own voice never counts as the person;
a genuine topic change stops the old reply promptly, preserves the person's
opening words and lets the new question proceed. Sustained conversations survive
realistic capture/playback clock skew, pauses and streaming bursts. The person
does not need to say a special stop command. Background talk and brief listener
acknowledgments must not become automatic interruptions merely because VAD fires.
Do not claim social/addressee recognition from audio-level rules alone.

## Ownership and working base

Work on `lamp-a1-duplex`, preserving `lamp-a1-playout` and its integration branch.
Start from the latest fetched integration plus your accepted playout proposal if
Main has not pushed that composition yet. Main is composing it with continuous
service, mute/reopen, ring isolation and camera teardown; do not wait for that
integration to start your audio work.

Own `crates/audio/**`, `crates/live/src/audio_workers.rs`,
`crates/live/src/playback.rs`, `crates/live/src/turn_taking.rs`, new audio/duplex
modules, their focused tests and `docs/duplex-listening.md`. You may implement a
complete integration proposal in your own branch, including minimal coordinator,
wire and option changes; isolate those shared files in a separate commit for
Main. Routine design decisions inside this scope need no further permission.
Do not edit Agent 2's `activity.rs`, endpoint model/worker or inference policy.
Main owns social vision, choreography, deployment and final default selection.

## Deliver the whole path

1. Reuse pinned V1-main safeguards and the recorded self-echo evidence. Explain
   which protections remain, and where duplex deliberately differs from the
   conservative baseline. Keep a usable conservative fallback.
2. Build an interruption decision using the available microphone, accepted
   playback reference and existing DSP. Avoid decisions from VAD probability or
   signal level alone. Preserve double talk, quiet onsets and a topic change;
   include clipped/loud echo, stale reference and resumed speech controls.
   If a particular distinction cannot be made reliably on this hardware, expose
   that limitation and a conservative fallback rather than inventing confidence.
3. Complete the stop/cancel path through the actual coordinator/speaker seam.
   Target <=200 ms from a clearly established genuine interruption onset to
   stopped speaker output, reporting detection delay and stop delay separately.
   Retain the original onset and request identity; no stale completion, burst,
   reconnect or buffered old answer may overwrite the new turn. This is a target,
   not a device result before Main measures it.
4. Solve sustained reference-clock behavior. Reproduce gradual USB clock skew
   through production reference handling, distinguish real missing/reordered
   packets, and propose a bounded correction using maintained DSP/resampling
   practice where appropriate. No silent sample-counter reset, fake continuity
   or unbounded buffer. Include hours of virtual-time stress and a paced process
   check; keep those distinct from hours on physical hardware.
5. Verify reference continuity, cancellation, no self-interruption, output gaps,
   retained input prefix, CPU/RSS and queue bounds together. Use both positive
   overlapping-human controls and negative self-playback/background controls.
   Synthetic overlaps stay labeled synthetic. No threshold selected only on
   known negatives. Preserve every failed case and a held-out test partition.

## Completion and coordination

Deliver production-path code, runnable deterministic regression/replay commands,
one opt-in integration mode, explicit fallback, model/dependency licenses if any,
host fmt/strict Clippy/tests and Linux/ARM64 compile evidence. Report measured
boundaries and failures, not only test counts. Main supplies and runs physical
trials and returns exact artifacts. Ask Main for a specific missing recording or
interface decision; continue independent implementation while it is pending.

Use small commits internally, but continue toward the full outcome. Push to your
work branch under the standing authorization. Send milestone SHA and blockers
directly when possible and write `coordination/agent-1.md`; the owner does not
relay routine messages. No device access or ARM VM overlap without Main's concrete
reservation. Raw office recordings stay local. No Harness/memory/agentic tasks,
motor motion, new provider purchase or fleet release belongs in this workstream.
