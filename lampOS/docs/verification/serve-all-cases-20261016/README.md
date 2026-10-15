# All original Serve-session cases use the coordinated fixture clock

Recorded from `date`: 2026-10-16 05:51:12 +0700. The current base is `d60124f8280f9c76576d6ca7bf859bc5c3983828`. The complete local Evaluator gate passes **498 tests, zero failures, ignores or filters**, with formatting, strict Clippy, compiled-identity comparison and the workspace build also passing. All **14 original Serve-session identities, 114 assertion macro sites and 22 expanded attempts** remain; every attempt now uses the controlled fixture, including the three Recorder cases and the delayed-transport case. Native qualification of this new source remains pending.

## Current source and preserved checks

The change contains 25 Evaluator Rust files and the appended realtime-test documentation. [The scenario proof](integration/original-serve-preservation.json) preserves every complete original test body and assertion helper. Reversing only the shared constructor, import and introductory comment reproduces the original file. The five-fault loop, all durations, rejection criteria and 100 ms lateness checks are unchanged. The six direct `tests/serve_relay.rs` migrations are excluded.

The controlled participants retain the original 2 ms polls and real pipe, cue and datagram consumption. Readiness still requires the actual fresh cue and prepared-player resources. Signal receipt, worker progress, final output, real child status and reaping remain distinct. Recorder readiness follows its real ready-file write and the original bounded backend parser. Its complete 6 s, 6 s and 23 s lifetimes remain; declaring capture end at 20 s does not prove exit. The backend holds logical time until its owned Child actually exits, with recorder/player drain ordering preserved.

SlowTransport keeps the complete 400 ms wait, actual exec, inherited streams and literal arguments. After terminal delay progress the controller waits for a fresh real ServeRelay registration; that progress is not exec or readiness evidence. The original 2300 ms first-preparer delay remains. One cumulative startup deadline spans rounds, and an earlier Recorder due time or shorter preparation deadline still wins.

[The integration stages](integration/stage-provenance.json), [scenario reversals](integration/original-serve-preservation.json) and [corrected source proof](current/final-source-proof.json) pin the published Serve seed and qualified Recorder/SlowTransport additions. All 6,729 nonowned tracked entries retain bytes or symlink targets and modes. Of the 377 original test bodies, 376 are byte-exact; the existing RelayPollProgress literal adds only `stop_signal_sent_us: None`, whose removal restores the entire remaining body and assertion. The existing non-test Step helper adds only `wait_for_sigterm: false`. These compatibility changes are explicit in [the complete test-source proof](integration/test-source-preservation.json).

## Initial identity rejection and corrected fresh gate

The first compiled list had 498 entries but had renamed ten existing module-qualified identities from `channel_closed_peer_tests` to `closed_peer_tests`. The identity gate **rejected this before any test execution**. Its raw formatting, Clippy, list output and [rejection record](integration/root-compiled-rejection.json) are retained unedited. [The one-line correction](current/correction.diff) restores the current-base module name without changing a test body or assertion. This was not a retry of a failing executable test.

The subsequent [compiled comparison](current/root-identity-proof.json) preserves **all 377 original identities and ignored flags**, with exactly **121 additions**: 45 Serve, 41 Recorder and 35 SlowTransport boundary cases. There are no ignored cases. The current source therefore does **not** directly match all 75 historical Evaluator Rust files: `fixture_control.rs` differs by this exact module declaration. The timestamped integration handoff and earlier proofs remain archival records of the pre-correction source; reversing that declaration correction is required before using their historical whole-source equality claim.

The complete fresh sequence ran from `lampOS/` on the shared x86_64 Mac with Rust 1.96, locked/offline Cargo and the dedicated `/private/tmp/tests-823-slow-progress-target` cache:

