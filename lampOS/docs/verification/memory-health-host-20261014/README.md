# Memory health: host evidence, 2026-10-14

Task 28, built on `fourth/memory-health` from the claimed task at
`bf8a34924d1b410d82fbb1659e6723351bd3dac6`. The fetched integration base was
`17d312147908c04c97a9d433775844387ff89a5d`. No lamp, provider, account, key
or deployment was used. Synthetic records only.

## Machines and source

Mac: macOS 26.6.2 (25G83), x86_64, Intel Core i7-10700K at 3.80 GHz,
16 logical CPUs, 64 GiB RAM. Rust/Cargo 1.96.0; rustc
`ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96`, LLVM 22.1.2.

The existing local Podman VM runs Linux 6.12.13-200.fc41.x86_64, six
vCPUs and 8192 MiB configured RAM (8,313,036,800 bytes reported by Linux).
Its name `lampos-arm64` is not its architecture: these are **x86_64 host
tests, not ARM64 or lamp tests**. The cached image is
`localhost/lampos-fourth-native-check:latest`, inspected as
`203a72b687c4c5a96633420af4eccf839ff34fa448a26258ca5c89a077d7e6ad`
and run by that immutable image ID.

The source-only copy at
`/private/tmp/lamp-fourth-memory-health-source-33d_i_9q` has 1086 source
files copied from `lampOS/`, including the four new Memory files. It has
no parent repository, Git metadata, target directory or credentials.
The copy is mounted read-only, the container has `--network none`, and
only the existing Cargo registry/git caches and build-target volume are
also mounted. `source-sha256.json` binds Memory's source, fixtures and
the workspace/lock manifests to the tested copy. No crate depends on
Memory (`cargo tree -i lamp-memory --offline`), so Memory is the affected
crate closure.

## Results

Final gates: **135 passed, zero failed, four manual measurements ignored**
on each OS. Formatting, strict Memory Clippy and workspace builds passed.
The two new measurements were then run explicitly in release on the Mac,
one pass each; the two unchanged legacy timing tests were not repeated.
Linux's workspace build retains an unrelated `unused_mut` warning at
`crates/body/src/bin/lamp-body.rs:632`. Body is unchanged; Memory's Clippy
gate has no warning.

The unchanged Mac baseline passed 106 tests with two manual measurements
ignored. A development run passed 130; a subsequent gate before the final
JSON-key/cadence edge fixes passed 134 on each OS. The final source was
then gated again on both OSes, and both manual measurements were repeated.
Development check failures are retained in
[development-failures.txt](development-failures.txt): Clippy's two
`is_multiple_of` diagnostics and formatting of the added CLI test. Both
were fixed; no test run failed. A later staged whitespace check rejected
trailing whitespace in three captured outputs; those captures were trimmed
without changing diagnostics or numbers, and the check was rerun before
committing. The earlier timing runs are development
observations; the numbers below and attached raw timing files are from
the final source.

Evidence: [results.json](results.json), [Mac gate](mac-gate.txt),
[Linux gate](linux-gate.txt), [source hashes](source-sha256.json),
[held publisher](held-publisher.txt), [corpus run](corpus-run.txt) and
[corpus JSONL](corpus.jsonl).

## Commands

From `lampOS/`, sequentially:

```sh
cargo fmt --all --check
cargo clippy -p lamp-memory --all-targets --offline --locked -- -D warnings
RUST_TEST_THREADS=1 cargo test -p lamp-memory --offline --locked
cargo build --workspace --offline --locked
```

