# Executed-test manifests and crate timing

Each `RUN.tests-passed.txt` has sorted, tab-separated nextest-compatible binary ID and test name. Library target IDs use the crate name; bins use `crate::bin/name`; integration tests use `crate::target`; doctests use `crate::doc`. A single `<custom-harness>` record represents `lamp-ipc::relay_signals`, which only prints a harness-level result. `RUN.tests-ignored.txt` keeps Cargo ignored names separately. `LEAK` counts as passing because nextest reported it that way.

For older Cargo logs, binary headers and result groups are matched by ordinal within each serial job, independently of stdout/stderr interleaving. Each result group’s passing-name count is validated against its Cargo summary. The extracted JSON anomaly lists are empty. Comparing different commits can legitimately add/change tests; a before/after parity proof requires the same source/test tree.

| Run | Commit | Passing manifest records | Ignored names recovered | SHA-256 |
|---|---|---:|---:|---|
| 38700545585 | `3327608771e9` | 4533 | 35 | `a7c7e248da6a5c76e61055053efd2717fed6406d89865359333f1801c06f9bd6` |
| 38697914780 | `33d9c66df2a0` | 4479 | 0 | `7539b376757adbaf1932c90b1bec9cdc254da6b6e88b8d813c018ac62bdb0203` |
| 38697844896 | `c4b68ea583cc` | 4496 | 35 | `033c9940f6a0beb6b5e1c8b1c372c3d874209c4c602a93a7e4e3e9fafed39f2f` |
| 38697522833 | `67e35b206f89` | 4484 | 35 | `0b50262bad499271f0347bec91574e68d2472876216888b7e9bd1e8ac9adcdd1` |
| 38696914090 | `2e8b5aff290a` | 4479 | 35 | `7539b376757adbaf1932c90b1bec9cdc254da6b6e88b8d813c018ac62bdb0203` |
| 38695601860 | `bf0472b55d8a` | 4472 | 35 | `c4966c0b8e24afcbf8cc561521f5357028fd7d868e9125cf7988e8c6b6415309` |
| 38695339133 | `504244b64bcf` | 4559 | 36 | `021bb7459de6a83a0dabadecb2f49c489ed5c1682474b12a63cd3134f284a97d` |
| 38694990477 | `abed767930bc` | 4483 | 35 | `529b6efb453efde91b17db53470076f086c92c2d4a4ef1a22901ea425776b8a6` |
| 38694193893 | `0b11d6857e39` | 4491 | 35 | `8ce1c8e542d0902060667be77ebc940b74a42184604d87a7f2d21d2d0ef43be0` |
| 38693725124 | `a2804888610e` | 4479 | 35 | `7539b376757adbaf1932c90b1bec9cdc254da6b6e88b8d813c018ac62bdb0203` |

## Per-crate test time

Reference 38697914780 uses sums of nextest individual test durations, plus Cargo doctest summaries. These are execution sums, not elapsed runner time; nextest per-process overhead is reflected in the shard’s elapsed summary. Older single-runner runs use Cargo’s per-target `finished in` summary sums. The custom process harness adds its printed per-case milliseconds; `relay_signals` has no printed duration and is excluded from the time sum.

| Crate | Reference 38697914780 seconds | Reference manifest records | Serial 38700545585 seconds | Serial manifest records |
|---|---:|---:|---:|---:|
| lamp-actions | 10.849 | 34 | 10.720 | 34 |
| lamp-air | 0.017 | 5 | 0.000 | 5 |
| lamp-audio | 11.117 | 179 | 10.420 | 179 |
| lamp-body | 1.320 | 290 | 0.380 | 290 |
| lamp-bus | 10.445 | 65 | 10.240 | 65 |
| lamp-camera | 0.461 | 82 | 0.200 | 82 |
| lamp-controls | 3.096 | 372 | 2.040 | 372 |
| lamp-ears | 0.037 | 10 | 0.000 | 10 |
| lamp-environment | 0.036 | 12 | 0.000 | 12 |
| lamp-expression | 3.472 | 228 | 4.320 | 228 |
| lamp-eyes | 0.187 | 30 | 0.080 | 35 |
| lamp-gemini | 20.647 | 255 | 19.630 | 255 |
| lamp-health | 14.364 | 55 | 14.110 | 55 |
| lamp-ipc | 0.091 | 20 | 0.030 | 20 |
| lamp-keeper | 0.105 | 21 | 0.030 | 21 |
| lamp-lean | 0.256 | 61 | 0.040 | 61 |
| lamp-memory | 44.139 | 149 | 44.000 | 149 |
| lamp-mood | 1.622 | 95 | 1.290 | 95 |
| lamp-msgs | 0.237 | 76 | 0.000 | 76 |
| lamp-name | 0.036 | 9 | 0.000 | 9 |
| lamp-observer | 0.200 | 46 | 0.040 | 46 |
| lamp-policy | 32.445 | 144 | 31.700 | 144 |
| lamp-records | 5.642 | 115 | 5.310 | 115 |
| lamp-ring | 0.135 | 22 | 0.060 | 22 |
| lamp-schedule | 7.584 | 161 | 7.190 | 161 |
| lamp-sound-fixtures | 0.111 | 15 | 0.080 | 15 |
| lamp-surround | 7.738 | 260 | 9.010 | 260 |
| lamp-tasks | 7.810 | 104 | 8.080 | 107 |
| lamp-turn-end | 0.207 | 29 | 0.090 | 29 |
| lamp-turn-end-wire | 4.445 | 16 | 4.439 | 16 |
| lamp-turn-owner | 0.123 | 37 | 0.000 | 37 |
| lamp-update | 1.330 | 44 | 1.260 | 44 |
| lamp-voice | 311.193 | 1154 | 213.030 | 1200 |
| lamp-voice-eval | 674.020 | 284 | 673.380 | 284 |

The reference’s 4,479 passing manifest records are 4,462 nextest cases + 4 doctest cases + 12 custom process cases + 1 custom relay harness record. Its 35 ignored tests are not printed by default in nextest logs, so this source alone does not give their names. Those skipped test identities need a machine-readable nextest list from the same test tree.

## Existing serial-to-four-runner parity

The executed-test manifest of four-runner run 38697914780 is identical to both serial runs 38696914090 and 38693725124: 4,479 unique records, no additions or removals, SHA-256 `7539b376757adbaf1932c90b1bec9cdc254da6b6e88b8d813c018ac62bdb0203`. The Cargo parser normalizes the ` - should panic` presentation suffix and uses the Cargo target name, including integration targets with a nested `main.rs`. Newest serial run 38700545585 has 54 extra cases and no missing cases relative to the reference. These comparisons prove the emitted test identities, not identical test source across differing commits.

`crate-times-all-ten.csv` and `crate-test-counts-all-ten.csv` contain every crate for all ten successful runs. Durations are the logged sums defined above, with rounding inherited from the test harness.
