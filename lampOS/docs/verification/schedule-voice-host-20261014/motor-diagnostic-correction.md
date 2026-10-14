# Correction to the retained clean-7f diagnostic interpretation

Recorded 2026-10-15 on Third's development Mac. This corrects one statement
in [the original report](motor-diagnostics-clean7f/report.md), lines 99–100:

> Four recorded instances intentionally spawned no child.

Three markerless records actually belong to
`configured_camera_or_original_permissions_are_required_before_spawn`.
Each case starts an original `fixture("normal")` child, immediately requests
shutdown and waits for reap, then checks a separate candidate whose startup
is refused. No entry marker was captured from those original children;
their executed phase is unknown. The fourth record belongs to
`native_head_startup_announcement_has_no_input_or_motion_authority`, which
does not start its child and explicitly checks `!m.started()`.

The no-start conclusion applies to that fourth test and the three separate
denied candidates. Missing markers alone do not prove that no child was
spawned. The original report and raw readings remain unchanged; no test
count, hash or measured interval changes. The detailed
[source and ordering audit](motor-fullsuite-order-audit.md) identifies the
test locations and keeps hypotheses separate from observed failures.

No additional test or diagnostic was executed for this correction. It does
not identify the cause of the inherited timeouts or qualify Task 31.
