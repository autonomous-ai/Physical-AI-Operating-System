# Evaluator reporting and recording coverage

Main composed A2's reviewed report-validity checkpoints onto the canonical
source, preserving the per-cue decision clock and microphone option forwarding.
All 19 changed files match the tested source. No runtime policy or dependency
changed.

Interruption attribution now uses the admitting successor and its owner, rather
than an earlier provisional speech candidate. Endpoint timings retain their
policy provenance. An early recorder exit or a prompt that outlives the recorded
window invalidates both acoustic timings and reviewed content. Prepared playback
uses the earlier of its original deadline and the last start covered by the
recording; retained actual first-copy times are checked again at finish.

From a standalone source export, `cargo fmt --all -- --check`, strict all-target
Clippy for `lamp-voice-eval` and `lamp-observer`, and both packages' full serial
test suites passed: **328 passed, 0 failed, 0 ignored**. The complete commands
and source identities are in [results.json](results.json). Empty main/doc test
stages are included in the receipt but contribute no passing tests. The shared
room-evidence guard's original counterexamples and valid controls passed.

These are host tests using local fixtures. This checkpoint contains no ARM,
device or acoustic qualification. All captured room audio remains local.
