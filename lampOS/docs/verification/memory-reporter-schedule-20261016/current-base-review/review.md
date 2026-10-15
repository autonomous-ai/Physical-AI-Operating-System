# Independent Reporter schedule preservation refresh

Recorded 2026-10-16 03:23:08+0700 from `date`. This is a read-only source review of `/private/tmp/lampos-tests-memory-clock` at current HEAD `c5f8d73372cf5addf14dcf7990425265beb7bd90`, after Bus #901 merged. No repository edits, Cargo/build/test execution, Git mutation, network or devices. Root owns runtime gates. This pure preparation is separate from the child control protocol and health scenario migration in `/private/tmp/lampos-tests-memory-progress`.

No concrete source blocker was found. The current change is exactly three files: `memory/src/health.rs`, new `memory/src/reporter_schedule_tests.rs`, and the schedule section in `docs/memory.md`.

## Fresh preservation proof against current HEAD

`audit_current_base.py` performs explicit, single-occurrence reversals of the reviewed seam. It does not merely apply the inverse of an arbitrary diff. The reconstructed entire `health.rs` is byte-identical to `git show HEAD:lampOS/crates/memory/src/health.rs`; its SHA-256 is `1b4f0c7b7f4a1f598e630a3f4611e081792eaaa67b1e750c70ed3a96ebb67ba0`. The digest and individual checks are recorded in `current-base-source-preservation.json`, alongside the actual reconstructed file. Removing only the added documentation section likewise reconstructs current HEAD's complete `memory.md`.

The entire original health unit-test module, including helper bodies, attributes, comments, assertions and all timing bounds, remains byte-identical. It contains nine active test cases and the existing ignored `measure_health_update_and_socket_with_a_held_publisher` timing case. All 42 original assertion macro sites are unchanged. The new eight-case module contains 41 assertion macro sites, with no sleep or ignored test.

All 12 original Memory integration source files match current HEAD byte-for-byte and retain the historical pre-seam hashes, including `tests/health.rs` (`31add261e22dda578ca5f5c0b9016a1382f7a1e3d87f6968a1d083ec722e394d`). All 25 tracked Memory Rust files other than `src/health.rs` match current HEAD, including those 12 integration files. The application factory/source preparation already merged upstream remains intact. There are no fixture binary, protocol, scenario constructor, request or original test-body changes in this tree.

Both owned Rust file hashes still match the original seam handoff and the post-Bus-merge record. The changed documentation hash matches the post-merge record; the new upstream documentation is preserved by reversal against current HEAD, not against the older baseline.

## Default path and clock boundary review

`health.rs:392–438` routes ordinary `Reporter::start` and the existing private test helper through `SystemSchedule`. Its `now` is exactly `Instant::now` and its wait is exactly `thread::park_timeout` (`:369–379`). There is no new default clock sample. The worker's one cycle-start sample (`:457`) remains before the real legacy count scan; its one deadline-check sample (`:506–508`) replaces the original `started.elapsed()` read with the same saturating duration calculation. The stopping checks, full one-second period, repeated positive-wait check after spurious wakes, and immediate next cycle after a real overrun retain their ordering. No catch-up queue or publication retry is added.

The State, diagnostic construction, count freshness/latency measurements, and legacy generation fence are byte-identical before the seam (`:1–328`). They retain their real clock. The real diagnostic header timestamp remains `lamp_ipc::monotonic_us()` (`:421`). Optional count scan, actual publication result, failure/recovery transition and publisher recreation retain their original branches.

The merged Bus source is itself unchanged from HEAD. Ordinary `Publisher::declared` already calls `declared_with_cadence_clock(..., Instant::now)` (`bus/src/publish.rs:230–248`). The Reporter supplies a closure reading the same schedule before the first declaration/presence work. Both paths converge on the same constructor, which does not sample time, then the same first presence sample after the real successful declaration (`:255–266`), and the same publication delivery sample (`:406–435`). Rescan/presence timing is therefore controllable without replacing the real Event, filesystem operation, datagram send, UTC or lease clock. Invalid declarations/failed declaration writes still do not add samples.

