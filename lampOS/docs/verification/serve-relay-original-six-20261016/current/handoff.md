# Direct Serve-relay migration on complete Serve-session source

Recorded from `date`: 2026-10-16 06:00:45 +0700. Source is frozen in `/private/tmp/lampos-tests-serve-relay-all-cases`, branch `tests/823-serve-relay-all-cases`, on the assigned base `8af53fd459795029f6277679ded49bf7826def73`. This is a source integration only. No Cargo, build, test binary, target-cache access, Git mutation, network or device action was performed. Root owns all fresh gates, measurements and publication after the actual Serve dependency merge. The newer release integration being prepared elsewhere was deliberately not chased or edited.

## Exact six-file scope

The qualified input is `/private/tmp/lampos-tests-serve-relay-coordination`, based on `e057db620acb2bd61bac1ae7f157d87cdf635a18`. Every one of its 70 Rust hashes still matches `root-source-before.json`; the retained fresh gate reparses as **432 passed / zero failed or ignored**, 25 summaries, and all recorded stage exits are zero. `qualified-provenance.json` pins the source manifest and every raw gate file. That older result is provenance only and does not qualify this newly composed tree.

Exactly four existing files and two new files change:

- `fake_lamp.rs`: the exact qualified invocation adapter and test-module addition. The received argument vector is reported once by the existing flushed stdout record; only its validated suffix is parsed. The current base equals the qualified delta's old source, so the resulting file is byte-identical to the qualified continuation. Reconstructing the original function from the live shared body, then removing the added module, restores the entire original file.
- `fixture_commands.rs`: only the same two substitutions inside `serve_runtime`: retain the original complete received argument vector and call the invocation adapter. Its entire Recorder and SlowTransport code, CLI adapters, tests and module declarations remain intact. Replacing this one exact function with its old form restores the whole current-base file.
- New `serve_invocation_tests.rs`: exact qualified four boundary cases.
- `tests/serve_relay.rs`: exact qualified helper/module/import migration. No scenario body or assertion changes.
- New `tests/common/serve_relay_clock.rs`: exact qualified six boundary cases and the existing reviewed current-thread coordinator.
- `docs/realtime-tests.md`: the exact corrected Serve relay coordinator section is appended. Every byte of the current base remains an unchanged prefix, including the Recorder and SlowTransport sections.

`participant-only.diff` is the complete six-file delta, SHA-256 `5c6c74d14027c669d6c51ff026dec2b086beeb32fa6cca257c61e075a2667b1e`. Every hunk applies and reverses exactly from the actual worktree bytes; the four old files reconstruct fully, and both new files reverse to absence. All **6,899 nonowned tracked entries** retain their bytes or symlink targets and modes. The 75 original Evaluator Rust files are snapshotted; the result has 77. Protocol, main dispatch, backend, drivers, session, all Recorder/SlowTransport tests, and `tests/serve_session.rs` remain byte-identical. In particular, the original `channel_closed_peer_tests` declaration and its ten compiled names remain unchanged.

## Original checks and expected inventory

All **498 existing test function bodies** remain byte-identical. `test-source-preservation.json` records each file, name, body hash and static assertion count. The original six relay scenarios retain their entire shared tail, SHA-256 `8040e3cbda58f22296ef057df328d8c3017fde6afc162954852ec675daff4ce2`, including **37 original assertions**, their fault inputs, complete three-second Stop/Close waits, one-second run plus five-second allowance, full ten-second ignored-signal grace, and exact child-versus-spawn argv comparison. `original-six-preservation.json` and the complete file reversal provide separate evidence.

| Original relay identity | Assertion sites | Body SHA-256 |
| --- | ---: | --- |
| `a_runner_stop_is_one_sigterm_and_a_clean_reaped_end` | 11 | `24f30b59c030653b85c2bbcf8cfff09843fa764276bdec36979b1f70d40c8601` |
| `a_closed_runner_transport_stops_the_runtime_through_the_same_path` | 4 | `4af2fbd73fcd008a69162e0debbd12612d7da13dbc83f030e900ccd8e293edea` |
| `the_hard_deadline_stops_the_runtime_and_is_an_error` | 4 | `baa93d280ee49b3b137250d7121bd2814300a37850af9df7dab767a4009643df` |
| `a_runtime_that_ignores_sigterm_is_force_killed_and_never_clean` | 7 | `8b7c24fd64496c741935767b93d5405fddebf3c0b58809f020ca2672b9c593a4` |
| `the_pilot_runtime_settings_reach_the_serve_child_exactly` | 6 | `0640ae1b9c99c0141188428206b152c08e8625d00e3aa617fc0ecf345a6e674e` |
| `a_nonzero_runtime_exit_after_clean_metadata_is_a_relay_error` | 5 | `84c0bb43468122577f548bbe81f22a5be54569b18dbc3e0869b2b8b2182d7858` |

