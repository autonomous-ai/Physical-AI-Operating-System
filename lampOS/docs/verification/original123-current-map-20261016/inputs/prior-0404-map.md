# Original 123: merged, local and remaining clock work

Read-only snapshot from `date`: **2026-10-16 04:04:14 +0700**. Source is pinned to **`48b3737d5e83efa00278172cb602d3525d368a43`**. No builds, tests, reruns, Git mutations, network, device calls or repository edits were performed.

The denominator stays **123 identities in 11 binaries**, exactly the immutable inventory at `4596d2cac9e25e31bc2b3509ad565ffde66a5ab1:lampOS/docs/verification/realtime-inventory-20261015/inventory.json`, based on source `8ddffb629`. Its set also exactly matches the retained `verified-measurements.json`. Every original identity still has one source definition at the pinned base. That is a static check, not a new compiled list. New helper cases, later whole-module additions, and new flaky Body/motor/second-route tests do not enlarge this count.

The [machine-readable identity map](tests-823-original123-remaining-map-20261016.json) retains every exact name, current source line/hash/blob, original wait description, stage, remaining host waits, evidence paths/hashes, native observations and paired case durations. It records **47 merged clock routes, 12 merged partial wait reductions, 25 qualified local migrations, one locally measured but broader-gate-blocked migration, 34 pending host migrations, three original pure checks and one original event-driven child case**. These categories sum to 123; they are not a count of tests entirely free of host time. In particular, all 13 final-dispatch cases retain real staged/observation windows.

| Binary | Original IDs | Present at pinned base / next slice | Baseline ARM64 max sum (s) | #889 observed case sum (s) |
|---|---:|---|---:|---:|
| `lamp-voice` |63|37 merged ScenarioClock;2 pure;capacity1 local pair but blocked gates;23 pending|241.717|137.499|
| `lamp-voice-eval::physical_loopback` |12|10 clocked helper routes merged #889;1 pure restriction check;1 unchanged real partial-trace/EOF case|235.864|6.797|
| `lamp-voice-eval::final_dispatch` |13|12 owner-EOF cleanup reductions merged #843;1 intentionally closing helper unchanged;all 13 retain host observation/staging|29.187|5.249|
| `lamp-voice-eval::serve_session` |14|Base host;10 qualified local #909,3 qualified localRecorder,1 qualified localSlowTransport|369.907|369.857|
| `lamp-voice-eval::serve_relay` |6|Base host;6 qualified local original-case migrations|31.137|31.121|
| `lamp-memory::health` |7|Base host;5 qualified local reporting cases;2 intentionally host negative/failure cases|36.070|36.043|
| `lamp-voice::capture_hold_worker` |4|Linux real DSP/capture worker remains host;no qualified migration|13.932|13.938|
| `lamp-voice-eval` |1|Original5 s actual transport-exit expiry remains host|5.019|5.023|
| `lamp-health::program` |1|Real5 s heartbeat gap/3 s Bus liveness lease remains host|5.013|5.009|
| `lamp-records::records` |1|Real1 s reporter/file-reload cadence remains host|5.008|5.007|
| `lamp-actions::server` |1|Real2 s catalog cadence/owner expiry remains host|4.081|4.112|
| **Total** |**123**|Current implementation and qualification are separate columns in JSON|**976.935**|**619.655**|

Baseline is the maximum per identity in retained runs 38715042828/38715014248. #889 is full ARM64 run 38780759392 at `8a7521661`: all 123 present,121 PASS plus two historical endpointLEAK outcomes. These are sums of printed case times across four concurrent serial shards, **not a measured standalone serial group duration**. They are not a matched before/after benchmark and cannot be combined with local pairs as projected savings. The later #872 full run 38787351254 covered 123/123 with zero FAIL/LEAK. Its actual head `22d369ad4` has parent/common ancestor `587a361a3`; merged finite commit `2bf2eaf76` is not its ancestor. Its `physical_loopback.rs` lacks `with_fixture_driver`, and `physical/mod.rs` lacks the fixture-driver module. The finite commit and pinned 48base contain both. Thus the endpoint run does not include the finite migration, established from ancestry and source rather than execution date. Its cleanup fix is now merged and changes no endpoint clock route. Exact blobs and SHA-256 values are in JSON.

