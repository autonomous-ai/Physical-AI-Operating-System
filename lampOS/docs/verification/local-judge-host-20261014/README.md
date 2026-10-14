# Task 17: local judge host evidence

Fourth built this on `fourth/local-judge`, from claim `574c2e5ed`, pinned
for the task to the upstream cycle at `1750c3d08`. This is **development-host
evidence, not a lamp measurement**. No device, real provider, account or
credential was accessed. HTTP/TLS services in the tests are local fixtures.

The implementation, exact operator command, fitting schema and failure
semantics are in [Policy](../../policy.md#the-local-second-judge-task-17).
The machine-readable [results](results.json) include counts and log hashes;
[source hashes](source-sha256.json) identify every Policy Rust file, its
manifest and the seed checked in both environments. The source-only copy
was checked byte-for-byte against these files after the final tests.

## Machines and gate

Mac: macOS 26.6.2 (25G83), x86_64 Intel Core i7-10700K at 3.80 GHz,
16 logical CPUs, 64 GiB RAM. Rust/Cargo 1.96.0; rustc commit
`ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96`, LLVM 22.1.2.

Run sequentially from `lampOS/`:

```sh
cargo fmt --all --check
cargo clippy -p lamp-policy --all-targets --offline -- -D warnings
RUST_TEST_THREADS=1 cargo test -p lamp-policy --offline
cargo build --workspace --offline
```

All stages passed. **142 tests passed, zero failed, two explicitly ignored
measurement tests**; both ignored tests were separately run in release mode
below. The previous 81-test Policy suite remains included. No workspace
crate depends on Policy, so it is the only affected crate gate. Final Clippy
reported 2.54 seconds and the cached workspace build 2.17 seconds. These are
Cargo elapsed times, not cold-build or interaction-latency measurements.

Portability: an independently copied source tree under `/private/tmp`,
without parent sources, Git metadata, target artifacts or credentials,
was mounted read-only in `localhost/lampos-fourth-native-check:latest`.
The existing VM is named `lampos-arm64` but actually runs **x86_64 Linux**,
kernel `6.12.13-200.fc41.x86_64`, six vCPUs and 8192 MiB RAM on the same
Mac. It is not an ARM64 test. Registry/git caches were mounted read-only,
target output used the existing separate build volume, and external
networking was disabled with `--network none`. Container-local loopback
fixtures still ran. Rust is the same version, target
`x86_64-unknown-linux-gnu`.

With `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=1`, `CARGO_TARGET_DIR=/target`:

```sh
cargo clippy -p lamp-policy --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo test -p lamp-policy --offline --locked --target x86_64-unknown-linux-gnu
cargo build --workspace --offline --locked --target x86_64-unknown-linux-gnu
```

All three stages passed on the final source: **142 passed, zero failed,
two ignored**. A preceding complete source-copy run also passed before the
last-wait correction. Its workspace build reported 1 minute 56 seconds;
the final cached workspace build reported 4.22 seconds. The Linux workspace
build retained inherited Body warnings at `inspection.rs:615` (unused
`run`) and `bin/lamp-body.rs:592` (unused `mut`). Body was not changed and
its owner is Fifth. Policy's strict Clippy gate had no warning.

## Failures retained and tests exercised

[Development diagnostics](development-failures.txt) retain three failed
Clippy/compilation attempts: an unnecessary `mut` and an overlong argument
list; a test's incorrect `Message.decode` method; and a constant assertion
in the manual timing test. They were fixed without lint suppression. The
first formatting check also found the newly written files unformatted;
rustfmt was applied before the passing gate. These failures are not hidden
as completed checks.

One later Linux gate failed the unchanged legacy test
`the_lamps_name_heard_while_someone_spoke_is_joined_to_what_they_said`, at
`tests/policy.rs:727`: the fourth utterance retained `name_heard:0.97`
where the fixture expected `0.0`. The [complete failed gate](linux-name-gate-failure.txt)
and [investigation](name-fixture-investigation.txt) are retained. One isolated
run of the task source and five isolated runs of untouched baseline
`574c2e5ed` passed. In a **separate diagnostic baseline copy only**, inserting
40 ms between the third `Thinking` state and its transcript reproduced the
same assertion with unchanged production code. The exact diagnostic patch
and failing output are included; it is not part of the repository tests.

The source explains this sensitivity: a transcript arriving after the state
uses the 80 ms gather wait, whereas the other receipt order can retain
300 ms. That changes whether the previous name remains in the next
utterance's one-second pre-roll window. The original failure retained no
receipt timestamps, so this is a reproduced pre-existing trigger, not proof
of its exact schedule. No assertion, timing limit, production name window or
test inclusion was changed. The full final gate was rerun after the
investigation. The fixture's scheduling sensitivity and the possibility of
cross-utterance name association remain for First to review; they are not
claimed fixed by task 17.

Switching the baseline and task copies through the same `/source` mount
also exposed a stale Policy build artifact: imports from the new modules
were reported missing although the mounted source contained them. The
[compiler output](linux-stale-cache.txt) is retained. Clearing only Policy
artifacts with `cargo clean -p lamp-policy --target x86_64-unknown-linux-gnu`
resolved it; the final strict lint, full tests and workspace build then
passed. No source was changed for that cache correction.

New tests cover each cue and unknown value, all seven typed questions,
exactly neutral all-unknown priors, missing/stale/future face observations,
head-pose knownness, model/input bounds, independent session splits, held-out
label invariance, insufficient-class priors, private create-new outputs,
and the exact busy-floor truth table and name predicate. Real Unix bus tests
hold and release the slow judge explicitly: on-time combination, late audit,
utterance ordering, failure/recovery, stop while a request remains held,
a deliberately exited slow worker with a truthful local-fallback diagnostic,
and continued decisions after an actual log append failure. Logger tests
hold an append while filling/cancelling its bounded queue and distinguish
accepted, written, failed, dropped, abandoned and pending records.

The actual `lamp-policy-judges` command is tested with a local HTTP fixture:
both judges' numbers appear, the command exits normally, private text/model
names/fake credentials do not appear in terminal output, and missing opt-in
does not create a bus or log. A startup subscription failure cancels its
60-second timer. The original executable's tests remain unchanged.

## Arithmetic timing

Run alone on the Mac, one fresh test process, OS file cache **not cleared**:

```sh
cargo test -p lamp-policy --release --test local_timing --offline -- --ignored --nocapture
```

The [raw arithmetic result](arithmetic-timing.json) times `Decider::ask`,
including cue JSON parsing and allocation of seven typed answers. It
excludes weights loading, cue collection, final probability combination,
logging, bus scheduling and sound. Weights loading was measured separately
at **124.678 µs**; no claim of cold-disk behavior is made.

| Sample | Time |
|---|---:|
| First `ask` | 9.072 µs |
| Next 1000, median | 1.434 µs |
| Next 1000, p95 | 1.453 µs |
| Next 1000, p99 | 1.466 µs |
| Next 1000, maximum | 2.379 µs |

The stated local arithmetic targets (p95 ≤20 ms, p99 ≤50 ms) were met on
this host. These are not Orange Pi performance predictions.

## Full internal shadow-choice timing

The same controlled bus fixture ran twice, alone on the same Mac, using:

```sh
RUST_TEST_THREADS=1 cargo test -p lamp-policy --release --test judges --offline -- --ignored --nocapture
```

Its first request is explicitly held while the first choice reaches its
100 ms admission deadline. The next 100 utterances complete locally while
that same slow request remains held; assertions verify their `remote_busy`
reason. The private logger completed all accepted records with no drops.
The boundary is **gatherer eligibility to the final internal choice just
before bus publication**. It includes local inference, immediate bounded
log enqueue, the test's note callback and closing the previous outcome.
It excludes gathering, bus delivery, the later final-row enqueue, terminal
output, microphone, ASR, network response and acoustic playback.

| Same authored workload | Before fixed 5 ms wait was capped | After capping by remaining budget |
|---|---:|---:|
| First choice | 104.080 ms | 100.223 ms |
| Next 100, median | 16 µs | 16 µs |
| Next 100, p95 | 19 µs | 23 µs |
| Next 100, p99 | 29 µs | 37 µs |
| Next 100, maximum | 36 µs | 47 µs |

Both [before](shadow-timing-before.json) and [after](shadow-timing-after.json)
results are retained. These runs preceded the subsequent worker-exit
diagnostic wording fix; the held-worker decision path is unchanged. The
first overrun exposed avoidable polling delay;
the loop now limits its final wait to the remaining deadline. The final
choice still exceeded the nominal 100 ms completion target by **0.223 ms**.
Slow-answer admission remains strictly before 100 ms; this is not a hard
real-time completion guarantee. The repeated-case variation is reported,
not called a speed improvement. One pair of fixture runs does not establish
a statistical latency distribution for the lamp.

## Same-record comparison, with authored inputs only

Reproduce the [complete eight-record result](authored-comparison.json):

```sh
cargo test -p lamp-policy --test local_comparison --offline -- --nocapture
```

These numerical cue snapshots and intended labels are authored tests, not
recordings of the office cases. Each row supplies the same snapshot to the
busy-floor mirror and the local-only final choice, including the no-call
guard. No Jev contribution is simulated. All eight are `same_snapshot`;
**zero reproduce Voice's actual first-output snapshot**.

| Authored case | Busy-floor permission | Local-only choice |
|---|---|---|
| Quiet short scrap, no call | play | look_up |
| Busy talk containing an acoustic name score | hold | look_up |
| Busy explicit call, pose unknown | play | look_up |
| Busy explicit call, known facing face | play | answer |
| Busy follow-up | play | look_up |
| Unknown-room scrap | play | look_up |
| Busy colleague speech, no call | hold | look_up |
| Quiet question with an acoustic name score | play | look_up |

Five of eight speech permissions differ. Against the authored intentions,
the floor permits two of four negative cases and misses none of four calls;
the local-only choice permits none of the four negatives but misses three
of the four calls. This demonstrates a seed tradeoff and its insufficient
qualification; it **does not show that the judge beats the floor**. The
fitter's test-only report separately evaluates fitted local probabilities,
without this runtime guard or a fused slow judge. Missing/invalid baseline
snapshots are excluded with explicit coverage counts.

## Not shown

No independent labelled lamp corpus, real model-service traffic, acoustic
response/stop timing, ARM64 execution, speaker-to-face association, joint
motion or on-device run. The seed and mixture remain unfitted. No source
timestamp alignment or playback attribution is invented from receipt times.
No file append is claimed as fsync/power-loss durability, and the bounded
queue does not cap the daily archive's disk use. First still must run the
single-owner played-scene command, supply lawful independent labels, fit
the model and compare held-out lamp sessions before any action wiring.
