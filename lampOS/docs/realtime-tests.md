# Realtime tests (skipped on pull requests)

**Status: temporary, to be reviewed.** Since 2026-10-15 the tests that wait
on the real clock form the nextest test group `realtime`
([`.config/nextest.toml`](../.config/nextest.toml)). In the ARM64 run
(`.github/workflows/lampos-arm64.yml`):

| Run | Runners | nextest profile | Tests at once | Realtime group |
|---|---|---|---|---|
| Pull request into `lamp-voice`, manual run | 1 | `pr` | one per core | skipped |
| Push to `lamp-voice` (every merge; its binaries are what [updates](updates.md) downloads) | 1 | `pr` | one per core | skipped |
| Full run: nightly, or manual with `full` | 4 | `default` | one, as before | run |

The nightly full run starts at 19:00 UTC (02:00 in Vietnam) from
`.github/workflows/lampos-nightly.yml` on `main`: GitHub starts a scheduled
workflow only from the default branch. It starts this workflow on
`lamp-voice` as a manual run with `full`, so the job is defined in one place.
By hand: Actions, "lampOS on ARM64", "Run workflow", branch `lamp-voice`,
tick `full`; or `gh workflow run lampos-arm64.yml --ref lamp-voice -f full=true`.

When a full run fails, a job opens an issue titled "lampOS: the full ARM64
run failed", or comments on the open one. Nothing the tests assert was
changed. The runners are unchanged (`ubuntu-22.04-arm`): larger runners cost
too much.

Each runner compiles every test binary again (about 5 of its 10 minutes), so
one runner is used where the realtime group is skipped; those tests take
about a minute in parallel (run 38721013037: 4,399 tests, 55 s, 4 cores).
A merge costs one runner for about 11 minutes; until 2026-10-15 it cost four
for about 15.

## Why

The ARM64 run tests one test at a time, as on a lamp's few cores. On the
2026-10-15 runs 38715042828 and 38715014248, all 4,507 tests took 1,191 s,
and the 123 tests in this group took 977 s of it (82%). Each of the four
shards spent 4 to 6 minutes on tests, mostly waiting on sleeps, deadlines
and fakes that play a conversation at real speed. For example,
`serve_session::normal_eight_turn_conversation_in_one_serve_session` takes
65 s because `fixtures/voice-eval-serve-v1.json` has 26.5 s of `speech_ms`,
2 s `startup_ms`, 1.5 s `first_audio_ms` per turn and 6.6 s of trigger
`delay_ms`. Without the group, the rest take about 215 s one at a time.

The waiting is real time by design in most of these suites (see each file's
header). They are slow, not flaky; they did not fail in these runs (the
timed tests added later, under "What is in the group", are the exception). Skipping
them everywhere was rejected: they catch timing faults in whole
conversations, so the nightly full run still runs them.

## What is in the group

The group is a nextest filterset, written three times in
`.config/nextest.toml` (group override, `pr` exclusion, `realtime`
selection), which must stay identical:

- whole test binaries: `lamp-voice-eval::serve_session`,
  `lamp-voice-eval::physical_loopback`, `lamp-voice-eval::serve_relay`,
  `lamp-voice-eval::final_dispatch`, `lamp-memory::health`,
  `lamp-voice::capture_hold_worker`;
- whole modules of `lamp-voice`'s unit tests:
  `coordinator::tests::pilot_faults`, `coordinator::tests::endpoint_session`
  (`coordinator::tests::early_end` left on 2026-10-16, below);
- single tests that took 4 s or more outside those, named exactly;
- tests with a real-clock bound that a loaded runner misses. Pull requests
  run one test per core, and this failed twice on 2026-10-15/16, on changes
  that did not touch it:
  - `lamp-voice`'s module `coordinator::motor::tests::native_head`: a motor
    packet older
    than `MAX_AGE_US` (100 ms, `crates/body/src/runtime/channel.rs`) when it
    is read is refused as "motor packet boot/order/freshness violation"
    (runs 38797329569 on `lamp-voice`, 38776871505).

  It runs one at a time in the nightly full run. The fix is to give the
  test its own time, not to raise the bound the lamp runs with.

Because whole binaries and modules are matched, a new test added to one of
them is also skipped on pull requests and merges, and is first run by the
nightly full run after it is merged.

## Running it

From `lampOS/`, the group alone, one test at a time as on the runner:

```
cargo nextest run --release --workspace --profile realtime -j 1
```

What a pull request runs: `--profile pr`. Everything, as the full run:
`cargo nextest run --release --workspace -j 1`. `cargo test` is unaffected
and runs everything.

## Slow-transport wait entry point (#823)

`fake_lamp::slow_transport_with_wait` accepts a fallible `FnOnce(Duration)`
after the original argument parsing. It passes the complete millisecond delay
once, then uses the original real `exec`, arguments, and inherited streams.
A wait error returns unchanged before exec. The default `slow_transport`
still performs its one host sleep; neither entrypoint adds a clock sample.

This is preparation only. It adds no CLI or global switch and migrates no
test: coordinating a delayed relay still needs an explicit bounded parent
wait and proof that the real exec reached the relay before runtime stepping.

## Relay clock preparation (#823)

`lamp_voice_eval::physical::session::run_with_clock` accepts an explicit
`Fn() -> lamp_turn_owner::MonoTime`. It reads that source for relay protocol
timestamps, the session deadline and the SIGTERM grace, including a fresh
sample after a successful signal. A body `Clock` can be passed through
`|| clock.now()`. Clock reads must not advance time, and the relay and runtime
must use the same monotonic domain.

`session::run` remains the system-clock entrypoint used by the CLI. The 2 ms
relay poll, 10 s stop grace, process-group cleanup and late-stdout drain keep
their existing bounds and behavior; cleanup and the drain stay on host time.
No current CLI option selects a test clock.

[Boundary regressions and local gate](verification/relay-clock-20261015/README.md):
287 evaluator tests passed, including the unchanged realtime cases.

