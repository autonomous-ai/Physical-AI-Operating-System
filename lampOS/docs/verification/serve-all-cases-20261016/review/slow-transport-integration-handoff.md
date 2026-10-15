# SlowTransport participant continuation plan

Read-only plan recorded from `date`: 2026-10-16 03:33:54 +0700. Target is the frozen local `/private/tmp/lampos-tests-recorder-coordination` on base `48b3737d5e83efa00278172cb602d3525d368a43` with the captured #909 Serve seed and Recorder port. No repository files were edited; no Cargo/build/test, Git, network or device commands were run. This is a later separate PR after the Recorder dependency, not authorization to modify or publish the running-gate tree.

## Exact delta and dependency boundary

There are **nine remaining Rust files: six existing files plus three new modules**. Every existing target file to be changed is byte-identical to the qualified Recorder precursor. The target's entire `fake_lamp.rs` and `slow_transport_clock_tests.rs` already match the historical qualified `/private/tmp/lampos-tests-slow-transport-progress` bytes exactly. Do not add #894's pure wait seam or its six tests again, and do not touch the merged Recorder seam.

`participant-only.diff` contains only those nine source files. The in-memory port makes every evaluator Rust file match the historical qualified SlowTransport tree; `source-audit.json` verifies that old tree still matches its full-gate source manifest. Keep the target's Cargo feature ordering and all newer nonowned files. No Cargo, mac/backend, prepared-player, session, fake-runtime or product change is needed for this continuation.

## Ownership and integration hunks

All source paths below are under `lampOS/crates/voice-eval/`.

| Slice | Exact files and hunks | Required preservation |
|---|---|---|
| Wire/command adapter | `src/physical/fixture_control.rs`: SlowTransport role, bit 32, fixed-wait progress, validation/match arms and new module. `fixture_commands.rs`: explicit handler, unchanged argument forwarding into the existing wait seam, checked due arithmetic, strict same-executable/same-control target validation, final-Ack/participant-drop before real exec, module. New `slow_transport_fixture_control_tests.rs` (12 cases) and `slow_transport_fixture_commands_tests.rs` (9). | Version/Step fields, existing role encoding, framing/round validation, ordinary command/default wait, actual stdin/stdout and exec remain. Terminal Ack proves only completed delay. |
| Scheduler | `src/physical/serve_fixture_driver.rs`: SlowTransportControl fields, atomic wrapper configuration, startup stepping, split `bootstrap_at`, real relay registration barrier, retained `transport_until`, close path and module. New `slow_transport_fixture_driver_tests.rs` (14). | Full 400 ms wait, original at-most-2 ms steps, Recorder due/actual-exit barrier, prepared work ordering, original runtime/relay bootstrap sequence and actual enqueued/input counters. |
| Exhaustive old helper | `src/physical/recorder_fixture_control_tests.rs:67`: only `Role::Recorder => unreachable!()` becomes `Role::Recorder | Role::SlowTransport => unreachable!()`. | Required enum exhaustiveness only. The original helper's callers do not execute that arm; old test names, bodies, inputs and all assertion macro bytes remain exact. Record this exception rather than claiming that entire old test file is unchanged. |
| Root dispatch/scenario wiring | `src/main.rs:196`: one `fake-clock-slow-transport` dispatch. `tests/serve_session.rs`: introductory comment, removal of unused FakePreparedRoomPlayer import, and replacement of its helper's old conditional constructor with the existing controlled constructor. | The actual wrapper/delay command builder is untouched. No original test body, scenario, fault, timing, assertion or request changes; only the remaining delayed case changes execution route. |
| Root docs/evidence | Narrow edits to current `lampOS/docs/realtime-tests.md`: Serve coverage becomes all 14 identities/22 attempts; explain controlled transport/first preparation instead of their prior host-path wording; add one participant section with full-delay/real-exec/fresh-Hello boundaries. | Preserve the existing pure-seam, Recorder, Body and other accumulated sections. Do not copy the old progress branch's docs. Its only doc diff appends an unrelated finite-runtime section already present and updated on the current base. |

The three new participant modules add **35 identities**, not 41: #894's six pure-seam tests are already present. Exact context hunk coordinates (1-based `start+line_count`, target → qualified SlowTransport) are:

