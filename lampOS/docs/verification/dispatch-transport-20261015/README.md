# Dispatch fixture shutdown (#823)

Measured on x86_64 macOS, Rust 1.96.0, source `bd37eb8087ff61148b48be889dfb640f6617831d` plus only the fixture shutdown change. Recorded 2026-10-15 19:22:57 +0700 (from `date`). These are local results, not ARM64 measurements.

The 13 selected `final_dispatch` tests retain their exact identities, assertions, fixture evidence and scheduled delays. Twelve stay-open transports previously ran `exec sleep 30`, which ignored stdin EOF and consumed the backend's existing two-second kill fallback during Drop. Their shell now silently reads control lines until the owner closes stdin. Stdout stays open, input is neither echoed nor interpreted, and the shell itself exits for the owner to reap. The one deliberately closing transport is unchanged.

The old independent 30-second fixture lifetime is removed. Current test paths keep their existing observation guards and the backend's two-second forced-cleanup fallback; a permanently suspended live parent that keeps stdin open is not covered by an independent helper timer. No production deadline or test assertion bound is changed. Existing 300 ms staged evidence, injection targets, 50 ms no-event windows, 100 ms lateness threshold and the 1,024-line drain budget remain exact.

## Comparable measurement

Both runs used the same host, toolchain, release profile, target and serial execution, with retries explicitly zero. Other independent compilation work could use the host during these measurements; no controlled unloaded-host or statistical distribution claim is made. This change removes twelve forced-cleanup waits; it does not complete the whole realtime group's clock migration or two-minute target.

| Boundary | Before | After |
| --- | ---: | ---: |
| Nextest execution, all 13 serial tests | 29.938 s | 6.487 s |
| Whole command, shell `date` boundaries | 31 s | 8 s |
| Passed / failed / skipped | 13 / 0 / 0 | 13 / 0 / 0 |

Execution saved 23.451 s. The whole-command boundary also includes Cargo checking and nextest startup; initial compilation was completed by `nextest list` before each timed command. [Raw before](before.log), [raw after](after.log), [exact identities](identities.tsv), [structured results and date boundaries](comparison.json).

Run from `lampOS/`, once on each version:

```sh
cargo nextest list --locked --release --build-jobs 2 -p lamp-voice-eval --test final_dispatch --profile realtime --message-format json
date '+%Y-%m-%d %H:%M:%S %z'
cargo nextest run --locked --release --build-jobs 2 -p lamp-voice-eval --test final_dispatch --profile realtime --test-threads 1 --retries 0 --no-fail-fast --status-level pass --final-status-level fail --color never
date '+%Y-%m-%d %H:%M:%S %z'
```

Compiled list selection required non-ignored, matching tests in the exact `lamp-voice-eval::final_dispatch` binary. Before and after identity files compare byte-for-byte; every selected identity appears exactly once as PASS in each run. Reversing the single helper replacement and its explanatory comment restores the entire original Rust test file byte-for-byte, so all existing test bodies and assertions are unchanged.

| Test | Before (s) | After (s) |
| --- | ---: | ---: |
| `a_final_drain_that_exhausts_its_budget_requires_revalidation` | 2.399 | 0.474 |
| `a_healthy_session_still_dispatches_once` | 2.303 | 0.347 |
| `a_known_invalid_cue_withholds_dispatch_as_a_protocol_failure` | 2.142 | 0.152 |
| `a_known_run_end_cue_withholds_dispatch` | 2.093 | 0.136 |
| `a_known_session_end_withholds_dispatch` | 2.110 | 0.141 |
| `a_relay_framing_error_first_seen_in_the_final_drain_withholds` | 2.901 | 0.949 |
| `a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept` | 2.885 | 0.930 |
| `a_transport_closed_during_the_final_drain_withholds` | 0.881 | 0.911 |
| `an_invalid_cue_first_seen_in_the_final_drain_withholds` | 2.923 | 0.953 |
| `an_unobserved_event_in_the_final_drain_requires_revalidation` | 2.905 | 0.949 |
| `audit_physical_boundary_does_not_dispatch_during_post_exit_trace_relay` | 2.138 | 0.198 |
| `audit_physical_boundary_rejects_a_target_already_over_100ms_late` | 2.162 | 0.189 |
| `the_first_terminal_cause_wins_and_a_provider_failure_stays_provider_unavailable` | 2.094 | 0.149 |

These focused results do not stand in for the whole evaluator package gate. Full gate and native ARM64 results are recorded on the pull request for its exact head. No workflow, profile membership, device or lamp was changed.
