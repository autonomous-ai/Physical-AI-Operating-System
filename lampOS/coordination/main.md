# Main agent status

- Protocol: `direct-and-git-v2`
- Updated: 2026-10-11
- Branch: `lamp-v2-chat`
- Integrated code checkpoint: `a253abe5eb4fd09b666959e059e8249930af5c0a`
- State: first audio-only V2 deployment and recorded conversation take priority; release unfinished.

## Direct coordination

The three existing sessions are members of the Harness team **Lamp Rust
implementation**. Main and Agent 1 run on office; Agent 2 is the M2 session
**lampOS voice-eval runner**. Agent 2 has replied through Harness and accepted
MAIN-005/006/014/015. Agent 1 publishes acknowledgments in Git: its session
cannot obtain a verified Harness team context (`CHANNEL_SCOPE_MISSING`).
Respect that guard; use its Git status and ordinary messages to the verified
session. Checkpoint 5 answers MAIN-007/008/011/012; Main is reviewing it.
No owner report relay is needed. Git retains exact code, evidence and decisions.

MAIN-001 (protocol acknowledgment), MAIN-002 (Agent 1 publication), and MAIN-003
(Agent 2 publication) are answered by both agents' published status files.
Later checkpoints still require review; publication is not integration.

## Accepted checkpoint and verification

Integrated Agent 1 `64806f8624ec3425f820e5b087afeb58d2bbbc94` as `a253abe5e`:
failed reconnect attempts are harvested and retried within existing bounds even
when the previous outage notice remains unread. This changes Gemini recovery;
the directed CLI still uses Legacy reporting until the combined consumer passes.

Main reran from the integration tree on macOS:

```sh
cargo fmt --all -- --check
cargo clippy --locked --offline -p lamp-gemini --all-targets -- -D warnings
cargo test --locked --offline -p lamp-gemini -- --test-threads=1
```

All passed: 136 Gemini tests, zero failures, two long soaks ignored. No claim of
whole-workspace, ARM or physical conversation qualification for this merge.
Agent 1's later echo-evidence, notice and visual-input changes are separate
checkpoints awaiting review.

## Current interface ownership and open requests

Main owns `provider_worker.rs`, `provider_link.rs`, `coordinator.rs`,
`coordinator/**`, `wire.rs`, `admission.rs`, and fixture migrations for the
priority availability batch. Agent 1 owns `crates/gemini/**` and its audio scope.
Agent 2 owns camera, perception and evaluation. Shared edits remain isolated.

- **MAIN-007 / A1-002:** prefer failing the affected answer and bounded recovery
  after playback backpressure, with no replay and one lost-request report.
  Authentication/quota faults and missing resumption context remain terminal.
  Agent 1 supplies focused regressions; Main enables the behavior with the
  coordinator's failure reporting after review.
- **MAIN-008:** add checked start against the coordinator-selected provider
  session before enqueueing any input. A deterministic private regression proved
  a delayed Start selected on A can otherwise reach recovered B. A second
  regression proved a legitimate lost request may precede Started; Main must
  accept its exact failure without granting PCM authority. The frozen consumer
  is not approved for activation while either regression remains red.
- **A1-003:** optional frame-local echo evidence is acceptable; Main owns wire
  migration. Synthetic evidence does not establish an acoustic admission
  threshold on the board.
- **MAIN-005/006:** Agent 2 accepted corrections for exhausted perception control
  drains, the perception/live dependency cycle, blocking frame connection,
  never-terminated relay-line byte limits, frame freshness semantics, and
  cleanup after the coordinator leader has already exited. Review covered
  `0239ac27c` (code `9bfd58242`); that code is not integrated.
- **MAIN-005 / A2 shared seams:** a narrow camera registration and perception
  workspace change are acceptable in principle after fixes. Hardware-profile
  injection must be behind explicit test support, leaving production executable
  and legacy-owner checks intact. Evaluate the actual directed coordinator with
  scripted Gemini; do not wait for a future service.
- **MAIN-009:** evaluation must retain `provider_failed` as a terminal failure
  before or after partial playback, never as success. Drain-end evidence is not
  a normal submitted-input endpoint; cue ordering must account for that path.
- **A1/A2 visual context:** provisional maximum two JPEGs of 64 KiB per request,
  preserving source freshness, privacy and owner. Reduction precedes provider
  submission; image work must not block capture or fill the audio queue.

