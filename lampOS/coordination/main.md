# Main agent status

- Protocol: `direct-and-git-v2`
- Updated: 2026-10-11
- Branch: `lamp-v2-chat`
- Integrated runtime checkpoint: `5a2f0076bbddd1f4d38c1de743c29531ba327ecd`
- Integrated evaluator checkpoint: `4127dd2bed2bf9b478fe86d721b3576ece45a5bf`
- State: the 1200 ms V1-style baseline completed ten consecutive physical Gemini replies with no extra admission, split question, cancellation or provider failure. Quality still fails: nine replies had accepted-zero gaps, and reply two was Spanish after the English stimulus. The independent source recorder timed out after all replies and target shutdown, so this run has no accepted room-audio latency. Natural interruption, varied conversation and full choreography remain unfinished.

## Direct coordination

The existing sessions are members of Harness team **Lamp Rust implementation**.
Main and Agent 1 run on office; Agent 2 is the M2 session **lampOS voice-eval
runner**. Main replies directly to team questions; Git retains source, interface
contracts, results and decisions. No owner relay is needed. Agent 1 previously
could not obtain its team context (`CHANNEL_SCOPE_MISSING`); do not circumvent
that guard. Its latest A1-009/010/011 question arrived through Harness and Main
answered there as MAIN-028. Queued delivery is not proof that an agent read it.

## Current V1 baseline checkpoint

`7dde135af` ports stock V1's conservative own-playback suppression into Rust as
`--turn-taking v1-baseline`. Default `vad-only` remains unchanged. Capture,
reference and AEC continue; prospective input cannot replace a reply while
confirmed speaker output or its declared, unmeasured 100 ms tail is active.
Original read/write ownership and bounded delayed receipts decide the mask.
A reproduced delayed-receipt regression also removes old masked pre-roll before
committing a genuinely fresh onset. See the [baseline contract](../docs/turn-taking-baseline.md).
This deliberately suppresses overlapping speech and is not a full-duplex fix.

Root verified a standalone source export: workspace fmt, strict all-target
Clippy and **1,065 tests passed, zero failed, four ignored**. The first sandboxed
test run failed at local sockets/process setup; the identical source passed
when local IPC was permitted. Linux ARM64 release build and strict all-target
audio/Gemini/live Clippy passed. Release QEMU checks passed five paired-worker,
26 turn-taking and seven prefix-filter test executions (filters overlap).
Combined source manifest SHA256:
`83d30b80a0318444ef632b41eb06b370884ecd3c212375bf6679a65dd497e79a`.
ARM binary SHA256:
`9a46f270e975f3ff43011682e1172d19efc34b15ee1ae05d65aa049bbe7d0b09`.
Native staged CLI/loader check passed; that opened no audio device.

The first fixed-reply attempt, `20261011T054535Z`, transferred audio ownership
on a0ae but aborted in source52's recorder setup before target runtime launch
or any stimulus. Applying observer controls plus repeated checks exceeded the
existing 600 ms cooperative guard. Both source capture controls were restored,
the target rollback verified, and original volumes returned to target 77/source 94.
It is a retained failed attempt, not self-echo or acoustic evidence. The recorder
was corrected by splitting startup work into five cooperative slices while
retaining the 600 ms per-slice, 350 ms mixer-command and original 3 s startup
bounds. Later runs used fresh ownership windows. Recordings remain local.

### Actual hardware results after that correction

- Fixed reply, `20261011T060157Z`: two admitted inputs, one complete 107,760-frame
  reply (the fixture intentionally does not answer its second input), no extra
  input, cancellation, fault or accepted-zero gap. Independent review matched
  every accepted speech sample to the fixed reply and verified recorder and
  diagnostic identities. This is a hardware lifecycle check, not Gemini.
