# Main agent status

- Protocol: `tmux-and-git-v3`
- Updated: 2026-10-11
- Branch: `lamp-v2-chat`
- Integrated source checkpoint: `8feaf64b9` adds continuous serve, bounded metadata and receipt ownership, physical-mute lifecycle, camera teardown/diagnostics and optional-ring containment over the opt-in playout policy at `3bf5219b2`. The qualified ARM binary `7acfabcd...` completed two additional eight-turn runs on 4ace; their playback comparison is recorded separately from the earlier `6e6c58b42` / `0aa2387a...` evidence.
- Integrated evaluator checkpoint: `4127dd2bed2bf9b478fe86d721b3576ece45a5bf`
- State: four eight-turn Rust runs on lamp-4ace completed all 32 prompts under conservative V1-baseline turn taking. The latest two runs have zero detected speech-starvation gaps, with no acoustic acceptance or natural-interruption claim. A continuous-session harness startup race stopped the next trial before any prompt; Rust stopped cleanly and rollback verified. Camera startup correction and motor-worker lifecycle fixes remain under qualification. Main coordinates the complete duplex and endpoint workstreams through tmux.

## Latest native qualification

The composed binary completed both new eight-turn runs. All sixteen replies
have verified ownership/diagnostic closure and local room-ASR corroboration;
neither run has an accepted-zero speech gap. Endpoint-to-first-ALSA medians are
1.475 s for 200 ms coverage and 1.647 s for continuity, after the unchanged
1.2-second endpoint wait. Different generated replies prevent a causal policy
comparison; continuity remains optional. Four recorded cohorts now contain 32
completed turns, with no acoustic acceptance or natural-interruption claim.
See [the separate comparisons](../docs/connected-conversation-20261011.md).

The first continuous `serve` qualification attempt
`/private/tmp/lamp-serve-native-20261011-01` sent no prompts. Rust reached real
listening readiness and acknowledged clean Stop with runtime exit 0, but the
private harness treated not-yet-published asynchronous metadata as a runtime
failure. It stopped at 2.189 s. All sixteen planned prompts are retained as
withheld; stationary muted rollback verified. Correct the harness publication
race without renewing the original readiness cue or claiming metadata before
observing it, then retry in a fresh namespace. This is not a completed
long-session test or evidence that Rust crashed.

Native camera inspection now identifies the startup failure exactly: the first
reported monotonic exposure is 400,929 us old against the unchanged 250,000 us
limit. Zero frames were accepted. A private correction discards otherwise-valid
old startup buffers one per poll within the original two-second deadline;
128 affected host tests pass, with ten additional root-review test executions.
ARM and native qualification remain pending. No age limit was relaxed.

The independent motor worker candidate remains inactive. Review reproduced
failed priority-send handling, out-of-order fault receipts and leaked child IPC
paths after a crash. Its author is correcting the complete lifecycle with those
production-path regressions. No physical motor motion has been performed with this candidate.

## Current private integration and latency target

Owner's latest question clarified the target as natural connected conversation.
[Conversation latency](../docs/conversation-latency.md) records the human
reference, 300–500 ms typical / 1 s p95 engineering targets, and the present
roughly 3 s unaccepted acoustic estimates. The earlier 2 s target is interim.

MAIN-048 accepts Agent 1's continuity patch for a physical experiment after
root's 565 passing affected tests. MAIN-049 keeps Agent 2's current endpoint
policy experimental, requests an independent inference process and full-turn
human replay. Neither candidate has a measured hardware speedup.

The combined source is integrated at `8feaf64b9`: camera teardown/timestamp
diagnostics, opt-in continuity, continuous service/mute-reopen, settled terminal
provider exit, and optional-ring containment. Source-only ARM gates pass: strict
all-target audio/camera/live Clippy, 138 audio tests (one existing ignored),
75 camera tests, and 515 live tests (two existing ignored), followed by release
build and AArch64 loader verification. Host component gates also passed.

The first ARM live run failed two test assertions: fixture children briefly
retained their inherited peer socket descriptors under QEMU. A descriptor probe
reproduced this; a bounded test-only exec marker removes the race. Production
transport and the strict first-send failure assertions are unchanged. Original
failures remain recorded. This does not establish optical or acoustic behavior.

