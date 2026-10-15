# All original Serve-session cases: source integration handoff

Recorded from `date`: 2026-10-16 05:20:10 +0700. Source is stable in `/private/tmp/lampos-tests-serve-all-cases`, assigned branch `tests/823-serve-all-cases`, on unchanged current base `d60124f8280f9c76576d6ca7bf859bc5c3983828`. Root explicitly released source edits after its clean baseline compiled list completed at the recorded `05:07:43`. No Cargo/build/test, Git mutation, network or device command was run by this worker. Root owns fresh execution, same-family measurements, review and publication to the existing draft #909.

## Integrated source and current-base preservation

The complete change is **25 evaluator Rust files plus `lampOS/docs/realtime-tests.md`**: **12 existing modifications and 14 new Rust files**. No scripts, workflow/profile changes, old verification directories or binary artifacts were copied into the repository. The six direct `tests/serve_relay.rs` cases remain untouched.

The current base has no evaluator Rust or realtime-document delta from the previously qualified Recorder base `48b3737d5e83efa00278172cb602d3525d368a43`. `preflight.json` pins the exact 18 published Serve Rust blobs from `e057db620acb2bd61bac1ae7f157d87cdf635a18`; all match the reviewed seed. The ten-file Recorder stage then matches the qualified Recorder source exactly. Finally, the exact nine-file `participant-only.diff` from `/private/tmp/tests-823-slow-transport-onto-recorder-plan` applies without fuzzy matching and reproduces the qualified SlowTransport source. No design or failure fix was added.

All **75 resulting evaluator Rust files** match the retained historical qualified SlowTransport source manifest, including inherited files. Every one of **6,729 nonowned tracked entries** retains exact bytes or symlink target and mode. This includes the latest Voice/second-provider, Msgs, Memory and Selftest changes, all Cargo/lockfiles, fixtures, workflows, profiles and earlier verification material. The complete pure Recorder/SlowTransport suffix of `fake_lamp.rs` remains byte-exact from the current base; its separate pure-clock test modules also remain unchanged.

`inherited-base-delta.txt` records the 70 changed paths between the older dependency base and current base. Relevant compilation inputs include newer Voice source/manifest, Msgs source, Memory health/reporting source and Selftest source. Source equality within Evaluator therefore does **not** qualify the new dependency inputs. These fresh gates are not a retry of the unchanged earlier #914 source, and this integration establishes neither that failure's cause nor a fix for it.

## Coverage and exact reversals

`original-serve-preservation.json` compares the original frozen `base-a3dba0c0a.serve_session.rs` with the current base and integrated result. The current base equals that original file. All **14 complete test bodies, 114 assertion macro byte strings, assertion helpers, 18 source run calls and 22 expanded attempts** remain exact. The five-fault loop is unchanged. The sole scenario change replaces the shared fixture constructor, import and introductory comment; reversing those explicit substitutions restores the original file byte-for-byte. All fourteen identities and twenty-two attempts now select the controlled fixture; there is no conditional host fallback in this helper.

`test-source-preservation.json` also audits the entire evaluator test source: **376 of the 377 existing test bodies are byte-exact**. The one required compatibility adaptation, already present in the published Serve seed, adds only `stop_signal_sent_us: None` to the existing `RelayPollProgress` literal in `fixture_commands::tests::relay_terminal_progress_keeps_the_last_real_poll_and_final_output_count`. Removing that single field restores the entire old test body, including its assertion. The existing `fixture_control_tests.rs` non-test Step helper adds only `wait_for_sigterm: false`. Between the Recorder and Slow stages, the new Recorder test helper adds only `Role::SlowTransport` to an unreachable exhaustive enum arm. These are documented rather than represented as whole-file byte identity.

The new source contributes **121 cases: 45 Serve + 41 Recorder + 35 SlowTransport**. Root's clean compiled baseline is **377 cases, zero ignored**. `baseline-compiled-identities.json` extracts that actual list. `expected-after-identities-source-only.json` combines it with the actual new Rust module declarations: **296 library + 202 integration = 498 expected**, zero ignored. This is a comparison target for root's fresh compiled list, not an after-build or execution result.