- Gemini, `20261011T062138Z`: the same cached English greeting played twice from
  source52, with a0ae running the exact qualified Rust binary. Two inputs,
  endpoints and complete replies, zero extra admissions/cancellations/provider
  failures. Both diagnostic certificates and room recording identities verify;
  source and target settings were restored. **Lifecycle/integrity pass,
  conversational quality fail.** Reply two had 1,920 accepted zero frames
  (80 ms at 24 kHz), and its output transcript was Spanish. The session-scoped
  input transcription was Spanish too; that input hypothesis is not reliable
  per-turn correlation. Both stimulus receipts name the identical cached WAV.
- Local endpoint to first ALSA acceptance was 1,212,722 us and 1,146,240 us.
  These two software intervals exclude the existing 600 ms endpoint silence
  wait and are not acoustic speech-end to first-word measurements or a
  before/after benchmark. Reply one was English and contextually appropriate.

Private evidence: `/private/tmp/lamp-rust-v1-gemini-evidence-20261011T062138Z`.
Independent audit: `/private/tmp/lamp-v1-gemini-audit-vkh0xxhk/report.json`, SHA256
`6359c98756c9f996d3e239f10c6e660ee5bcc161bdb97ca7ca806b522704c63b`;
input manifest SHA256
`fdfbc180394c826f8a63082cb65475b3b726fb5f8c1bce82947cee7c74b208b5`.
Local-only forced-English ASR corroborated the first exchange, but hallucinated
on one microphone during the second reply. It is not accepted human listening
or acoustic timing. The provider output transcript establishes the language
failure independently. No camera, ring or motor path was exercised.

Main's sequencing mistake was attempting full-duplex admission before porting
V1's working playback protection, then expanding component hardening before
closing the physical conversation loop. Immediate priority is repeated real
conversation with this baseline, followed by one measured correction at a time
for playback gaps and language handling. The repeated-greeting ten-turn evidence is recorded below; it cannot stand in
for varied conversation, natural interruption or multi-speaker acceptance.
The original Harness team context currently returns CHANNEL_NOT_FOUND to Main;
Git status is the coordination fallback. Do not claim fresh peer acknowledgment.


## Repeated-turn defect and endpoint correction

Run `20261011T063758Z` used the same `9a46f270...` ARM binary, original provider
configuration, cached stimulus and levels. Five prompts were dispatched; four
replies completed; the fifth input ended prematurely and was replaced 132 ms
later while the source was still speaking. No accepted earlier-reply speech
overlapped that trigger. Prompts 6–10 were withheld. The first four replies had
five accepted-zero intervals totaling 330 ms. Restoration verified. SIGTERM
left unsealed diagnostic prefixes and no final coordinator trace; source
recordings were revoked during cleanup. No acoustic or quality pass is claimed.
Audit: `/private/tmp/lamp-v1-ten-audit-7bt_f44w/report.md`, JSON SHA256
`845717ee0718b7246565a599ff73a5af69c38ec117e28ec9c99cb922b77e154f`.

The focused Rust correction changes only the opt-in baseline endpoint from
600 to 1200 ms of consecutive low VAD scores. Default `vad-only`, entry/prefix,
DSP, provider and playback stay unchanged. VAD-only reset replay did not help;
it is excluded. The 1.2 s replay preserves all five prefixes and keeps prompt
five together in the retained input; the final endpoint is censored. First four
endpoints move ~600 ms later. This is V1's configured silence fallback only,
not its separate Silero/Smart Turn path. Tests cover mid-question pause, exact
frame retention, short utterances, bounded endpoint, reset persistence, quiet
onset limits and capture faults. Host fmt, strict all-target live Clippy and the
complete serial live suite passed. Standalone Linux ARM64 release build and
strict audio/Gemini/live Clippy passed, plus release QEMU checks for the ten
core regressions, five paired-worker cases and 26 turn-taking tests. Native
CLI/loader staging passed without audio. The first physical repeated-turn
result is recorded below; full qualification remains pending.
Source manifest: `f2218b918b52f43f1932cac9def145abfa2a6dc90c34bb4d7236e3f16ee65d66`.
ARM binary: `608c2491c71ca9de52e9154a09ce7e64d8fbb248c1e8ae6bac01d1a130717781`.
Only this status documentation changed after the source export.