Frozen compiled source manifest SHA256:
`6150f1a1500de04b24c3b0437258b1b77d9386c87fc7b210bf7850a3863ba481`.
Native binary SHA256:
`7acfabcd6f38e4cf1d9a8f145c8b3c1ab479118a8bc1edc97897d0e767d9871c`.
Root gate/extraction receipts live under
`/private/tmp/lamp-camera-diagnostic-composed-1_21tfw9/arm/`.
All compiled files match the canonical tree; newer coordination/product docs
were preserved. Staging verified native usage for serve and continuity; it did
not activate either policy. The connected script has already run twice on the
old binary, with all 16 attempts retained. Eight exact stimuli are additionally
preserved in ignored `.cache/acoustic/connected-desk-20261011/`. No V1 benchmark
is active.

Native camera correction `a4cac2dd...` safely revoked and exited without a forced
kill; driver teardown took 50.227 ms in this attempt. Inspection still failed
before any accepted frame with `InvalidDriverTimestamp`. This is not a camera
success. The later diagnostic inspection identifies the original timestamp and too-old
predicate, as recorded above. Stationary, muted legacy restoration was verified
after both attempts.

## Camera integration checkpoint

The opt-in camera context is integrated locally and has passed combined host
formatting, strict Clippy and 1,166 tests (zero failed, four existing ignored).
The source-only combined ARM build passed strict affected Clippy and 213 selected
serial QEMU tests. Two concurrent visual timeouts remain recorded. The implementation and explicit
limits are in [camera context](../docs/gemini-camera-context.md). Device USB
identity/mode provisioning is complete. The first native inspection selected the
correct mode, then stalled before its first frame; camera cleanup required SIGKILL.
Stationary muted restoration passed. A second, syscall-traced inspection captured
31 frames with no sequence gaps, then hung in V4L2 STREAMOFF until the 20 ms
cleanup path killed the worker. This narrows one defect to teardown; it does not
explain the first run's lack of frames or qualify sight. A private correction
separates prompt authority revocation from bounded driver cleanup and keeps
process waiting outside the conversation loop. Motor control remains inactive.

## Current owner priority

The owner deferred V1 benchmarking and requested V2 implementation and real-device
improvement as soon as possible. No further V1 cohort work is active. The attempted
V1 setup sent no prompts; it failed its capture-readiness check immediately after
unmute. Isolated V1 stopped, the exact rollback/finalization passed, and the owner
then canceled the Mac-only recording. Retain that failed setup; do not count it
as a comparison or silently rerun it. MAIN-045 records the priority change.

## Architecture scope clarification

The owner reaffirmed a small core with independent component processes and
failure containment. Long-running Harness tasks and shared memory are future
placeholders only; first deliver natural live interaction. See the updated
[architecture](../docs/architecture.md) and [team ownership proposal](../docs/team-ownership.md).
MAIN-047 directs Agent 2's endpoint inference into a separate worker/binary and
holds activation of its candidate, which currently increases false splits in
its synthetic cohort. Those main-runtime changes are now integrated at `8feaf64b9`; continuous-service
and physical camera qualification are still incomplete, as recorded above.

## Current agent assignments and communication

The owner requested full autonomous workstreams, then explicitly required all
communication through tmux. MAIN-050 and MAIN-051 are published at `56326af41`:

- Agent 1 owns continuous hearing, echo-resistant admission, natural interruption
  and sustained USB clock/reference behavior on `lamp-a1-duplex`.
- Agent 2 owns the isolated endpoint worker, full conversational policy, human
  pause/continuation evaluation, integration and performance on
  `lamp-a2-endpoint-runtime`.

Main delivered MAIN-050 to office pane `%216` and observed Agent 1 fetch the brief
and begin implementation/replay work. The tmux-only update was also sent there.
Main verified Agent 2 on M2 at pane `%37`, sent MAIN-051 through its existing
trusted remote tmux terminal input, and observed `turn_started`. No claim that its
new implementation is complete follows from that delivery. Both briefs include
code, integration, stress tests and documentation; neither agent should wait for
another small assignment after a single component or audit.

[Coordination](README.md) names the verified routes and guards. Routine messages
use tmux; Git retains code and durable evidence. Previous Harness team context
errors are historical and do not require changing settings or credentials. No
owner relay is needed. Main retains device/shared-VM scheduling and default
selection. All earlier physical evidence and incomplete work remain recorded.

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

