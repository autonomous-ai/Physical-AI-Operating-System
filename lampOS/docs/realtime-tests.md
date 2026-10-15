# Realtime tests (skipped on pull requests)

**Status: temporary, to be reviewed.** Since 2026-10-15 the tests that wait
on the real clock form the nextest test group `realtime`
([`.config/nextest.toml`](../.config/nextest.toml)). In the ARM64 run
(`.github/workflows/lampos-arm64.yml`):

| Event | nextest profile | Realtime group | Runners | Tests at once |
|---|---|---|---|---|
| Pull request into `lamp-voice`, manual run | `pr` | skipped | 1 | one per core |
| Push to `lamp-voice` (every merge) | `default` | run | 4 | one, as before |

A pull request uses one runner because each runner compiles every test
binary again (about 5 of its 10 minutes); its tests without the realtime
group take about a minute in parallel. Run 38719393271 (four shards, one
test at a time) spent 47 to 59 s per shard on tests.

When a run on `lamp-voice` fails, a job opens an issue titled "lampOS: the
full ARM64 run on lamp-voice failed", or comments on the open one. Nothing
the tests assert was changed. The runners are unchanged (`ubuntu-22.04-arm`):
larger runners cost too much.

## Why

The ARM64 run tests one test at a time, as on a lamp's few cores. On the
2026-10-15 runs 38715042828 and 38715014248, all 4,507 tests took 1,191 s,
and the 123 tests in this group took 977 s of it (82%). Each of the four
shards spent 4 to 6 minutes on tests, mostly waiting on sleeps, deadlines
and fakes that play a conversation at real speed. For example,
`serve_session::normal_eight_turn_conversation_in_one_serve_session` takes
65 s because `fixtures/voice-eval-serve-v1.json` has 26.5 s of `speech_ms`,
2 s `startup_ms`, 1.5 s `first_audio_ms` per turn and 6.6 s of trigger
`delay_ms`. Without the group, a pull request's tests take about 215 s in
total, about 54 s per shard. A merge into `lamp-voice` still takes the full
time.

The waiting is real time by design in most of these suites (see each file's
header). They are slow, not flaky; they did not fail in these runs. Skipping
them everywhere was rejected: they catch timing faults in whole
conversations, so every merge still runs them.

## What is in the group

The group is a nextest filterset, written three times in
`.config/nextest.toml` (group override, `pr` exclusion, `realtime`
selection), which must stay identical:

- whole test binaries: `lamp-voice-eval::serve_session`,
  `lamp-voice-eval::physical_loopback`, `lamp-voice-eval::serve_relay`,
  `lamp-voice-eval::final_dispatch`, `lamp-memory::health`,
  `lamp-voice::capture_hold_worker`;
- whole modules of `lamp-voice`'s unit tests:
  `coordinator::tests::pilot_faults`, `coordinator::tests::early_end`,
  `coordinator::tests::endpoint_session`;
- single tests that took 4 s or more outside those, named exactly.

Because whole binaries and modules are matched, a new test added to one of
them is also skipped on pull requests, and is first run when it is merged.

## Running it

From `lampOS/`, the group alone, one test at a time as on the runner:

```
cargo nextest run --release --workspace --profile realtime -j 1
```

What a pull request runs: `--profile pr`. Everything, as on `lamp-voice`:
`cargo nextest run --release --workspace -j 1`. `cargo test` is unaffected
and runs everything.

## Bringing a test back

A test leaves the group when it no longer waits on the real clock: an
injectable clock, a paused tokio clock, a time scale on a fake, or a shorter
fixture, without weakening what it asserts. Remove it from all three
filtersets in the same pull request, and update this list. When the group is
empty, delete it, the `pr` and `realtime` profiles, the `--profile`
selection in the workflow, and this page.

Review points:

- How often does the full run on `lamp-voice` fail on a realtime test, and
  how long until it is fixed?
- Who decides that a red `lamp-voice` run holds the next merge? The workflow
  only opens the issue; it does not block merging.
- A nightly full run would need the workflow on the default branch (`main`);
  a scheduled run starts only from there.
- Does a pull request's run list the expected number of tests (about 4,384
  run plus the skipped)?

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
