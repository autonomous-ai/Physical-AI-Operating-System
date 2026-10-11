# Main agent status

- Protocol: `direct-and-git-v2`
- Updated: 2026-10-11
- Branch: `lamp-v2-chat`
- Integrated runtime checkpoint: `02148ed4d580046194fe6492ea480bc2502eba5d`
- Integrated evaluator checkpoint: `4127dd2bed2bf9b478fe86d721b3576ece45a5bf`
- State: coordinated runtime has completed a native greeting; evaluator dispatch corrections are integrated. A private playback candidate exposed repeated answer interruptions. Natural conversation and benchmark qualification remain unfinished.

## Direct coordination

The existing sessions are members of Harness team **Lamp Rust implementation**.
Main and Agent 1 run on office; Agent 2 is the M2 session **lampOS voice-eval
runner**. Main replies directly to team questions; Git retains source, interface
contracts, results and decisions. No owner relay is needed. Agent 1 previously
could not obtain its team context (`CHANNEL_SCOPE_MISSING`); do not circumvent
that guard. Its latest A1-009/010/011 question arrived through Harness and Main
answered there as MAIN-028. Queued delivery is not proof that an agent read it.

## Integrated checkpoint

`02148ed4d` combines Agent 1's `ae4f05db4` capture/Gemini source with Main's
coordinated worker and consumer. Directed mode now uses `Reporting::Coordinated`.
Connection availability has a priority channel independent of buffered PCM.
Every Start is checked against its selected provider session before submission;
a refused request cannot enter a newly recovered connection. Exact failures
before Started are retained without granting PCM authority. An unfinished input
drains through its real endpoint before fresh speech may enter the recovered
session. Previously received audio drains with a failed outcome, never success.
Require remains the cancellation policy; no input is replayed.

Source clock observations carry conservative correlation intervals sampled from
both clocks. They are not exact latency measurements and cannot renew old source
freshness. See the [provider link](../docs/provider-link-channel.md),
[coordinator outage contract](../docs/coordinator-outage-contract.md), and
[failed-turn cue schema](failure-cue-contract.md).

Verification from the integration tree:

```sh
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1
```

All passed: **960 tests, zero failures, four ignored**, including one doctest.
The ignored tests are two prolonged Gemini recovery soaks and real-time 60 s /
180 s provider-worker tests. A source-only Linux ARM64 build and strict all-target
Clippy for audio/Gemini/live passed, plus five paired coordinator/provider tests
under release QEMU. An independent source review found no blocking defect.

Binary SHA256:
`fb7e11911b7b84ef7a0af40a0c0c8ec26e7fc852dcd1b4228d3b173a2b979371`.
It is staged on a0ae at `/var/tmp/lampos-v2-a0ae-20261011-0305/lamp-live`, with
matching hash and a successful native CLI/provider check (setup 1.858044 s).
That check opened no microphone and sent no audio. A subsequent native greeting
completed with a relevant English answer, but two accepted-render gaps of 80 ms
and 140 ms. One successful turn does not qualify outage recovery or natural
conversation. See the physical evidence below.

Evaluator-only checkpoint `2a1522eba` integrates the reviewed 18-file subset
through Agent 2 tested code `9ed94e42`, excluding importer and camera/perception
changes. Root ran fmt, package all-target strict Clippy and all **170 evaluator
tests**, zero failures/ignored. An independent audit passed 72 focused executions,
including six reproduced dispatch regressions and drain-budget exhaustion.
These checks use scripted processes/players, not physical audio.

Receipt-import checkpoint `4127dd2be` adds the reviewed 14-file subset from
Agent 2 through `a86f2cea`, with its checked frame bounds, exact recorder-receipt
rebinding, target-render verification and independent importer regressions.
Root ran workspace fmt, package all-target strict Clippy and the complete
serial evaluator suite: **197 passed, zero failed or ignored**. The combined
tree excludes automatic boundary proposals, alignment code and their CLI.
These are host checks; this checkpoint changes no deployed audio runtime.
Proposals and unlistened recordings cannot become accepted acoustic timings.
The receipt file and selected input remain an operator-supplied anchor, not
an authenticated recording identity.

## Ownership and decisions

