# Recent playout: development failures and limits

2026-10-14, Third's x86_64 development Mac. No lamp execution.

## Reproduced behavior before the correction

`cargo test --offline -p lamp-expression --test recent_playout
a_twenty_ms_piece_published_after_onset_is_heard_when_read_just_after_end`
failed with zero passed and one failed before the engine change. The report
was refused as expired, too far ahead, overlapping or out of order. The
authored interval was 100,000–120,000 us, published at 101,700 us and received
at 121,700 us. The publication offset follows First's reported median; the
20,000 us transport delay is authored, not a measured lamp delay.

The red command had no `--exact`, `--nocapture` or test-thread flag. It also
emitted three unused-item warnings from the incomplete test harness, removed
when the harness was completed.

The raw red output is retained as `recent-before-fix.txt`. The same case
passes after the bounded recent-level change; it is part of the final gate.

The subsequent engine runs passed 27 cases, then 92 (27 new plus 65
inherited), then 94 after two more requested regressions. The initial
92-case green log was overwritten by the final 94-case run; the separate
27-case output and the final 94-case output are retained. The red failure
log was not overwritten.

## Unix-socket sandbox denial

The first `cargo test --offline -p lamp-expression --test
recent_playout_runtime` run passed its two pure tests, while the two tests
that bind local Unix sockets failed with sandbox permission denial. The
raw output is retained as `runtime-sandbox.txt`. The same four tests passed
with host permission, without a code change (`runtime-host.txt`). This was
a local environment failure; no product assertion was bypassed or weakened.

## Preparation

Three documentation patch attempts did not match their expected line context;
they made no changes. The exact current paragraphs were then read and the
patch applied. A test-file lookup and a source-file search initially named
nonexistent files; the actual files were located with `rg --files`. Neither
was a compiler or test failure. No automatic approval review rejected an
action in this addendum.

## Not shown

The final command results are recorded separately in `results.json`. These
failures and their corrections are development evidence, not lamp results.
The chosen 100 ms tail, optical response, acoustic onset and Body response
remain unqualified; First's real recording was not supplied to Third.
