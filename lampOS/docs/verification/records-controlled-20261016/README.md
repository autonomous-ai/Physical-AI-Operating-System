# Records controlled reporter — local qualification

Recorded 2026-10-16 05:51:39 +0700, from shell `date`. This separate continuation is based on Records preparation #922, exact head `e3d3366cad8113c3374b8e29faedb099b5e47d92`. Publication into lamp-voice waits for that dependency and native qualification; this is not a ready or native claim.

The original `real_reporter_reloads_switch_and_policy_and_requires_a_matching_receipt` now drives a dedicated real child through bounded inherited pipes. A single pure clock controls the original complete one-second report periods and Bus cadence. The original six status/receipt transitions, all 16 assertions, actual files and Bus Event Diagnostics, the four-second status deadline and 30-second child lifetime, and actual kill/reap remain. A Checked receipt reports completed construction/publication result at the next actual reporting wait; it cannot supply a Diagnostic, prove delivery or stand in for process exit. Ordinary executable/application/Reporter behavior stays byte-identical to the preparation.

The [independent source review](review/tests-823-records-controlled-independent-review.md), [mechanical reversal proof](local/source-proof.json) and [handoff](local/handoff.md) record exact boundaries and limitations. A source-review coverage caveat in one new test was corrected before execution: complete frame write, sole writer close, then exact read and EOF. Original assertions and IDs did not change; [the narrow diff](local/closed-peer-order.diff), pre-edit evidence and root review remain. No test failure or retry preceded that correction.

## Complete local gate

[Commands and date boundaries](local/root-gates.json) and complete unedited logs show:

- `cargo fmt --all --check`: pass.
- `cargo clippy --locked --offline -p lamp-records -p lamp-settings --all-targets -- -D warnings`: pass.
- Compiled before/after lists: every **172** original identity/ignore flag retained, exactly **20** boundary cases added; **192** total.
- `cargo test --locked --offline -p lamp-records -p lamp-settings --no-fail-fast -- --test-threads=1`: **190 passed / 0 failed / 2 existing ignores**.
- `cargo build --locked --offline --workspace`: pass.

Six new schedule cases and fourteen real-child/controller/framing cases include held actual publisher work, real Events, invalid/partial frames, immutable deadlines, failed commit, actual exit64 and cleanup. The actual one-second lifetime and four-second pipe-pressure cases intentionally keep real host waits. No production timeout is shortened.

## One original-case pair

The exact same serial nextest command, profile and zero-retry setting ran once before and once after. [Raw pair and metadata](pair/comparison.json): **5.614 seconds before → 0.735 seconds after**. Both passed the exact original identity. Baseline was at 04:53 with other gates active; migrated run at 05:47 followed their completion. These are shared-Mac debug observations with unequal background load, not an isolated benchmark, native result or whole-original-123 serial proof. Build time is excluded from the reported case durations. The four other profile-skipped entries in this filtered measurement are outside its one-case selection; the complete gate above covers the affected suites.

All ten owned source hashes stayed exact through the full gate and measurement. [Provenance](provenance.json) retains every byte copy and [SHA256SUMS](SHA256SUMS) covers packaged files except itself. No original check, retry policy, workflow/profile, runner, setting or device changed. #823's combined under-two-minute standalone serial target remains unproved.