This is preparation only: it does not shorten the existing realtime tests.
Stepping a whole evaluator attempt still needs coordinated runner, relay,
child and player wakeups and acknowledgements, plus recorder/transport
participants where present. Their original logical durations, test identities
and assertion bounds must remain unchanged.

## Bringing a test back

A test leaves the group when it no longer waits on the real clock: an
injectable clock, a paused tokio clock, a time scale on a fake, or a shorter
fixture, without weakening what it asserts. Remove it from all three
filtersets in the same pull request, and update this list. When the group is
empty, delete it, the `pr` and `realtime` profiles, the `full` input and the
nightly workflow, and this page.

Review points:

- How often does the nightly full run fail on a realtime test, and how long
  until it is fixed? A failure names a day's merges, not one pull request.
- Who decides that a red full run holds the next merge? The workflow only
  opens the issue; it does not block merging.
- Does a pull request's run list the expected number of tests (about 4,400
  run, about 100 skipped)?

## The measured list

Seconds per test on the ARM64 runner, 2026-10-15 (the longer of the two
runs' times when both ran it). Binary, then test.

| s | Binary | Test |
|---|---|---|
| 65.0 | `lamp-voice-eval::serve_session` | `normal_eight_turn_conversation_in_one_serve_session` |
| 63.6 | `lamp-voice-eval::serve_session` | `stale_duplicate_reordered_foreign_and_lost_cues_end_the_conversation` |
| 48.3 | `lamp-voice-eval::serve_session` | `a_runtime_stop_mute_or_ignored_sigterm_is_retained_and_reaped` |
| 34.7 | `lamp-voice-eval::serve_session` | `a_provider_outage_is_waited_out_by_event_and_a_terminal_link_ends_it` |
| 34.1 | `lamp-voice-eval::physical_loopback` | `directed_missing_or_stale_retirement_never_falls_back_to_a_timer_or_final_trace` |
| 34.1 | `lamp-voice-eval::physical_loopback` | `missing_or_stale_whole_completion_withholds_followup_after_valid_retirement` |
| 30.3 | `lamp-voice-eval::serve_session` | `delayed_readiness_is_awaited_by_event_not_by_completion` |
| 26.7 | `lamp-voice-eval::serve_session` | `a_late_first_copy_or_failed_player_withholds_every_later_turn` |
| 26.1 | `lamp-voice-eval::physical_loopback` | `directed_interruption_during_second_occurrence_preserves_owner_and_yields` |
| 26.1 | `lamp-voice-eval::physical_loopback` | `directed_natural_interruption_runs_during_reply_with_original_cancel_reason` |
| 25.1 | `lamp-voice-eval::physical_loopback` | `directed_two_occurrences_wait_for_true_completion_before_followup` |
| 24.1 | `lamp-voice-eval::physical_loopback` | `directed_followup_waits_for_whole_reply_completion_then_admits_a_new_turn` |
| 24.1 | `lamp-voice-eval::physical_loopback` | `physical_next_turn_ready_follows_lamp_voice_readiness_cues` |
| 24.0 | `lamp-voice` | `provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity` |
| 23.1 | `lamp-voice-eval::serve_session` | `a_declared_capture_end_before_the_next_prompt_and_its_window_withholds_it` |
| 21.7 | `lamp-voice-eval::serve_session` | `startup_cues_stay_fresh_through_a_slow_transport_and_a_slow_first_preparer` |
| 14.1 | `lamp-voice-eval::physical_loopback` | `cue_triggered_acknowledgment_is_injected_during_playback_and_scored` |
| 14.0 | `lamp-voice-eval::physical_loopback` | `echo_only_window_passes_through_the_relay_without_extra_admissions` |
| 14.0 | `lamp-voice-eval::physical_loopback` | `undelivered_stimuli_are_withheld_not_scored_as_silence` |
| 13.0 | `lamp-voice-eval::serve_relay` | `a_runtime_that_ignores_sigterm_is_force_killed_and_never_clean` |
| 12.7 | `lamp-voice-eval::serve_session` | `an_extra_candidate_during_the_wait_withholds_the_rest` |
| 10.8 | `lamp-voice` | `coordinator::tests::early_end::an_answer_that_arrives_after_the_lamp_is_sure_begins_400_ms_sooner_than_without` |
| 10.5 | `lamp-voice-eval::serve_session` | `the_runner_forwards_pilot_runtime_settings_through_the_relay` |
| 10.5 | `lamp-voice-eval::serve_session` | `a_nonzero_runtime_exit_never_completes_a_serve_attempt` |
| 10.5 | `lamp-voice-eval::serve_session` | `serve_readiness_comes_only_from_the_fresh_cue_never_from_stdout` |
| 9.4 | `lamp-voice` | `coordinator::tests::endpoint_session::a_worker_failure_falls_back_through_recovery_and_then_resumes` |
| 8.3 | `lamp-voice` | `coordinator::tests::pilot_faults::rapid_mute_and_reopen_never_overlaps_workers_or_carries_speech_across` |
| 7.0 | `lamp-memory::health` | `legacy_reports_retained_counts_and_stays_well_after_caller_refusals` |
| 7.0 | `lamp-memory::health` | `a_failed_legacy_write_stays_warn_through_reads_until_a_write_succeeds` |
| 7.0 | `lamp-memory::health` | `sqlite_reports_retained_counts_and_stays_well_after_caller_refusals` |
| 7.0 | `lamp-memory::health` | `a_working_bus_that_fails_stays_failed_until_publication_really_recovers` |
| 6.4 | `lamp-voice` | `coordinator::tests::early_end::the_link_dropping_in_between_fails_the_request_unheard_and_the_next_is_answered` |
| 6.4 | `lamp-voice` | `coordinator::tests::early_end::speech_resumed_at_three_tenths_nothing_of_the_first_answer_is_played_and_one_turn_is_heard` |
| 6.4 | `lamp-voice` | `coordinator::tests::early_end::speech_resumed_at_three_tenths_under_the_lamps_duplex_policy` |
| 6.4 | `lamp-voice` | `coordinator::tests::early_end::without_the_option_every_message_and_record_is_what_it_was` |
| 6.4 | `lamp-voice` | `coordinator::tests::endpoint_session::hung_worker_never_delays_capture_endpoint_or_session_end` |
| 6.4 | `lamp-voice` | `coordinator::tests::endpoint_session::verdicts_after_the_result_timeout_fall_back_on_every_turn` |
| 6.1 | `lamp-voice-eval::serve_session` | `a_recorder_that_exits_zero_mid_conversation_withholds_the_rest_and_certifies_nothing` |
| 6.1 | `lamp-voice-eval::serve_session` | `a_room_recording_that_ends_early_withholds_the_rest` |
| 6.0 | `lamp-voice-eval::serve_relay` | `the_hard_deadline_stops_the_runtime_and_is_an_error` |
| 5.4 | `lamp-voice` | `coordinator::tests::early_end::a_lone_lamp_and_a_pause_and_the_question_go_as_one_request` |
| 5.4 | `lamp-voice` | `coordinator::tests::early_end::lamp_stop_over_an_answer_stops_it_and_only_the_answer_to_the_whole_is_played` |
| 5.4 | `lamp-voice` | `coordinator::tests::endpoint_session::duplex_complete_verdict_ends_at_one_second_and_its_answer_plays` |
| 5.4 | `lamp-voice` | `coordinator::tests::endpoint_session::a_complete_verdict_ends_at_one_second_and_its_answer_plays` |
| 5.4 | `lamp-voice` | `coordinator::tests::endpoint_session::rapid_resumes_never_end_the_turn_early` |
| 5.2 | `lamp-voice` | `provider_worker::tests::review_2026_10_14::review_a_name_heard_after_a_requests_sound_ended_does_not_let_out_its_answer` |
| 5.1 | `lamp-voice::capture_hold_worker` | `another_microphone_is_handed_on_at_once_and_its_early_sound_is_not_accounted_for` |
| 5.1 | `lamp-voice::capture_hold_worker` | `mic1_frames_wait_four_blocks_with_their_own_times_and_a_stop_drops_what_is_held` |
| 5.0 | `lamp-voice-eval` | `physical::mac::audit_grant_tests::audit_transport_that_outlives_its_bound_is_killed_and_never_certified` |
| 5.0 | `lamp-health::program` | `between_two_healths_five_seconds_apart_the_program_is_not_taken_for_gone` |
| 5.0 | `lamp-records::records` | `real_reporter_reloads_switch_and_policy_and_requires_a_matching_receipt` |
| 5.0 | `lamp-voice` | `coordinator::tests::pilot_faults::a_turn_under_way_when_paused_is_finished_and_the_next_is_not_taken` |
| 4.9 | `lamp-voice` | `coordinator::tests::endpoint_session::a_worker_still_loading_leaves_the_baseline_window` |
| 4.5 | `lamp-voice` | `provider_worker::tests::kept_start::with_no_first_answer_the_patience_ends_in_the_strict_timeout_and_no_end` |
| 4.5 | `lamp-memory::health` | `the_environment_alone_does_not_enable_reporting_or_create_bus_data` |
| 4.4 | `lamp-voice` | `provider_worker::tests::review_2026_10_14::review_sound_a_name_heard_as_a_request_began_lets_out_the_answer_the_rule_would_hold` |
| 4.4 | `lamp-voice` | `coordinator::tests::early_end::an_answer_that_arrives_before_the_lamp_is_sure_begins_at_that_moment` |
| 4.4 | `lamp-voice` | `coordinator::tests::early_end::an_answer_with_nothing_said_before_the_lamp_is_sure_is_never_played` |
| 4.4 | `lamp-voice` | `coordinator::tests::endpoint_session::duplex_missing_model_session_ends_on_the_unchanged_silence_fallback` |
| 4.4 | `lamp-voice` | `coordinator::tests::endpoint_session::missing_model_session_ends_on_the_unchanged_silence_fallback` |
| 4.4 | `lamp-voice` | `coordinator::tests::early_end::quiet_to_the_end_the_answer_starts_when_the_lamp_is_sure_and_not_before` |
| 4.4 | `lamp-voice` | `coordinator::tests::endpoint_session::complete_verdict_ends_early_but_resumed_speech_keeps_the_turn` |
| 4.2 | `lamp-voice` | `coordinator::tests::pilot_faults::failure_notice_plays_once_keeps_the_failure_and_yields_to_the_next_owner` |
| 4.1 | `lamp-actions::server` | `the_light_is_offered_when_its_owner_appears_and_withdrawn_when_it_goes` |
| 4.1 | `lamp-voice` | `provider_worker::tests::resumed_question_survives_a_silent_service_and_the_assumption_is_reported` |
| 3.6 | `lamp-voice` | `coordinator::tests::pilot_faults::resumed_the_lamp_answers_again` |
| 3.0 | `lamp-voice` | `coordinator::tests::pilot_faults::failure_notice_yields_mid_play_to_a_new_owner_under_vad_only` |
| 3.0 | `lamp-voice-eval::serve_relay` | `a_nonzero_runtime_exit_after_clean_metadata_is_a_relay_error` |
| 3.0 | `lamp-voice-eval::serve_relay` | `a_runner_stop_is_one_sigterm_and_a_clean_reaped_end` |
| 3.0 | `lamp-voice-eval::serve_relay` | `the_pilot_runtime_settings_reach_the_serve_child_exactly` |
| 3.0 | `lamp-memory::health` | `an_unusable_bus_does_not_prevent_serving_and_reports_recover_when_it_returns` |
| 3.0 | `lamp-voice-eval::serve_relay` | `a_closed_runner_transport_stops_the_runtime_through_the_same_path` |
| 2.9 | `lamp-voice-eval::final_dispatch` | `a_relay_framing_error_first_seen_in_the_final_drain_withholds` |
| 2.8 | `lamp-voice-eval::final_dispatch` | `an_invalid_cue_first_seen_in_the_final_drain_withholds` |
| 2.8 | `lamp-voice-eval::final_dispatch` | `an_unobserved_event_in_the_final_drain_requires_revalidation` |
| 2.8 | `lamp-voice-eval::final_dispatch` | `a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept` |
| 2.6 | `lamp-voice` | `coordinator::tests::pilot_faults::sigterm_while_metadata_storage_is_stalled_stops_cleanly_and_reports_the_loss` |
| 2.5 | `lamp-voice` | `coordinator::tests::pilot_faults::camera_loss_during_an_answer_revokes_only_sight` |
| 2.5 | `lamp-voice` | `coordinator::tests::pilot_faults::control_one_question_is_answered_once_then_mute_ends_the_run` |
| 2.5 | `lamp-voice` | `coordinator::tests::pilot_faults::readiness_publishes_the_link_state_it_was_validated_against` |
| 2.5 | `lamp-voice` | `coordinator::tests::pilot_faults::ring_fault_during_an_answer_keeps_the_conversation_and_cleans_up` |
| 2.5 | `lamp-voice` | `coordinator::tests::pilot_faults::duplex_control_answers_once` |
| 2.5 | `lamp-voice` | `coordinator::tests::pilot_faults::ring_lost_without_blanking_keeps_the_answer_but_cleanup_fails_truthfully` |
| 2.5 | `lamp-voice` | `coordinator::tests::pilot_faults::provider_failure_during_output_never_becomes_a_completed_answer` |
| 2.4 | `lamp-voice-eval::final_dispatch` | `a_final_drain_that_exhausts_its_budget_requires_revalidation` |
| 2.3 | `lamp-voice` | `coordinator::tests::pilot_faults::terminal_provider_link_during_output_settles_failed_and_ends_service` |
| 2.2 | `lamp-voice-eval::final_dispatch` | `a_healthy_session_still_dispatches_once` |
| 2.2 | `lamp-voice` | `coordinator::tests::pilot_faults::partial_write_then_stall_is_bounded_under_v1_baseline` |
| 2.2 | `lamp-voice` | `coordinator::tests::pilot_faults::control_progress_without_settlement_is_bounded_under_v1_baseline` |
| 2.2 | `lamp-voice` | `coordinator::tests::pilot_faults::duplex_stalled_speaker_is_bounded` |
| 2.2 | `lamp-voice` | `coordinator::tests::pilot_faults::stalled_speaker_during_output_is_bounded_and_never_completes_the_answer` |
| 2.1 | `lamp-voice::capture_hold_worker` | `mic1_muted_while_frames_are_held_hands_none_of_them_on` |
| 2.1 | `lamp-voice-eval::final_dispatch` | `audit_physical_boundary_does_not_dispatch_during_post_exit_trace_relay` |
| 2.1 | `lamp-voice-eval::final_dispatch` | `audit_physical_boundary_rejects_a_target_already_over_100ms_late` |
| 2.1 | `lamp-voice-eval::final_dispatch` | `a_known_invalid_cue_withholds_dispatch_as_a_protocol_failure` |
| 2.0 | `lamp-voice-eval::final_dispatch` | `a_known_session_end_withholds_dispatch` |
| 2.0 | `lamp-voice-eval::final_dispatch` | `the_first_terminal_cause_wins_and_a_provider_failure_stays_provider_unavailable` |
| 2.0 | `lamp-voice-eval::final_dispatch` | `a_known_run_end_cue_withholds_dispatch` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::provider_process_lost_before_output_ends_the_run_without_speech` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::provider_process_lost_during_output_stops_speech_and_fails_the_turn` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::mute_while_answer_speech_is_queued_cuts_it_and_never_resumes` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::mute_while_the_failure_notice_plays_cuts_it_and_keeps_the_failure` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::capture_fault_during_output_cancels_the_answer_and_ends_the_run` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::sigterm_while_answer_speech_is_pending_stops_cleanly` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::sigterm_while_the_failure_notice_plays_stops_cleanly` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::provider_failure_before_output_settles_failed_without_speech` |
| 2.0 | `lamp-voice` | `coordinator::tests::pilot_faults::provider_worker_fault_during_output_fails_the_turn_and_ends_the_run` |
| 1.6 | `lamp-voice::capture_hold_worker` | `a_microphone_busy_for_a_moment_at_start_is_opened_once_it_is_free` |
| 1.6 | `lamp-voice` | `coordinator::tests::pilot_faults::a_paused_lamp_takes_no_question_and_says_why` |
| 1.6 | `lamp-voice` | `coordinator::tests::pilot_faults::stalled_speaker_under_vad_only_is_bounded_too` |
| 1.6 | `lamp-voice` | `coordinator::tests::pilot_faults::control_progress_without_settlement_is_bounded_under_vad_only` |
| 1.6 | `lamp-voice` | `coordinator::tests::pilot_faults::partial_write_then_stall_is_bounded_under_vad_only` |
| 1.1 | `lamp-voice` | `coordinator::tests::early_end::a_name_note_told_with_a_request_ended_early_does_not_survive_it` |
| 0.8 | `lamp-voice-eval::final_dispatch` | `a_transport_closed_during_the_final_drain_withholds` |
| 0.7 | `lamp-voice` | `coordinator::tests::endpoint_session::unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service` |
| 0.7 | `lamp-voice` | `coordinator::tests::pilot_faults::capture_stall_during_a_question_ends_the_run_without_an_answer` |
| 0.6 | `lamp-voice` | `coordinator::tests::endpoint_session::mute_then_reopen_never_overlaps_endpoint_workers` |
| 0.5 | `lamp-memory::health` | `a_store_that_cannot_open_never_claims_readiness_and_any_delivered_report_is_warn` |
| 0.3 | `lamp-voice` | `coordinator::tests::endpoint_session::confirmed_endpoint_exit_keeps_the_restartable_mute` |
| 0.0 | `lamp-voice-eval::physical_loopback` | `a_transport_that_closes_mid_trace_invalidates_the_attempt` |
| 0.0 | `lamp-voice-eval::physical_loopback` | `directed_mode_keeps_provider_fault_and_asset_restrictions` |
| 0.0 | `lamp-voice` | `coordinator::tests::pilot_faults::distinct_rejects_an_item_that_returns_later` |
| 0.0 | `lamp-voice` | `coordinator::tests::pilot_faults::speaker_progress_is_due_strictly_after_lease_plus_report_age` |

## Prepared fake-player clock source

`FakePreparedRoomPlayer { room, faults, prepared }.with_clock(...)` accepts a
pure, cloneable, thread-safe `Fn() -> MonoTime`. Each worker owns a clone; all
clones must read the same clock domain without advancing it. Attempt identity,
readiness, grant receipt and first copy retain four distinct clock samples.
The first-copy sample still refuses a grant at equality with its expiry.

Both paths share the original preparation implementation. Plain struct
construction keeps the host clock's full nanosecond precision. The explicit
clock uses checked microsecond-to-nanosecond conversion: overflow before
thread creation returns an error; later overflow returns an invalid failed
report identifying the sample stage, without sending the room datagram.
Host resource polling, fault delays, grant waits, validation, cancellation,
datagram sends and joins retain their existing behavior.

Focused tests synchronize through the existing ready and grant channels and
join the worker before checking delivery. Scripted read-order probes cover
fresh first-copy expiry and overflow; these probes are instrumentation, not
scheduler clocks. Existing tests are unchanged. This #823 seam does not
migrate a serve session or establish a speedup.

## Dispatch fixture shutdown

The 13 final-dispatch cases keep their assertions and timed evidence. The
stay-open shell transport now consumes control lines silently and exits on
stdin EOF from the existing backend Drop, avoiding its two-second kill
fallback on twelve cases. This replaces the helper's independent 30-second
lifetime; it does not change a production deadline or accelerate the remaining
timed stimuli. The selected group is unchanged.

[Local serial comparison](verification/dispatch-transport-20261015/README.md):
13 passed before and after, 29.938 s to 6.487 s on x86_64 macOS. This is one
fixture cleanup step in #823; the whole-group two-minute goal remains open.

## Recorder clock and wait preparation (#823)

`physical::fake_lamp::room_recorder_with_clock_and_wait` accepts a pure
nanosecond clock and one wait adapter for the recording-failure fixture. The
optional declared capture end samples that clock before the actual
`ready.json` write. After the write, the adapter receives the full original
wait duration and the same clock, so it can take a separate fresh sample for
its wait start. The declared capture end does not establish when the recorder
exits. Reading the clock must not advance time.

The ordinary `room_recorder` entrypoint still uses `lamp_ipc::monotonic_ns`
and one host sleep for `min(fail_after_ms, seconds * 1000)` milliseconds. Its
arguments, optional sample count, arithmetic, private directory and ready-file
contents, and configured return code are unchanged. This failure fixture
still writes no audio or final `metadata.json`, including when its return
code is zero. A function return does not replace observing the owned child
process's actual exit and reap.

This seam does not select a new CLI path, add a recorder participant, alter
backend readiness or cleanup bounds, migrate the three recorder integration
cases, or establish a speedup. Those cases retain their original bodies and
assertions. Coordinated recorder progress still needs a separate ready-write
acknowledgement, its full wait, and an actual-exit barrier in the shared clock
domain before it can join a controlled serve attempt.

## Evaluator backend clock entry point

`PhysicalBackend::new_with_clock` accepts a pure `Fn() -> MonoTime` for
runner timestamps, clock-map samples, cue freshness, grants, dispatch and
pre-end logical deadlines. `PhysicalBackend::new` keeps the existing system
clock. `FakeRoomPlayer { room }.with_clock(...)` uses the same source for its
immediate delivery report; the original struct construction still uses the
system clock and both paths share the datagram/report implementation. The
microsecond-to-nanosecond report conversion rejects overflow before delivery.

This prepares #823's coordinated evaluator fixture. It does not advance time,
replace the existing channel waits or sleeps, or accelerate any existing test.
Every participating relay, runtime and player must share a clock before a
scenario can use a controlled time source. Prepared players and recorders have
not been migrated. The 10 s runtime self-end wait and the overall pre-end bound
use the runner clock; the 300 ms drain after `session_end`, actual child exit,
25 s player join, 5 s unused-player join, recorder readiness and all cleanup
bounds remain on host time, with their existing limits.

Focused regressions cover pure reads, separate ping/receipt/cue-decision
samples, a completed session drained and reaped with a frozen clock, identical
fake-room datagrams and report shape, timestamp overflow refusal, and the
unchanged system defaults. Existing test assertions, identities, profiles and
workflows remain unchanged. This entry point alone establishes no speedup.

## Stepped finite loopback fixtures

The finite `physical_loopback` scenarios now select an explicit
`physical::fixture_driver::ClockedFixture`. The runner, immediate fake player,
real relay process and finite fake runtime share a pure `MonoTime` source.
Only the parent scheduler advances it. The two quick transport/restriction
checks keep their original construction. All 12 identities, all 100 assertion
macros, the original scenario durations, fault iterations, cue freshness and
grant bounds remain unchanged.

The driver recognizes only the evaluator's direct local fake-runtime command.
It selects `fake-clock-session` and `fake-clock-lamp-voice` with a private
control socket. Regular commands and constructors keep their system clocks,
2 ms host polling and existing reader bounds. Serve, prepared players,
recorders, arbitrary transports and reuse for another attempt are rejected by
this finite driver; they need their own coordinated participants.

Each control round names its time and the complete input frames it must
consume. The runtime polls first, then the relay consumes the acknowledged
runtime stdout and runner controls. Only successful room sends count as
expected runtime input. The relay acknowledges complete output after flushing;
the backend reader must actually enqueue that output before the parent moves
on. `next_event` drains queued frames before advancing. A scheduled injection
preserves its existing wait and final-drain ordering. Reads never move time,
and the original 2 ms runtime/relay and 10 ms model tick intervals remain.

The private socket uses bounded newline-delimited frames, role/round/counter
checks and absolute host watchdogs. Nonblocking reads/writes use bounded
readiness polling, so queued final frames remain readable after peer closure.
A failed participant closes control peers before
the backend's existing process cleanup. Terminal runtime progress freezes
logical time while the relay observes actual child exit and streams the final
trace. Only the relay's final flushed `Trace`/`SessionEnd` output completes its
acknowledgement; logical completion never substitutes for OS reaping.

Final trace output can exceed the normal 4,096-line reader queue while the
parent waits for that acknowledgement. The explicit fixture reader therefore
accepts at most 50,404 transcript records plus one reserved error slot, using
nonblocking enqueue. That bound covers a maximum 25,000-line trace, up to
25,000 cues, 400 pings and four control/fault records. The existing per-line and 96 MiB
byte caps still apply. Overflow publishes a terminal error and closes the
reader instead of blocking the child before its acknowledgement. The default
reader retains its original queue and blocking behavior.

[Fresh corrected-channel release evidence](verification/finite-loopback-clock-20261015/README.md)
passes the same 12 compiled tests before and after: **239.720 s to 11.439 s**,
serial nextest on x86_64 macOS, with no retries, ignored or skipped cases.
The evaluator helper hash is identical; only the authorized test constructors
change. Nextest execution excludes build/list time. This is an observed pair
on a shared host with uncontrolled independent load, not a quiet benchmark.

The corrected full local gate passed 352 evaluator tests before rebase. All
57 evaluator Rust files remain byte-identical across rebase; new qualification
passed 150 release library tests, evaluator all-target clippy, formatting,
workspace build and the release pair. The full 352-test run was not repeated
after rebase. The earlier 248.575 s to 28.471 s debug pair remains historical
pre-#879 evidence. Native ARM64 speed and the complete 123-test under-two-minute
target are not established. Profiles and workflow selection are unchanged.

## Finite fake-runtime clock and poll boundary

`physical::fake_lamp::run_with_clock_and_poll` is an explicit library seam for
coordinated finite fixtures. The clock callback returns `MonoTime` without
advancing it. A separate poll hook runs after the original tick, event/stdout
drain, production `CueSink` scan, fault relay and room-input consumption. The
snapshot counts complete flushed stdout lines, successfully decoded and
injected room datagrams, and trace records. Its poll number advances even when
time is frozen, so a sender's successful datagram write cannot masquerade as
runtime consumption.

The wrapper keeps the original base sample and fresh clock reads for the tick,
CueSink and fault relay. Runtime ticks remain 10 ms; the stale-cue fault still
holds the original bytes for 150 ms with their original 100 ms expiry. The
existing `run` and CLI keep system time, the two-millisecond sleep between
continuing polls, and their existing serve path. The controlled entrypoint
rejects serve before producing output because that path still needs
coordinated clock advancement and acknowledgements of completed polls. Its
clock source alone does not provide that control. No CLI flag or environment
variable enables this entrypoint.

A hook may acknowledge completed work and park until the coordinator's next
poll, with a separate bounded host watchdog supplied by that coordinator. The
terminal poll reports that the simulated runtime ended; the final trace is
written and synced only after the hook returns. That snapshot does not prove
file durability or subprocess exit. The outer fixture must still wait for the
actual child and relay completion. Focused tests cover frozen-time polling,
the exact tick boundary, consumed room input before its next tick, production
cue delivery and the unchanged stale-completion delay, serve refusal, and hook
failure propagation. This preparatory seam does not migrate the loopback
identities or establish a speedup.

## Serve fixture clock source

The offline `physical::fake_serve` runtime exposes
`run_with_clock(FakeServe, Fn() -> MonoTime)`. Its existing `run` entry point
still supplies the host monotonic clock. The caller owns time advancement;
reading the supplied clock does not advance it. The nine existing logical
clock reads use this same source, including separate fresh samples for cycle
metadata and the runtime's origin. Retained startup readiness keeps its
150 ms age and saturates at zero, as before.

The 2 ms host sleep, SIGTERM/SIGINT handling, room and cue sockets, fault
behavior, metadata flushes and final stdout receipt retain their existing
behavior. The focused clock tests call cycle helpers directly, without
registering process-wide signal handlers. They check repeated reads at the
same instant, distinct startup samples, restart identity and retained-history
timestamps. The 14 existing `serve_session` cases and their assertions remain
unchanged.

This is a clock-source seam for #823, not a serve-session speed measurement.
The CLI still uses the system clock. A later controlled serve scheduler must
coordinate prepared-player readiness and grants, recorder and delayed
transport progress, signal observation, final metadata output and actual
child exit before those cases can use logical time. A logical end marker is
not evidence that the process has exited.

## Body's live-hand age bound (#902)

`bus_out::tests::a_hand_that_waited_in_the_queue_is_not_said_as_fresh`
(`crates/body/src/bus_out.rs`) failed once on a loaded ARM64 runner (run
38787471620): its live case asserted `expires_ms >= 990`, and the step took
more than 10 ms before the hand was said. The live case now measures its own
interval, from the observation's start to `step` returning, and asserts
`expires_ms + took_ms + 1 >= 1000`: the observation's age can be no more than
that whole interval, and 1 ms covers truncation to whole milliseconds. Nothing
is relaxed for stale hands: the 1.2 s queued sample is still dropped, the
0.6 s one still expires within its residual 400 ms, and a slow preceding
joint-state write still ages the hand
(`a_hand_after_a_slow_joints_write_is_aged_by_it`).

Author's evidence (fifth, a Mac, not a reviewer run): 200 runs of this
test under parallel build load (load average about 25), no failure.

## Expression's runtime fixtures and the wall clock (#912)

The reviewer's full serial gate on #844 failed six Expression tests on
stale inputs (joints 1,263 ms and 2,301 ms old at the step):
`real_service_subscribes_to_exactly_one_complete_voice_protocol`
(`tests/rest_replay.rs`), and in `tests/runtime.rs`
`enabled_output_uses_a_new_session_and_expires_after_input_loss`,
`paused_by_the_keeper_nothing_is_asked_of_the_body_or_the_light_until_resumed`,
`without_a_pause_said_expression_asks_exactly_as_before` and the two
`sleep_name` cases. The service judges each input's age by the wall clock it
is given against the bus's own publication stamp, and four of these tests
read that clock only after making the service, with the inputs already
published.

What changed, tests only:
- `populate` returns the wall clock read just after its last publication,
  and the tests step with it (and `+ 2_000` where they step 2 s later). The
  sleeping-name fixture and the real-service case read it the same way. A
  stall after that read, making the service or anything else, no longer ages
  the inputs.
- Each fixture publishes from the loosest limit to the strictest, so the
  inputs with short limits go back to back just before the clock read:
  `populate` the light (a setting, no age), the voice timing (500 ms), the
  joints (300 ms in the engine); the sleeping-name fixture the room (2 s),
  the voice's progress and the faces (500 ms), the joints; the real-service
  case the room and the light, the progress (500 ms), the joints, the
  playout and sound (200 ms).
- Nothing in the service changed. The deliberately stale and future inputs,
  the expiry after input loss (`wall + 2_000`), the restart that must not
  reuse old joints, and the pause's one-second look are all as before.

The limit that remains: a stall inside a fixture's own publications, after
an input with a short limit and before the clock read, longer than that
limit (200 to 500 ms), still fails. Two injected 1.3 s delays at
`fe8dd0ebd`, both in throwaway copies:
- Third's, after the first publication of each fixture (before the
  reorder): `lamp-voice` `48b3737d5` failed exactly the six, `fe8dd0ebd`
  passed all 34.
