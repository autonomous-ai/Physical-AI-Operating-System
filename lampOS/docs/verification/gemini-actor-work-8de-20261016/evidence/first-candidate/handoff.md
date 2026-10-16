Actor command work and maintenance — external source candidate

Recorded from date: 2026-10-16 10:15:43 +0700.

Ready for source review only. No Cargo, compilation or test execution occurred.
Pinned Rust 1.96 rustfmt parsed and formatted the four external Rust files; the
final check passed. The active worktree, Git/GitHub and released target were
not written or used for this candidate. Root's TaskJoins publication added 165
evidence files while this work ran; every one of the original 7,152 frozen inputs
remains unchanged. Those publication additions are recorded separately.

Basis: qualified corrected TaskJoins source on exact base
`1fb07c6f57ee280a49de7b4ddd74008c2a32a20d`, parent six-file diff
`82c92c5d8e6f9fb5f0dd297106a8c31ba2fbace319883be895f5c9b32e25504d`.
This is not claimed to be the eventual integration base. Existing gate evidence
belongs only to that parent preparation, not to this Actor candidate.

Five owned paths:

- `lampOS/crates/gemini/src/session.rs`: `59a72dce62b0e3340ba355fdc760ef819f79c385fc72bffcf7a9a7409962ae3c`
- `lampOS/crates/gemini/src/testing.rs`: `49e2a90924476d9abf3a896e26bb2750467cc594e6fa5ce3ee154b470d11e0a7`
- `lampOS/crates/gemini/src/testing/actor_work.rs`: `01b788d9b47efce06fec3547564af81782c9a829865bdadd0868aa740e8fa52a`
- `lampOS/crates/gemini/src/session/actor_work_tests.rs`: `4a3ff2c8feda35a184064efaed76eee2114d45fe11bbc900b69b5e118b76d6d5`
- `lampOS/docs/gemini-session-reliability.md`: `5e4ee47e1e422283bffbbfb4972dff25ea5490dc6d32aa6230b48839431ce946`

Proposed diff SHA `51eac14311476f5e6dd1707498fabc8de3a967a47a0e25972b82b4e0f6dc4665`.
Inverse SHA `3500ed8bb3e20606ff4ee4eef8ba239195823c47afd97ae0ccf94cd8de759bea`. The inverse was applied only to
external copies, restoring the three original files exactly and removing both
new files. `unchanged-body-proof.json` separately proves the original enqueue
and command bodies are byte-exact in their unobserved helpers. Removing only the
installation/field hooks restores the original setup body, and removing only the
two maintenance hooks and after-stop counter restores the original serve body.
Original Actor run/drain, write, image write, message, flush, next_event and age
checks are byte-exact. Every existing testing.rs function body is byte-exact.
No original test source or threshold was edited.

The new single-pair factory returns its real setup future, FakeService and one
ActorWork reader. It installs observation after real setup acknowledgement and
before InputSender publication or Actor spawn. Existing factories select Plain;
without fixture-clock/test configuration, the observer fields and hooks are
absent. Default wrappers delegate the original bodies exactly once, without an
extra task, select arm, yield, clock sample or protocol marker. The actual task
spawn, body, stop handling and Connection drop remain in their original order.

State is fixed-size: one Mutex, one AtomicWaker, immutable allocation identity,
monotonic checked counters, one active command, one last returned receipt and
one last completed maintenance receipt. The public reader is not Clone; its
single changed waiter requires mutable access. Recorder locks end before real
operations, awaits or wakes. Overflow, poisoning, wrong installation and crossed
snapshot identities cannot make observation successful or replace real results.
No input payload or unbounded receipt history is retained.

Accepted enqueue, entered handler, actual returned Result, dropped entered body,
post-stop discard and completed maintenance remain separate facts. An unpolled
handler wrapper creates no handling guard. A returned Ok may mean intentionally
kept audio; buffer facts do not claim a send. Both original self.command calls
(normal receive and image-held replay) reach the same wrapper. Image-queued
commands may leave mpsc yet never enter a handler; aborted image maintenance
stays incomplete. Acceptance publication can legitimately follow handler return
on another thread, so in-flight publications are explicit and not treated as
bookkeeping corruption.

Fifteen new source identities are listed in `added-source-identities.json`.
They are proposed tests, not passing tests. Real-path cases cover public factory
installation; accepted versus handled versus unconsumed output; a causally held
Audio write; original Closed/InputSequence results versus actual unit joins;
actual Actor panic and cancellation; KeptStart; image-held replay and aborted
maintenance; concurrent owned producer tasks; overflow/poison with real wire and
consumer results; canceled observation waits; unpolled setup; and default factories.
A separate explicit state interleaving covers publication after handler return,
late/wrong installation and post-stop count semantics. The exact after-stop
select-discard branch is hooked at its actual branch, but that direct branch is
not deterministically forced by these proposed tests; the actual stop regression
covers the original stop-aware write path instead. This coverage limit is explicit.
All real cases use one fixed five-second host deadline per fixture and no sleeps,
retries, marker messages or yield-count polling. The controlled AsyncWrite gate
creates a causal transport hold only inside new tests.

After source review, the proposed first gate keeps the same five-package graph
and Rust 1.96, jobs 2, incremental 0, locked/offline environment. First fmt and
strict all-target Clippy; then compile the complete graph and compare to a fresh
actual-base list before edits. Only after verifying exactly these 15 additions
should the focused filter `test(session::actor_work_tests::)` run once, with the
same package graph, default profile, one test thread, no-fail-fast and retries 0.
A whole-graph/build/doc gate follows only when authorized. No target is reserved
or claimed for this candidate, and none of these commands has been run.

This preparation offers no quiescence or clock-advance API. Even matching accepted
and returned counts require a separate frozen producer frontier, zero publication
in flight, no abandoned work/fault and a maintenance receipt covering that count.
That still does not prove pending select/timer work is exhausted. Actual socket
flush, remote frame/service handling, output enqueue versus consumption, service
join, outer worker return and child exit remain distinct unimplemented boundaries.
TaskJoins stays shutdown evidence only. Reconnect/Supervisor observation and every
original Voice scenario remain outside this slice.

The first external freeze stopped on whole-tree equality after root added its
publication files. `first-freeze-runner.py`, `first-freeze-stop.txt` and
`external-active-tree-change.json` preserve that bookkeeping event. The final
proof checks every original input plus separately recorded publication additions;
no source/test failure was hidden or rerun.