That device comparison changed the endpoint window alone; the startup-buffer
candidate was absent. The next comparison now explicitly selects its 200 ms
horizon after the combined host and ARM gates passed. Graceful coordinator termination is also being prepared to
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
| lamp-4ace | 172.168.20.159 | Main | Rediscovered and real Rust audio trial completed; installed legacy restored stationary and software-muted between runs |

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
ignored. Linux ARM release and strict audio/live Clippy passed, plus 60 focused QEMU
tests (12 playout-start, 26 turn-taking, ten core, 11 StartGate and one arrival
schedule). Native CLI/loader staging passed. Physical qualification is pending.
Frozen source manifest: `c6d6edb5a94f69a4de597fa00c094e91d28bfc53d645aef27375b70ab57e36bf`;
ARM binary: `a30055ba007d1e425205a771cfd022520ac702dea405adf8c654cbc53a75cf3c`.
Only the three result/status documents changed after this source freeze.
No graceful-signal patch is included.
A small private recorder diagnostic change preserves the original failed
request's phase and time; it does not retry or weaken privacy checks.

Peer branches were fetched again on 2026-10-11: Agent 1 `12caf1b38`, Agent 2
`95029a03d`, with no new checkpoint since the previous integration review.
Harness direct delivery remains unavailable; Git is the coordination record.


## Completed buffered run and next model comparison

The first `a30055ba...` / 200 ms run, `20261011T073625Z`, completed eight
actual replies. The private runner rejected turn eight's legitimate second
playback occurrence under the same owner. Its forced cleanup left unsealed
recordings; this remains a failed attempt. The parser now accepts one or more
ordered, uniquely identified, fully retired playback occurrences before the
single turn completion. Actual retained cues reproduced the defect before the
fix and pass through both production caller paths afterward. Fresh ownership,
nonce, clock, privacy and rollback checks remain intact.

The rerun, `20261011T075239Z`, completed ten inputs, ten endpoints and ten
replies, each with one playback occurrence. No extra input, cancellation or
provider failure event appeared. Both target completion markers and coordinator
ACKs verify, as do both source WAVs (179.875 s each). The source and target
returned to their saved settings, including volumes 94 and 77. The runtime,
provider configuration, 1200 ms endpoint, noise-suppression setting, cached
English greeting and levels were unchanged from the first buffered attempt.

Three accepted-zero gaps total 440 ms: turn two 320 ms, turn five 50 ms and
turn seven 70 ms. Seven replies have no such gap. Provider output transcripts
for turns six and seven are Spanish; turn two does not answer the greeting.
Turn four's local-endpoint-to-first-ALSA interval is 10.755097 s, including
10.655587 s before provider first audio. The ten software intervals have median
1.3898665 s and nearest-rank p95 10.755097 s. These exclude the 1200 ms endpoint
wait and are not speech-end-to-first-audible-word measurements.

Compared descriptively with `20261011T070623Z`, gaps change from 15 / 1390 ms
to three / 440 ms; maximum gap changes from 310 to 320 ms. Reply duration and
content differ, so no causal buffer win or acoustic speedup is established.
The start-gate records match actual enqueue history and all ten select 200 ms.
Turn eight receives one sample before a 389.623 ms packet delay; its gate age
is not entirely added delay. Keep `added_latency_us` unmeasured.

Independent report: `/private/tmp/lamp-v1-ten200-complete-audit-3_bmsbbb/report.json`,
SHA256 `43b9bcee3a9e2728f5c8ce045ac4fa10c71b9a52e5ba03562079dfbf6ebfd62e`;
41-input manifest SHA256
`8582622bc359bd37026817d294130d0264a2af96b4c09dc2db66d0bb0d219858`.
All recordings stay local. No human listening or acoustic acceptance is claimed.

The subsequent regular-Live attempt, `20261011T080421Z`, played eight prompts
but admitted only seven; the eighth had high-VAD blocks without a new admission.
The old whole-process-group termination left no final coordinator trace. This
is a retained failed comparison, not a model winner. The later 4ace trial below
returns to Extended Thinking/LOW for the V1 pilot comparison.

Separately, `6e6c58b42` adds cooperative SIGINT/SIGTERM handling through existing
reply cancellation, privacy denial, worker shutdown and failed-trace writing.
Root fmt, strict live Clippy and all 415 serial live tests passed (zero failed,
two existing ignored). Standalone ARM release/strict Clippy and 73 focused QEMU
tests passed. It was absent from those spare-Lamp comparisons and is now
exercised in the later 4ace trial below. Signal the coordinator first; a whole-group signal cannot promise
worker acknowledgements. See the [termination contract](../docs/directed-termination.md).


## 4ace restored: real exchange and locked-screen access

On 2026-10-11, the iMac explicitly played the cached Daniel/190 question,
"Lamp, how are you doing today?" Lamp-4ace, running Rust `6e6c58b42`, answered,
"I'm doing great, thanks for asking. How are you doing?" The owner heard Lamp,
then heard the replay of the iMac microphone recording and confirmed both voices
were clear. Cached local ASR independently recovered the answer. No recorded
room audio was uploaded.

