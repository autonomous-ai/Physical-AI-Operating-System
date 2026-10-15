# Local before-change run

Source `8ddffb6290557fbfd28ea02c8982d16cd5d0743a`; platform `x86_64-apple-darwin`, verified from nextest build metadata and `rustc -vV`. Start `2026-10-15 18:19:01 +0700`, finish `2026-10-15 18:35:07 +0700` (both from `date`).

One release run, one test at a time, no retries: **119 run, 116 passed (two leaky), three failed**, 1,032 filtered out. Nextest reported **964.984 s** and exit code **100**. All 119 compiled selected identities appear in the results exactly once after removing the final failure-summary duplicates. The four Linux-only capture tests are not supported on this host and remain in the ARM64 inventory.

This failed run is evidence of the existing timing problem, not a successful performance baseline. The failing tests exited early, so its aggregate duration must not be compared as an equivalent passing-suite speedup. It is separate from the two original ARM64 runs. The source was unchanged and no clock patch was present.

```sh
cargo nextest run --locked --release --build-jobs 4 -p lamp-voice -p lamp-voice-eval -p lamp-memory -p lamp-health -p lamp-records -p lamp-actions --profile realtime --test-threads 1 --no-fail-fast --status-level pass --final-status-level fail
```

| Binary | Run | Passed | Failed | Leaky | Sum of test durations |
| --- | ---: | ---: | ---: | ---: | ---: |
| `lamp-actions::server` | 1 | 1 | 0 | 0 | 4.178 s |
| `lamp-health::program` | 1 | 1 | 0 | 0 | 5.748 s |
| `lamp-memory::health` | 7 | 7 | 0 | 0 | 37.882 s |
| `lamp-records::records` | 1 | 1 | 0 | 0 | 5.761 s |
| `lamp-voice` | 63 | 60 | 3 | 2 | 230.931 s |
| `lamp-voice-eval` | 1 | 1 | 0 | 0 | 5.042 s |
| `lamp-voice-eval::final_dispatch` | 13 | 13 | 0 | 0 | 29.894 s |
| `lamp-voice-eval::physical_loopback` | 12 | 12 | 0 | 0 | 239.226 s |
| `lamp-voice-eval::serve_relay` | 6 | 6 | 0 | 0 | 31.334 s |
| `lamp-voice-eval::serve_session` | 14 | 14 | 0 | 0 | 374.963 s |

Failures tracked with the existing host-timing issue [#739](https://github.com/autonomous-ai/lampOS/issues/739):

- `lamp-voice::coordinator::tests::early_end::without_the_option_every_message_and_record_is_what_it_was` (0.583 s).
- `lamp-voice::coordinator::tests::endpoint_session::a_worker_failure_falls_back_through_recovery_and_then_resumes` (3.495 s).
- `lamp-voice::coordinator::tests::pilot_faults::sigterm_while_metadata_storage_is_stalled_stops_cleanly_and_reports_the_loss` (2.001 s).

The first two reported physical privacy closed or lost freshness; the third reported an invalid or stale service privacy observation. The raw output also retains the forced shutdown and trace-thread diagnostics. No rerun was used to hide these outcomes.

Raw output: [local-before.log](local-before.log). Exact identities, outcomes and times: [local-before.json](local-before.json).
