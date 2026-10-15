# Finite loopback on a coordinated fixture clock

Part of #823. The corrected-channel release pair passed the same 12
`physical_loopback` tests: **239.720 s before → 11.439 s after**
(95.23% less test execution time). No failures, retries, ignored or skipped
cases. This is one local binary; native ARM64 qualification and the complete
123-test under-two-minute target remain open. Profiles, workflows and ordinary
runtime commands are unchanged.

## Current release pair and qualification

The pair ran on x86_64 macOS in optimized release mode at source
`763faf285c320aebb589e04863c2aff5a5b03415`, based on
`6b08a70d56e37b061e7a9a6b013072b13639a0b7`:

| Run | Started / finished from `date` (+0700, 2026-10-16) | Passed | Nextest execution |
|---|---|---:|---:|
| Before, original constructors | 00:33:06 / 00:37:06 | 12 | 239.720 s |
| After, controlled constructors | 00:37:12 / 00:37:24 | 12 | 11.439 s |

Both invocations were serial, with `--retries 0 --no-fail-fast`; before ran
first. Nextest execution excludes compilation/listing and shell launch
overhead. The same host also ran independent work: the recorder-seam full gate
may overlap. Load was uncontrolled, so this is not a quiet benchmark or a
measurement of ARM64, device latency, the entire realtime group or PR wait time.

[Comparison and every measured row](release-pair-comparison.json) verifies exact
compiled identities and ignored/filter flags. The `release-pair-*.log` files,
command date/exit markers and both test-source snapshots are retained byte for
byte. [Helper proof](release-pair-binary-proof.json) records the identical
release evaluator SHA-256 in both runs:
`5af71f030d17b8cc6dc339e3867e7fd32513a01b859453bc1c463cdbb7ea067d`.
Only the authorized test construction changes between sources; the current
test source matches the restored after snapshot. No binaries or Python
scripts are included.

From `lampOS/`, with Cargo and cargo-nextest on PATH:

```sh
cargo nextest list --locked --offline --release -p lamp-voice-eval --test physical_loopback --message-format json
cargo nextest run --locked --offline --release -p lamp-voice-eval --test physical_loopback --profile realtime --test-threads 1 --retries 0 --no-fail-fast --status-level pass --final-status-level fail --color never
```

Both used `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0` and
`CARGO_TARGET_DIR=/private/tmp/lampos-tests-capacity/lampOS/target`.

The corrected pre-rebase full gate passed **352 evaluator tests**, with zero
failures, ignored or filtered cases, including all 12 loopback cases in 8.10 s
in the release libtest harness. Formatting, strict workspace all-target clippy
and workspace build passed. Exact records remain in
[channel-fixed-gates.json](channel-fixed-gates.json) and
[channel-fixed-local-sockets-gates.json](channel-fixed-local-sockets-gates.json).
The 8.10 s harness run is separate from the fresh paired nextest result.

After rebase, **all 57 evaluator Rust files are byte-identical** to the gated
source ([proof](rebase-source-proof.json),
[pre-rebase manifest](before-rebase-source.json)). New qualification passed
formatting, evaluator all-target clippy with `-D warnings`, all **150 release
library tests** in 6.46 s, workspace build, and the release pair above. Those
[commands and dates](rebased-channel-gates.json) and raw logs are retained.
The full 352-test invocation was not repeated after rebase. The ordinary PR
profile excludes this binary, so its native execution requires the full run.

The finite section in `realtime-tests.md` sits before the existing
"Finite fake-runtime clock and poll boundary" section to avoid the sibling
serve-clock documentation append. Removing our section leaves upstream text
and order byte-identical; this placement is documentation only.

## Preserved coverage and scheduling

[Source reversal](release-pair-constructor.diff) changes only the header,
import and two constructor regions. Reversing them restores the original file
byte for byte. All 12 identities, **100 assertion macros**, scenario durations,
plan edits, both two-case fault loops, seeds, admission/cancellation/scoring
checks and 100 ms lateness bounds remain unchanged. Eleven test bodies are
byte-identical; the twelfth changes only construction. The two quick
transport/restriction cases retain their original constructors.

The real relay, fake runtime child, CueSink, sockets, stdout framing, grants,
trace files and process exit/reaping stay in the path. Pure clock reads do not
advance time; parent scheduling retains every original 2 ms runtime/relay poll
and 10 ms model tick. Acknowledgements follow completed input/output work, and
the backend must enqueue acknowledged output. Queued events drain before
`next_event` advances; injection retains its wait/final-drain order. Terminal
time freezes until actual child exit and final relay output, not merely a
logical completion report.

Only explicit local fake-clock commands select coordination. Defaults keep
system clocks, sleeps and queue bounds. The controlled reader allows 50,404
transcript records plus one reserved error slot, with nonblocking enqueue and
the original byte/line caps. It cannot deadlock on a final trace larger than
4,096 lines. Controls validate framing, roles, rounds and counters under
absolute host watchdogs; Drop closes peers before existing process cleanup.

## Closed-peer correction and historical evidence

Serve integration exposed #879: Darwin rejected socket timeout setters after
a peer closed even while its final frame remained readable. The shared channel
now uses nonblocking I/O and bounded readiness polling with the same absolute
host deadline; queued bytes precede actual EOF. Ten regressions cover closed
peers, complete/truncated frames, expiry, partial I/O and timeout range. All 21
original protocol tests and the loopback assertions remain intact. Original
failure, diagnostic source and exact port/review evidence remain in
[closed-peer/](closed-peer/). Diagnostic stdout was not saved; it is not claimed
as a raw transcript.

An initial corrected release compile found a stale local Bus artifact whose
dep-info omitted the source logging module. The failed log and
[cache diagnostic](release-cache-diagnostic.json) are retained. Only that
package's release output was invalidated before the successful rebuild; no
source or assertion was changed to bypass the failure.

The **historical pre-#879 debug pair** was 248.575 s → 28.471 s, all 12 passing,
serial on x86_64 macOS. Its original `before/after` logs, listings, artifact
manifests and [comparison](comparison.json) remain unchanged. It used the
shared `/private/tmp/lampos-tests-relay-clock/lampOS/target` directory. That
measurement describes the earlier channel, not the corrected release pair.
The original pre-correction 342-test gate, 46-test focused run and earlier
clippy diagnostics also remain in their raw logs and [gate record](final-gates.json).
