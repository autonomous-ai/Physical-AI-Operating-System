# Task 17: comparison after the next fetch

Fourth rebased onto `e93b9684929d62248394d61c87ec57120db43229` after the
original task 17 handoff. First had added the sure-name branch in
`e07449e13`: Voice's `first_output_busy` permits output in a busy room
when its retained per-request `named_sure` bit is true. Auditing only
`decide_busy` would miss that branch. The original eight-case evidence
and failures remain in [the original record](../local-judge-host-20261014/README.md).

The current mirror requires that boolean explicitly, alongside the other
snapshot fields; missing or null is rejected. A captured `voice_first_output`
snapshot must supply Voice's actual latch. The runtime's `same_snapshot`
track applies the 0.999 threshold to the supplied Policy name score and
does not reconstruct Voice's request window. The source revision is in
model records and fitting reports. The shadow guard accepts this strong
call evidence but does not force an answer; probabilities and weights are
unchanged. The optional path remains off by default and always shadow.

## Host gates

Same development hosts as the original record: macOS 26.6.2 (25G83),
x86_64 Intel i7-10700K at 3.80 GHz, 16 logical CPUs, 64 GiB RAM; and its
existing Podman VM, Linux 6.12.13-200.fc41.x86_64, six vCPUs and 8192 MiB
RAM. The VM's name `lampos-arm64` does not describe its architecture:
these are **x86_64 tests, not ARM64 or lamp tests**. Rust/Cargo 1.96.0,
rustc `ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96`, LLVM 22.1.2.

Mac commands, from `lampOS/`:

```sh
cargo fmt --all --check
cargo clippy -p lamp-policy --all-targets --offline -- -D warnings
RUST_TEST_THREADS=1 cargo test -p lamp-policy --offline
cargo build --workspace --offline
```

All passed: **143 tests passed, zero failed, two manual measurements ignored
by the normal gate**. Both measurements ran separately below. Strict Clippy
reported 5.06 seconds and the workspace build 24.08 seconds, with the
existing cache. No crate depends on Policy; previously integrated Fourth
code in other crates has no source difference from the fetched upstream.

The standalone copy at `/private/tmp/lamp-fourth-floor-source-a23d3bfh`
contains source without parent-repository files, Git metadata, credentials
or target artifacts. It was mounted read-only in
`localhost/lampos-fourth-native-check:latest`, with `--network none`,
read-only Cargo registry/git caches and the existing separate target volume.
Container-local test loopback remained available. With `CARGO_BUILD_JOBS=4`,
`RUST_TEST_THREADS=1`, `CARGO_TARGET_DIR=/target`, sequentially:

```sh
cargo clean -p lamp-policy --target x86_64-unknown-linux-gnu
cargo clippy -p lamp-policy --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo test -p lamp-policy --offline --locked --target x86_64-unknown-linux-gnu
cargo build --workspace --offline --locked --target x86_64-unknown-linux-gnu
```

All passed: **143 passed, zero failed, two ignored**. Clippy took 3.63 seconds
and the workspace build 21.50 seconds. Cargo elapsed times are not response
latency or cold-build measurements. The workspace retained the same two
inherited Body warnings; Policy's strict lint was clean. No test or gate
failed in this follow-up. The original failure investigation is retained;
passing this gate does not resolve that existing name-window sensitivity.

The new boundary test covers absent, invalid, 0.997, just-below-0.999,
0.999 and 1.0 scores. The full permission truth table includes both latch
values; snapshot tests reject missing/null latch evidence. Fitter reports
identify the audited source. A read-only audit compared the current Voice
source and the follow-up code/tests/docs and found no discrepancy.

The [results](results.json) retain commands, counts and local log hashes.
All 32 [source hashes](source-sha256.json), including Cargo manifests/lock
and the seed, matched the tested standalone copy and working tree.

## Measurements on the Mac

Run sequentially, without another build or test running:

```sh
cargo test -p lamp-policy --release --test local_timing --offline -- --ignored --nocapture
RUST_TEST_THREADS=1 cargo test -p lamp-policy --release --test judges --offline -- --ignored --nocapture
cargo test -p lamp-policy --test local_comparison --offline -- --nocapture
```

One fresh process, OS cache not cleared: local `Decider::ask`, including
cue JSON parsing and seven answer allocations, took **8.427 µs first**;
the next 1,000 had median 1.425 µs, p95 1.442 µs, p99 1.454 µs, maximum
2.149 µs. Weights loading separately took 102.271 µs. See the
[raw arithmetic result](arithmetic-timing.json). This excludes cue gathering,
combination, logs, bus scheduling and sound.

The controlled [shadow timing](shadow-timing.json) holds one slow request.
Eligibility to final internal choice immediately before bus publication
took **100.244 ms first**, then median 15 µs, p95 19 µs, p99 30 µs and
maximum 34 µs for 100 local-only choices while the same request remained
held. The logger drained without drops. This boundary includes local
inference, immediate bounded enqueue, callback and previous-outcome closure;
it excludes gathering, bus delivery, final-row enqueue, terminal output and
acoustics. The nominal 100 ms completion target was exceeded by **0.244 ms**.
Admission still requires a slow answer completed and observed before the
fixed cutoff. This is not a hard real-time guarantee or a lamp prediction.

The [nine authored snapshots](authored-comparison.json) add a busy-room
0.999 name score without a facing observation. That row permits speech
under the current floor; the local seed chooses `look_up` at probability
0.4067034 with no guard override. Six of nine permissions differ. Against
four authored negative and five positive intentions, the floor permits two
negatives and misses no calls; the local seed permits no negatives but
misses four calls. All rows are `same_snapshot`; zero are actual Voice
first-output records. This exposes an insufficient seed, **not improvement
over the floor** or measured accuracy.

## Not shown

No fitted weights, independent lamp labels, real-service traffic, ARM64
execution, on-device or acoustic latency, stable speaker/name/face binding,
or action qualification. The operator command in [Policy](../../policy.md#the-local-second-judge-task-17)
is unchanged. First must keep a single Policy owner, capture the real
first-output latch and other inputs, fit independent sessions, and compare
held-out lamp records before considering action wiring.