- The reviewer's, after the faces in the sleeping-name fixture's first step:
  the old order failed 6 of 34 and `fe8dd0ebd` failed the 2 `sleep_name`
  cases, the faces being 500 ms long.

With the reorder the faces are followed only by the joints' publication, so
that window is a single publication, not removed.

## Coordinated Serve-session fixtures (#823)

`ClockedServeFixture` coordinates the real relay process, fake Serve runtime,
prepared-player threads, optional fake recorder and delayed local transport.
All fourteen original `serve_session` identities use this explicit clock,
including their twenty-two attempts. Their complete test bodies, 114 assertion
macros and original fault inputs remain unchanged. Ordinary fake commands keep
their host clocks; control requires the explicit `fake-clock-serve-session`,
`fake-clock-serve-runtime`, `fake-clock-room-recorder` and
`fake-clock-slow-transport` commands.

Clock reads remain pure. Advances retain the original 2 ms poll boundaries and
require actual completed work and counted pipe, cue and room-input progress.
Prepared readiness and expiring grants still use the original channels, real
room datagrams and 100 ms lateness bound. A continuing relay's successful
SIGTERM send requires actual runtime signal observation at the next original
poll. A completed terminal relay stops further runtime steps, including after
forced kill. Runtime completion holds logical time during final metadata,
relay completion and child exit.
A terminal progress receipt does not prove an OS process or prepared worker has
exited. Bounded host guards, real kill/reap and thread completion remain separate.

