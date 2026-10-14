# Task 33: per-unit working-electrode update

The subsequent standalone CLI timeout, buffered-output correction and its
source manifest are recorded in [touch-output-host-20261014](../touch-output-host-20261014/).
Results in this folder precede that correction.

First's [20:05 +07 answer on #711](https://github.com/autonomous-ai/lampOS/issues/711#issuecomment-6169047710)
confirms that E4 and E8 are faulty on lamp-4ace, and that shell sensitivity
varies by electrode. Each lamp therefore supplies its working set and twelve
explicit touch/release threshold pairs. This folder records development on an
Intel i7-10700K x86_64 Mac, macOS 26.6.2 / Darwin 25.6, using the workspace's
pinned Rust 1.96.0. No lamp or electrical input was accessed.

`working-source.json` hashes the Controls/Msgs sources, tests, fixtures,
workspace manifests, lockfile and toolchain while based on `721d7c072`.
That earlier commit passed native ARM64 run **38564855931** with 3,987 passing,
zero failing and 29 inherited ignored tests. That run predates these changes
and is not the gate for this update. The final exact task head, full host gate,
source-only build and new native run are recorded in the Third log and PR.

## Behavior and boundaries

Three to twelve unique working electrodes may be selected. A sparse set must
be ascending or descending; its coordinates retain all twelve physical slots,
so E3 to E5 spans two slots when E4 is excluded. A complete twelve-pad order
keeps its earlier permutation and footprint rules. Sparse connectivity skips
only explicitly excluded electrodes; an inactive selected electrode still
makes an internally disconnected footprint ambiguous.

Raw recordings retain the complete acquisition. Logical contact state and
gestures use only working electrodes. An excluded electrode cannot start or
extend contact, advance held duration, or create Sleep. If the finger is over
only an unobservable faulty pad, the recognizer cannot prove continued touch;
the existing bounded release grace applies. This is not guessed continuity.

The MPR121 still scans all twelve electrodes because ECR cannot skip interior
channels. Only individual out-of-range bits for excluded electrodes are
ignored. Selected-electrode, proximity, reserved-status, over-current,
auto-configuration, auto-reconfiguration, static-register and I/O faults
remain terminal unknown. A faulty electrode can still cause a global
auto-configuration failure; the reader does not clear that failure or retry
with a guessed profile.

The new ten-pad fixture excludes E4/E8 and uses deliberately unequal authored
threshold pairs to exercise indexed configuration. Its values are not a
calibration for lamp-4ace or another lamp. The older fixture remains the
historical explicit profile reported by First. The existing 1,052-row
reconstruction produces byte-identical output with either working set: its
57 changed masks came from First, while continuous samples and monotonic
timing remain authored. See the [fixture provenance](../../../crates/controls/fixtures/strip-711-README.md).

## Focused evidence and review

`focused-results.json` identifies the exact commands and retained raw files.
The passing focused runs contain 53 strip checks, 43 MPR121 integration
checks, one filtered transport unit check, 40 coordinator checks, 16 pin
configuration checks, 17 replay checks and nine executable checks. These
overlap the full suite and must not be added to it as unique tests.

The first strip compile and concurrently attempted coordinator compile failed
with E0277 after the rank-gap bound became `u8`; the loop range now explicitly
uses `0u8..12`. Both raw failures and the subsequent passes are retained.
No assertion or inherited test was disabled. Scoped strict Clippy and format
checks passed; only the strip and coordinator format streams were retained.
The MPR121 format check produced no output and has no saved raw stream.

The focused commands ran from `lampOS/` with pinned Rust 1.96.0. A version
query from the parent repository instead reported its ambient Rust 1.97.1;
that was not the toolchain used for these tests. `toolchain.txt` records the
workspace version and active toolchain explicitly.

A read-only review covered sparse geometry, raw versus selected state,
contact freshness, exact selected-mask/profile agreement, independent
per-electrode threshold writes, and excluded OOR versus global faults across
the four changed implementation files. It found no additional defect. Tests
include first and repeated contacts at five authored sampling phases. Their
41.699 ms position and 61.699 ms Touch upper bounds assume 20 ms acquisition
and 1.7 ms receipt delay; they are injected-clock assertions, not measured
electrical or person-to-output latency.

## What First must run

Follow the concrete commands and qualification sequence in
[Controls](../../controls.md#run-without-a-lamp-then-qualify-on-one).
First runs the required switch-off minute using the exact green CI artifact,
then discovers that unit's head lines and measures each working electrode's
untouched/touched range through its shell before setting independent touch
and release thresholds. Start with an untouched strip and shadow requests.
Cross E4/E8 in both directions with a narrow finger and a broad hand, pause,
lift, hold near the two-second boundary, and inject read/configuration faults.
Check Voice's final-owner Stop/Sleep/Wake and privacy behavior separately.

The likely physical failures are uneven shell sensitivity, near-hand noise,
boot-held baseline, a faulty excluded electrode causing a whole-chip failure,
a narrow finger producing no working-pad evidence while crossing that gap,
TTP223 pulse behavior, I²C contention, service lock permissions and owner
latency. No on-lamp calibration, timing, sound, light or movement is claimed.