- `lampOS/crates/voice-eval/src/main.rs`: 193+6 → 193+7.
- `lampOS/crates/voice-eval/src/physical/fixture_commands.rs`: 1+4 → 1+5, 112+6 → 113+105, 656+3 → 756+7.
- `lampOS/crates/voice-eval/src/physical/fixture_control.rs`: 32+6 → 32+7, 40+9 → 41+11, 119+6 → 122+15, 129+6 → 141+7, 138+7 → 151+8, 148+7 → 162+8, 239+6 → 254+7, 447+6 → 463+7, 533+6 → 550+18, 575+6 → 604+7, 644+6 → 674+7, 727+7 → 758+10, 778+6 → 812+60, 1051+3 → 1139+7.
- `lampOS/crates/voice-eval/src/physical/recorder_fixture_control_tests.rs`: 64+7 → 64+7.
- `lampOS/crates/voice-eval/src/physical/serve_fixture_driver.rs`: 1+6 → 1+7, 36+11 → 37+22, 84+6 → 96+9, 121+6 → 136+7, 130+7 → 146+8, 139+6 → 156+7, 150+6 → 168+7, 389+17 → 408+158, 416+7 → 576+10, 442+6 → 605+9, 488+7 → 654+13, 642+6 → 814+49, 844+3 → 1059+7.
- `lampOS/crates/voice-eval/tests/serve_session.rs`: 6+16 → 6+14, 136+22 → 134+14.

## Boundaries that must survive the port

- The wrapper waits the complete original duration without consuming stdin or writing synthetic stdout. It forwards the original slice into the existing real `CommandExt::exec`; inherited queued setup Pings stay in the actual pipe. Dropping the completed SlowTransport Participant precedes exec.
- The driver holds time at the delay terminal boundary and separately accepts a fresh **actual ServeRelay Hello**. Ack, EOF, a changed role enum, or a mock participant is not proof of exec, relay readiness or OS exit. Public command validation requires this executable's controlled relay/runtime and the same private socket; test-only shell targets do not relax it.
- At 400 ms, the wrapper uses the initial held epoch plus 200 original 2 ms advances (201 delay observations). Zero and 1 ms delays still need fresh relay registration before any advance toward the original epoch+2 ms bootstrap. Delayed startup does not synthesize runtime polls for time before the runtime exists.
- Recorder progression runs at the same held instants and caps advancement at its nearer due time. Terminal Recorder progress must still return to the backend for actual Child exit notification; an accepted relay can remain held without a Step. Leave `mac.rs` and its actual exit/coverage/cleanup logic untouched.
- Preserve the repaired **cumulative startup host deadline**: `transport_until` latches the first budget, later rounds cannot extend it, a shorter preparation deadline wins, and it clears only after relay/runtime bootstrap completes. Do not reintroduce a fresh 30 s watchdog per logical round.
- Preserve actual setup-Ping writes/counters, real room readiness, and full **2300 ms** first-preparer delay. The original 100 ms freshness/lateness bound, grant checks and datagram send/consumption remain. The driver does not read stdin or invent these observations.
- Closing/error paths freeze/close all owned peers and release owned threads; actual product children stay owned by the backend. Real exec, process exit/reap and recording validity remain separate evidence boundaries.

The existing qualified reviews are `/private/tmp/tests-823-slow-transport-control-review.md` and `/private/tmp/tests-823-slow-transport-driver-evidence/review.md`. Their focused cases distinguish mock protocol receipts from actual exec children. No new design or weakened condition is needed on this source snapshot.

## Original coverage and source proof

Fresh comparison with original `base-a3dba0c0a.serve_session.rs` confirms all **14 complete bodies**, all **114 assertion macro byte strings**, the entire helper/test tail from `fn statuses`, **18 source run sites** and **22 expanded attempts** remain exact. Reversing only the three known comment/import/constructor edits reproduces the current Recorder source byte-for-byte (`485ef6bc…`); `serve-session.restored.rs` records it. Qualified SlowTransport after-source is `1850248165b8c7d5022ec06f3267b5de1a829fa375135cdda26aa3bfe539b905`.

One historical evidence wording needs correction in derived docs: `pair-source-audit.json` says 113 assertion sites because its tail begins at `fn statuses`. There are **113 in that tail plus the unchanged run helper's ledger assertion = 114 total**. Fresh extraction compared every macro byte string before/after. Do not edit the retained raw historical JSON; explain its scope when citing it.

