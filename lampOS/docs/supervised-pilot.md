# Supervised real-user pilot

Owner priority, 2026-10-11: prepare Lamp for real-user feedback within 12 hours,
approximately 2026-10-12 03:18 UTC / 10:18 Vietnam time. This is a supervised
pilot target, not a claim of release qualification or human-level latency.
The complete live-interaction goal remains in [architecture](architecture.md).

## Pilot configuration

Use a fixed Lamp beside one person at a desk with the qualified microphone,
speaker and physical privacy controls. Keep one explicit runtime/configuration
identity and retain the prior stationary runtime for rollback. Keep motor motion
disabled until the actual placement, joint qualification and clearance are
verified. Optional sight and ring behavior may be enabled only with their
qualified setup; their failure must not end healthy conversation. Agentic tasks
and new memory services remain outside the pilot.

Preserve ordinary chat, relevant short replies, follow-ups, corrections and
thoughtful pauses. Continuous listening/interruptions remain experimental until
the full capture, admission, provider cancellation and playback path is verified.
Do not advertise that capability based on offline mixes or mute input to claim a
successful interruption test. Do not use stock spoken fillers or arbitrary motion.

## Release gate for an attended session

- Complete a continuous conversation/idle/conversation run without a fatal
  audio-reference fault, lost accepted answer or unexplained extra admission.
  Keep all attempts and whole-room evidence, including failures.
- Verify local mute and stop, truthful readiness, bounded shutdown, and a fresh
  allowed state before reopening. Failed or uncertain cleanup must stay visible.
- Confirm clear audible prompts/replies at the intended desk. Synthetic iMac
  playback is a fixture; the first users provide the direct-human cohort.
- Exercise corrections and pauses, and qualify natural interruptions separately.
  List any limitation in the test guide rather than claim complete coverage.
- Give the facilitator the installed revision/configuration, recovery procedure,
  known limitations and a short feedback sheet. Preserve user-reported misses,
  cutoffs, unwanted answers, unclear replies, pauses and perceived pace.

These gates qualify an attended feedback session, not unattended all-day use.
Do not hide a missed gate to meet the clock. Report a narrower ready scope and
the actual blocker if the deadline arrives first.

## Current critical path and ownership

The latest continuous native run completed eight exchanges then failed during
idle at 378.6 seconds with `reference_backlog`. Later scheduled prompts were
withheld. A complete 600-second local room recording covers the target interval;
stationary rollback is verified. Agent 1 owns bounded reference recovery and the
fixed-margin backlog diagnosis, including actual loss versus scheduling stalls.
Main owns integration, process lifecycle, deployment and native qualification.
Agent 2 owns the end-of-turn/whole-thought workstream, including the independent
identity, closure and cleanup review findings. All direct coordination uses tmux.

The existing latency evidence remains approximately 2.68–2.85 seconds by adding
the fixed 1.2-second silence wait to recent endpoint-to-first-speaker-write medians.
This is a software estimate, not accepted first-audible-word latency. No new V1
benchmark is planned. Reliability and truthful pilot limits take priority over
turning an unqualified timing improvement into the default.

## Continuation prerequisite

The real service probe on 2026-10-11 failed the strict continuation prerequisite:
the previous response's settlement timer expired while the successor question
was still uploading. The [full evidence](verification/strict-continuation-20261011/README.md)
is functional and not acoustic. Keep `smart-turn-early` disabled until the actual
whole-thought path qualifies; a simulated provider settling interrupted requests
is insufficient. Conservative endpointing remains a separate optional candidate.

Agent 1 corrected the three independently reproduced reference-recovery defects
at `5df4248c3`. Main integrated that source with the duplex prefix-retention
correction and a separate prime-handshake repair. The latter distinguishes
buffered preparation from actual DSP analysis and keeps startup privacy, cursor,
epoch and deadline checks. Default flags remain unchanged. The current native
candidate explicitly selects capture-clock reference, V1-baseline turn-taking,
and recovery off. Its sustained run is still pending; a passing soak would prove
that run's continuity and cleanup, not general conversational latency,
interruptions, addressee recognition or unattended use.


## Native04 failure and Native05 candidate

Native04 (`ff20fe845c3d` source manifest, `f9f5f03e8029` binary) failed before
any prompt: the speaker required analysed cursor 480 while capture-clock mode
truthfully reported prepared 480 and analysed zero. All 16 prompts were withheld.
The outer stationary rollback succeeded; the runtime's abnormal worker exit
remains a failure. Its metadata writer acknowledged all 14 events without loss.

The sole room recording completed its 600-second budget but lost 37,376 frames
across 73 callbacks (0.778667 s at 48 kHz). It remains invalid for continuous
acoustic scoring. The gap was about 261 s after the target ended. No permission
loss, input-device change or CoreAudio stream error was recorded; the underlying
writer scheduling versus I/O cause is unresolved. No room audio was uploaded.

Native05 uses source manifest
`b0bada56b63e5c8887965fd0108f18c79b6dc83a9c8213cc4b6ab9298873cdb8`
and binary `738503a803d3718e52ca9e8886a669c0e41c007f7fe35873642afd4a83f5b8d0`.
The exact source passes formatting, strict audio/live all-target Clippy,
120 targeted Linux/ARM64 tests under emulation and a release build. These include
14 real Unix-socket prime/channel tests; ALSA startup is checked separately on
the device. This is a focused gate, not a fresh complete-workspace pass. A prior
same-base full live run retained one load-sensitive ring-trace failure, followed
by a passing isolated rerun; that earlier failure is not erased.

Native05 started on 4ace at about 17:06 UTC on 2026-10-11. Startup and the first
three completed prompt/reply lifecycles were observed in native cues; continuous
room recording is running. The conversation/idle/conversation result and final
recording integrity remain pending. Evidence stays in
`/private/tmp/lamp-serve-native-20261011-05`; the failed attempt stays in
`/private/tmp/lamp-serve-native-20261011-04`. Motors remain disabled.

A separate direct-service synthetic probe on the same date confirmed an
interruption after output had begun: the old request's interrupted event arrived
358 ms after the successor Start was queued and its idle completion after
362.990 ms; the successor was answered and the eight-second aftermath completed.
This is one functional service trial, without microphone or speaker, and does
not qualify early endpointing or natural interruption. The proposed 10-second
observation probe failed configuration locally because the library caps that
timeout at five seconds. It supplied no additional service observation.
