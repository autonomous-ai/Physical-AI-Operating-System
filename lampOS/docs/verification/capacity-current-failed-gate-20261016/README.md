# Capacity current-base gate: failures retained

Recorded 2026-10-16 07:49:59 +0700, from shell `date`. **Ready for lamp-voice: no.** The first full gate after integrating actual Schedule fix #928 is **2,385 passed / three failed / 13 existing ignores**, based on `e5aa43747ec1540893b4da4509d14288e7e70806`. Fmt, strict affected Clippy and workspace build pass. All 2,401 compiled identities/ignore flags are exact before and after; the independent runtime audit accounts for every identity exactly once across 83 binary paths. All 885 frozen inputs stayed unchanged. No test was retried.

The original capacity case `provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity` passes. Its original twenty-second answer/five-second buffer and all 24 capacity assertions remain unchanged. `integration/` records the exact clock-only delta, full reverse to the current base, 544 unchanged worker assertions and retained history. This fresh gate contains no new paired timing measurement or native qualification. Prior local/native failures and the old paired measurement remain historical in [PR #868](https://github.com/autonomous-ai/lampOS/pull/868).

## Exact failures

| Binary and identity | Observed boundary |
| --- | --- |
| `lamp-memory::voice_people` / `days_two_known_people_and_the_room_are_never_answered_from_each_other` | `voice_people.rs:251`: outer `open_session_for(...).unwrap()` receives None. The following person-snapshot unwrap and isolation assertions do not run. |
| `lamp-voice` / `coordinator::motor::tests::same_owner_input_end_cancels_queued_gesture_before_inert_bus_write` | `coordinator/motor/tests.rs:1193`: required child-position-writes file is absent; exact identity and symptom already on #906. |
| `lamp-voice` / `coordinator::tests::second_route::without_a_second_route_the_packets_and_records_are_as_before` | `second_route.rs:162`: service privacy freshness lost before next observation; exact identity is listed on #893. Its preceding comparison run reaches the scripted physical mute; the failing run does not. |

All three are outside the frozen original 123. [Full unedited gate](gate/tests.log), [exact failure extracts](failures.txt), and [independent runtime audit](gate/root-result-audit.json) keep their distinct outcomes. The default 50 ms privacy bound, motor receipt/file assertions and Memory deadline are untouched. Source snapshots for each failure and the actual DaysClient implementation are byte-exact to the base.

## Memory failure: known and unknown

The Memory test already completed both remember calls. It next calls ordinary `DaysClient::open_session_for(1, Duration::from_millis(300), Some(person))`. The default client clamps to `FIRST_READ` (300 ms), obtains the room and optional person snapshot under one deadline, and then checks its context again. Outer None may arise from initial context locking, room transport/answer validation, or final context/deadline acceptance. The log prints none of those intermediate outcomes. A missed host deadline is a hypothesis, not a diagnosis; no assertion has established the initial session or subsequent scope isolation.

The fixture uses a real Memory server thread/socket/store and a trace channel; its Drop requests Stop and joins the worker. The failed test does not start the capacity Provider worker, and the unchanged Memory/DaysClient sources narrow the direct path without excluding indirect host effects. Exact-name and broader voice_people/day-name issue searches found no existing report at publication time. This new identity goes to the Memory owner for classification and a deterministic fix preserving the intended scope checks and production deadline. It is not silently covered by another failure's exception.

## Reproduction and preservation

Commands run from the task tree's `lampOS/`, Rust 1.96.0, locked/offline, jobs 2, incremental 0, shared Mac debug target. `gate/gates.json` records shell-date boundaries and exits; tests were one thread with no fail-fast. The current clean baseline is the exact e5aa list from the Moments tree, with all relevant baseline source hashes verified and no baseline rerun. The gate is followed once by the previously planned workspace build, which passes and does not erase the failed tests.

```sh
cargo fmt --all --check
cargo clippy --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-surround --all-targets -- -D warnings
cargo nextest list --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-surround --message-format json
cargo test --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-surround --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

[proof.json](proof.json) records the source/log hashes and exact copy provenance. `SHA256SUMS` covers every other published file. External Python audited list/result identities, source hashes and raw summary totals; no orchestration script is committed. This evidence is not a lamp result or the original 123 standalone serial timing proof.
