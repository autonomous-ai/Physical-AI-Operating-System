# Provider observations: corrected gate handoff

Recorded 2026-10-16 07:27:35 +0700 from shell `date`. **UNREADY:** the corrected source's complete affected gate has one distinct existing test failure whose cause remains unclassified. Owner classification or a fix is required before this change is called ready. Workspace build and all 20 new tests pass; they do not make the full gate green.

The tree is `/private/tmp/lampos-tests-provider-observations`, branch `tests/823-provider-observations`, base `e7d0f673d0c4bde4a770b19783d6084a8d935ec3`. The approved correction changes only the new unpolled-worker test: a successful empty receive is checked before Drop, and after Drop only an empty receive or `ConnectionReset` is accepted. Every event and every different error still fails. The actual attempt-future Drop receipt is awaited under the same deadline. The reviewed diff is [proposed-unpolled-close-only.diff](proposed-unpolled-close-only.diff).

## Final corrected result

| Stage | Started (+0700) | Finished (+0700) | Exit |
|---|---|---|---|
| fmt | 06:49:53 | 06:49:58 | 0 |
| clippy | 06:49:58 | 06:50:22 | 0 |
| list | 06:50:22 | 06:51:20 | 0 |
| focused | 06:51:20 | 06:52:09 | 0 |
| tests | 06:52:09 | 07:18:33 | 101 |
| workspace-build | 07:18:33 | 07:18:34 | 0 |

- Focused new cases: **20 passed**, zero failures; one invocation with `--retries 0`.
- Full Voice, Memory, Evaluator and Surround: **2,263 passed, one failed, 13 ignored**, zero filtered or measured.
- Runtime identity audit: all **82 binaries and 2,277 named statuses** match the compiled list, with every ignored flag preserved. Memory 205 passed/5 ignored; Surround 260 passed; Voice 1,421 passed/1 failed/8 ignored; Evaluator 377 passed. Four empty doc-test groups also completed.
- Clean e7d comparison: all **2,257 original identities** and their 13 ignored flags retained, with exactly the reviewed **20 additions**. First and corrected compiled identity/flag maps are identical.

Raw commands, logs, timestamps and exits are in `corrected-*`; see [corrected-completed.json](corrected-completed.json), [corrected-runtime-identity-proof.json](corrected-runtime-identity-proof.json) and [corrected-compiled-list-comparison.json](corrected-compiled-list-comparison.json). The fresh gate ended 07:18:36 +0700; native handle 90916 is complete and no test/build job remains.

## Exact remaining failure

`lamp-voice` / `coordinator::tests::pilot_faults::a_microphone_closed_by_a_stall_reopens_under_a_new_epoch_and_the_voice_goes_on`

At original `coordinator/tests/pilot_faults.rs:1887`, `assert!(is_mute(&run.result), ...)` fails with:

```text
Err(Custom { kind: Other, error: "service privacy freshness lost before next observation" })
```

The earlier `run.assert_invariants()` completed. `is_mute` requires a `PhysicalMute` error; the observed result is the `io::Error::other` path. This is a **newly observed failure of an existing e7d identity**, outside the frozen original 123. It is distinct from the `a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on` identity in #866; the earlier mistaken exact mapping was corrected before this package was frozen. #866/#739 are related families only. Root owns the separate issue and owner routing.

The static source path is the ordinary `conversation` → `conversation_with_controls` → `conversation_with_gate` call with `PrivacyGate::default()`. The matching error is returned at `coordinator.rs:2892` when continuous lifetime, nonzero `privacy_sequence` and `gate.state(now()) == Unavailable` hold, before the next privacy receive loop. `privacy.rs:96` requires a known accepted observation with age strictly below the unchanged default 50,000 µs bound. This path identifies the error boundary, not its cause.

The Threaded fixture reports host packet/policy time, 2.379080296 s conversation and cleanup, a 72,046 µs producer privacy gap, 409,278 µs capture send-attempt gap, and 63,109 µs oldest source age at send attempt. Consumption age and the last coordinator-consumed privacy timestamp are unreported. Its raw `stopped` labels are provider/privacy/speaker/capture and `abandoned` is empty; these labels are not promoted into new Provider Actor-join or child-exit claims.

