# Records cadence preparation — #823

Recorded 2026-10-16 04:50:20 +0700, from shell `date`. Base `7ae0eb647914e286817c29bff2abc2934fb19546`.
This is an off-unless-explicitly-constructed fixture preparation. The ordinary
executable retains its real clock and one-second report cadence. The original
realtime Records case is unchanged; this slice claims no speedup.

## Change and preservation

The private application body is shared through `application.rs`; the normal
entrypoint supplies `SystemSchedule`, the original reporting wrapper and
`Reporter::new`. The explicit schedule reads at the two original cadence sites,
then receives the actual combined construction/publication result and original
host lifetime after work. The ordinary wait still sleeps the one-second
residual, including zero. The real config/receipt reloads, matching fingerprint,
UTC/header timestamps, 60-second error-log throttle, cleanup and exit behavior
remain unchanged. A wait receipt does not mean a subscriber consumed a message
or a child exited.

The explicit Reporter constructor installs the already-merged Bus cadence
clock before declaration presence. Ordinary construction supplies `Instant::now`;
the independent host header epoch and Bus 200 ms/one-second cadence remain.
There is no fixture executable, controller, environment switch or original-case
migration in this change.

The [source reversal](source-preparation/source-preservation.json) and
[independent review](independent-review/review.md) reconstruct the complete old
main and Reporter source. Every original main/Reporter/integration test body
is byte-identical, including all 16 assertions in the selected realtime case.
The [compiled comparison](local-gates/fixed-result.json) preserves all **160**
Records/Settings identities and ignored flags and adds exactly **12** cases.
New cases exercise actual files/Unix datagrams, held construction, failed
construction/publication, full cadence boundaries and separate host lifetime.

## Local gate and retained first failure

The first current-base gate was **164 passed, 6 failed, 2 ignored**. Six new
fixtures constructed Unix socket paths of 104–109 bytes under the macOS
environment temporary directory; this platform permits 103. Those cases failed
before their substantive checks. The raw [first log](local-gates/root-tests.log),
[failure analysis](local-gates/initial-failure.json) and both original failing
fixture source files remain unedited here. The workspace build was not reached
in that first attempt.

Only the two new `Scratch::new` roots changed to canonical `/tmp`, with private
per-test ownership retained. All test bodies and assertions remained identical;
the [independent fix proof](independent-review/short-root-fix-review.json) pins
both versions. This source repair, rather than a flaky-test retry, justified the
fresh full gate.

The corrected gate finished **2026-10-16 04:42:32 +0700**:

- `cargo fmt --all --check`: pass.
- `cargo clippy --locked --offline -p lamp-records -p lamp-settings --all-targets -- -D warnings`: pass.
- `cargo nextest list --locked --offline -p lamp-records -p lamp-settings --message-format json`: 172 identities, all 160 original names and ignored flags retained.
- `cargo test --locked --offline -p lamp-records -p lamp-settings --no-fail-fast -- --test-threads=1`: **170 passed, 0 failed, 2 existing ignored**, nine test summaries.
- `cargo build --locked --offline --workspace`: pass.

Commands ran from `lampOS/` with Rust 1.96, two build jobs, incremental compilation
disabled and the existing `/private/tmp/tests-823-memory-application-target`
cache. Actual socket/process operations ran outside the restrictive sandbox.
Settings is the direct Records-dependent crate. All stage commands, timestamps
from `date`, exit codes, unedited output and source hashes are in
[fixed-gates.json](local-gates/fixed-gates.json) and its adjacent files. The clean
actual-base compiled list is retained as `root-baseline-list.log`.

## Limits and provenance

Native ARM64 qualification is pending on this pull request's exact head. No
native, lamp, end-to-end or combined-original-123 timing result is inferred from
this Mac gate. The original integration case still waits on the real clock.
The later bounded child/controller migration is a separate change. No workflow,
profile, runner, production timeout, original assertion, retry or device change
is included.

The initial author handoff and diff describe the first fixture-root version;
they remain historical evidence. `local-gates/corrected-integration.json` and
`provenance.json` identify the current six-file source. The independent review
includes a dated fix addendum. Scratch measuring scripts are not committed;
they sequentially ran the recorded commands, captured stdout/stderr unchanged,
checked exact compiled identities and hashes, and parsed Rust test summaries.
[SHA256SUMS](SHA256SUMS) covers every other file in this folder.
