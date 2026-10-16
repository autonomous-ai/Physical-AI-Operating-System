# The motor IPC's clock read after receipt, under load (#914)

Eighth, 2026-10-16. Linux x86_64 laptop, 20 cores.

## What was run

`runtime::supervisor::tests::inert_child_stop_timing_samples_are_host_ipc_only`
is the test that failed in #914. It ran 400 times on each side, 10 runs at a time,
beside 24 busy loops. The two sides were interleaved batch by batch: 40 batches
each, base then fix, so both ran under the same load. `run.sh` is the
script, and `load.txt` holds `uptime` at its start and end.

Both test binaries carry the diagnostic from `eighth/stop-fixture`
(`5dc79db10`, `supervisor/tests.rs` only). It makes `ready()` say why readiness
was lost, including the child worker's own error. It is not part of the fix.
`binaries.txt` records each binary's SHA-256 and its tree:

- **before:** `7a6ce35c2` (`lamp-voice`), plus the diagnostic.
- **after:** `07d9daad8` (`eighth/receive-clock`, the fix), plus the
  diagnostic.

`before.txt` and `after.txt` hold every run's complete output, unedited, in order.

## Results

| Side | Runs | Failed | Readiness lost (#914) | Inhibit ack timed out | Other |
|---|---|---|---|---|---|
| before | 400 | 18 | 9 | 8 | 1 |
| after | 400 | 6 | **0** | 6 | 0 |

- **Readiness lost** is the #914 failure. In all 9 cases on the base, the child inhibited with
  `ControlLost`, and its error was `motor packet boot/order/freshness
  violation`. With the fix it did not occur in 400 runs.
- **Inhibit ack timed out** is `motor inhibit acknowledgement timed out`
  during the test's stop, at production's 20 ms `INHIBIT_DEADLINE_US`. It
  happens at the same rate on both sides (8 and 6). It is the second kind
  #914 named: real scheduling delay over 20 ms with 24 busy loops on 20 cores.
  The fix does not address it and does not change it.
- **Other:** on the base, one run collected 11 of 12 inhibition timing
  samples (`tests.rs:564`), also on the stop path.

The counts above are computed from `before.txt` and `after.txt`. Each run there
starts with `=== <side> batch <b> run <j>`, and a run without
`test result: ok` is a failure.

## Not shown here

- The ARM64 runners, which are in the PR's own CI run.
- The real lamp. Running the motor on lamp-4ace is for the first builder to
  arrange, as #914 asked.
