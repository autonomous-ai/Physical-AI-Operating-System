# Candidate full-gate binary timings

These are libtest-reported seconds from the candidate full gate; the baseline was compiled, not fully executed. They are not before/after binary speedups.

| Binary | Baseline IDs | Candidate IDs | Pass / fail / ignore | Seconds |
| --- | ---: | ---: | --- | ---: |
| `lamp-voice-eval` | 296 | 296 | 296 / 0 / 0 | 9.73 |
| `lamp-voice-eval::bin/lamp-voice-eval` | 0 | 0 | 0 / 0 / 0 | 0.00 |
| `lamp-voice-eval::acceptance_boundaries` | 5 | 5 | 5 / 0 / 0 | 0.50 |
| `lamp-voice-eval::answer_review` | 14 | 14 | 14 / 0 / 0 | 0.79 |
| `lamp-voice-eval::audit_final_dispatch` | 2 | 2 | 2 / 0 / 0 | 0.62 |
| `lamp-voice-eval::audit_importer` | 11 | 11 | 11 / 0 / 0 | 0.67 |
| `lamp-voice-eval::audit_runner_dispatch` | 2 | 2 | 2 / 0 / 0 | 0.03 |
| `lamp-voice-eval::canary_plan` | 3 | 3 | 3 / 0 / 0 | 0.21 |
| `lamp-voice-eval::evaluator` | 14 | 14 | 14 / 0 / 0 | 1.65 |
| `lamp-voice-eval::fake_runtime` | 23 | 23 | 23 / 0 / 0 | 9.58 |
| `lamp-voice-eval::final_dispatch` | 19 | 19 | 19 / 0 / 0 | 9.05 |
| `lamp-voice-eval::next_turn_ready` | 2 | 2 | 2 / 0 / 0 | 0.14 |
| `lamp-voice-eval::physical_loopback` | 12 | 12 | 12 / 0 / 0 | 29.10 |
| `lamp-voice-eval::playback_occurrences` | 10 | 10 | 10 / 0 / 0 | 2.05 |
| `lamp-voice-eval::provider_failed` | 7 | 7 | 7 / 0 / 0 | 0.45 |
| `lamp-voice-eval::recording_evidence` | 19 | 19 | 19 / 0 / 0 | 2.29 |
| `lamp-voice-eval::recording_import` | 10 | 10 | 10 / 0 / 0 | 0.64 |
| `lamp-voice-eval::relay_clock` | 2 | 2 | 2 / 0 / 0 | 0.07 |
| `lamp-voice-eval::relay_hardening` | 7 | 7 | 7 / 0 / 0 | 0.48 |
| `lamp-voice-eval::report_validity` | 14 | 14 | 14 / 0 / 0 | 1.55 |
| `lamp-voice-eval::serve_plan` | 6 | 6 | 6 / 0 / 0 | 5.69 |
| `lamp-voice-eval::serve_relay` | 6 | 6 | 6 / 0 / 0 | 31.37 |
| `lamp-voice-eval::serve_session` | 14 | 14 | 14 / 0 / 0 | 85.53 |
| `lamp-voice-eval::target_audio` | 6 | 6 | 6 / 0 / 0 | 1.48 |
