# Corrected full-gate binary timings

These are libtest-reported seconds from the corrected full gate; the baseline was compiled, not fully executed. They are not before/after binary speedups.

| Binary | Baseline IDs | Corrected IDs | Pass / fail / ignore | Seconds |
| --- | ---: | ---: | --- | ---: |
| `lamp-voice-eval` | 296 | 296 | 296 / 0 / 0 | 9.20 |
| `lamp-voice-eval::bin/lamp-voice-eval` | 0 | 0 | 0 / 0 / 0 | 0.00 |
| `lamp-voice-eval::acceptance_boundaries` | 5 | 5 | 5 / 0 / 0 | 0.50 |
| `lamp-voice-eval::answer_review` | 14 | 14 | 14 / 0 / 0 | 0.71 |
| `lamp-voice-eval::audit_final_dispatch` | 2 | 2 | 2 / 0 / 0 | 0.54 |
| `lamp-voice-eval::audit_importer` | 11 | 11 | 11 / 0 / 0 | 0.63 |
| `lamp-voice-eval::audit_runner_dispatch` | 2 | 2 | 2 / 0 / 0 | 0.02 |
| `lamp-voice-eval::canary_plan` | 3 | 3 | 3 / 0 / 0 | 0.20 |
| `lamp-voice-eval::evaluator` | 14 | 14 | 14 / 0 / 0 | 1.61 |
| `lamp-voice-eval::fake_runtime` | 23 | 23 | 23 / 0 / 0 | 9.31 |
| `lamp-voice-eval::final_dispatch` | 13 | 19 | 19 / 0 / 0 | 11.10 |
| `lamp-voice-eval::next_turn_ready` | 2 | 2 | 2 / 0 / 0 | 0.13 |
| `lamp-voice-eval::physical_loopback` | 12 | 12 | 12 / 0 / 0 | 26.87 |
| `lamp-voice-eval::playback_occurrences` | 10 | 10 | 10 / 0 / 0 | 2.06 |
| `lamp-voice-eval::provider_failed` | 7 | 7 | 7 / 0 / 0 | 0.45 |
| `lamp-voice-eval::recording_evidence` | 19 | 19 | 19 / 0 / 0 | 2.22 |
| `lamp-voice-eval::recording_import` | 10 | 10 | 10 / 0 / 0 | 0.63 |
| `lamp-voice-eval::relay_clock` | 2 | 2 | 2 / 0 / 0 | 0.07 |
| `lamp-voice-eval::relay_hardening` | 7 | 7 | 7 / 0 / 0 | 0.46 |
| `lamp-voice-eval::report_validity` | 14 | 14 | 14 / 0 / 0 | 1.48 |
| `lamp-voice-eval::serve_plan` | 6 | 6 | 6 / 0 / 0 | 5.04 |
| `lamp-voice-eval::serve_relay` | 6 | 6 | 6 / 0 / 0 | 31.31 |
| `lamp-voice-eval::serve_session` | 14 | 14 | 14 / 0 / 0 | 61.72 |
| `lamp-voice-eval::target_audio` | 6 | 6 | 6 / 0 / 0 | 1.28 |
