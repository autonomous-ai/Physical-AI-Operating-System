# Capture clock: first gate handoff

Recorded 2026-10-16 08:12:34 +0700, from shell date. Base `e5aa43747ec1540893b4da4509d14288e7e70806`, branch `tests/823-capture-clock`, tree `/private/tmp/lampos-tests-capture-clock`. This preparation remains locally red and native Linux qualification is pending. No test was retried.

The source delta is exactly `capture.rs`, the new `capture/clock_tests.rs`, and the English fixture document. MicrophoneOn's ordinary constructor still delegates to the same host clock; all nine sampling sites, device-call/check ordering, bounds, results and original branches remain in place. The original 13-test module and all 86 assertion macros are byte-exact. Whole-file inverses restore both existing input files. All 7,224 unowned tracked files remain exact. No worker, Capture Rig, warm-up, original capture_hold_worker, pacing or transport source changed.

## Completed local checks

| Stage | Result | Evidence |
| --- | --- | --- |
| Rust 1.96 and whole-workspace format check | Passed | `first-gate/gates.json` |
| Strict all-target Clippy: Audio, Memory, Voice, VoiceEval | Passed | `first-gate/clippy.log` |
| Exact compiled comparison | 2,303 originals retained + 9 new = 2,312; all 15 existing ignore flags exact | `first-gate/compiled-list-comparison.json` |
| First standalone focused command | Exit 4, selected and executed zero tests | `first-gate/focused.log` |
| Corrected four-package selection proof | Exactly nine selected; zero bodies executed by the listing | `first-gate/selection-correction/selected-nine-proof.json` |
| First actual focused execution | Nine passed once | `first-gate/continued-execution/focused-result.json` |
| Full affected, serial, no fail-fast, retries zero | 2,296 passed / one failed / 15 existing ignores | `first-gate/continued-execution/terminal-result-audit.json` |
| Workspace build | Passed | `first-gate/continued-execution/workspace-build.log` |

The full gate ended at 08:06:07 and build at 08:07:39. Session 52182 is terminal with exit one. All 7,227 first-gate source hashes and 90 Audio-baseline closure inputs stayed exact through completion. The Audio reverse-dependency scope is Audio, Memory, Voice and VoiceEval; the unchanged Audio baseline closure is Audio, IPC and TurnOwner, with no Evaluator dependency.

The zero-selection command found the cached 70-test standalone library artifact; the accepted combined list and corrected focused command use the same 79-test library artifact, including all nine additions. Audio declares no features; the earlier feature-unification explanation was disproved. Depfile source paths, dependency differences, source/artifact timestamps and exact artifact hashes are retained. Cargo's untraced internal freshness decision remains a hypothesis. No timestamp, cache, source or target cleaning was used to correct selection. The final workspace build explicitly compiled Audio from the candidate tree; its main Audio rlib changed during that build and embeds the candidate tree and with_clock API.

The raw first result parser counted nextest's one repeated failure-footer row after Summary as another execution. Its original false exact-once result is preserved. The separate terminal audit proves all 2,297 nonignored compiled identities have one execution row and ordinal before Summary, plus 15 unchanged ignored identities; the repeated footer is recorded separately. No test execution was needed for this correction.

## Preserved failure and native boundary

The sole failure is `lamp-voice::coordinator::tests::reminder_roundtrip::actual_consumer_ipc_coordinator_and_two_retirements_ack_only_after_whole_completion`, the existing #935 AckConfirmed assertion at line 338. The whole test source is exact to clean e5aa and historical evidence. Pending was already false and exactly one Ack request had been captured; the actual resolution and Get/persist/Ack hop times are unprinted. Later offered-count and parent-offer assertions were not reached. The matching current Provider gate is a separate retained recurrence, with its own counts and source proof. No timing cause, accepted exception, green local qualification, scenario migration or speedup is claimed.

Capture is cfg(unix), so all nine new cases ran on macOS. ALSA, audio_workers and the entire capture_hold_worker binary remain Linux-only. Native validation must compile the unchanged default constructor/type callers and run all five existing capture_hold_worker cases, including the four original inventory identities and the later repeated-stall case. The PR profile excludes that binary; default or explicit realtime coverage is required. See `native-coverage-plan.json`.

## Final source and target

After all execution finished, the approved one-line document phrase changed from `realtime-profile exclusion` to `PR-profile exclusion`, checked against the actual nextest profiles. Both documentation versions and the exact one-line diff are retained; no Rust changed and no tests reran for this wording. Final hashes:

- `lampOS/crates/audio/src/capture.rs`: `4661ad6a4aa347bf2b0803a08ad2f163e2ac3f5f6a5def0062be6eed1b875449`
- `lampOS/crates/audio/src/capture/clock_tests.rs`: `15d8c7a1c33f855e1e1b08e8b7d678e56ce1caaf6ee7edfd78867c673eb33578`
- `lampOS/docs/voice-test-fixtures.md`: `8f2a48e3b9fc1f1ecebc8ca742b40a771d4063d7c8e5f862dd5085f3d9be740b`

`publication-source-proof.json` and `publication-inverse.json` record the final source freeze. The target `/private/tmp/lampos-tests-capacity/lampOS/target` is released; no further Cargo command or artifact inspection is planned. No commits, pushes, GitHub writes or device operations were performed.