## Qualified local work awaiting integration or complete native proof

| Slice | Original identities moved by this slice | Retained serial pair | Retained package qualification |
|---|---:|---|---|
| Serve #909 |10 of 14,18 of 22 attempts|Whole14:396.970→111.763 s;controlled 10 case sum334.354→49.402 s|Fresh 48base Serve 422 PASS/0 FAIL/0 ignore; full native old head incomplete|
| Recorder |3 more;13 of 14 controlled after slice|Whole14:117.106→90.202 s;new 3 case sum38.914→7.689 s|Historical457 PASS; fresh48base+Serve+Recorder463 PASS/0 FAIL/0 ignore at03:37:40|
| SlowTransport |Last Serve identity;14 of 14 controlled|Whole14:86.271→73.874 s;new case22.713→3.608 s|Historical498 PASS/0 FAIL/0 ignore; precise current Recorder port plan exists, source not yet ported or gated|
| Serve relay |All 6|31.474→2.204 s|Historical combined467 PASS/0 FAIL/0 ignore; current base clean dependency integration/native pending|
| Memory health |5 of 7 cases;5 of 9 children|Whole7:39.498→14.547 s;controlled 5 case sum33.437→5.590 s|Fresh clean Bus+Memory296 PASS/0 FAIL/6 existing ignores at03:38:09; depends on Reporter #910; native pending|
| Capacity #868 |1|Nextest invocation24.054→0.532 s;case24.049→0.516 s|Pair passes; retained later full local gate has unchanged stall failure #866, native gateSchedule #869; no fresh current base qualification|

Except capacity/release, these local pairs are debug x86_64 macOS and share a loaded host. Their original before/after helper identity and assertion proofs are retained separately. Some independent jobs overlapped only one side. Full gates are package qualifications, not new timing pairs. Native and broader current base qualification cannot transfer merely because owned source hashes match.

Fresh Serve qualification supersedes the historical 416-test row: on base `48b3737d5`, raw libtest summaries total **422 PASS / zero FAIL / zero ignored**, with all 14 original compiled Serve identities intact; fmt, strict Clippy and workspace build exit 0. Build completion is **2026-10-16 03:44:54 +0700** from retained `date` records. Published head is `e057db620acb2bd61bac1ae7f157d87cdf635a18`; root reports normal ARM64 run 38806386829 running at this update, with **no new full run**. No result is inferred from the running handle.

Latest retained #909 full run 38801026642 at `188d97b7f` records **93 original PASS and 30 original identities not run** after shard4's Body failure. Serve has13/14 PASS; `delayed_readiness_is_awaited_by_event_not_by_completion` was not reached. Nine of the 10 new controlled Serve identities ran. No original failure was observed, but this is not complete native qualification. The failed `lamp-body::runtime::supervisor::tests::inert_child_stop_timing_samples_are_host_ipc_only` remains outside the 123. The adjacent native audit/metadata in JSON preserves the exact omissions.

## What still costs real time

The 37 merged Voice scenarios comprise 31 single-cycle pilot cases and six in-process endpoint classifier cases. The endpoint six still use real sentinel children and the original shared 300 ms cleanup grace. The metadata-stall pilot retains its real 500 ms writer finish bound. Startup, transport, signal delivery, actual exit/reap and watchdogs remain real across all clocked fixtures. Claiming all37 are wall-clock-free would be wrong.

The 34 pending identities, plus the 12 partially improved dispatch cases, have these retained #889 observed contributions. These rows identify work, not forecast savings:

| Remaining family | IDs | Retained case sum(s) | Required boundary |
|---|---:|---:|---|
| Early-end coordinator/provider scenarios |10|60.205|4/5/6 s sessions,10 ms capture/privacy,40 ms answer packets; separate Tokio and OS peers; actual300 ms cleanup|
| Capacity, implemented but gate-blocked |1|24.029|20 s audio,5 s buffer/full4 s hold; paired migration already exists|
| Real endpoint process/mute scenarios |7|21.659|ProcessClassifier startup/hang/backoff, fresh authority and actual exit;#872 cleanup alone does not accelerate them|
| Other selected provider-worker cases |4|18.263|Two real Bus name-age tests, kept-start2500 ms+strict2000 ms barriers, and two2000 ms silent-service branches|
| Linux capture-hold worker |4|13.938|Real DSP,10 ms blocks,2 ms worker cadence,40 ms freshness, post-Stop no-frame proof; do not shorten samples|
| Pilot cross-cycle mute/reopen |1|8.586|Four host cycles and service-privacy stable-unmute/authority/exit boundaries|
| Final dispatch residual windows |13|5.249|Original300 ms staged lines,50 ms observation drains,100 ms lateness; owner EOF only removed the forced cleanup|
| Transport exit-bound unit test |1|5.023|A genuinely live child must survive the entire5 s logical bound, then actual kill/reap;default wait_child still host|
| Health program |1|5.009|5 s publication gap exceeds3 s real file-mtime liveness lease;cadence-only injection is insufficient|
| Records reporter |1|5.007|Six real switch/policy/receipt transitions through five 1 s periods|
| Memory retained host cases |2|5.004|Both2200 ms default-off windows and both 250 ms post-reap drains stay real|
| Actions catalog |1|4.112|2 s catalog refresh,600 ms owner expiry, actual Bus/socket confirmation|
| Direct name-note early-end case |1|1.116|Two paths with150/100/150/150 ms Tokio negative drains;no child/LiveRig/LinkClock|

All 13 dispatch cases appear here because the 12 merged cleanup reductions do not remove their observation/staging clocks. This corrects any coarse count that calls the unchanged closing-transport case already free of duration waits. The partial-trace physical-loopback child case is the separate event-driven identity.

## Next reviewable work

1. **Finish integrating already-qualified migrations first.** Serve+Recorder+Slow cover the largest retained family (369.857 s in #889), followed by relay31.121 s and Memory36.043 s. Their implementation/pairs exist, so publication/current base/native gates are the next steps; this is not an estimate that those seconds disappear.
2. **The largest new provider family is the ten early-end scenarios (60.205 s).** The narrow first vertical case remains `the_link_dropping_in_between_fails_the_request_unheard_and_the_next_is_answered` (6.385 s in #889), preserving its full 6 s conversation,100 ms first-link close,150 ms second-answer delay and actual channels. Preparations are not a migration: Moment's 20 clock tests pass but its full affected gate remains1,741 PASS/4 FAIL/8 existing ignores; Gemini's explicit capture source preserves old factories and has 12 new boundary cases, while root owns its ongoing gates. Still required are worker/Actor/service completed-work receipts, coherent std/Tokio/MonoTime and capture age, a no-auto-advance owner, and a reviewed frozen-time ShutdownDrain path. Keep `require_real_fixture_clock` rejecting unsupported Scope use. No one-field clock switch is safe.
3. **The smallest independent scenario change is the direct name-note negative-window test**, `early_end.rs:1546`, currently1.116 s in #889 and1.114 s in the partial #909 run. Review paused current-thread Tokio only for that test, preserving both continued/discarded branches, all eight original quiet drains,3 s guards, actual in-memory service messages and every assertion. Its existing host-Scope guard must remain. This is lower serial impact than the provider family; do not imply a measured post-change time. It can be prepared without the multi-thread provider coordinator.
4. **A separate small five-second candidate is the transport-exit-bound test.** A pure clock/wait seam around `wait_child` can preserve its default samples,20 ms waits and full5 s bound, with an explicit actual-child startup barrier and actual kill/wait. This is more ownership-sensitive than the name-note case because mac.rs is shared by the active Recorder integration; use a separate reviewed slice after that source is stable. Never substitute a synthetic exit or shorten the timeout. The Health/Records/Actions alternatives need broader child/lease coordination.

No original123 migration is credited for merged clock-only APIs (#852,#861,#867,#875,#877,#888,#894,#897,#901), or for local Reporter/Moment/Gemini preparations alone. The current nextest whole-module/profile selection has grown beyond 123 and remains unchanged by this audit. The under-two-minute complete serial target is **still unproved**.

Input hashes, pinned-base ancestry proofs, all 123 per-identity source locations, per-run statuses and raw-pair line numbers are retained in the adjacent JSON. This report neither clears retained failures nor predicts a combined timing by adding incompatible measurements.