Main's next device comparison changes this endpoint window alone. The separate
startup-buffer proposal remains private and inactive, so it cannot confound
the comparison. Graceful coordinator termination is also being prepared to
retain failed-run traces through the existing shutdown path; no fabricated
diagnostic certification is permitted. Natural full duplex, language consistency,
playback continuity and the broader embodied interaction goal remain unfinished.

## Prior integrated checkpoint

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
turn-taking from optional adaptive live interruption. The stock protection is
now an opt-in Rust candidate in `7dde135af`;
the default VAD-only path and optional adaptive interruption remain unchanged.
Agent 1 supplied the actual V1 adaptive-gate comparison and qualified its
warm-up/onset limits through MAIN-040/041. Main owns integration and physical
qualification; no successful physical interruption result is claimed.

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


## Ten replies completed; playback quality remains the next defect

Run `20261011T070623Z` used endpoint checkpoint `5a2f0076b` (ARM binary
`608c2491...` above), the unchanged provider configuration `3c52a80e...`, noise
suppression off, the same cached English greeting and source/target test levels.
All ten prompts produced exactly ten local endpoints and ten completed replies;
there were no extra admissions, cancellations or provider failures. The runtime
exited 0 with its completed-unscored cue. Both target diagnostic streams have
matching durable completion markers, hashes and coordinator acknowledgments.
This improves the specific earlier run that split question five; one cohort
cannot establish general reliability or natural conversation.

Nine replies had 15 accepted-zero intervals totaling 1390 ms, maximum 310 ms.
These are speaker sample counts, not acoustic measurements. Reply two was
Spanish and invented a name; all source requests still identify the same
English cached WAV. Software local-endpoint to first accepted-write values were
945.445, 1276.742, 1044.011, 987.133, 1066.709, 1144.752, 1093.172, 1121.528,
1411.532 and 1018.231 ms. They exclude the 1200 ms endpoint wait and cannot be
reported as speech-end-to-first-word latency.

The source observer failed 62.519 s after the tenth reply and 5.083 s after
normal target shutdown, during the planned quiet tail. Its local `/voice/status`
request timed out; the host then correctly rejected stale permission evidence.
The capture window still had approximately 11 s remaining. Original source PCM
was discarded, so no room recording from this attempt can be scored. Both
source and target restoration verified, including target volume 77 and source
volume 94. The source made 4544 privacy-status requests over 169.184 s; that is
a possible load contributor, not an established cause of the HAL timeout.

Evidence: `/private/tmp/lamp-rust-v1-ten-evidence-20261011T070623Z`.
Independent audit JSON SHA256:
`72fc97defb53286851b1d3ec951af0e5d79471c81db3173848013071a094ef4f`;
input manifest SHA256:
`a480842cd2dca5844929fcd0b6e5734372676150eead785041e7afa291cba86c`.
The observer fault report is
`/private/tmp/lamp-observer-timeout-review-dcbj1q7k/report.md`.
No cloud replay or human listening is claimed.

Main is qualifying an optional 0–200 ms reply-start horizon that leaves the
capture/control loop running while initial audio accumulates. Default zero is
unchanged; only an explicit 200 ms candidate will be compared next, keeping
provider, endpoint, DSP and levels fixed. Host formatting, strict all-target
Clippy for audio/live, and 508 serial tests passed, zero failed, two existing
ignored. Linux ARM/device qualification is still pending for that candidate.
A small private recorder diagnostic change preserves the original failed
request's phase and time; it does not retry or weaken privacy checks.

Peer branches were fetched again on 2026-10-11: Agent 1 `12caf1b38`, Agent 2
`95029a03d`, with no new checkpoint since the previous integration review.
Harness direct delivery remains unavailable; Git is the coordination record.