The first reply contains 74,880 accepted samples at 24 kHz (3.120 s), 312
contiguous chunks, zero accepted-zero gaps, a complete provider outcome and
matching final retirement. Capture and render completion certificates match
the actual coordinator acknowledgements. A second input near the finite test
end received only 210 ms before `deadline_cancelled`; keep it incomplete.
Local endpoint to first ALSA write was 1.346475 s. The proposed acoustic gap is
2.65–2.75 s, pending manual boundary confirmation: this is neither an accepted
latency measurement nor a p50/p95 result. Owner confirmation establishes
intelligibility, not timing annotation or full-duplex behavior.

Runtime SHA256:
`0aa2387a4c382ea25568f5099e11427f5a05b8caa93ea01beff23836aaccb822`.
The run uses `v1-baseline`, NS off, 1200 ms endpoint and 200 ms playback horizon,
with Extended Thinking/LOW and Kore. The finite session has ended; legacy is
restored with the permanent serial guard and both software mutes confirmed.
No motor movement or complete choreography was tested. Three legacy restorations
reopened the microphone at startup; strict cleanup reported each failure.
Explicit mute and fresh readback restored the required state. Those failures
remain recorded; a private bench-helper correction is not yet deployed.

Evidence stays local:

- Exchange: `/var/folders/qg/lhfdx6q166z5cj705q22mh480000gn/T/lamp-4ace-real-exchange-b8hpwyqg`.
  Room WAV SHA256 `e2a87aa932a8a3c1e82357f963d93d29cc2ddbbd4d953e63eb5cdc9011a510dd`.
- Independent audit: `/private/tmp/lamp-imac-native03-match-g1xwnbj7/report.md`.
  Report SHA256 `279651877d3501b27324e847ceb5eb433743282d0a92fdd963bfce9085da83a8`;
  input manifest SHA256 `01e009f50e868d3f1f3332eaf4fb6a6526b3ca2fc0010f1ad9c6a799bd3b9ed5`.
- Locked-screen check: `/var/folders/qg/lhfdx6q166z5cj705q22mh480000gn/T/lamp-imac-locked-check-r2pgum03`.
  Screen lock was confirmed before and after. Explicit iMac Speakers playback
  is valid; exact iMac Microphone recording retains all 720,000 frames (15 s at
  48 kHz) with zero clipping, drops, stream errors or discontinuities. The
  cached question matches that recording and local ASR recovers it. A fresh
  photo was visually inspected and shows Lamp with its red ring and full base.
- Locked audio audit: `/private/tmp/lamp-locked-audio-audit-9q7PSa/report.md`,
  SHA256 `422e3e07e0832c431bb3553ef33fc14df1da7919eff0bc87ba68a38acb16e9b0`.
  WAV SHA256 `abb2f65b6d760f4d6121c93ea4a04bdd832a44693d3a2fbb7df412b89ef762ad`.

No additional OS consent is pending for this tested setup. Retain the existing
approved app identities. A 24-hour system-sleep inhibitor began at
08:50:29 UTC; it permits display sleep and locking. Locking worked in this check;
logging out, shutting down or future permission changes are separate conditions.
The owner requests a single stored V1-main benchmark; inspect existing source
provenance before starting any additional baseline run.


## Six varied turns on locked-screen 4ace

`varied-run-42445a9297` used the unchanged deployed `6e6c58b42` /
`0aa2387a4c382ea25568f5099e11427f5a05b8caa93ea01beff23836aaccb822`
with V1-baseline turn taking, 1.2 s silence endpoint, NS off and 200 ms startup
horizon. The iMac played six previously cached Daniel/190 clips: greeting,
division, blue sky, coffee, ocean fact and a short robot story. Each follow-up
waited for actual whole-turn completion and the declared tail; no blind delay
or automatic repeat advanced the plan. This is synthetic loudspeaker input.

The trace has six admitted inputs, six endpoints and six completed replies,
with no cancellation or extra input. Replies 1–3 have no accepted-zero gaps;
reply 4 has 70 ms, reply 5 has 160 ms, and reply 6 has 140 + 70 ms. The story
has two playback occurrences: an introductory sentence followed by a separate
roughly 2.27 s retirement-to-next-write interval. That interval is not hidden
inside the four starvation-gap count. Substantive story onset must be scored
separately from the introductory sentence. All six provider output transcripts
are English and relevant; topicality is not factual or human-experience approval.

