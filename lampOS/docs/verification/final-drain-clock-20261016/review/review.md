# Final-drain clock preparation: independent source review

Reviewed 2026-10-16 09:10:58 +0700, from `date`, at base `1fb07c6f57ee280a49de7b4ddd74008c2a32a20d`. Final reviewed variant has six new tests. No source edit, Cargo/test execution, Git/GitHub operation or device action was performed by this reviewer.

**No remaining source blocker found for the first executable gate.** This is source approval, not a claim that the new code compiles, passes, is faster, or is qualified on native ARM64. Root owns the first changed-source gate and must retain its first failures. Source expectations are 498 retained compiled identities plus six additions = 504; the original 13-case final_dispatch family must remain complete.

## Preserved behavior and source evidence

`original-tests-proof.json` compares all original test functions. Twelve are byte-identical. The only changed original function is `a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept`; replacing `final_drain_clock::backend(\n        &rig,` with `rig.backend(` restores its entire original function byte-for-byte. All its authoring arguments, session_end authoring/serialization, +800,000 target, withholding checks and retained RunEnd assertion are exact.

The complete original `final_dispatch.rs` is restored by the recorded source inverse, including its helper. `Rig::config` extracts the same directory creation and unchanged PhysicalConfig literal. The ordinary helper still writes the same bytes, runs the same cat/sleep/cat/tail script and constructs the same default PhysicalBackend with the same recording-only Player. There is no product constructor, production default, relay reader, final drain, workflow/profile or test-assertion change.

All 1,020 other files from the pinned baseline gate source inventory still match their baseline hashes. The new helper file was absent from that inventory. The doc addition is append-only and its inverse restores the retained clean69bae doc bytes; the baseline gate manifest does not include this prose file, so it is not falsely described as an independently captured clean1fb doc blob. The final original test file and docs are unchanged from the first reviewed variant; only the new helper changed during review.

## Real boundaries and ownership

The test-local module is selected by an explicit `#[path]` declaration in the final_dispatch integration binary. Its placement under tests/common does not automatically add it to other test binaries. Its backend constructor rejects unsupported delay/tail/line counts, and only the one selected original calls it.

The clock closure reads a Cell and performs no release, callback, polling or clock mutation. `Driver::wait` uses the unchanged public FixtureDriver hook. Its first wait holds the logical epoch until both the after-cat initial marker and the actual reader's complete-frame count of one exist. Since the shell writes that marker only after `cat now` returns, the original 300 ms delay begins after the same completed command boundary. Enqueue alone is not mistaken for cat completion.

At origin+300,000 us the driver sends one nonblocking release token, holds the clock there, and requires both the after-cat later marker and actual cumulative enqueue count two before advancing farther. The later file is written from the unchanged original serialized string and is emitted through the real shell stdout pipe, real framer and real backend channel. It is not inserted into the receiver or converted into a synthetic completion event.

The selected original reaches these waits through `inject`'s existing maximum 1,000 us steps and `queue_drained=false`. The helper ignores the queue-drained hint and never consumes a backend line. The original +800,000 target, 1,024-line final drain, terminal-cause latch and pending RunEnd retention remain unchanged. Its authored session_end status remains distinct from the actual shell's lifetime: that shell deliberately stays open until stdin EOF or owner cleanup.

The O_RDWR FIFO is held before the backend spawns the shell. Control writes are explicitly nonblocking; failure propagates from release rather than silently authorizing progress. Driver Drop attempts an abort token and then its File is closed. Actual child ownership remains with PhysicalBackend: fixture drop, stdin close, original host-clock two-second wait/kill/wait cleanup. No marker, count or queue-emptiness observation is presented as a child-reap receipt.

The direct held-shell and released-shell tests install an owning ChildGuard immediately after spawn. They observe actual try_wait status (which reaps an exited child), rather than a completion flag, before reporting success. The fallback ChildGuard kills/waits on unwinding. Their stdout is intentionally null because these two tests validate the shell's ownership protocol; actual pipe/framer progress is covered separately by the real backend boundary case and the selected original.

## Findings corrected during review

1. The first variant checked the host deadline before reading a filesystem marker, then accepted a positive result without a post-read check. A blocked read could therefore return late and authorize clock progress. The final helper checks before and after each marker read, then again before release-time publication and final logical advancement. The absolute ten-second deadline remains fixed; the new test-only expired-deadline override happens before that test invokes a driver wait. Direct child-test waits also reject late positive results.
2. The first four tests lacked negative evidence that the marker and enqueue requirements were a conjunction. Two new focused predicate tests now distinguish enqueue-only, marker-only, partial marker, both matching, invalid marker bytes and excess frames. They verify those observations neither advance the clock nor publish a release. These are explicitly predicate unit tests using manually supplied counters and files, not fabricated claims of real pipe progress. The actual driver still receives the real backend-owned counter; the actual pipe test exercises that production path.

The first reviewed source and its hashes are preserved alongside the correction diff. No changed-source test was run and then retried as part of this review.

## Limits and optional next evidence

The six-case variant does not execute a panic/abandonment before the initial marker. Its held-shell test first waits for that marker, so its name and evidence establish the initial hold after the shell is connected. Earlier abandonment remains covered by the unchanged backend cleanup owner in source and its kill/wait fallback, not by a new dedicated regression. The current doc describes that ownership and does not claim this absent regression has run. This is a coverage limitation, not a newly found faulty ownership path or a reason to infer an exemption from later gates.

A deterministic optional fixture-only regression is feasible without a new product API: prefix only that new test's script with a complete private shell PID record, followed by `exec 4< unopened_private_fifo`, and never provide that FIFO's writer. After the record and an actual waitpid(pid, NOHANG) observation establish that the owned child is live, deliberately unwind a closure owning the actual PhysicalBackend. The private FIFO causally prevents the original cat/initial-marker step, so neither stdin EOF nor the driver's abort token releases the prefix. Backend Drop must exercise its actual unchanged two-second kill/wait fallback. After catch_unwind, require waitpid(pid, NOHANG) to return ECHILD; a returned status fails because that observation itself would have done the reaping. An emergency PID cleanup guard must remain outside the catch and must not act before this verification. This adds the real fallback cost once; it needs no timing guess, release timer, fake join, or PID-absence inference. If added, review and freeze that additional source before compiling it.

The ten-second watchdog checks do not interrupt filesystem access, process spawning, OS kill/wait or cleanup; the doc correctly avoids an end-to-end host-bound promise. The helper currently supports only the nonempty StayOpen case. Actual transport EOF and receiver-sender closure still require separate evidence for the original closed-transport case. No whole-family clock migration or native qualification is claimed.

## Final source hashes

- `lampOS/crates/voice-eval/tests/final_dispatch.rs`: `aca9379bbb7c993267251c1103a88f91d06ad8468e7f3cc936efb0d44797227c`
- `lampOS/crates/voice-eval/tests/common/final_drain_clock.rs`: `fcc6988f7837440550a090ea6e2b04c9b9f569f3f7589558234780477880148e`
- `lampOS/docs/voice-test-fixtures.md`: `bd618f4471d5ec69dad490abc9e4747a91fdb21ef9fb1033a1e9d8f20e332940`

Snapshots, source inverses, all original function hashes, the unowned-input comparison, the six expected new identities and both review versions are retained in this directory. `SHA256SUMS` covers every retained review file except itself.
