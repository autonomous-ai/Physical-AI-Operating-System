# Provider capture-clock ordering integration

Main integrated the exact four-file delta from Agent 1's `4d40fff3d` through
`da9891f7e` (published as `2e435da9e`). Ordered shared capture timestamps retain
monotonic transport instants even when the two host clock reads are preempted.
Genuinely decreasing timestamps in one request are refused before mapping,
history update or audio submission; equal times remain legal and a new request
starts a new comparison. Original capture receipts and the input-age bound are
unchanged.

The independent review first reproduced that an Instant-only check could hide
true shared-clock disorder. On the corrected source, six focused controls passed:
that regression, ordered preemption, equal timestamps/new request/state after
refusal, original stale/future bounds, retired successor isolation, and production
intake with valid audio, disorder and a queued End. The last proves intake latches
stopped and the queued End cannot pass under either outage-tolerance setting.
That extra audit test remains in the private review; it is not in this patch.
Scoped formatting and strict live/Gemini library Clippy passed.

This is a local fatal intake error (`capture time went backwards`), not a
recoverable per-turn outcome. The prior kept-start timeout under host load is
not resolved by this change. No new device, acoustic or service qualification is
claimed. The combined ARM gate follows integration; the installed runtime is
unchanged. The paired duplex tests use scripted capture and service events.

The patch changes four code files only and preserves the motor composition.
Private review artifacts and original failing outputs remain local. Digests and
scope are retained in [results.json](results.json).