Local endpoint to first ALSA acceptance, seconds, in prompt order:
`1.656733, 1.504063, 1.846100, 1.634824, 3.792812, 1.538589`. These exclude
the 1.2 s endpoint wait and are not acoustic speech-end to first-word results.
The 165 s observer captured all 7,920,000 mono 48 kHz frames with zero reported
clipping, drops, discontinuities or stream errors. The runtime's requested
cooperative SIGTERM exit remains exit 1; both distinct diagnostic acknowledgments
and completion markers verify. Strict rollback restored stationary legacy
services, microphone and speaker muted. The run is retained as
`captured_unscored`, not a quality pass.

Evidence is local only:
`/var/folders/qg/lhfdx6q166z5cj705q22mh480000gn/T/lamp-4ace-varied-12io918n`.
Room WAV SHA256:
`03d56b84a2063c5207330ffb4cfee4ccffef48b7d8a026afab341e6349d70385`.
Native evidence archive SHA256:
`aef1b809420198f89d32f36be4ff51d56b564a4d52138bbc3dbad95a1d311e7d`.
The exact executed host runner was not hashed at launch; its author finalized
the source receipt during execution, so that later freeze must not be assigned
retroactively. Remote helper/ledger source hashes, native binary, copied prompts,
player receipts and sealed diagnostic bytes are independently available.
Mac volume provenance is also missing from that executed host receipt.

## Guarded motor source integration

`46a9d6275` integrates 12 reviewed files: the motor core, bounded position-only
codec/trajectory, exclusive Linux adapter, tests, two English docs and its single
internal `lamp-interaction` dependency. It starts idle and has no live actor,
motion CLI, default calibration or automatic torque change. Root independently
ran workspace formatting, strict all-target motor Clippy and all 88 host motor
tests successfully. The exact source also passed the author's 93 ARM/QEMU
tests, strict Clippy and release build; those are emulation, not hardware.
The first root command failed before compilation because Cargo was absent from
PATH; the absolute installed Cargo path resolved it.

Root gate receipts: `/private/tmp/lamp-motor-integration-_36e466d/receipts.json`.
Reviewed patch SHA256:
`981634de757a11a8e5429489871bfd2dc39e0c3ed86683bd2d62b7ea8fda21de`.
See [the motor contract](../docs/motor-runtime-contract.md) for the remaining
supervised actor, physical calibration, placement and stopping/holding work.
No motor or camera activation is included in this audio result.

## V1-main source provenance and saved baseline policy

The old ten-turn table was **modified desk5b versus further-modified desk6**,
not main versus modified V1. Preserve its original labels; neither is the Rust
comparison baseline. The separate pinned-main pilot is still n=1, with unaccepted
acoustic boundaries. Its complete 22-file, 2,810,659-byte evidence is now retained
in ignored local `.cache/benchmarks/v1-main-d5efe9d7-pilot-e363pbra/`; preservation
manifest SHA256:
`7ed812e5c9708e483269412e2b147aa3372731a9b006d16d89db69f020b0f016`.
The source audit is saved beside it in `historical-v1-source-provenance/`;
report SHA256 `245240e185364a97395c979bf597af11f02a3d6976100f22e63429c6f38eba21`.

The owner requests one stored V1-main cohort, then reuse. The matched six-question
cohort is being prepared, not yet run. Today the existing device package's
695 HAL entries and os-server digest were verified against its pinned manifest,
and an app-free systemd namespace preflight passed. Neither check starts V1
voice or counts as a benchmark. Main remains
`d5efe9d7b73cc529b34cd4abe97624682a82ca94`, os-server binary
`cfdcbef53a02c47b4ef0225e69dcad8476b0cd997a34c86698f13c9176dd0d05`.
Do not pool the old spare-microphone pilot with the new iMac-microphone cohort.


Independent six-turn review is complete at
`/private/tmp/lamp-4ace-varied-audit-20261011-01/report.json`, SHA256
`ae08180c0ddb4fe26962b54c06e1cf494fc996b2204b4b5974cb9420776c7396`.
All six source/player identities, both diagnostic streams and queued/accepted
frame coverage verify. Local offline ASR corroborates six complete English
replies. Automated acoustic proposals are roughly 3.0, 2.7, 3.3, 3.0, 5.1 and
2.8 s to the first spoken segment, but roughly 6.9 s to the substantive story.
These are waveform/ASR review proposals with unresolved word-boundary uncertainty,
`human_listened=false`, and no accepted p50/p95. The full physical evidence is
now additionally preserved byte-for-byte in ignored local
`.cache/benchmarks/v2-4ace-varied-20261011-12io918n/`.
