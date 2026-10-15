# Full ARM64 inventory audit

Observed using `date`: 2026-10-15 21:44:44 +0700.

[Run 38737583302](https://github.com/autonomous-ai/lampOS/actions/runs/38737583302) succeeded at exact head `a25450bd742cca4108920d1dff5bc5fc9e7a6c4e`. It used the default profile, one test at a time per shard, across four hash partitions.

**Correct nextest total: 4,633 unique passing identities: 4,631 PASS and 2 LEAK.** The two warnings are retained below. Every per-shard record index is present, with no duplicate identity. All **123/123** frozen binary/name identities ran: 121 PASS and 2 LEAK, with none missing. The **13/13 final_dispatch identities all PASS**, totaling **5.226 s** of printed per-test durations.

| Binary | Expected / present | PASS | LEAK | Sum of test durations (s) |
| --- | ---: | ---: | ---: | ---: |
| `lamp-actions::server` | 1 / 1 | 1 | 0 | 4.110 |
| `lamp-health::program` | 1 / 1 | 1 | 0 | 5.009 |
| `lamp-memory::health` | 7 / 7 | 7 | 0 | 36.072 |
| `lamp-records::records` | 1 / 1 | 1 | 0 | 5.009 |
| `lamp-voice` | 63 / 63 | 61 | 2 | 137.449 |
| `lamp-voice-eval` | 1 / 1 | 1 | 0 | 5.011 |
| `lamp-voice-eval::final_dispatch` | 13 / 13 | 13 | 0 | 5.226 |
| `lamp-voice-eval::physical_loopback` | 12 / 12 | 12 | 0 | 235.776 |
| `lamp-voice-eval::serve_relay` | 6 / 6 | 6 | 0 | 31.111 |
| `lamp-voice-eval::serve_session` | 14 / 14 | 14 | 0 | 369.741 |
| `lamp-voice::capture_hold_worker` | 4 / 4 | 4 | 0 | 13.935 |

The frozen-group sum is **848.449 s** across separate shards. It is not one measured serial-group wall time and does not establish the <120 s target. This compares identity coverage, not assertion/source equality for every frozen test. Other builders changed upstream clock fixtures between the inventory baseline and this head; no aggregate reduction is attributed to the dispatch change.

The nextest total excludes doc tests and the two own-harness binaries. The separate step succeeded; its log says `relay_signals: ok` and the process harness reports 12 passed, 0 failed. Do not sum partition skip totals or label the arithmetic remainder of 35 tests as ignored without an enumeration.

## Two endpoint-session leak warnings

Both use binary `lamp-voice`; both are in the frozen 123. Raw log records:

```text
LEAK [0.717s] shard 1/4 coordinator::tests::endpoint_session::unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service (full-arm64.log:1977)
LEAK [4.881s] shard 3/4 coordinator::tests::endpoint_session::a_worker_still_loading_leaves_the_baseline_window (full-arm64.log:9263)
```

The source below was read with `git show` at the tested head, not from a later branch. This is a static explanation consistent with the warnings; the run log has no PID/process-tree trace proving which inherited descriptor survived.

1. `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service`: `coordinator/tests/endpoint_session.rs:1325` deliberately starts `exec sleep 60` with a 1,500 ms classifier grace. The runtime cleanup waits at least 500 ms (`coordinator.rs:718`), and the assertions correctly require `reaped == false`, `Pending`, and no restart. The test then returns without a final fixture reap. `ProcessClassifier::drop` only calls close and does not wait (`turn-end-wire/src/client.rs:341`); its detached supervisor would perform kill/wait only after the 1,500 ms grace, but a nextest test process can exit first. `client.rs:695` gives the child inherited stderr. Thus the simulated unconfirmed teardown is intentional; leaving that fixture alive when the test ends is a separate cleanup defect, not evidence that LEAK is harmless.

2. `a_worker_still_loading_leaves_the_baseline_window`: `endpoint_session.rs:1411` runs a 4 s session against `/bin/sh -c` with `sleep 6; exit 0` (`scripted_process`, line 939). Unlike `exec sleep`, the shell can own a separate sleep descendant. The supervisor kills/waits only its direct Child (`client.rs:655`), not a process group; the sleep descendant can retain inherited stderr after the shell is reaped. The test proves loading falls back to the unchanged 1,200 ms silence window, but neither its assertions nor the helper join that descendant. That is a second fixture cleanup defect consistent with the LEAK.

The production `WorkerCommand::smart_turn` has a separate parent-death contract via `--parent`; the ad hoc shell fixtures do not use that constructor. This audit does not establish a production-worker leak.

## Proposed issue

**Title:** lampOS: reap endpoint-session fixture children after expected teardown failures

Full ARM64 run 38737583302 at a25450bd742cca4108920d1dff5bc5fc9e7a6c4e passes with the two LEAK records above. These are observed evidence from this run, not clean cleanup results and not a reason to retry. Preserve both test identities and all existing assertions/timeout boundaries.

- For the intentional unconfirmed-exit case, retain a test-owned Monitor before moving the EndpointRig into the helper. After asserting the original Pending/error/no-restart result, wait with a bounded host cleanup guard and require the supervisor to reap its worker before the test returns. Cleanup must also run if an assertion panics. Do not change the 500 ms runtime bound or 1,500 ms grace that creates the scenario.
- For the loading case, use an owned single process or explicit descendant cleanup. `exec sleep 6` is a candidate because it keeps the original 6 s no-Ready behavior while making the sleeper the owned child; verify unchanged 4 s conversation and 1,200 ms fallback assertions.
- Verify both cases once in nextest with their original assertions, confirm PASS without LEAK and bounded cleanup even on an injected assertion failure, and retain the failing run as evidence. Do not suppress inherited stderr, raise the leak timeout, ignore tests, or add flaky retries.

The supplied `/private/tmp/tests-leak-issues.json` currently contains an empty array; its search scope is not recorded here. The parent should check for an existing owner issue before filing.

No tests, builds, Git mutations, repository edits, or network calls were made for this audit. Scratch source snapshots and JSON evidence are in this directory.
