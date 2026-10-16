# Corrected full-gate binary timings

These are libtest-reported seconds from the corrected full gate; the baseline was compiled, not fully executed. They are not before/after binary speedups.

| Binary | Baseline IDs | Corrected IDs | Pass / fail / ignore | Seconds |
| --- | ---: | ---: | --- | ---: |
| `lamp-voice-eval` | 296 | 303 | 303 / 0 / 0 | 4.45 |
| `lamp-voice-eval::bin/lamp-voice-eval` | 0 | 0 | 0 / 0 / 0 | 0.00 |
| `lamp-voice-eval::acceptance_boundaries` | 5 | 5 | 5 / 0 / 0 | 0.56 |
| `lamp-voice-eval::answer_review` | 14 | 14 | 14 / 0 / 0 | 0.78 |
| `lamp-voice-eval::audit_final_dispatch` | 2 | 2 | 2 / 0 / 0 | 0.64 |
| `lamp-voice-eval::audit_importer` | 11 | 11 | 11 / 0 / 0 | 0.64 |
| `lamp-voice-eval::audit_runner_dispatch` | 2 | 2 | 2 / 0 / 0 | 0.01 |
| `lamp-voice-eval::canary_plan` | 3 | 3 | 3 / 0 / 0 | 0.18 |
| `lamp-voice-eval::evaluator` | 14 | 14 | 14 / 0 / 0 | 1.62 |
| `lamp-voice-eval::fake_runtime` | 23 | 23 | 23 / 0 / 0 | 9.45 |
| `lamp-voice-eval::final_dispatch` | 13 | 13 | 13 / 0 / 0 | 9.77 |
| `lamp-voice-eval::next_turn_ready` | 2 | 2 | 2 / 0 / 0 | 0.13 |
| `lamp-voice-eval::physical_loopback` | 12 | 12 | 12 / 0 / 0 | 28.72 |
| `lamp-voice-eval::playback_occurrences` | 10 | 10 | 10 / 0 / 0 | 2.04 |
| `lamp-voice-eval::provider_failed` | 7 | 7 | 7 / 0 / 0 | 0.46 |
| `lamp-voice-eval::recording_evidence` | 19 | 19 | 19 / 0 / 0 | 2.27 |
| `lamp-voice-eval::recording_import` | 10 | 10 | 10 / 0 / 0 | 0.62 |
| `lamp-voice-eval::relay_clock` | 2 | 2 | 2 / 0 / 0 | 0.07 |
| `lamp-voice-eval::relay_hardening` | 7 | 7 | 7 / 0 / 0 | 0.49 |
| `lamp-voice-eval::report_validity` | 14 | 14 | 14 / 0 / 0 | 1.51 |
| `lamp-voice-eval::serve_plan` | 6 | 6 | 6 / 0 / 0 | 5.06 |
| `lamp-voice-eval::serve_relay` | 6 | 6 | 6 / 0 / 0 | 31.23 |
| `lamp-voice-eval::serve_session` | 14 | 14 | 14 / 0 / 0 | 60.41 |
| `lamp-voice-eval::target_audio` | 6 | 6 | 6 / 0 / 0 | 1.22 |