The recorder adapter uses the existing parser and clock/wait entry point. Its
first progress receipt follows the actual `ready.json` write; the backend still
reads that file through its bounded parser under the original 15 s readiness
guard. The three recorder controls retain their full 6 s, 6 s and 23 s waits.
The optional declared 20 s capture end is not an exit receipt. A terminal
recorder acknowledgement freezes time until the backend observes its owned
`Child` exit; recording coverage and validity still use the original checks.
After relay completion, recorder and prepared-player work can drain at the
original 2 ms cadence, with recorder exit confirmed before player completion.

The delayed-transport control retains the entire 400 ms delay and 2300 ms first
preparation. Its wrapper neither consumes queued stdin nor fabricates stdout.
After acknowledging the completed delay it closes its control participant and
performs the original exec with the original arguments and inherited streams.
The controller separately requires a fresh ServeRelay registration from that
real relay before runtime bootstrap. The delay acknowledgement alone cannot
establish exec, relay readiness or child exit. Startup uses one cumulative host
deadline; later logical rounds cannot reset it, and a shorter preparation
deadline still wins. Recorder due times also bound each logical advance.

This migration covers the original Serve-session family. The six separate
direct `serve_relay` tests retain their existing path. Clock-controlled results
check ordering and the full configured windows; they do not measure hardware
latency or establish a result for the complete original 123-test inventory.