The standalone Linux gate runs these in `/source`, with
`CARGO_TARGET_DIR=/target`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=1`:

```sh
cargo clean -p lamp-memory --target x86_64-unknown-linux-gnu
cargo clippy -p lamp-memory --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo test -p lamp-memory --offline --locked --target x86_64-unknown-linux-gnu
cargo build --workspace --offline --locked --target x86_64-unknown-linux-gnu
```

The two new manual measurements run separately on the Mac, without a
concurrent build or test job:

```sh
RUST_TEST_THREADS=1 cargo test -p lamp-memory --release --offline --locked --lib health::tests::measure_health_update_and_socket_with_a_held_publisher -- --ignored --exact --nocapture
RUST_TEST_THREADS=1 cargo test -p lamp-memory --release --offline --locked --test health_timing -- --ignored --nocapture
```

## What the measurements mean

`held-publisher.txt` measures begin/end bookkeeping and real empty-store
socket requests while a publisher is explicitly held on a channel. The
first bookkeeping sample is separate from 10,000 repeated samples; each
socket case has one first request plus 100 repeats. That socket boundary
includes response JSON decoding/assertion in the test helper. It is a
causal isolation test, not a simulated lamp bus benchmark.
Its quantiles select the upper observation at sorted index `floor(n*p)`;
for the 100-repeat socket cases, the displayed p99 is also the maximum.
The corpus measurement below instead uses nearest-rank quantiles.

`corpus.jsonl` uses 6000 exchanges over 30 days plus 200 kept entries.
Each JSONL/Days, reporting off/on, kept/append socket case starts from a
fresh identical fixture, with one first request plus 50 repeats. Append
adds 51 records. Enabled cases wait for an actual Diagnostic before
sampling. Caches are not evicted; first means first measured request,
not cold storage. The boundary is connect, serialize/send, read the
complete response and close; it excludes response JSON decoding and
startup. Durability remains enabled.

Separate measurements cover the legacy count scan (first plus ten),
Days TEMP counter initialization (once), and in-loop bookkeeping
(first plus 1000 for each backend, for both a note and a 60 KiB append
request). Bookkeeping includes classification, the optional constant-time
Days count read and state update, but excludes the actual store operation,
legacy scans and bus I/O. The host target is p99 below 1 ms. Ordinary
fast batches can fit between reports; the held-publisher test covers bus
blocking independently. Differences between individual off/on samples
are not evidence of a speedup.

## Final Mac timings

All numbers in this table are milliseconds, **reporting off / on**.
Each case has one first request and 50 repeated requests; maxima refer
to repeats only. The full context and exact nanoseconds are in the JSONL.

| Store / request | First | Median | p95 | Maximum |
|---|---:|---:|---:|---:|
| JSONL / kept | 0.259 / 0.201 | 0.158 / 0.150 | 0.217 / 0.171 | 0.246 / 0.176 |
| JSONL / append | 25.839 / 28.385 | 18.473 / 18.478 | 22.000 / 19.902 | 27.162 / 25.507 |
| Days / kept | 1.605 / 1.772 | 1.397 / 1.481 | 1.500 / 1.682 | 1.513 / 1.788 |
| Days / append | 59.538 / 61.227 | 60.531 / 60.199 | 62.651 / 68.992 | 97.784 / 79.706 |

Bookkeeping below is in **microseconds**, first / p99 over 1000 repeats:
JSONL note **12.019 / 0.562**, Days note **10.894 / 4.689**; with a valid
61,493-byte append request, JSONL **12.835 / 12.373**, Days
**19.283 / 15.493**. Every p99 is below the 1000 µs host target.

The legacy count scan took 3.438 ms first, 3.178 ms median and 3.280 ms
maximum over ten repeats. Days counter initialization took 1.031 ms once,
excluding open/import and occurring before readiness.

With the publisher explicitly held, bookkeeping took 24.771 µs first,
0.533 µs median, 0.556 µs p99 and 1.921 µs maximum over 10,000 repeats.
Empty-store socket requests took 0.361 / 0.281 ms first and
0.035 / 0.036 ms median with reporting off / on; the on case's maximum
over 100 repeats was 0.049 ms. All requests completed while publication
was still held. Separate functional tests hold publication and publisher
destruction through the 100 ms shutdown wait, and hold a real socket
handler until its active-budget warning is received.

## What was not shown

No ARM64 execution, lamp flash measurements, acoustic or end-to-end Voice
latency, physical power loss, real model work, long-lived filesystem stall
or live Health unit. Health reports handler time, which excludes client
queueing, transfer, logging and answer delivery. A successful write clears
the observed storage-failure latch; it does not test every other file or
future write. The reporter's 100 ms shutdown wait is bounded; termination
of a thread held in filesystem I/O is not guaranteed, and no replacement
thread is started.

The most likely lamp difference is flash contention and scheduling: a
legacy count scan may stretch report cadence, while Days TEMP triggers
add work to its real transactions. First should preserve the live store's
`--memory-days` setting, test the new reporting switch off first, then add
`--bus-root /run/lampos-bus` to Memory and `--expect memory` to Health's
existing commands. No unit was edited here.