`final-source-proof.json` independently reverses and reapplies each staged patch from the actual final worktree bytes, and also reverses/reapplies the complete current-base delta. All prior snapshots and the current base are exact; newly introduced paths reverse to absence. The complete current document remains an unchanged prefix, followed only by the integrated Serve/Recorder/SlowTransport explanation. `reversed-to-current-base/`, `stage-provenance.json` and `current-base-to-integrated.diff` retain the reviewable evidence.

## Preserved completion boundaries

The controlled entry points retain the original 2 ms polls, actual pipe/cue/datagram consumption, real prepared readiness/grants and 100 ms lateness bound. A continuing relay's actual SIGTERM-send receipt requires signal-handler observation at the next original poll; after a completed terminal relay no runtime Step is sent, including forced-kill paths. Final metadata/output and actual child exit/reap remain separate from poll progress.

Recorder readiness follows the real ready-file write and its original backend parser/15 s guard. The three controls retain full 6 s, 6 s and 23 s lifetimes; declared capture end at 20 s does not establish exit. Terminal Recorder progress holds logical time until the backend observes its owned Child exit. Recorder and player drain ordering remains enforced after relay completion.

SlowTransport retains the entire 400 ms wait and 2300 ms first-preparer delay, inherited streams and original arguments. It closes the completed participant before real exec, then the controller requires fresh actual ServeRelay registration before bootstrap. Terminal delay Ack is not exec/readiness/exit proof. The cumulative startup host deadline cannot reset across rounds; earlier Recorder due times and shorter preparation deadlines still win. No old duration, fault, acceptance predicate, retry or skip was changed.

## Local checks and limits

Pinned Rust **1.96.0** `rustfmt --check --edition 2024 --config skip_children=true` passed all **25 owned Rust files**, exit **0**, with empty output; exact argv, cwd, shell-date timestamps and raw hash are in `worker-rustfmt.json`/`.log`. Final hashes were rechecked afterward. All source, inverse, scope and identity assertions described above passed. Root's compiled baseline is the only compilation result produced for this current-base integration so far by the audit inputs; no fresh execution or latency claim is made here. Historical 498-pass results remain outside this tree and are not represented as current qualification. Native ARM64 and the complete original 123-test target remain separate requirements.

## Exact owned paths and final hashes