## Serve relay test coordinator

The six `serve_relay` cases use an explicit helper clock while retaining their
original test bodies, runner delays, fault settings and assertions. The relay
and its coordinator run on the test thread; the existing session body still
owns the real child, reader threads, Unix sockets, SIGTERM, process-group kill,
reap, metadata checks and final output. The helper replaces its runner sleep
with the full logical three-second boundary and advances the runtime and relay
only after completed polls, using the original two-millisecond interval.

A real Stop write is counted only after success. Closing the runner produces
actual EOF; the helper holds time until the relay's real stop-signal receipt.
The runtime then observes the actual signal through its existing handler.
Final runtime progress freezes time until the relay observes actual child
exit. The one-second run plus five-second allowance and the full ten-second
ignored-signal grace remain unchanged. Host control, cleanup and late-output
bounds remain active; a held post-stop boundary is checked against a ten-second
host budget before and after polls. The existing reader-enqueue barrier can
still wait up to thirty seconds before returning a failing poll; this is not a
strict ten-second abort deadline. No progress receipt substitutes for a process
status.

The explicit controlled serve command reports its complete received argv,
including its leading control pair, exactly once through the existing flushed
JSON stdout record. Only the validated suffix is parsed as runtime arguments.
Ordinary fake commands and the direct controlled library API retain their
existing argument reporting. The original serve suffix and child-versus-spawn
argv assertion remain unchanged. This implementation needs fresh paired and
native qualification before a speedup can be claimed.