`CycleComplete` follows actual count/snapshot work, optional publish return and the failure/recovery logging decision (`health.rs:459–499`). `WaitChecked` follows a positive deadline calculation and precedes the wait (`:505–517`). Neither claims parking, subscriber delivery, final downstream log visibility, cleanup or OS exit. The custom schedule contract explicitly requires pure nondecreasing reads, a retained thread park token, and local nonblocking observation; the trait cannot enforce an arbitrary implementer's behavior.

Publisher drop still precedes the completion-channel send (`:520–523`), byte-for-byte. `state`, `finish`, and `Drop` are unchanged (`:533–557`): actual unpark, the original 100 ms host receive bound, and no unbounded join or replacement worker. No logical completion is promoted to actual thread or process exit.

## What the eight new cases exercise

| New case | Meaningful boundary checked |
| --- | --- |
| `schedule_reads_are_pure_and_the_default_source_uses_host_time` (`:169`) | Repeated explicit reads retain one instant; the default read lies between two real host reads. |
| `the_shared_worker_samples_once_at_cycle_start_and_once_at_the_wait_check` (`:184`) | Uses the actual shared worker and read-order instrumentation to prove one sample at each original site before any controlled advance. |
| `frozen_spurious_and_predeadline_wakes_do_not_publish_but_the_exact_deadline_does` (`:198`) | An actual unpark and the one-nanosecond pre-deadline boundary produce positive waits without publication; the exact deadline performs one real publication callback. |
| `completion_waits_for_actual_publication_and_overrun_has_no_catchup_queue` (`:238`) | A channel-held callback cannot produce CycleComplete; moving to ten seconds while held still produces only the completed old cycle and one new current cycle after release. The release guard also handles assertion failure. |
| `stopping_a_frozen_wait_unparks_and_preserves_the_final_best_effort_report` (`:278`) | Existing finish wakes a frozen schedule without advancing its time and performs the actual final best-effort callback; repeated finish does not add another report. |
| `stopping_a_held_publication_keeps_the_host_bound_without_false_completion` (`:303`) | A held callback preserves the host shutdown bound and keeps actual done empty until release, then completion occurs. A removed JoinHandle is not treated as proof of worker exit. |
| `publication_outcomes_follow_the_actual_error_and_recovery_results` (`:336`) | Typed observations follow actual callback error then success on two full due cycles, rather than assuming publication succeeded. |
| `successful_publication_is_not_a_receipt_and_the_shared_bus_clock_rescans_real_sockets` (`:388`) | The real Bus first publishes with no subscriber; a newly created real subscriber receives nothing retroactively. A full next due causes rescan and actual Event sequence 2 delivery, which is parsed and checked separately from the observation. |

The helper uses bounded progress channels and real host watchdogs for observation waits. These are focused seam tests, not a complete controller, runtime scenario migration, end-to-end duration measurement or proof that arbitrary filesystem operations are preemptible. The eight tests were read, not executed by this review.

## Held source and evidence

- `lampOS/crates/memory/src/health.rs`: `013c99487db64189c2ae78ca4588cb8449358e1496707311c3d79a7e0fd0632b`
- `lampOS/crates/memory/src/reporter_schedule_tests.rs`: `c324372847d39007cf5302890890947a844fdaa4e4b7853ec62ce13d278eaf28`
- `lampOS/docs/memory.md`: `4691b06b78f3de7e905f13beb7b8030c7e4ccf5055662bf5d6060a41c2adfc61`

Machine evidence: `current-base-source-preservation.json`, `audit_current_base.py`, exact `health.head.rs` / `health.reversed.rs`, exact `docs.head.md` / `docs.reversed.md`, and the current two textual diffs, all in this scratch directory. No runtime or speedup qualification is claimed.