The private priority-provider candidate has 264 distinct focused host tests
passing, but an independent adversarial audit found the two MAIN-008 blockers.
Those passing tests do not qualify its combination with newer peer changes.
Agent 1 has now supplied the checked start; Main has a private regression fix
for legitimate failure before Started. Integration and combined verification
remain pending. Production remains Legacy/Require.

MAIN-013/016: the exact provisional failure schema is now durable in
[failure-cue-contract.md](failure-cue-contract.md). Agent 2 did not receive the
first native reply; Main resent it as a new question. Do not infer successful
delivery from an accepted CLI command.

MAIN-014 priority: Main deploys and records the first finite audio-only run.
Agent 1 supplies provider fixes; Agent 2 supplies actual directed-runner cues
and scoring. Optional camera/perception/image changes remain inactive and do
not gate this first measurement. Agent 2 `b010aa0b8` corrects failure during
the trigger delay; the attempt-level latch and final dispatch guard follow.
Its simulated EPIPE/ReferenceLate failures remain simulation evidence pending
trace review, not measured Lamp failures.

## Devices and reservations

| Device | Address | Owner | State |
|---|---|---|---|
| lamp-a0ae | 172.168.20.197 | Main | Primary native Rust/provider qualification; no motion |
| lamp-52e6 | 172.168.20.190 | Main | Repeat checks; preserve existing STT experiment drop-in |
| lamp-4ace | last known 172.168.20.159 | Main | Not rediscovered in the last authorized scan |

The iMac source was not recognizably captured by these distant Lamps. A bounded
cross-Lamp route test now completed: four 44.875-second recordings, both Jieli
speech inputs and both C-Media ambient inputs, with exclusive capture receipts.
Both Lamps' original settings were restored and verified. Local cached ASR
recovered the a0ae-to-52e6 Jieli phrase exactly; the reverse phrase was
recognizable with substitutions. The ambient channels were less faithful and
own-speaker ambient recordings clipped. These are installed-V1 speaker-replay
route observations, not human or V1-main/V2 latency benchmarks.

On a0ae, the real Gemini handshake took 1.952295 s. Endpoint host:
`campaign-api.autonomous.ai`; model `gemini-3.8-live-extended-thinking`, voice
`Kore`, thinking `LOW`. Bounded service probes then observed a resumption handle
after an answer and accepted it on a replacement connection. The initial
interrupt-before-output probe was inconclusive: local command coalescing did
not prove that the first End was sent. Agent 1 corrected that probe. None of
these probes opened microphone/speaker hardware or measured acoustic latency.

The new ARM64 `lamp-live` build from exact code tree `51bd815e9` is staged on
a0ae. SHA256:
`d3318abe080336dbd4c50d9d04e4517eb21cd0b7cec5afaa5d5f62e990b525e7`.
Offline ARM64 release build, strict all-target live Clippy and 12 provider-flow
tests under QEMU passed. The staged file's hash matches. This does not yet
establish a successful Rust conversation on either replacement device.

Main has a temporary key-only maintenance SSH service on a0ae, restricted to
the authorized Mac source and a two-hour lifetime, leaving the existing SSH
service unchanged. It retains control while the legacy server is stopped.
Main owns cleanup and the separate finite audio trial/rollback. Private keys,
provider credentials and room recordings stay outside Git.

The installed HAL shutdown path can park/release servos. No ordinary HAL stop
or restart is authorized by this reservation: Main must verify the installed
units, motor exclusion and independent rollback before hardware handover. The
private motor candidate remains unintegrated and physically unqualified.

## Integration ledger

- Earlier `d6b813695` and Agent 2 `3eaa0365`: included before `2535fb565`.
- Agent 1 `64806f862`: integrated and focused-tested as `a253abe5e`.
- Agent 1 checkpoint 5 `0bf54e63f` / tested `d938a3101`: fetched; review pending.
- Agent 2 `0239ac27c`: reviewed, six corrections requested and accepted.
- Agent 2 `bb7e61aec` and `b010aa0b8`: fetched; six prior corrections confirmed,
  runner and image privacy follow-ups acknowledged. Not integrated.
- Provider/coordinator consumer: private, two reproduced blockers; not deployed.