The sole route change is `startup_cues_stay_fresh_through_a_slow_transport_and_a_slow_first_preparer`: one host identity/attempt becomes controlled. All fourteen identities and all twenty-two attempts then use coordinated execution; the five-fault loop and every original 400 ms/2300 ms/100 ms/recorder bound remain. No new original assertion or acceptance change is permitted.

## Qualification plan and historical limits

After Recorder is independently merged/qualified, root can create a new clean branch, snapshot its base, apply only the nine-file participant patch plus narrow current docs, and regenerate inverse/hash proofs. Recheck that pure #894 files and the Recorder backend are still exact. Fresh compiled-list equality and complete execution must include all fourteen cases, no skips/retries; full result success plus unchanged expanded loops establishes all 22 attempts. Run a fresh same-helper serial pair if making a new timing claim, then formatting, strict evaluator all-target Clippy, full evaluator tests and workspace build, followed by native qualification.

Source equality derives an expected **296 library + 202 integration = 498** cases: current Recorder's 261 library +35 participant cases, with integrations unchanged. This is not a fresh compiled or executed result. The old complete log was independently read as **498 passed, zero failures/ignores/filters**, with 25 summary blocks. The retained old pair reads **86.271 s → 73.874 s**, fourteen passes/no skips each, and the recorded helper hash is identical. Those x86_64 macOS results had shared uncontrolled host load; they do not qualify the newer Voice/dependency base, native ARM64 or the complete #823 group target.

## Stable source hashes for the later port

- `lampOS/crates/voice-eval/src/main.rs`: current `1c58c33e90aeff37cd0e895655373dc435d2570f1cbb819966c91d2d5dda9ba9` → qualified `1751caa8d8246f8217674397c2c979bab40dc6eaa3d81834784e88cc29ef6607`.
- `lampOS/crates/voice-eval/src/physical/fixture_commands.rs`: current `464820ece7888f7d0767d7024d7b1e399d9d18601f09922f14dba5b0e328234c` → qualified `df424358eb99ab6982ef8cd3ea3d7484f55bf3c033aabcf29ca0b07c0df5ab0a`.
- `lampOS/crates/voice-eval/src/physical/fixture_control.rs`: current `16f31e16dcb8c03168c8f8d2e76323c6d09882936c88bd05c1691bdaa7a888fa` → qualified `fd764a6a67b3c83295e3fad677a39ef0c6a10a70a7305f9d92aa2fe10d64daf2`.
- `lampOS/crates/voice-eval/src/physical/recorder_fixture_control_tests.rs`: current `832b0d1a345f119ec83f1579dc2864ed7935b9abb86468810292a1c02b01f25f` → qualified `3e9c7eb09f574cbda46a06fb9ac452993df5a9b4417c5f0d9691f9deba27cf80`.
- `lampOS/crates/voice-eval/src/physical/serve_fixture_driver.rs`: current `b7deaea4fb28cc44f48675cade095d38aea77f9dba668f7b9e32e9060e5170bb` → qualified `3ba9b3405c82198e4c56e4baa3c982be74d8a2e0588766be11c34a2fa62e8014`.
- `lampOS/crates/voice-eval/src/physical/slow_transport_fixture_commands_tests.rs`: current `absent` → qualified `dee37c32bd463a6603ab0b2f1dc5f0b2a9ae4b62da98b26a9b9aea560b7ef7c1`.
- `lampOS/crates/voice-eval/src/physical/slow_transport_fixture_control_tests.rs`: current `absent` → qualified `1032118bfae8aec9a555f87ca6f3f334c16599c361a90b38768d5c1d454d3359`.
- `lampOS/crates/voice-eval/src/physical/slow_transport_fixture_driver_tests.rs`: current `absent` → qualified `0026046f596309aa8cc2698de1893825d0b4d006e5f7951a5af81d1158a85187`.
- `lampOS/crates/voice-eval/tests/serve_session.rs`: current `485ef6bc04c292666b633c819baa7c16465aaa756e97271c9c0708cd2c3e6186` → qualified `1850248165b8c7d5022ec06f3267b5de1a829fa375135cdda26aa3bfe539b905`.