```sh
cargo fmt --all --check
cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings
cargo nextest list --locked --offline -p lamp-voice-eval --message-format json
cargo test --locked --offline -p lamp-voice-eval --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

[Exact commands, shell-date boundaries and exit codes](current/root-gates.json) record **05:25:49–05:31:58 +0700 on 2026-10-16**. The [raw test log](current/root-tests.log) has 25 successful Cargo summary blocks: 296 library tests and 202 integration tests, **498/0/0** overall. All 14 original Serve identities report success once; their unchanged loop bodies retain 22 expanded attempts. Two existing library cases interleave child stdout before their success marker; [the independent recount](qualification.json) records that format rather than dropping them. Source hashes remain unchanged after the gate. This is complete Evaluator coverage and a workspace build, not a workspace-wide test run or ARM64 qualification.

## Historical paired measurements

These are three separately recorded incremental pairs, each covering all 14 original Serve identities and 22 attempts without skips or retries. They use different historical helper/source stages; their totals must not be joined into one before/after experiment or attributed to this current base.

| Historical step | Complete 14-case pair | Newly controlled subset | Evidence |
| --- | ---: | ---: | --- |
| Ten Serve identities | 396.970 → 111.763 s | 334.354 → 49.402 s, ten identities | [Pair and source/load provenance](https://github.com/autonomous-ai/lampOS/blob/d9cc28b141e317c51e71a0d06b450be628103cc7/lampOS/docs/verification/serve-fixture-pair-20261016/README.md) |
| Three Recorder identities | 117.106 → 90.202 s | 38.914 → 7.689 s, three identities | [Pair and source/load provenance](https://github.com/autonomous-ai/lampOS/blob/d9cc28b141e317c51e71a0d06b450be628103cc7/lampOS/docs/verification/recorder-fixture-pair-20261016/README.md) |
| Delayed transport identity | 86.271 → 73.874 s | 22.713 → 3.608 s, one identity | [Pair and source/load provenance](https://github.com/autonomous-ai/lampOS/blob/99611db2bbcf3724c83a28db6ec3b6bddce53997/lampOS/docs/verification/slow-transport-fixture-pair-20261016/README.md) |

All are serial debug runs on a shared x86_64 Mac. The first pair ran the after variant first, with known unequal overlap from other full gates. The Recorder pair ran before then after; its recorded seam build overlapped the before listing but finished before the measured run, without establishing equal or quiet background load. The SlowTransport pair overlapped parts of independent Moment/Bus gates. There is **no fresh paired measurement** for this current base, no native speedup claim, and no combined original-123 serial result.

## Retained native failures and qualification limits

The first normal [ARM64 38800988605](https://github.com/autonomous-ai/lampOS/actions/runs/38800988605), on old head `188d97b7fcb6a78adaec67a3d3939843f759e1d3`, failed in Body queue-age before the actual owner fix #902. The same head's first full [38801026642](https://github.com/autonomous-ai/lampOS/actions/runs/38801026642) failed separately in Body readiness, tracked by [#914](https://github.com/autonomous-ai/lampOS/issues/914): 3,845 unique passes, one failure and 1,225 unrun entries in the stopped shard. It observed only **93/123 original identities and 13/14 Serve identities**. [The immutable raw logs, membership audit and triage](https://github.com/autonomous-ai/lampOS/blob/d9cc28b141e317c51e71a0d06b450be628103cc7/lampOS/docs/verification/clock-migration-integration-20261016-0350/README.md) preserve those failures and the incomplete coverage.

Later head `e057db620acb2bd61bac1ae7f157d87cdf635a18` passed normal [ARM64 38806386829](https://github.com/autonomous-ai/lampOS/actions/runs/38806386829), with 4,939 unique passes and 130 total skipped, including 93 by the PR default filter. That profile executed none of the original Serve-session cases. [Its retained raw evidence and the later owner's #914 diagnosis](https://github.com/autonomous-ai/lampOS/blob/99611db2bbcf3724c83a28db6ec3b6bddce53997/lampOS/docs/verification/test-clock-progress-20261016-0425/README.md) remain separate from this new integration. Neither that pass nor the owner's proposed read-after-receive clock fix qualifies this new source or proves that #914 is fixed.

Ready for lamp-voice: **no**, until the new source has its own successful native run and complete relevant coverage. No failure was hidden, test skipped, duration shortened, flaky retry added or workflow/profile changed. #823 remains open for the other migrations and the combined standalone serial under-two-minute target.

[Historical evidence locations](historical-evidence.json) pin exact Git objects and SHA-256 hashes. [Copy provenance](provenance.json) pins every unedited raw/proof copy; `SHA256SUMS` covers all files here except itself. Scratch Python remains outside the repository. No binaries, devices or provider calls are part of this package.
