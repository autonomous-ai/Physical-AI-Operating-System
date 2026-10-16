Actor command work — corrected maintenance frontier, source review only

Recorded from date: 2026-10-16 10:27:20 +0700.

The root review finding is corrected only in this external candidate. The first
candidate remains byte-exact in both
`/private/tmp/tests-823-gemini-actor-work-candidate` and its frozen copy
`/private/tmp/tests-823-gemini-actor-work-candidate-first`. All 29 files covered by
its manifest were reverified; first manifest SHA is
`7a887f785a8a24af9de03032244352ee82e24f886689e0bce736381c72bbd903`. No first evidence was overwritten.

The first recorder read command/discard counts at maintenance completion. That
could count image-held commands replayed late inside send_image, even though
retire_active, apply_discard, purge, drain_held, flush and check_deadlines had
already run. The corrected guard captures both counts under the same lock at
maintenance entry, before the first original operation. Completion publishes
only those guard values. Commands returning later need a subsequent complete
pass before the coverage frontier includes them. Buffer facts remain explicitly
sampled at completion; they do not describe the entry frontier.

This adds two u64 fields to the existing stack-owned Maintenance guard. It adds
no allocation, task, await, clock sample, public counter or retained history.
The existing fault/result behavior is untouched. session.rs and testing.rs are
byte-exact to the first candidate: observation placement, default wrappers,
original operation order, actual consumer results and joins are unchanged.

The narrow correction changes only actor_work.rs, its new test file and docs:
`frontier-correction.diff` SHA `2645a9bdec131d005fd97bd03ea2c2017b2065bcc5f3ac031c8a31dc082079db`.
Its inverse SHA is `937549d01493b33bc2edd42291c27258cf25e0b2daa67ebbbe45edc3a1c667c1`. The inverse was applied to
external copies and restores all five first-candidate source files exactly.
`frontier-correction-proof.json` records command, dates, exit and hashes.

There are now 16 proposed source test identities, not executed tests. The new
`session::actor_work_tests::maintenance_covers_only_its_entry_frontier_until_a_later_complete_pass`
starts at returned/discard counts 1/1, raises both to 2/2 during a pass, and
requires that first completed receipt to cover only 1/1. The following complete
pass must cover 2/2. It separately checks completion-time buffer facts, intact
current counts, no incomplete pass and no fault. This is an explicit recorder
interleaving; it does not claim the Actor's after-stop select runs during
maintenance.

The real image-replay case still requires actual image and all queued Audio
frames, unchanged command counts/results, its original fixed host deadline and
actual task join. It now also requires the covering receipt's ordinal to be
later than the causally blocked image pass. Every original statement/assertion
in that case remains. Fourteen other first-candidate test bodies are byte-exact;
removing the exact three added statements (one saved ordinal and two assertions),
the accompanying comment and the appended new test restores the entire first
test file exactly. `first-test-preservation.json` records that proof. No old
repository test body or threshold was changed.

The full five-file candidate remains based on qualified corrected TaskJoins at
`1fb07c6f57ee280a49de7b4ddd74008c2a32a20d`, parent diff
`82c92c5d8e6f9fb5f0dd297106a8c31ba2fbace319883be895f5c9b32e25504d`. It is not the eventual integration base. Full
`proposed.diff` SHA is `84736e2e20e51b7732bd95cb2baad0e7d23a688487531db2dede9c9fd543a790`; full inverse SHA is
`8c2967d4b497368fb33138bf8159f38bf2b7f1484ef579b24c506c767a74bb86`. Applying that inverse externally restores all
three original files exactly and removes both new files. The original-body
proof and all 7,152 frozen active inputs still verify; root's
165 TaskJoins publication additions are separately recorded. Neither the
active worktree nor any build target was written or used for this correction.

The corrected five source hashes are:

- `lampOS/crates/gemini/src/session.rs`: `59a72dce62b0e3340ba355fdc760ef819f79c385fc72bffcf7a9a7409962ae3c`
- `lampOS/crates/gemini/src/testing.rs`: `49e2a90924476d9abf3a896e26bb2750467cc594e6fa5ce3ee154b470d11e0a7`
- `lampOS/crates/gemini/src/testing/actor_work.rs`: `ee2932572e624ea4b861aa708760b09e8a5f59067a2bf82eb25bd41adf59261f`
- `lampOS/crates/gemini/src/session/actor_work_tests.rs`: `9f7a8e26b83007e2a364dd11a490f3e64ee934f5504ddfd999265b9bf949527e`
- `lampOS/docs/gemini-session-reliability.md`: `6efaed0a12d3bd6038fcc8d5d819d3011d048572003a2eb1e5f30b99ae8bbb83`

Pinned Rust 1.96 direct rustfmt --check on all four external Rust files passed
at 2026-10-16 10:24:46 +0700. `rustfmt-check.json` and its raw log retain the
exact command and exit 0. This checks Rust formatting/parsing only: Cargo
commands, compilations and test executions for both Actor candidate versions
are all zero. Git, GitHub and devices were not used.

After source review, integrate on the then-current base and capture that clean
base's compiled list before edits. Use the same complete affected package graph
for Clippy, full list, filtered list and focused run; the parent preparation's
graph was lamp-gemini, lamp-memory, lamp-surround, lamp-voice and lamp-voice-eval.
Verify exactly these 16 added identities and unchanged old identities/ignore
flags before running the filter `test(session::actor_work_tests::)` once with
Rust 1.96, locked/offline, jobs 2, incremental 0, default profile, one test thread,
no-fail-fast and retries 0. No target is currently reserved for this candidate.
None of that qualification has been started.

This still does not establish idleness or permit clock advancement. A covered
frontier is historical maintenance evidence only. Acceptance publication,
actual handling return, socket flush, remote service/frame handling, output
enqueue/consumption, pending select/timer work, Actor join, outer worker return,
service join and child exit stay distinct. TaskJoins remains shutdown evidence.
Supervisor/reconnect integration and all original Voice scenarios remain outside
this slice. The direct post-stop select-discard branch is hooked but still not
deterministically forced by a real-path test; the real stop case instead covers
the unchanged stop-aware write path. That earlier coverage limit is retained.
