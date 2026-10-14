# Records process-exit checks on the development host, 2026-10-14

Task [30 / #707](https://github.com/autonomous-ai/lampOS/issues/707). These are
development-host results, not lamp-4ace results. No lamp was contacted, and no
deployment, retention setting or real session directory was changed.

## Machine and source

- macOS 26.6.2 (25G83), x86_64, Intel Core i7-10700K at 3.80 GHz,
  16 logical CPUs, 68,719,476,736 bytes RAM (64 GiB).
- Rust/Cargo 1.96.0; rustc commit
  `ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96`, LLVM 22.1.2.
- Isolated worktree `/private/tmp/lamp-fourth-integration`, branch
  `fourth/records-exit`; its own `lampOS/target` directory. It did not use the
  parent checkout's build target. No new dependency was added.
- [source-sha256.json](source-sha256.json) identifies the source files and
  manifests checked on this host. The final commit is recorded in the handoff; the Linux/ARM64 checks
  are below.

## Commands and outcomes

Commands ran from this worktree's `lampOS/`, with
`/Users/autonomous/.cargo/bin/cargo`. The unchanged baseline and the pre-review
test run each ran once. After review added the zero-thread race and prioritized
the known kernel-thread exclusion, formatting, Clippy and the tests each ran
once again on the source identified above.

| Command | Outcome |
|---|---|
| `cargo test -p lamp-records --offline --locked` (baseline) | 97 passed: 88 library, 5 binary, 4 integration; 0 failed; 1 ignored existing manual timing |
| `cargo clippy -p lamp-records --all-targets --offline --locked -- -D warnings` | Pre-review and final runs passed, no warnings |
| `RUST_TEST_THREADS=1 cargo test -p lamp-records --offline --locked` (pre-review) | 111 passed: 102 library, 5 binary, 4 integration; 0 failed; 1 ignored existing manual timing |
| `RUST_TEST_THREADS=1 cargo test -p lamp-records --offline --locked` (final) | 113 passed: 104 library, 5 binary, 4 integration; 0 failed; 1 ignored existing manual timing |
| `cargo fmt --all --check` | Pre-review and final runs passed |
| `git diff --check` (repository root) | Passed |

[checks.json](checks.json) keeps the command outputs as verbatim JSON strings,
including their original newlines, so they can be reconstructed without
normalizing the raw text. It also records exit statuses. Cargo's test durations
are test-run durations, not cleanup or conversation latency measurements.

The new checks cover disappeared pinned processes and reused PID names;
`ESRCH`; `Z` and `X`; `PF_EXITING` with empty and nonempty arguments; stopped,
blocked and live empty-command processes; denied/missing/corrupt metadata;
zero thread count followed by gone, live, malformed or still-zero metadata;
kernel-thread flags combined with zombie/dead state; surviving threads;
current main-process failures; retaining already-discovered
session paths; and a live file holder alongside an exited process. Injected
read counters verify one recheck, with at most one final command-line read.
The report tests cover legacy receipts, omitted empty observations, bounded
PID examples, refusal details, count limits and invalid reasons or PIDs.

A Linux-only test additionally starts an owned shell helper with a synthetic
record held open. It verifies the fd-based protection while that child lives,
then the zombie observation before reaping, and a pinned `cmdline` read's
actual `ESRCH` plus the gone observation after reaping. This test was compiled
out on macOS; its execution must be recorded with the Linux results.

## Standalone Linux and ARM64 build

The source was copied without the checkout's target directory into
`/private/tmp/lamp-fourth-records-exit-final-y7xo_meu` and mounted read-only.
The existing Podman VM runs Linux x86_64, kernel 6.12.13, with six virtual
CPUs and 8 GiB configured RAM. The image is Debian 12,
`localhost/lampos-arm64-cross:latest`; Rust 1.96.0. No lamp or native ARM64
machine was used for these local runs. No new package or network access
was needed. The Cargo build used four jobs and tests used one thread.

```sh
cargo fmt --all --check
cargo clippy -p lamp-records --all-targets --offline --locked \
  --target x86_64-unknown-linux-gnu -- -D warnings
cargo test -p lamp-records --offline --locked --target x86_64-unknown-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release -p lamp-records --offline --locked \
  --target aarch64-unknown-linux-gnu
file /target/aarch64-unknown-linux-gnu/release/lamp-records
sha256sum /target/aarch64-unknown-linux-gnu/release/lamp-records
```

[linux-final.txt](linux-final.txt) is the unedited output, exit 0. Formatting,
strict Clippy and ARM64 release cross-build passed without warnings. Native
Linux tests: **115 passed** (106 library, 5 binary, 4 integration), zero
failed, two ignored: the manual cost measurement and the explicitly privileged
isolated-container bind-mount test. The live fd-holder/zombie/reaped test ran
and passed. The earlier pre-review Linux run also passed (113 tests), retained
in [linux-before-review.txt](linux-before-review.txt); it did not include the
two final regression tests and is not the final source qualification.

The final cross-built binary is an aarch64 ELF, SHA-256
`65d7d7e8b976c8611878004907e0a6c34d12b53a897df5edfc1a78e7aee15bc2`.
It was not executed or installed. The Mac's required
`cargo build --workspace --offline --locked` also passed without warnings;
[mac-workspace-build.txt](mac-workspace-build.txt) preserves its output.

The mandatory native ARM64 GitHub gate remains outstanding. The untouched
claim baseline failed in Controls because `/bin/kill` could not be spawned;
[the separate evidence](../arm64-baseline-20261014/README.md) records it.
This task is not ready for `lamp-voice` until that required gate is green.

## Failures and limits

No Rust compiler, Clippy or test invocation above failed. During preparation,
one read used the incorrect role path `lampOS/docs/builders/fourth.md`; it was
corrected to `lampOS/builders/fourth.md`. The web reader timed out on three
primary Linux source URLs; direct downloads succeeded, with one transient
HTTP 429 for `sched.h` before its successful retry. One documentation patch
missed its context and was reapplied correctly. The first sandboxed `sysctl`
read was denied; the authorized read-only retry supplied the machine identity
above. None of these failures changed runtime behavior or skipped a gate.

No new performance claim is made. The existing ignored `records_cost` test
measures inventory/planning/removal with injected protection, so it would not
measure this procfs change. The exit recovery adds no worker, queue, sleep or
retry loop and remains inside the existing ten-second guard budget. Kernel
filesystem calls can exceed that budget. Actual host procfs visibility,
systemd churn, kernel-specific exit ordering and concurrent conversation
latency on lamp-4ace remain for the first builder's qualification.

## First builder handoff

Use the matching new Records binary for both cleanup and the reporter. Keep
the current retention configuration. First run its normal dry check:

```sh
sudo /opt/lampOS/records/lamp-records check /etc/lampOS/records.json
```

It must leave `removed` empty, retain the running invocation in `active`, and
report positive process observations in `roots[].processes` when encountered.
An empty command line without positive exit or kernel-thread evidence still
fails closed, with the PID in `error`. No unknown live-process failure should
turn into a successful empty protection set. Once installed by First, inspect
the next normal timer receipt under `/run/lamp-records/last.json`; this work
does not start the timer or authorize any extra cleanup.

Most likely lamp mismatch: a process leader is exiting while other threads
still survive, or the procfs view is restricted. Those conditions remain
refusals. The original PID's state was not captured in the failed hourly run,
so the cause of that particular failure is still inferred, not proven.

## Gate after the CI workflow repair

Rebased the task onto Fourth's log branch `576454fcf` (upstream `41d09542f`).
Records implementation, original docs and measurement source are byte-identical
to the previously measured task. `mac-rebased-gate.txt` is the complete repeat
gate after this upstream change, on the same x86_64 Mac: formatting, strict
Clippy, all 113 tests (zero failures, one existing manual test ignored), and
workspace build passed. Commands in order, from `lampOS/`:

```sh
cargo fmt --all --check
cargo clippy -p lamp-records --all-targets --offline --locked -- -D warnings
RUST_TEST_THREADS=1 cargo test -p lamp-records --offline --locked
cargo build --workspace --offline --locked
```

The earlier ARM64 CI failures remain preserved on Fourth's log branch. First
changed CI from a root Debian container to native Ubuntu 22.04 ARM64 under
an ordinary user (`91a0fdb3a`), so the 0500-directory permission fixture can
exercise denial. The new required ARM64 run is pending, not yet a pass.

## Task-only branch after the ordinary-user Records repair

The handoff rules now require one task-only pull request. The branch was
rebased directly onto `1733307d7672ad302154984952a77772f8ebd6e5`, removing
log-branch ancestry. Upstream `21c24fc0a` repairs `O_NOATIME` opens of
root-owned paths as an ordinary user; Fourth did not change that helper.
The earlier native ARM64 run 38557569694 passed its release build but failed
the two JSON helper tests (104 passed, 2 failed). Its raw failure is retained
on `lamp-fourth` in `docs/verification/arm64-nonroot-20261014/`.

The same four-command Mac gate above was run again after this rebase. All
113 tests passed, zero failed, one manual test remained ignored; formatting,
strict Clippy and workspace build passed. The complete output is
[mac-pr-base-gate.txt](mac-pr-base-gate.txt). The new native ARM64 run on the
eventual task head remains required before a ready pull request.

## Not shown

No deployment, lamp timing, lamp procfs race reproduction or successful
hourly lamp receipt was measured here. The on-lamp handoff above and green
native ARM64 CI remain necessary; an earlier host pass is not their substitute.
