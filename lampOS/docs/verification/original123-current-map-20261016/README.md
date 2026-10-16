# Original 123: current source and remaining clock work

Frozen read-only audit completed at **2026-10-16 08:47:26 +0700**, from shell `date`. Clean source is **`69bae3831449bc03d1f11e3ed0c55eccfa1e6f3d`**. Every current source blob came from `git show HEAD:<path>` in the transport worktree, then all 28 current snapshots were checked against the explicit immutable commit. Its dirty candidate files were never used as the current base.

The denominator remains **123 exact identities in 11 binaries**, from the immutable [original inventory](https://github.com/autonomous-ai/lampOS/blob/4596d2cac9e25e31bc2b3509ad565ffde66a5ab1/lampOS/docs/verification/realtime-inventory-20261015/inventory.json), SHA-256 `72d671f8b22246898f9142c0216c9fbced55e350e46b83ffec787c81496fb1fc`. Every identity has exactly one module-qualified definition in the original 8ddff source, clean 69bae, and native Relay 917a snapshots. This is a static source proof, not a newly compiled current-base inventory. New helper tests never enter the denominator.

[current-map.json](current-map.json) records all 123 definitions, source lines, full-file/Git-blob/function hashes, assertion-macro hashes, original waits, current route, prepared qualification, remaining host boundaries and exact native rows. [source-definitions-audit.json](source-definitions-audit.json) and [namespace-audit.json](namespace-audit.json) retain the underlying source proof; snapshots live under `source/<head>/`.

| Disjoint source stage at the frozen base | Original identities |
| --- | ---: |
| Merged clock route | 67 |
| Merged partial cleanup reduction | 12 |
| Relay: native qualified, unmerged | 6 |
| Transport expiry: locally qualified, unpublished | 1 |
| Capacity/silent-service: prepared, broader gate failed | 2 |
| Remaining host migration | 31 |
| Original pure checks | 3 |
| Original event-driven child case | 1 |
| **Total** | **123** |

The 67 merged routes comprise 37 Voice scenarios, 10 finite physical-loopback cases, all 14 Serve-session cases, five Memory reporting cases and the Records reporter. A merged route still performs real I/O, participant startup, signal delivery, actual exit/reap and watchdog checks. The 12 dispatch reductions only removed forced cleanup waiting; their original observation/staging windows remain.

The two relevant heads are distinct. Ready [Relay #942](https://github.com/autonomous-ai/lampOS/pull/942) is `917a19c5c36bdecb965f404385df386c16b22af4`; its six migrations are absent from 69bae. Records' 69bae migration is absent from the Relay full-run head. Their common ancestor is `e5aa43747ec1540893b4da4509d14288e7e70806`. The ancestry checks and actual source blobs, not execution date, establish this distinction.

## One retained native observation

The times below all come from **one full workflow run, 38831310695 at 917a**, whose completed composite has 5,321 unique passes, all fixed 123 exactly once and no FAIL/LEAK. Shards 1, 2, 4 retain attempt 1 executions; only previously unexecuted infrastructure-failed shard 3 ran in attempt 2. [Complete native evidence is pinned at 1b12910c](https://github.com/autonomous-ai/lampOS/tree/1b12910c60eec95941b94e658e7bb3c695224721/lampOS/docs/verification/relay-native-composite-20261016). Its full raw composite log and audited memberships are also copied here.

| Binary | Original IDs | Clean 69bae source / prepared status | Relay 917a printed case sum(s) |
| --- | ---: | --- | ---: |
| `lamp-actions::server` | 1 | Real catalog/Bus/light clocks; pending | 4.125 |
| `lamp-health::program` | 1 | Real child and Bus liveness lease; pending | 5.008 |
| `lamp-memory::health` | 7 | 5 merged reporting routes; 2 ordinary host negative cases | 5.086 |
| `lamp-records::records` | 1 | Merged at 69bae; this native row is the older host route | 5.007 |
| `lamp-voice` | 63 | 37 merged routes; 2 pure; 2 blocked local; 22 remaining host | 137.760 |
| `lamp-voice-eval` | 1 | Transport expiry: local 505 green, unpublished/native pending | 5.022 |
| `lamp-voice-eval::final_dispatch` | 13 | 12 cleanup reductions; all 13 still have host windows | 5.222 |
| `lamp-voice-eval::physical_loopback` | 12 | 10 merged finite routes; 1 pure; 1 event-driven child case | 6.534 |
| `lamp-voice-eval::serve_relay` | 6 | 6 native-qualified at 917a; unmerged in 69bae | 0.397 |
| `lamp-voice-eval::serve_session` | 14 | All 14 merged, including Recorder/SlowTransport | 19.569 |
| `lamp-voice::capture_hold_worker` | 4 | 4 fixed Linux worker cases still host; preparation only | 13.930 |
| **Total** | **123** | **Source and observation heads remain separate** | **207.660** |

Records' 5.007 s is the old host reporter, not a measurement of its merged 69bae route. Relay's 0.397 s is the unmerged controlled route, not a measurement of clean 69bae's host helper. Four of the 15 source files differ between these heads; identical original function bodies do not imply identical shared helpers. There is no transfer of native qualification or timing to a different source merely because an individual test body matches.

## Prepared work and remaining waits

Relay's six cases are ready and native qualified, awaiting reviewer merge. Transport expiry is **locally qualified but unpublished**: corrected 505/0/0, fmt, strict Clippy and workspace build all pass on 69bae, with 1,022 frozen inputs exact. Its separately recorded original-case pair is 5.055→0.047 s on the local host; native qualification remains pending. The full 5 s logical deadline, 250 original 20 ms waits, actual live child and actual SIGKILL/reap remain. The host watchdog does not preempt blocking OS calls.

Capacity and silent service are prepared but **not qualified by their broader gates**. Capacity's e5aa gate is 2,385/3/13: failures are `lamp-memory::voice_people::days_two_known_people_and_the_room_are_never_answered_from_each_other`, `coordinator::motor::tests::same_owner_input_end_cancels_queued_gesture_before_inert_bus_write`, and `coordinator::tests::second_route::without_a_second_route_the_packets_and_records_are_as_before`. Silent service's 8cafa gate is 2,252/1/13, with the unchanged-source reminder-roundtrip #935 failure. Both selected migrated originals passed, but that does not clear either broader gate or establish causation/owner acceptance. Exact local source and failure proofs are retained under `inputs/` and `prepared/`.

| Remaining or partially reduced family | IDs | Same Relay 917a printed sum(s) |
| --- | ---: | ---: |
| EarlyEnd coordinator | 10 | 60.213 |
| Capacity, prepared | 1 | 24.029 |
| Real endpoint process/mute | 7 | 21.963 |
| Other Provider | 3 | 14.194 |
| Linux capture-hold | 4 | 13.930 |
| Cross-cycle privacy | 1 | 8.585 |
| Final dispatch residual | 13 | 5.222 |
| Transport expiry, locally qualified | 1 | 5.022 |
| Health | 1 | 5.008 |
| Memory ordinary negative | 2 | 4.987 |
| Actions | 1 | 4.125 |
| Silent service, prepared | 1 | 4.051 |
| Direct name-note | 1 | 1.113 |

EarlyEnd's ten coordinator cases still need the existing Provider Actor/service/worker ownership plan; the direct name-note case is separate. The 56 passing old-base Provider prototype cases and microphone-clock preparation migrate zero original cases. All seven real endpoint process/mute cases retain their real-process clock guard. All four original Linux capture-hold cases retain real DSP/frame/authority/worker timing; the current ordinary `capture_on` path is not replaced by the microphone preparation.

The three other Provider cases retain the two real 4 s name-age/Bus cases (9.662 s combined here) and kept-start's 2,500 ms patience plus strict 2 s barrier (4.532 s). The latter carries 20 original capture blocks; source age must never be restamped or clamped fresh. Memory's ordinary default-off case retains both 2,200 ms windows; failed-store coverage retains both 250 ms post-exit drains. Health crosses a real 5 s publication gap and 3 s Bus liveness lease. Actions uses a 2 s refresh and an explicit 600 ms owner-state lifetime; its 3 s BusLight fallback applies only when no expiry header is supplied, so it is not an extra wait in this case. Both production and fixture clocks must remain coherent.

## Small independent next slice

**Review one delayed final-dispatch case next:** `a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept`, in `lampOS/crates/voice-eval/tests/final_dispatch.rs:277`. Its printed native time is 0.849 s. Five sibling delayed-line cases total 4.225 s; all 13 dispatch cases total 5.222 s. The current `Rig::backend` still starts a real shell that writes initial lines, sleeps 300 ms, then writes late lines; these five cases dispatch at an unchanged 800 ms target.

The existing `PhysicalBackend::new_with_clock` and `FixtureDriver` seams already let `inject` pump a fixture with `queue_drained=false`. A small explicit controlled pipe fixture can therefore be reviewed without proposing another production clock API. It must preserve the full 300/800 ms logical boundaries, actual stdout enqueue acknowledgement, and queued-but-unconsumed lines until the original final drain. Keep exact relay bytes, all assertions, actual EOF/child status/reap and one absolute host watchdog. Do not call a command send or timer expiry a completed reader observation. This is a source-level candidate only, with no implementation or new timing claim.

The cross-cycle mute case is larger at 8.585 s, but explicitly uses `Fixture::Threaded` through four real `service::supervise` cycles. Stable-unmute, authority, real Stop/join, distinct boots and old-child-reaped-before-new-child-start make it a larger ownership change; simply replacing Threaded with Stepped would not preserve its cross-cycle time contract. The Provider/name-age work should reuse the existing owner plan, not start a duplicate framework.

## Inherited assertion changes are explicit

The fixed 123 identity set does **not** mean every assertion byte is unchanged since 8ddff. Exactly 121 identities retain byte-identical assertion-macro lists; two inherited cases differ, with 626 original macro invocations versus 630 now:

- EarlyEnd `lamp_stop_over_an_answer_stops_it_and_only_the_answer_to_the_whole_is_played`: product commit [39eb9308](https://github.com/autonomous-ai/lampOS/commit/39eb930850482e78086073aaae19e71852e6e1a6), [PR #738](https://github.com/autonomous-ai/lampOS/pull/738), adds the exact single continuation note and shifts the final-audio index. Earlier fixture stabilization from 8bcde112/[PR #789](https://github.com/autonomous-ai/lampOS/pull/789) selects its privacy-tolerant run.
- Pilot `sigterm_while_metadata_storage_is_stalled_stops_cleanly_and_reports_the_loss`: [8bcde112](https://github.com/autonomous-ai/lampOS/commit/8bcde112dcb8b46f0a2b3e37539fb107b95546ac), [PR #789](https://github.com/autonomous-ai/lampOS/pull/789), uses an actual blocked writer and host SIGTERM-to-reap measurement, and checks acknowledged cleanup plus zero written events.

The exact source differences, original commit patches, PR API associations and ancestry proof are retained under `history/`. These deltas predate this audit and occur identically in both inspected current/native heads. The audit changes no test or assertion and does not silently recertify those historical edits.

No Cargo command, new test execution, target use, Git/GitHub mutation or device action occurred. Two nonexistent supplementary source-path reads were corrected after tree inspection; the read-only diagnostic is retained separately. Python orchestration stays outside this package. **The original 123 standalone serial under 120-second target remains unproved.** Printed shard sums, different heads and local pairs are never added into a forecast.