The baseline list is the actual compiled **498-case** list committed with the complete Serve gate in `8af53fd`; all 75 current source hashes match that gate's frozen snapshot. It was copied and parsed, not rerun. `baseline-compiled-identities.json` records that provenance. The new source contributes exactly four library invocation cases and six relay helper cases, with names and bodies matching the earlier qualified 432-case continuation. Combining these with the current base produces the **source-derived expectation of 508 cases: 300 library + 208 integration, zero ignored**. The relay binary should contain the six unchanged originals plus six helper tests. `expected-after-identities-source-only.json` preserves all 498 current names and adds only those ten; it is not a new compiled list or execution result.

## Completion boundaries and host limits

The helper continues using actual runner Stop bytes or EOF, successful control receipts, the existing runtime signal handler, and the unchanged session's real `Child::try_wait` or full grace-expiry kill/wait. Runtime logical completion freezes time while the session observes its real child; it does not fabricate exit, reap, metadata, or a terminal session result. The session still owns readers, process-group checks, final drains and output. No helper-owned detached thread is introduced.

The retained **ten-second post-signal host budget is checked before and after polls and is a success condition**. It is not a strict ten-second failure/abort bound: the unchanged reader-enqueue barrier can take up to **thirty seconds** to return a failing poll, and its existing group-check/late-output work also remains. The original logical grace assertion is not a host or device shutdown-latency measurement. This wording is preserved exactly in the appended documentation.

## Verification and limits

Pinned Rust **1.96.0** standalone `rustfmt --check --edition 2024 --config skip_children=true` passed all five owned Rust files, exit zero and empty output. `rustfmt.json` records exact argv and shell-date boundaries. The formatter changed no source. All source hashes, complete inverses, 498 old bodies, 37 original relay assertions, expected identities and protected files were rechecked afterward; Git status contains exactly the six assigned paths.

An initial scratch audit stopped after applying the already-verified candidates because its trailing-span locator called the existing function `recorder` instead of `room_recorder`. `initial-audit-interruption.json` retains the failed audit and correction. Only the scratch locator changed; all source inverses, nonowned hashes and function-body comparisons were rerun as read-only checks. No executable test failed or was retried.

No current-base test result, new paired timing, native ARM64 result, #914 fix, or complete original-123 result is claimed. The dependency has not yet merged. Fresh qualified source can be ported later as this six-file delta; the current base and all earlier failures must remain separately identified.

## Frozen hashes

| Path | SHA-256 |
| --- | --- |
| `lampOS/crates/voice-eval/src/physical/fake_lamp.rs` | `b9637518aaf8e0dffc587273ab0c5e19c6ff1d72337796c71a4d0832cb4f1327` |
| `lampOS/crates/voice-eval/src/physical/fixture_commands.rs` | `b9cd63c46803efe08cdc32b9d308612b7a9b54b937b193b73f87624b1124c4db` |
| `lampOS/crates/voice-eval/src/physical/serve_invocation_tests.rs` | `89f89a010c4a240aa699fd37fb49e0a2c4f2ed8f65ce77332c5c9b575e3d12dc` |
| `lampOS/crates/voice-eval/tests/serve_relay.rs` | `836a72136db9573ca12e8b4aeab77ff95e42010cf71fcd652da6318945ec7bf2` |
| `lampOS/crates/voice-eval/tests/common/serve_relay_clock.rs` | `60f5ee58ee905b285ea54c5d6c1a2159964e97a2945a79654d283ca14d0ee9cb` |
| `lampOS/docs/realtime-tests.md` | `e7b9bc1d977eadae3c11e79f60517af4089a607bc052de5a7af29aad63a5c2a4` |

Source-proof SHA-256: `a6e12f18bd4fcbec5238a76f2b6255e8a3ae62e2880d7a256fb97ad15793c1af`.
