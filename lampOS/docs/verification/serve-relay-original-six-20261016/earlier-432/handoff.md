# Serve-relay migration directly onto Serve coordination

Prepared 2026-10-16 04:04:01+0700, from `date`. **Yes: this migration can be ported directly onto the Serve-only tree; Recorder coordination is not a dependency.** This is a scratch patch and source audit, not an applied change or new runtime qualification. Neither repository worktree was edited. No builds, tests, Git mutations, network or devices were used. Read-only `git apply --check` passes against the exact target below.

Target: `/private/tmp/lampos-tests-serve-coordination`, amended head `e057db620acb2bd61bac1ae7f157d87cdf635a18`, base `48b3737d5e83efa00278172cb602d3525d368a43`. Its implementation source equals the earlier reviewed `cce8f3cc6` source. Root should create an isolated tree from this target before applying `participant-only.diff`; this plan does not edit the shared Serve branch.

Qualified source: `/private/tmp/lampos-tests-serve-relay-progress`, seeded onto `fb54b25028a25d3767f968fcc354f2682fe34aa2` from Recorder coordination. All **73** current Rust files match the authoritative `root-final-source.json`; its full raw gate has **467/0/0**, 25 summaries, and all six original relay scenarios PASS. I used the final documentation hash `b56221b651472d4f5df878f4e0e192ffb73438a26b8be3fb9e9e43318c7cb191` confirmed by `root-watchdog-clarification.json`, not the earlier handoff's wording. Prior gate success does not qualify the newly composed tree.

## Exactly six patch paths

- `physical/fake_lamp.rs`: port only the controlled invocation/argument-view region and its new test-module declaration. Its real received vector is reported once after successful flush, while only the validated suffix is parsed. The original direct controlled function's live body reconstructs byte-for-byte from the moved shared body. The entire current pure SlowTransport function region, real exec path, and existing SlowTransport test-module declaration remain exact.
- `physical/fixture_commands.rs`: only retain the original complete received argument vector and call the invocation adapter in `serve_runtime`. No Recorder function/import/module, new CLI dispatch, protocol role or field is copied.
- New `physical/serve_invocation_tests.rs`: exact qualified bytes; four invocation boundary checks.
- `tests/serve_relay.rs`: exact qualified helper/module/import migration. All six original scenario bodies and 37 assertions remain exact.
- New `tests/common/serve_relay_clock.rs`: exact qualified bytes; current-thread relay/runtime scheduling and six helper boundary checks.
- `docs/realtime-tests.md`: append only the final corrected “Serve relay test coordinator” section. Every current doc byte, including the pure SlowTransport and Serve coordination sections, remains intact.

| Path | Target SHA-256 | Proposed SHA-256 |
| --- | --- | --- |
| `crates/voice-eval/src/physical/fake_lamp.rs` | `7707b987700293a31096338ccd4364e34aa70bce59bca2dd3c1cdaef15c0c9a1` | `b9637518aaf8e0dffc587273ab0c5e19c6ff1d72337796c71a4d0832cb4f1327` |
| `crates/voice-eval/src/physical/fixture_commands.rs` | `c755e44894013fa6ff40dc107b0d5a13ae2fea2235b6bd04a4fa4d8b68ba4474` | `7d68389f86d879d4fe9e64f925d169641d25ef0c8d9b62b0a9f062b2f2d23fc8` |
| `crates/voice-eval/src/physical/serve_invocation_tests.rs` | `new file` | `89f89a010c4a240aa699fd37fb49e0a2c4f2ed8f65ce77332c5c9b575e3d12dc` |
| `crates/voice-eval/tests/serve_relay.rs` | `53f5cc802a0101518da7214b63b77e052b3406fe1b5db3ce43d5c0c305145bf5` | `836a72136db9573ca12e8b4aeab77ff95e42010cf71fcd652da6318945ec7bf2` |
| `crates/voice-eval/tests/common/serve_relay_clock.rs` | `new file` | `60f5ee58ee905b285ea54c5d6c1a2159964e97a2945a79654d283ca14d0ee9cb` |
| `docs/realtime-tests.md` | `eb3f3e2b8beb25d5320d060507444d49b87ab3926422779ce15a038a5d487eef` | `0ba58a3b6399264eff127716af636fbac195e93801697edb0b041fdb4ae41edb` |