[corrected-pilot-failure.txt](corrected-pilot-failure.txt) preserves the exact failure block and trace. [corrected-pilot-triage.json](corrected-pilot-triage.json) records the diagnostics without a causal classification. [corrected-privacy-failure-path.json](corrected-privacy-failure-path.json) includes precise line locations and SHA-256 values for the full, unchanged coordinator, pilot fixture, privacy gate and service-privacy source snapshots. [original123-inventory.json](original123-inventory.json), SHA-256 `72d671f8b22246898f9142c0216c9fbced55e350e46b83ffec787c81496fb1fc`, independently proves this identity is outside that 123-case denominator.

## First failures remain preserved

The original frozen first source had focused **19 passed/1 failed** and full **2,262 passed/2 failed/13 ignored**. Its new unpolled-worker test failed on the unwrapped post-Drop `ConnectionReset`; its final attempt-Drop await was not reached. The approved correction is separately recorded and now passes both focused and full scopes.

The first full gate also failed original `coordinator::tests::reminder_roundtrip::actual_consumer_ipc_coordinator_and_two_retirements_ack_only_after_whole_completion` at line 338. Pending was false and one Ack request existed; the actual resolution was not printed. That identity passes in this fresh corrected gate, but its first failure and #935 remain valid evidence with unknown cause. See [first-gate-handoff.md](first-gate-handoff.md), [first-reminder-triage.json](first-reminder-triage.json) and [first-reminder-failure.txt](first-reminder-failure.txt). No failure-specific reruns were made. Each source version received one planned focused run and one complete affected run.

## Source and preservation proof

All **6,792 protected files** and the four corrected owned files were checked exact after the gate. The other three owned paths are byte-identical to the first frozen source; only the reviewed new-test file changed. All 24 entries in the first evidence manifest and all six raw logs per attempt were rehashed unchanged. The whole-worker inverse still reconstructs the original worker. The old inline module is **105 tests including two ignored, with 544 assertions**, byte-exact; the early count of 103 was a regex inventory error corrected in [handoff-inventory-addendum.md](handoff-inventory-addendum.md).

| Owned path | Corrected SHA-256 |
|---|---|
| `lampOS/crates/voice/src/provider_worker.rs` | `56b82b75f50026cb9036ed9f88f937e9df218df7702968d0bbf7798106df1e90` |
| `lampOS/crates/voice/src/provider_worker/observation.rs` | `8a370c2a4b85bd70890d51a34259524cdf5cb236d6a8210f16abb37f6b99ef5e` |
| `lampOS/crates/voice/src/provider_worker/tests/observation.rs` | `25e230b6ccec6a5273f65a6d37f97f6efbbb4cada473c6b7bf0a3d742fbbc3f8` |
| `lampOS/docs/voice-test-fixtures.md` | `0011049236738f2ebe8ae93fbc3269bfb2ba9952720e0a4397818992500a4dc6` |

[corrected-final-source-verification.json](corrected-final-source-verification.json) records the final checks and the successful full-run status of every new case. This preparation observes actual Stop handlers and body return/drop boundaries only. It does not migrate an original scenario, introduce clocks or change timers/policy, prove Actor or child completion, or establish a speedup.

## Reproduction commands and environment

Commands ran from the tree's `lampOS/` directory with `RUSTUP_TOOLCHAIN=1.96.0`, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `RUST_BACKTRACE=1`, and `CARGO_TARGET_DIR=/private/tmp/lampos-tests-capacity/lampOS/target`. Locked/offline dependency use and serial no-fail-fast tests are explicit below; local sockets and owned children ran under the approved escalation.

`cargo fmt --all -- --check`

`cargo clippy --locked --offline -p lamp-voice -p lamp-memory -p lamp-voice-eval -p lamp-surround --all-targets -- -D warnings`

`cargo nextest list --locked --offline -p lamp-voice -p lamp-memory -p lamp-voice-eval -p lamp-surround --message-format json`

`cargo nextest run --locked --offline -p lamp-voice --profile default --test-threads 1 --no-fail-fast --retries 0 -E test(/^provider_worker::observation::tests::/) | test(/^provider_worker::tests::observation::/)`

`cargo test --locked --offline -p lamp-voice -p lamp-memory -p lamp-voice-eval -p lamp-surround --no-fail-fast -- --test-threads=1`

`cargo build --locked --offline --workspace`

Both raw before/after compiled lists, all source snapshots, first and corrected stage logs, exact failure extracts, source hashes and validation scripts are retained in the package. No Git mutation, network publication, package publication or device action was performed by this agent.
