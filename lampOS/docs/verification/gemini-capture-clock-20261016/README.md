# Gemini fixture capture clock — qualification record

**Local qualification remains RED; this PR remains a draft. Native ARM64 is pending.**
The Gemini default and feature suites pass, but the full affected-package gate
contains one original Voice rapid-mute failure tracked by [#893](https://github.com/autonomous-ai/lampOS/issues/893).
No failing test was retried, skipped, relaxed or fixed by this preparation.

Packaged 2026-10-16 05:25:57 +0700 from shell `date`. Current source base is
`7ae0eb647914e286817c29bff2abc2934fb19546`. The [source integration record](current/handoff.md)
and [exact five-file confirmation](source/current-source-confirmation.json)
pin the reviewed/gated source. All raw records below are copied byte-for-byte;
[provenance](provenance.json) and [SHA256SUMS](SHA256SUMS) identify every copy.

## Change and preservation

An empty, default-off `fixture-clock` feature exposes an explicit pure capture
clock shared by the actual InputSender and Actor, plus pair/connector factories
that select it explicitly. Existing constructors still select System, including
when Cargo enables the feature through another crate. The custom APIs and
variant also use `cfg(test)` so all twelve new cases run in ordinary unit/native
builds without workflow changes. Shared setup and Actor code are retained.

All original test bodies/assertions and source-age comparisons remain. The six
semantic capture samples retain their placement, including two distinct fresh
Audio Actor samples; audio stays bounded at 2500 ms and images at 2 s. All 23
Tokio sample lines and both original 10 ms intervals remain unchanged. The
independent [review](review/independent-review.md) and
[mechanical source proof](review/proof.json) reconstruct the four prior files
exactly. The eleven inventoried original session/integration files retain 174
source test functions and 789 assertion sites; these are scoped static counts,
not the full compiled inventory. All 30 other original Gemini Rust files remain
byte-identical. No existing scenario is migrated and no provider completed-poll
or shutdown controller is introduced.

The twelve additions exercise actual setup acknowledgements, framed PCM/JPEG
receipts, Actor handling, rejection at the original exact age bounds, shared
connector attempts and unchanged write/read deadlines. Terminal helpers await
the actual Actor JoinHandle; they do not claim OS process exit or a remote
service result. Source proof does not establish runtime success by itself.

## Fresh local gates

The current full sequence ran 2026-10-16 04:31:59 +0700 through
2026-10-16 05:06:04 +0700. Exact argv, cwd, target and date markers are
in [full-gates.json](current/full-gates.json); unedited logs and `.exit` records
are adjacent. Tests use `--test-threads=1` within each invocation, with offline
locked dependencies. These are local debug gates, not a quiet latency benchmark.

| Stage | Observed result |
|---|---|
| Workspace formatting | exit 0 |
| Gemini strict all-target Clippy, default and fixture feature | both exit 0 |
| Gemini default tests | 268 passed / 0 failed / 2 existing ignored |
| Gemini fixture-feature tests | 268 passed / 0 failed / 2 existing ignored |
| Voice, Surround, Evaluator and Memory strict all-target Clippy with the feature | exit 0 |
| Complete serial tests of those four affected packages with the feature | **2196 passed / 1 failed / 13 existing ignored; exit 101** |
| Workspace builds, default and fixture feature | both exit 0 |

The [compiled comparison](compiled/identity-comparison.json) proves all 258
original identities and both original ignore flags retained, plus twelve new
unignored identities: **270 compiled** in both current default and feature
lists. All twelve new cases actually pass in both full suites. The two Gemini
ignores are the existing prolonged-recovery/conversation soak cases; no new
ignore was added. [Qualification details](qualification.json) independently
parse all six/six/eighty-four result summaries and retain the sole failure.

The failing identity is
`coordinator::tests::pilot_faults::rapid_mute_and_reopen_never_overlaps_workers_or_carries_speech_across`.
Its [raw log](current/full-affected-tests.log) reports Threaded host time, cycle
3, `service privacy freshness lost before next observation`, then the original
assertion at `pilot_faults.rs:2395`. This is recorded against #893; unchanged
source and issue identity alone do not establish causality. The affected gate
stays red. No owner fix or test change is included here.

## Earlier red gate remains durable

The earlier source on `48b3737d5` produced **2011 passed / 1 failed / 8 ignored**
in its three-package affected gate. Its different failure was StaleOwner in
`a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on`, the Threaded
scenario tracked by [#866](https://github.com/autonomous-ai/lampOS/issues/866). The complete
[old raw log](https://github.com/autonomous-ai/lampOS/blob/99611db2bbcf3724c83a28db6ec3b6bddce53997/lampOS/docs/verification/test-clock-progress-20261016-0425/gemini/full-affected-tests.log) and [read-only triage](https://github.com/autonomous-ai/lampOS/blob/99611db2bbcf3724c83a28db6ec3b6bddce53997/lampOS/docs/verification/test-clock-progress-20261016-0425/gemini/tests-823-gemini-voice-stall-triage.md) remain pinned
at `99611db2bbcf3724c83a28db6ec3b6bddce53997`. [Historical provenance](historical-red-provenance.json)
verifies those immutable Git blobs against the retained raw bytes. They are not
overwritten or reclassified as passing by this later run.

Fresh qualification used a changed base with twelve inherited compilation-input
changes in Voice, Msgs and Memory; the current affected invocation also includes
Memory. This is not a retry of the old unchanged combined tree. The five Gemini
change files remain exactly the reviewed bytes, with no inherited Gemini delta.
The full [integration proof](current/integration.json) records both bases,
protected source hashes and all inherited changes. No latency improvement,
native result, real device/service behavior or completion of the original 123
real-clock tests is claimed. Native and affected-gate qualification remain open.