| State | Path | SHA-256 |
|---|---|---|
| modified | `lampOS/crates/voice-eval/src/main.rs` | `1751caa8d8246f8217674397c2c979bab40dc6eaa3d81834784e88cc29ef6607` |
| modified | `lampOS/crates/voice-eval/src/physical/fake_lamp.rs` | `7707b987700293a31096338ccd4364e34aa70bce59bca2dd3c1cdaef15c0c9a1` |
| modified | `lampOS/crates/voice-eval/src/physical/fake_serve.rs` | `5168b08f121fc374972f9a1bb10f86e30396c83c17ee12c2ea9858cd74ef780d` |
| new | `lampOS/crates/voice-eval/src/physical/fake_serve_progress_tests.rs` | `43af2f78890ec502bc3d12cd95214bf2aaa65578eeac1e9abaa1a6c4d0611f7a` |
| modified | `lampOS/crates/voice-eval/src/physical/fixture_commands.rs` | `df424358eb99ab6982ef8cd3ea3d7484f55bf3c033aabcf29ca0b07c0df5ab0a` |
| modified | `lampOS/crates/voice-eval/src/physical/fixture_control.rs` | `fd764a6a67b3c83295e3fad677a39ef0c6a10a70a7305f9d92aa2fe10d64daf2` |
| modified | `lampOS/crates/voice-eval/src/physical/fixture_control_tests.rs` | `90dab0a1075f0fbb819d819f391aede2994363c3238e54445a5a73e5f12f604a` |
| modified | `lampOS/crates/voice-eval/src/physical/fixture_driver.rs` | `3e4a99a9e5029b7c770b2420c4df70271c5702e30c6f97a3dcfc954fce1871d9` |
| new | `lampOS/crates/voice-eval/src/physical/fixture_players.rs` | `2378acf5a32c53b013cc90b96d827bdb82307dbb054d7c94e30ede8752dc5dab` |
| modified | `lampOS/crates/voice-eval/src/physical/mac.rs` | `c14c40669ad423baf9c21672587bd6e97e47a64e102d6d22c2714e3e399d78e2` |
| modified | `lampOS/crates/voice-eval/src/physical/mod.rs` | `c55ab4bd4fad65b806f4e66f533d7334f947c25c87d8d072ce69ff848db2e582` |
| new | `lampOS/crates/voice-eval/src/physical/prepared_player_progress_tests.rs` | `1ac1f3a24b267c0968fe291befbd65739c06a1baf4e2f5c9b24eca8f841055b7` |
| new | `lampOS/crates/voice-eval/src/physical/recorder_backend_tests.rs` | `a9ed66168aea14d40b3ae28a17c46450ca8e966e6d6f46f550565daea2e7bbde` |
| new | `lampOS/crates/voice-eval/src/physical/recorder_fixture_commands_tests.rs` | `500cd9e92ed12a7328f67d99c60d68105f5130f31d8db2ecd76265fd5fb01d37` |
| new | `lampOS/crates/voice-eval/src/physical/recorder_fixture_control_tests.rs` | `3e9c7eb09f574cbda46a06fb9ac452993df5a9b4417c5f0d9691f9deba27cf80` |
| new | `lampOS/crates/voice-eval/src/physical/recorder_fixture_driver_tests.rs` | `da5c382c25fd8d63f7fac9e3ff17d6caff216df95ae2289af40da6c1225c3ff1` |
| new | `lampOS/crates/voice-eval/src/physical/serve_fixture_commands_tests.rs` | `54f60e0f676d94989b5220f707f9c5c085cbec83de0aa0e0a3d4ba5f892642ea` |
| new | `lampOS/crates/voice-eval/src/physical/serve_fixture_control_tests.rs` | `fdb57e00dd9e3028ae6f5a5aa6df81d1147bee4fc582ad00b117368951b80bcf` |
| new | `lampOS/crates/voice-eval/src/physical/serve_fixture_driver.rs` | `3ba9b3405c82198e4c56e4baa3c982be74d8a2e0588766be11c34a2fa62e8014` |
| new | `lampOS/crates/voice-eval/src/physical/serve_relay_progress_tests.rs` | `d7ccf7f0ddcd46a07d9d6cfc768d5126d507c133b0b708dc69881f124f238ed4` |
| modified | `lampOS/crates/voice-eval/src/physical/session.rs` | `660cea49d4e1389ad23e5786a5efded083e7f6d98912dfc9ad9887dad85ccd24` |
| new | `lampOS/crates/voice-eval/src/physical/slow_transport_fixture_commands_tests.rs` | `dee37c32bd463a6603ab0b2f1dc5f0b2a9ae4b62da98b26a9b9aea560b7ef7c1` |
| new | `lampOS/crates/voice-eval/src/physical/slow_transport_fixture_control_tests.rs` | `1032118bfae8aec9a555f87ca6f3f334c16599c361a90b38768d5c1d454d3359` |
| new | `lampOS/crates/voice-eval/src/physical/slow_transport_fixture_driver_tests.rs` | `0026046f596309aa8cc2698de1893825d0b4d006e5f7951a5af81d1158a85187` |
| modified | `lampOS/crates/voice-eval/tests/serve_session.rs` | `1850248165b8c7d5022ec06f3267b5de1a829fa375135cdda26aa3bfe539b905` |
| modified | `lampOS/docs/realtime-tests.md` | `ba461e398f74366f6eee76616e8569a932f591556ab811582952b187089b3f0d` |