Patch SHA-256: `703ea63c313a37d5de9bb55e1877733862b64650201d3d77cdcdaa47b4760d10`. The resulting three whole-file differences from the old qualified versions are intentional: `fake_lamp.rs` retains the newer pure SlowTransport seam; `fixture_commands.rs` excludes Recorder additions; docs retain today's full base and append only the relay section. The two new files and migrated `serve_relay.rs` equal their qualified files exactly.

## Direct Serve dependency

The helper needs `ParentListener::bind/accept_until`, `ParentPeer::step_until`, `Role::ServeRuntime`, `Progress::ServeRuntime`, `Step.wait_for_sigterm`, and `session::run_with_controlled_clock` with `RelayPollProgress.stop_signal_sent_us`. Serve coordination already provides these. Its main dispatch already exposes `fake-clock-serve-runtime`. The session, fake-Serve engine, Serve relay progress tests, and shared integration helper are byte-identical to the qualified relay tree. The saved `session.diff` describes the existing Serve signal receipt addition and is **already present**; do not replay it or edit `session.rs`.

There is no use of Recorder roles/progress, recorder backend hooks, `ClockedServeFixture`, SlowTransport scheduling, or Recorder CLI dispatch in either new file. A `let Progress::ServeRuntime { .. } = ... else` match works with the existing finite/Serve enum. The test invokes the controlled session directly on its own thread, so it needs neither the Recorder-enabled driver nor a separate relay participant. No manifest/dependency change is needed.

The publication dependency is therefore **Serve coordination (#909)** and its existing prerequisites. Root may prepare and gate this stacked local tree now; a later PR into `lamp-voice` must contain only this relay delta after the Serve source is available there. Recorder coordination can remain separate.

## Preservation and expected gate

The entire original six-test tail equals the qualified tail, SHA-256 `8040e3cbda58f22296ef057df328d8c3017fde6afc162954852ec675daff4ce2`. Per-body assertion counts remain **11 / 4 / 4 / 7 / 6 / 5**, total **37**. Independent reversal recreates the full current original `tests/serve_relay.rs` (SHA-256 `53f5cc802a0101518da7214b63b77e052b3406fe1b5db3ce43d5c0c305145bf5`). The complete three-second Stop/Close waits, one-second run plus five-second allowance, ten-second ignored-signal grace, fault settings, actual argv equality and original assertions remain unchanged.

The target's current compiled list contains **422** evaluator tests: 220 library cases and six in `serve_relay`. This patch adds **four** library invocation tests and **six** helper tests in the relay binary, without replacing any original. Expected complete result inventory: **432**, with **224** library tests and **12** relay-binary tests. This is a source-derived expectation, not a test result. Root must confirm the compiled list and fresh full gate; the prior Recorder-seeded 467 count must not be reused as this tree's count.

`source-audit.json` records every proposed path's before/after hash, all current non-owned evaluator Rust/manifest/lock/doc hashes, six per-body hashes, exact ten new identities, dependency hashes, and the successful non-mutating patch check. It confirms both worktrees' source preservation. The patch changes no integration test other than `serve_relay.rs`, no main dispatch, protocol, session, driver, backend, manifest, lockfile, or preexisting test module.

## Runtime and evidence limits

The same current-thread helper uses actual control receipts, runner Stop bytes/EOF, the original runtime signal handler, and the session's real `Child::try_wait` or full grace-expiry kill/wait. A runtime logical-end receipt is not OS-exit evidence. Terminal callback return leaves the original final drains, group checks, metadata validation and SessionEnd work intact. The helper's mock progress unit tests prove scheduling boundaries; the unchanged six real child scenarios retain the actual argv/signal/kill/reap assertions.

The retained **ten-second post-signal success budget is checked before and after polls**. It is not a strict ten-second abort guarantee: the unchanged reader-enqueue barrier can take **up to thirty seconds** to return a failing poll; existing group-check and late-output work also remain. The corrected documentation is carried exactly. A source-only port and the old qualified gate establish no new speedup, ARM64 pass, or complete original-123 result. Fresh isolated-tree gates, paired timing if claimed, and native qualification remain root's work.
