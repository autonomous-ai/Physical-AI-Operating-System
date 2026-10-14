# d8 focused integration checks

Ready for lamp-voice: no

The 12-stage sequence passed on Third's Intel i7-10700K x86_64 Mac, macOS 26.6.2 / Darwin 25.6.0, Rust/Cargo 1.96.0. All 697 frozen Rust/manifest hashes stayed unchanged. These are post-integration checks on `d8e9556dea3f1867a5af5297d1ebddbd165d9102`, not a complete Voice gate. The failed complete 1a gate remains failed. No lamp, live provider, audio device, camera or motor was used.

The sequence recorded **870 passed test executions, zero failures and two ignored timing probes across 60 completed harnesses**. Filtered suites may overlap; this is not a unique-test total. Durations include compilation and execution and do not measure lamp interaction latency.

| Step | Passed / failed / ignored | Command wall seconds |
|---|---:|---:|
| format | exit 0 | 3.883625644 |
| clippy | exit 0 | 51.395485419 |
| actions-schedule | 193 / 0 / 0 | 160.634666557 |
| reminder | 70 / 0 / 2 | 60.517282027 |
| second-look | 8 / 0 / 0 | 1.346257208 |
| tools | 18 / 0 / 0 | 3.290564397 |
| task-news | 5 / 0 / 0 | 1.354286346 |
| agent-tasks | 5 / 0 / 0 | 1.355172890 |
| early-end | 11 / 0 / 0 | 63.143348768 |
| body | 276 / 0 / 0 | 156.968959568 |
| voice-eval | 284 / 0 / 0 | 900.391516401 |
| workspace-build | exit 0 | 72.982100701 |

The Voice-eval tests use authored fake runtimes and socket-based room fixtures; they play no audio. The ignored reminder parent/transport poll probes have separate earlier host measurements; they were not remeasured here.

Exact sequential commands, unmodified logs, source identities and hashes are in [the original report](focused-d8.json) and [retention index](d8-focused-retention.json). Every command used the reported main workspace directory and `RUST_BACKTRACE=1`. The ordinary full Voice suite, including the five motor waits that failed at 1a, was not rerun in this focused sequence. The isolated default-off comparison and native ARM64 run have their own records; none is inferred from these results.
