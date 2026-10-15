# Serve fixture clock source (#823)

Updated 2026-10-16 00:42:02 +0700, from `date`. Rebased source head: `21390597fa569a068b8ac7b5e8720641dd3773ce`, on `6b08a70d56e37b061e7a9a6b013072b13639a0b7`. The original full-gate record was written 2026-10-15 23:33:18 +0700 on base `a3dba0c0af457774e5fe7e09af42828c311738b9`.

`fake_serve::run_with_clock` accepts a pure logical clock. The existing `run` and CLI still use host monotonic time. All nine original samples keep their order, including separate metadata and cycle-origin reads. The 2 ms host polling, real signals, room/cue sockets, fault behavior, metadata flushes and stdout remain unchanged. This clock-source change migrates no existing test and claims no speedup.

Four new helper tests cover frozen reads, distinct startup samples, restart identity and retained startup readiness's original 150 ms age/saturation. They do not install process-wide signal handlers. [Source audit](source-audit.json) reconstructs the full prior file byte-for-byte and preserves all 23 integration sources, including every serve-session assertion; its source-only gate note predates the completed gates here.

The [rebase proof](rebase-20261016.json), recorded 2026-10-16 00:31:36 +0700, compares the previous head `5020ed55a373e2cd57c25dc5f45a1924685ad85c` with the rebased head. The two implementation/test-helper files and all 23 integration source files are byte-identical across that rebase. Every recorded SHA-256 was also checked against this worktree. Upstream documentation and this change's complete appended section were retained.

Fresh checks on the rebased source passed:

| Command, from `lampOS/` | Result |
| --- | --- |
| `cargo fmt --all --check` | Exit 0 |
| `cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings` | Exit 0 |
| `cargo test --locked --offline -p lamp-voice-eval --lib -- --test-threads=1` | 98 passed; 0 failed, ignored or filtered out |
| `cargo build --locked --offline --workspace` | Exit 0 |

These checks ran from 2026-10-16 00:32:47 to 00:35:31 +0700, after a package-cache reset at 00:32:44–00:32:47. The library run covers the combined finite-fixture, backend-clock and serve-clock helpers, including all four new serve-clock cases. [Exact commands, target directory, exit codes and `date` boundaries](rebased-20261016-gates.json) and the unedited `rebased-20261016-*.log`, `.started`, `.finished` and `.exit` files are adjacent. This fresh test gate is the library suite; the full evaluator integration suite was not rerun after this rebase.

The original full evaluator gate passed **296 tests, zero failures or ignores**, from 2026-10-15 23:05:32 to 23:19:55 +0700. It included all 14 serve tests and all 12 physical loopbacks on their original host clocks, plus formatting, strict all-target clippy and workspace build. Its [commands and source hashes](gates.json) and [raw test log](tests.log) remain unchanged. The reviewer also independently confirmed 296 passes on [PR #875](https://github.com/autonomous-ai/lampOS/pull/875). These are the original full-suite results, not a fresh 296-test gate on the rebased head.

The earlier [native ARM64 run 38769997069](https://github.com/autonomous-ai/lampOS/actions/runs/38769997069) passed on **previous head `5020ed55a373e2cd57c25dc5f45a1924685ad85c`**: 4,745 passed and 126 existing profile skips, including all four new serve-clock cases. Its unedited [run metadata](prior-arm64-38769997069.json) retains that head SHA. **Native ARM64 for the rebased head is pending.** No retry, workflow change or device access was used for these local rebase checks.

A later fixture must coordinate prepared players, recorder/transport progress, signal observation and actual child exit before moving serve scenarios to logical time. A clock source alone proves no end-to-end speedup.
