# Provider capacity fixture clock (#823)

Recorded 2026-10-15 20:01:42 +0700, from shell `date`. Base `37b8be9ea36042a4d40dbf5bf82c81a8f87f6098`; x86_64 macOS, Rust 1.96.0. These are local fixture results, not ARM64 or device latency.

`provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity` now runs on a paused current-thread Tokio runtime. The modeled speaker and scripted service use Tokio Instant for elapsed time; the provider-to-parent Unix transport, real packet flow, actual provider worker, authority heartbeats and OS capture/freshness checks remain. Production code keeps its clock and all bounds. Tokio's test utility is enabled only as a pinned dev feature; no dependency version or lockfile changes.

All asynchronous WebSocket peers use bounded in-memory duplex streams on that same runtime. Its existing Tokio worker ticks and Gemini protocol timers also follow paused time; OS monotonic time, std Instant, IPC freshness and authority leases remain real. Existing timers and complete input/Ready/Started acknowledgements govern progress; the test does not jump time with `advance`, replace the worker, or use a sleep as an acknowledgement. Tokio auto-advances only when no task is runnable. Moving a peer onto another thread/runtime or external socket would require explicit progress coordination before retaining this fixture-clock design.

## Comparable execution

The same exact test ran once per version after compilation, in release, one test at a time, retries zero. Other independent builds could use the host; these are not unloaded-host or statistical distribution measurements.

| Boundary | Before | After |
| --- | ---: | ---: |
| Test process | 24.049 s | 0.516 s |
| Nextest execution | 24.054 s | 0.532 s |
| Whole command (`date`) | 25 s | 2 s |
| Passed / failed | 1 / 0 | 1 / 0 |

The after-run still models **20 seconds of audio plus a four-second hold**: 480,000 samples in 500 packets, a full five-second reply buffer, zero modeled starvation and a 24,004 ms fixture playback finish. These are virtual fixture durations; the stdout label now says so. Before-run measurements are retained verbatim. [Before](before.log), [after](after.log), [structured results and exact dates](comparison.json).

At the paired baseline, all 1,105 compiled voice-library identities and ignore flags are identical before/after (this is not the later rebased head’s inventory): [complete list](identities.tsv). The focused command filters out the other 1,104; it does not remove them. Reversing only five fixture Instant references, one test attribute and explanatory labels/comments restores the entire original provider worker source byte-for-byte. Every existing test body and assertion remains, including packet ownership/offset/content, total PCM, buffer capacity, hold, starvation, finish/completion and service-writing bounds.

```sh
cargo nextest list --locked --offline --release --build-jobs 2 -p lamp-voice --lib --message-format json
date '+%Y-%m-%d %H:%M:%S %z'
cargo nextest run --locked --offline --release --build-jobs 2 -p lamp-voice --lib --profile realtime -E 'test(=provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity)' --test-threads 1 --retries 0 --no-fail-fast --status-level pass --final-status-level fail --success-output immediate --color never
date '+%Y-%m-%d %H:%M:%S %z'
```

The initial command preflight could not find nextest on PATH and ran no test; the existing installed binary was then added to PATH. No test failure was retried. Shared helpers' unpaused interruption test remains unpaused; the existing ignored 60/180-second measurement tests retain their ignore status. Whole voice/dependent memory/evaluator gates and native ARM64 qualification are recorded on the PR for its exact head. No profile selection or workflow was changed, and the whole realtime group's two-minute target remains open.

## Rebased gate and retained failure

Recorded 2026-10-15 22:26:35 +0700, from `date`, after rebasing onto `71a15e75d`. Fmt, strict all-target clippy for Voice/Memory/Voice-eval and workspace build passed. The original earlier source gate passed 1733 tests with 13 existing ignores; its raw logs remain here. The rebased full serial release gate reports **1765 passed, one failed, 13 existing ignored**; it was not retried. The changed capacity test passed.

The unchanged threaded `coordinator::tests::pilot_faults::a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on` returned `invalid original microphone read interval` instead of physical mute. It ran before the capacity test, uses independent host-clock threads and has unchanged source. This is [#866](https://github.com/autonomous-ai/lampOS/issues/866), coordinated on #739. The [read-only audit](stall-failure-audit.md) distinguishes the observed error from the suspected unsent-frame sequence hole. Native ARM64 remains pending, and no green local gate is claimed on this rebased source.

[Gate commands and date boundaries](gates.json); raw `rebase-*.log` files retain the failure. [Source preservation](source-preservation.json) confirms that reversing only this fixture change restores the full provider worker file on the current base, and that paired identities and ignore flags match. This does not weaken or replace the whole-group goal.