The owner requires V1 main behavior as the rewrite baseline: port useful
safeguards, establish Rust parity, then improve measured behavior. The
[voice parity plan](../docs/v1-voice-parity.md) distinguishes stock
turn-taking from optional adaptive live interruption. Neither protection is
yet active in the current Rust VAD-only directed path. Agent 1 acknowledged
MAIN-037 and is comparing the actual V1 gate offline; Main owns its Rust
integration. No successful port or physical interruption result is claimed.

Main owns `provider_worker.rs`, `provider_link.rs`, `coordinator.rs`,
`coordinator/**`, `wire.rs`, `admission.rs` and fixture integration. Agent 1 owns
`crates/gemini/**` and its audio scope. Agent 2 owns evaluation, camera and
perception. Shared edits remain separate proposals; only Main integrates and
schedules devices.

- **MAIN-007/008/017/019:** integrated priority recovery, checked-session Start,
  exact pre-Started failures and endpoint draining. Regression coverage includes
  actual paired worker/Supervisor/consumer with private Unix datagrams. No claim
  of actual hardware outage UX follows from those checks.
- **MAIN-013/016/020/021:** failure schema is published in
  [failure-cue-contract.md](failure-cue-contract.md). Preserve failed answers,
  partial audio, actual stage/cause and unowned discarded endpoints. Only initial
  `listening_ready` exists; no authoritative post-outage re-ready cue exists yet.
- **MAIN-023/025/026/029/030:** all six reproduced evaluator dispatch defects
  are corrected in integrated `2a1522eba`. The final Player boundary rechecks the
  original 100 ms deadline, latches observed post-exit trace as unavailable and
  requests revalidation when its 1,024-line drain budget is exhausted. Provider
  failures retain stage/session evidence and do not become successful answers.
  The red review branches `lamp-voice-eval-review-20261011` (`60f98cb9a`) and
  `lamp-voice-eval-review2-20261011` (`cdc0ec26c`) remain evidence only. Do not
  merge them as runtime releases.
- **MAIN-021/027/029:** exact valid/invalid receipt examples are in
  [recording-import-examples.json](recording-import-examples.json). Agent 2 may
  implement the offline importer and boundary proposals. Proposals remain
  separate from accepted acoustic annotations; `listened` remains false until
  actually confirmed. Private recordings stay outside Git.
- **MAIN-022/024/028/031 / A1-009:** the 200 ms reply-start gate is a private
  experiment, not an integrated release policy. Root fixed its opening trace so
  an undispatchable held final sample cannot open the gate. Host audio/live gates
  passed (484 tests, two ignored), as did ARM release/strict Clippy and 11 release
  QEMU lifecycle tests. The stored-arrival replay results are counterfactuals
  with censored replies, not complete physical successes. First native candidate
  trials have no recorded gaps but repeated answer cancellations; do not select
  this policy on gap counts alone.
- **MAIN-034/036:** integrated the corrected receipt importer and unchanged-in-
  substance independent red/green regressions. Unbounded frame arithmetic and
  cached WAV claim substitution now reject without scoring; missing or revoked
  evidence remains unscored.
- **MAIN-038:** optional boundary proposals remain excluded. An independently
  reproduced zero-rate WAV causes a resampling allocation panic in that optional
  path; Agent 2 owns bounded rate/size validation and production regressions.
  This does not block the separately reviewed receipt-only importer.
- **MAIN-032:** Agent 1 is correcting the service probe to distinguish interrupted
  events from the idle completion that actually settles Require. It must count
  extra payload separately from late terminal markers and require a completed
  second answer before claiming post-answer behavior.
- **MAIN-033:** Agent 1 owns local DSP/reference analysis of the new candidate
  interruption windows. Root owns admission/coordinator changes and scheduling.
  Recorded room clips remain local; no cloud replay grant is made.
- **A1-010:** reject the proposed barrier-free continuation after a committed
  End. Its documented late answer can concern the truncated question instead
  of the complete question. A common owner does not make that answer correct,
  and no output yet does not prove unchanged intent. Keep Require and bounded
  recovery; a still-local, unsubmitted endpoint is a separate possible design.
- **A1-003/011:** echo-evidence primitives are integrated but do not control
  admission. Agent 1 reports a corrected diagnostic matcher on its branch; low
  residual explained energy does not prove a person is speaking. The correction
  is not integrated or physically qualified for admission.
- **A1-008:** environmental I2C work is deferred behind voice qualification.
- Camera/perception/image privacy proposals remain inactive and do not gate the
  current audio-only run. Image limits are provisional; there is no active
  provider camera consumer. Prerecorded audible failure notices are not wired.

