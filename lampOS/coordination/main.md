# Main agent status

- Protocol: `direct-and-git-v2`
- Updated: 2026-10-11
- Branch: `lamp-v2-chat`
- Integrated code checkpoint: `a253abe5eb4fd09b666959e059e8249930af5c0a`
- State: implementing and qualifying alternate devices; release unfinished.

## Direct coordination

The three existing sessions are members of the Harness team **Lamp Rust
implementation**. Main and Agent 1 run on office; Agent 2 is the M2 session
**lampOS voice-eval runner**. Agent 2 has replied through Harness and accepted
MAIN-005/006. Agent 1 acknowledged the shared protocol in its Git status;
MAIN-007/008 are queued through Harness pending its next safe boundary.
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
Production remains Legacy/Require until corrected combined verification.

## Devices and reservations

| Device | Address | Owner | State |
|---|---|---|---|
| lamp-a0ae | 172.168.20.197 | Main | Primary native Rust/provider qualification; no motion |
| lamp-52e6 | 172.168.20.190 | Main | Repeat checks; preserve existing STT experiment drop-in |
| lamp-4ace | last known 172.168.20.159 | Main | Not rediscovered in the last authorized scan |

Both alternate C-Media ambient inputs recorded their own speaker tests.
Temporary capture gain 8/35 with AGC off eliminated clipping at speaker volume
100; original gain, AGC, volume and software mute states were restored and
verified. The iMac question was not recognizably captured. The acoustic route
and new V1-main/Rust conversational latency cohort remain unqualified.

On a0ae, the staged Rust binary completed a real Gemini connection handshake in
1.952295 s using the existing device-side credential. Endpoint host:
`campaign-api.autonomous.ai`; model `gemini-3.8-live-extended-thinking`, voice
`Kore`, thinking `LOW`. No audio was sent and no microphone, speaker or motor
opened. Existing service states/PIDs and legacy configuration stayed unchanged.
This is cold setup timing, not speech-end to first audible word.

Staged `lamp-live` SHA256:
`199903b36fd3212df7e6d054adba03c6259fcb6248f60813cc7c27d4e303bf6c`.
It predates this recovery merge. A separate ARM64 `service_probe` was built from
`64806f862` with SHA256
`20942e62a256b002436de44b546ac649b213f21253adf13584de06f8a08544e7`;
only its offline QEMU fixture has run. It opens no hardware and will use the
credential on the device for bounded service behavior probes.

The installed HAL shutdown path can park/release servos. No ordinary HAL stop
or restart is authorized by this reservation: Main must verify the installed
units, motor exclusion and independent rollback before hardware handover. The
private motor candidate remains unintegrated and physically unqualified.

## Integration ledger

- Earlier `d6b813695` and Agent 2 `3eaa0365`: included before `2535fb565`.
- Agent 1 `64806f862`: integrated and focused-tested as `a253abe5e`.
- Agent 1 later code: published separately; review pending.
- Agent 2 `0239ac27c`: reviewed, six corrections requested and accepted.
- Agent 2 `6699284d4`: coordination acknowledgment published; code unchanged.
- Provider/coordinator consumer: private, two reproduced blockers; not deployed.
