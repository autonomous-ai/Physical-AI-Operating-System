# Memory child fixture handoff

Date from `date`: 2026-10-16 02:43:05 +0700. Worktree `/private/tmp/lampos-tests-memory-progress`, base `aa32b9d1222f0bb4c66152cfacfc431d3f584a1c`.

Source is stable for the parent's independent review and execution gates. No Cargo, builds, tests, Git, network, escalation or device calls were run by this agent. Direct pinned rustfmt and its `--check` passed for the four new owned Rust files. Twelve new source tests are present; they are not claimed as executed.

## Scope and preservation

Only these six files belong to this slice:

- `lampOS/crates/memory/Cargo.toml` — SHA256 `9c76d873ad6b870b46a3e3a7fad8cb3bf80ee0a31ca4b5ccaaa1d72dedac1875`
- `lampOS/crates/memory/src/fixture_protocol.rs` — SHA256 `21f2d5ca2fc309c8b63989ff6a889f37edd5e3eb8a4fc5de305348d11a0efb0c`
- `lampOS/crates/memory/src/fixture_schedule.rs` — SHA256 `aa4cd9fb4c59fc82011131a3461870efe8eef43af74186fdb606bb18e58f54f6`
- `lampOS/crates/memory/src/bin/lamp-memory-fixture.rs` — SHA256 `2885b1d48defea67e2381f46f944aaf38c2035a67820eb68ac64c9b899c1dc2a`
- `lampOS/crates/memory/tests/fixture_child.rs` — SHA256 `95934ea55a18e979fc2396255ff8b7c87ca0b623a94305c570eff6aec7cf9e6b`
- `lampOS/docs/memory-fixture-clock.md` — SHA256 `d04bfd01f4950a559d8d3d989fcaacab575b3171cf86fefcc66b85463cc4a149`

`child-source-preservation.json` records all original Memory source hashes, original test names/assertion counts, new test identities, and exact manifest reversal. All 27 original Memory Rust files and all five seeded Bus/Reporter dependency files remain byte-identical. Removing the new fixture bin block and the narrowly authorized rustix fs/event dependency restores the original Cargo.toml byte-for-byte. The fixture's `test=false` binary includes the unchanged application source. Normal parser, guard, bind, optional factory selection, serving, error handling and finish stay in that shared body. Original integration sources and scenario windows were not edited.

The parent may need Cargo.lock's existing lamp-memory dependency list updated by its normal gate to include already-locked rustix; this agent did not resolve or edit the lockfile.

## Stable protocol and API

`fixture_protocol` exports `VERSION=1`, `MAX_FRAME_BYTES=4096`, `HOST_WAIT=8s`, `MAX_MARKER_BYTES=128`; structs Bootstrap, Connected, Step, Checked; Outcome {NotAttempted, ReturnedOk, ReturnedErr} serialized snake_case; valid_nonce, marker, read_frame/write_frame and Channel. Marker text contains its trailing LF. Channel::new(UnixStream) -> io::Result<Channel>, read_until<T>(Instant), write_until<T>(&T, Instant), try_clone_stream(), shutdown(). Generic framing is only byte bounded; Channel is nonblocking/poll bounded under the absolute caller deadline and consumes queued data before EOF. No timeout socket option mutations remain.

Bootstrap expects piped stdin containing one bounded frame then EOF. It requires version 1, a 1–32 ASCII alphanumeric nonce, absolute socket path, immediate actual parent directory mode0700 and actual endpoint socket mode0600. Parent must chmod the socket. Connected says only that this control connection exists.

FixtureSchedule::pair(nonce) -> (Arc<FixtureSchedule>, ProgressDriver). Schedule implements the existing public ReportSchedule, with pure zero-based checked time, real thread park/unpark, 128-entry nonblocking observation channel and fail-closed overflow. ProgressDriver::settle(&Step, Instant) returns a Checked only from a completed cycle followed by an actual positive wait. Round1/time0 is mandatory; later rounds consecutive, nondecreasing and no later than previous next_due. Before due, counters unchanged. At due, exactly one new actual cycle; equal-time settle does not wake or invent a cycle. State measurements and publisher wall/lease clocks remain untouched.

ChildControl::bootstrap() performs fixture-only bootstrap before invoking the shared entrypoint, starts the bounded control loop and returns schedule() for the existing selected-after-bind factory. The loop writes a real <=128-byte marker to actual stderr only after the completed/positive boundary, then sends Checked. Parent must independently read that exact complete log marker and actual Diagnostic datagrams. Its ordinary real child try_wait/reap remains authoritative.

## Boundary tests added (unexecuted)

- `reads_are_pure_and_equal_time_steps_do_not_create_reports`
- `pre_deadline_wait_preserves_the_full_period_and_due_requires_real_publication`
- `invalid_first_step_stale_round_backwards_time_and_skipped_due_fail_closed`
- `zero_left_wait_and_observer_overflow_cannot_produce_checked`
- `held_work_then_positive_wait_is_required_and_wake_before_park_is_retained`
- `close_freezes_clock_and_does_not_block_the_real_reporter_finish`
- `malformed_oversized_and_unterminated_frames_are_rejected`
- `bootstrap_requires_the_explicit_private_socket_and_matching_version`
- `complete_queued_frame_survives_peer_close_and_eof_is_not_a_receipt`
- `an_expired_absolute_deadline_rejects_even_queued_data`
- `real_channel_shutdown_wakes_a_waiting_reader_without_fabricating_a_frame`
- `actual_marker_follows_prior_bytes_through_the_real_log_relay`

The real-publication case uses Reporter::start_with_schedule, its real publisher, and an independent Subscriber receive. The held-work adapter test intentionally models the observation boundary; the unchanged Reporter scheduler tests separately hold the actual publication closure. The marker case uses lamp_ipc::log_relay::relay itself with actual same-stream prior bytes. No original tests or assertions were shortened or changed.

## Review limits and cleanup

Bootstrap's blocking stdin reader/connect may remain blocked until the dedicated process exits after its eight-second receive watchdog. The root accepted this no-new-thread-outside-child contract. Control closes its real socket and freezes/wakes the schedule; it joins only an already finished thread. Reporter::finish retains its unchanged 100ms bound. A real blocked stderr write similarly requires the parent-owned process deadline/kill/reap; no logical marker or receipt claims exit.

The narrow planned health log interval bound is 1744 bytes, below4KiB and relay64KiB, with one observed marker per interval. Arbitrary errors/log flooding are not covered. Parent must not advance after Checked alone. First optional Reporter cycle can be Opening/NotAttempted; Connected does not mean readiness. Existing default-off/failed-open scenarios remain host timed until separately reviewed. Parent helper files are outside this ownership and were not audited here.