## Device reservations and physical evidence

| Device | Address | Owner | State |
|---|---|---|---|
| lamp-a0ae | 172.168.20.197 | Main | Primary finite native qualification; no motion |
| lamp-52e6 | 172.168.20.190 | Main | Cached stimulus and independent recordings; preserve existing STT drop-in |
| lamp-4ace | last known 172.168.20.159 | Main | Not rediscovered in the last authorized scan |

The first spare-Lamp physical runs used old source `51bd815e9`, binary SHA
`d3318abe080336dbd4c50d9d04e4517eb21cd0b7cec5afaa5d5f62e990b525e7`.
One greeting was misheard as Italian and answered with a 30 ms opening gap.
A repeat prematurely split one question, then failed at the 2 s Require barrier
without reply audio. Exact local replay isolates a 680 ms low-VAD interval
that contained only about 360 ms of actual quiet; disabling software noise
suppression kept that recorded question intact with AEC unchanged.

A subsequent fresh NS-off cohort played two prompts: both stayed one input,
one got a relevant English greeting with no gap, the other was misheard and had
a 260 ms opening gap. A third prompt was withheld when its readiness expired
9.917 ms before the final check. It was never played, not a dropped answer.
Both complete room recordings and runtime diagnostics were independently checked.
Observer DSP/clipping prevents accepted acoustic onset bounds; no p50/p95 or
V1-main/V2 speedup is claimed. NS off remains an experiment, not a release winner.

The new coordinated runtime (`02148ed4d`) completed a fresh 40-second greeting:
one question, a complete relevant English answer, and 80 ms + 140 ms accepted
playback gaps. Endpoint to first provider PCM was 1.024276 s and endpoint to
first ALSA write 1.025392 s. The endpoint includes 600 ms silence; these are not
acoustic speech-end-to-word timings.

A private 200 ms start-gate candidate then ran three planned attempts. Two
prompts played and produced six admitted turns: four answer prefixes were
cancelled, followed by two software-complete replies. All six report no gap,
but both conversations failed; cancelled prefixes are censored observations.
A third prompt was withheld for failed original-readiness confirmation and was
never played. Root retained every attempt and verified rollback and original
volumes. The independent recording/DSP audit matches all four cancellation windows
to preceding accepted reply audio, with continuous reference/DSP epochs.
This supports self-echo involvement, without certifying echo-only room audio.
No acoustic improvement is accepted. Evidence directory:
`lamp-rust-playout-evidence-20261011T033405Z`.

The same-Lamp playback/capture check recovered the greeting on both microphones
using installed V1 controls. It proves the local audio route, not a Rust or
natural-duplex conversation. A clean synthetic provider-only control recognized
the exact greeting and returned first audio 1.037035 s after input enqueue;
it bypassed microphones, endpointing and speakers. Planned recorded-room uploads
were rejected by automatic review and did not run; keep those clips local.
See [physical evidence and limits](../docs/real-device-audio-pilot.md).

After each completed run, both Lamps returned to their original volume/mode and
software-muted legacy services. Last verified output volumes: a0ae 77, 52e6 94.
V2 is not the continuously running default. Main owns the temporary key-only
a0ae maintenance service, restricted to the authorized Mac source with a finite
lifetime, and its cleanup. Keys, credentials and recordings stay private.

The installed HAL shutdown path can park/release servos. Main must retain the
serial-inaccessible stationary guard and verify fresh unit state, hardware
ownership and independent rollback before handover. Never reuse a completed
snapshot/rollback window or perform ordinary HAL stop/restart. The motor
candidate remains physically unqualified; there is no motion grant here.

## Integration ledger

- Earlier Agent 1 `d6b813695` and Agent 2 `3eaa0365`: included before `2535fb565`.
- Agent 1 `64806f862`: integrated as `a253abe5e`.
- Agent 1 checkpoint 7 `ae4f05db4` / tested `a77bdb0fd`: integrated with Main's
  coordinated consumer as `02148ed4d`; optional image/notice paths inactive.
- Agent 1 later speaker seam / start-gate / barrier regression checkpoints
  `6d7c39303` through `95cd4b263`: published, not integrated.
- Agent 2 camera/perception checkpoints: reviewed with corrections, not integrated.
- Agent 2 evaluator subset through `9ed94e42` / status `7edd8d94`: integrated
  as `2a1522eba`; importer changes remain under separate review.