## Pilot capture stalls on the stepped fixture (#893)

Six `coordinator::tests::pilot_faults` capture-stall cases now select the
existing `Fixture::Stepped`, as `second_route` did (lampOS#890, #944):
`a_half_second_capture_stall_is_a_gap_and_the_voice_goes_on`,
`a_microphone_closed_by_a_stall_reopens_under_a_new_epoch_and_the_voice_goes_on`,
`a_stall_inside_a_question_abandons_it_and_the_voice_goes_on`,
`a_reopen_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on`,
`a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on` and
`a_short_xrun_reopen_inside_a_question_abandons_it_and_the_voice_goes_on`.
On `Fixture::Threaded` their privacy peer ran on the host clock, and a loaded
host let it fall behind the 50 ms freshness check. Their assertions, stall
lengths and every production bound are unchanged.

The stepped capture peer sends the blocks a stall held back in one step.
When that burst fills the 64 KiB data socket, the block that did not fit is
no longer lost: it stays due and goes next step with its own acquisition
time, as the real capture worker never skips a sequence. A lost block left a
sequence hole that the coordinator rightly refuses as `invalid original
microphone read interval`. With the socket buffer cut to 40 KiB,
`a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on` failed
that way before the change and passed after it.

Still separate: `provider_terminal`'s same cause is #975 (PR #982), and the
original-123 realtime-group denominator above stays frozen.

## Pilot SIGTERM raised on the stepping thread (#739)

`pilot_faults`'s `Act::Sigterm` used to send SIGTERM to the whole process.
Any thread could take that signal, and the handler could run late on a loaded
host. The stepped scenario clock kept moving in the meantime: the stop was
seen 0.27 to 1.7 s of scenario time after it was sent. That let
`sigterm_while_the_failure_notice_plays_stops_cleanly` play its notice to the
final sample, which failed 2 to 6 of 40 runs with 20 `yes` workers. The act
now raises the same SIGTERM on its own thread, which runs the installed
handler before `raise` returns. The production `Termination` path, every
assertion and every bound are unchanged. The three `sigterm_while_*` pilot
cases then passed 40/40 under 20 `yes` workers and 20/20 under SIGSTOP 80 ms
every 300 ms.

## Pilot faults all on the stepped fixture (#973)

#972 moved the six capture stalls above. #973 moves the last threaded
scenario, `rapid_mute_and_reopen_never_overlaps_workers_or_carries_speech_across`,
so every `coordinator::tests::pilot_faults` scenario runs on the scenario
clock, and the threaded fixture, unused after, is gone. A loaded host cannot
starve a peer past the 50 ms privacy or 100 ms capture lease, because logical
time advances only after every peer has run. Production bounds, stall
lengths and assertions are unchanged.

A run's `started_us`/`finished_us` (first child spawned, every child
reaped) are taken on the host clock: they are real process events, and
`rapid_mute…` compares them across runs, whose logical times are their own.
Its cross-cycle privacy wait was already a separate mock thread. The
SIGTERM raised on the stepping thread is the section above (#739).

## early_end on one logical clock (#823)

`coordinator::tests::early_end`'s 13 conversations ran in real time: the
coordinator, three peer threads polling every millisecond, and the real
provider worker, Gemini session and scripted service on a runtime of their
own. Each lasted its script's `seconds` (4 to 8; the cut-in case runs twice),
93 s for the module. They now share one logical clock, all on the test's
thread:

- the coordinator and its capture, privacy and speaker peers on
  `scenario_clock`, the peers run inside its step, as in `pilot_faults`;
- the provider worker, its Gemini session and the scripted service on a
  paused current-thread Tokio runtime. Each step first advances it to the
  step's logical instant (`sleep_until`), its timers firing in order on the
  way, and then the peers run. In cleanup the logical instant stands and the
  runtime goes on 2 ms a step, so that the worker takes Stop;
- `provider_link.rs` reads `scenario_clock::monotonic_us` under `cfg(test)`,
  as `provider_worker.rs` does since #1035: the worker's link clock pairs its
  Tokio instants with the voice's microseconds, and in a scope both are
  logical. Unscoped tests and release builds read `lamp_ipc::monotonic_us` as
  before;
- what the service and the peers record (`Seen`) is stamped with
  `scenario_clock::instant()`, which is `Instant::now()` without a scope.

The child processes (`sleep 60`), their start barrier, the coordinator's
cleanup and the 12 s conversation watchdog stay on the host clock. The
assertions, the scripts, the service's timings and every production bound
are unchanged. `a_name_note_told_with_a_request_ended_early_does_not_survive_it`
runs on `#[tokio::test(start_paused = true)]`.

On one Intel Mac, one test at a time: the 19 tests took 93.17 s before and
16.26 s after; the name note 1.12 s and 0.01 s. Each conversation's summary
line (connections, activity starts and ends, audio blocks sent, requests
given up, answers begun, blocks played, last sound to the first block handed
to the speaker) is the same before and after, except that times differ by up
to 6 ms (602 ms where real time gave 601 to 604: the 2 ms step), and in the
cut-in case 94 blocks are played before "stop" cuts in, on every run, where
real time played 91 or 92 (not asserted). Six copies of the module at once
on 16 logical CPUs passed 24 of 24 runs in 23 to 32 s each, and their summary lines
were byte for byte those of an unloaded run.

So `coordinator::tests::early_end` left the group, in all three filtersets,
on 2026-10-16.
