# Main agent status

- Protocol: `direct-and-git-v2`
- Updated: 2026-10-11
- Branch: `lamp-v2-chat`
- Integrated code checkpoint: `a253abe5eb4fd09b666959e059e8249930af5c0a`
- State: first physical Rust response recorded; content and playback defects remain. Release and benchmark unfinished.

## Direct coordination

The three existing sessions are members of the Harness team **Lamp Rust
implementation**. Main and Agent 1 run on office; Agent 2 is the M2 session
**lampOS voice-eval runner**. Agent 2 has replied through Harness and accepted
MAIN-005/006/014/015. Agent 1 publishes acknowledgments in Git: its session
cannot obtain a verified Harness team context (`CHANNEL_SCOPE_MISSING`).
Respect that guard; use its Git status and ordinary messages to the verified
session. Checkpoints 5–7 answer MAIN-007/008/011/012/017/019; Main is reviewing the combined provider/audio source.
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

The two MAIN-008 failures were reproduced and corrected privately. The checked
Start binds input to the selected session, and a valid failure before Started
retains endpoint draining without granting PCM authority. The corrected slice
has 152 focused host tests passing with strict live Clippy and formatting; two
existing long tests were not rerun. Main is now combining it with Agent 1's
latest capture/provider source. Canonical integration and combined verification
remain pending. The physical trial used Legacy/Require.

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
tests under QEMU passed. The staged file's hash matches. The first finite
physical run then produced one answer with legacy services stopped. It failed
content quality: an English greeting was interpreted as Italian. One 30 ms
inserted-zero playback gap interrupted the opening voiced segment. Source and
target diagnostic recordings, hashes and restoration were independently checked.
Local endpoint to first ALSA write was 1.046792 s for this single turn; acoustic
first-word latency is unscored. No V1-main comparison or speedup exists yet.

The owner's subsequent same-Lamp check used 52e6's speaker and both its own
microphones at volume 60. Both recordings recovered the sentence except for
ASR spelling of “Lamp”; Jieli had no clipping, C-Media still clipped. This was
an installed-V1 audio-route check, not a Rust conversation. See the exact
[physical trial evidence and limits](../docs/real-device-audio-pilot.md).

The target returned to software-muted legacy services with the serial guard
retained. V2 is not the continuously running default yet. Main is pinning test
stimuli and isolating recognition before a repeated benchmark. MAIN-022 gives
Agent 1 the actual chunk timing and gap; MAIN-020/021 gives Agent 2 the exact
failure contract and an offline recording-import/annotation-proposal task.

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
- Agent 1 checkpoint 7 `ae4f05db4` / tested `a77bdb0fd`: fetched; combined provider/capture review in progress. Images stay inactive.
- Agent 2 `0239ac27c`: reviewed, six corrections requested and accepted.
- Agent 2 `b7fbe352e` / tested `d5b848b3`: fetched; published runner, failure-evidence and image-privacy corrections awaiting combined review. Not integrated.
- Provider/coordinator consumer: two reproduced blockers corrected in private; combined candidate not yet integrated or deployed.
