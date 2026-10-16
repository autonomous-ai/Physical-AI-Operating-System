# Original 123: one full-run observation at db7201d0bc4c

Captured 2026-10-16 11:33:54 +0700; run 38849189990, attempt 1. All 123 occurred exactly once.
Printed case-duration sum: **196.159 s across concurrent shards**. This is not standalone serial wall time or a matched timing pair. No new benchmark was executed.

| Rank | Binary | Original identity | Shard | Job | Reported seconds |
| ---: | --- | --- | ---: | --- | ---: |
| 1 | `lamp-voice` | `provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity` | 1/4 | [116806405928](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405928) | 24.033 |
| 2 | `lamp-voice` | `coordinator::tests::early_end::an_answer_that_arrives_after_the_lamp_is_sure_begins_400_ms_sooner_than_without` | 4/4 | [116806405950](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405950) | 10.760 |
| 3 | `lamp-voice` | `coordinator::tests::pilot_faults::rapid_mute_and_reopen_never_overlaps_workers_or_carries_speech_across` | 1/4 | [116806405928](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405928) | 8.592 |
| 4 | `lamp-voice` | `coordinator::tests::early_end::speech_resumed_at_three_tenths_under_the_lamps_duplex_policy` | 1/4 | [116806405928](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405928) | 6.384 |
| 5 | `lamp-voice` | `coordinator::tests::early_end::speech_resumed_at_three_tenths_nothing_of_the_first_answer_is_played_and_one_turn_is_heard` | 2/4 | [116806405934](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405934) | 6.382 |
| 6 | `lamp-voice` | `coordinator::tests::early_end::without_the_option_every_message_and_record_is_what_it_was` | 3/4 | [116806405555](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405555) | 6.382 |
| 7 | `lamp-voice` | `coordinator::tests::early_end::the_link_dropping_in_between_fails_the_request_unheard_and_the_next_is_answered` | 1/4 | [116806405928](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405928) | 6.381 |
| 8 | `lamp-voice` | `coordinator::tests::endpoint_session::hung_worker_never_delays_capture_endpoint_or_session_end` | 3/4 | [116806405555](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405555) | 6.379 |
| 9 | `lamp-voice` | `coordinator::tests::early_end::a_lone_lamp_and_a_pause_and_the_question_go_as_one_request` | 1/4 | [116806405928](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405928) | 5.384 |
| 10 | `lamp-voice` | `coordinator::tests::early_end::lamp_stop_over_an_answer_stops_it_and_only_the_answer_to_the_whole_is_played` | 3/4 | [116806405555](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405555) | 5.384 |
| 11 | `lamp-voice` | `provider_worker::tests::review_2026_10_14::review_a_name_heard_after_a_requests_sound_ended_does_not_let_out_its_answer` | 3/4 | [116806405555](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405555) | 5.228 |
| 12 | `lamp-voice::capture_hold_worker` | `another_microphone_is_handed_on_at_once_and_its_early_sound_is_not_accounted_for` | 4/4 | [116806405950](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405950) | 5.107 |
| 13 | `lamp-voice::capture_hold_worker` | `mic1_frames_wait_four_blocks_with_their_own_times_and_a_stop_drops_what_is_held` | 4/4 | [116806405950](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405950) | 5.107 |
| 14 | `lamp-health::program` | `between_two_healths_five_seconds_apart_the_program_is_not_taken_for_gone` | 2/4 | [116806405934](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405934) | 5.009 |
| 15 | `lamp-voice` | `provider_worker::tests::kept_start::with_no_first_answer_the_patience_ends_in_the_strict_timeout_and_no_end` | 2/4 | [116806405934](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405934) | 4.522 |
| 16 | `lamp-memory::health` | `the_environment_alone_does_not_enable_reporting_or_create_bus_data` | 3/4 | [116806405555](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405555) | 4.441 |
| 17 | `lamp-voice` | `provider_worker::tests::review_2026_10_14::review_sound_a_name_heard_as_a_request_began_lets_out_the_answer_the_rule_would_hold` | 4/4 | [116806405950](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405950) | 4.426 |
| 18 | `lamp-voice` | `coordinator::tests::early_end::an_answer_with_nothing_said_before_the_lamp_is_sure_is_never_played` | 2/4 | [116806405934](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405934) | 4.385 |
| 19 | `lamp-voice` | `coordinator::tests::early_end::an_answer_that_arrives_before_the_lamp_is_sure_begins_at_that_moment` | 2/4 | [116806405934](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405934) | 4.383 |
| 20 | `lamp-voice` | `coordinator::tests::early_end::quiet_to_the_end_the_answer_starts_when_the_lamp_is_sure_and_not_before` | 4/4 | [116806405950](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405950) | 4.378 |

All 123 rows, raw line references and per-binary sums are in `original123-durations.json`. The observations come from the immutable first-attempt logs.
