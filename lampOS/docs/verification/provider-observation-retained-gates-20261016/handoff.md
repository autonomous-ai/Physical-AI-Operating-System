# Provider body observation preparation handoff

Frozen 2026-10-16 06:11:12 +0700, from shell `date`. Tree `/private/tmp/lampos-tests-provider-observations`, branch `tests/823-provider-observations`, base `e7d0f673d0c4bde4a770b19783d6084a8d935ec3`. No source edits remain in progress.

Implemented only the four assigned paths. The explicit observed factory installs ownership before any worker poll. The existing default wrapper calls the one original shared body with no observer. Two cfg-test hooks record actual explicit Stop/authority-revocation causes only after their original `intake.stop()` calls; generic error cleanup is not a Stop cause. An independent wrapper witnesses the original body future's actual Ready result and returns that same Result object. Drop before the first poll and Drop after entry remain distinct; actual task owners classify cancellation/panic only from their awaited JoinHandle.

Observation storage is one bounded packed atomic state plus a permanent fault latch and immutable Arc identity. There is no growing counter or log, clock read, I/O, wait, retry, runtime controller, Actor completion or child-exit claim. Invalid/duplicate/terminal publication cannot change the real worker Result. All ordinary constructors remain unattached.

## Preservation proof

- `worker-reversal-edits.json` contains eight exact inverse segments. Reversal reconstructs the entire 388,832-byte starting worker, SHA-256 `9e2a59f6cfbe5e39a4baf96ccda72ea1ec90e95b1e0c9e9d999bd6916df18219`.
- The entire old inline test module, including helpers, is byte-exact after removing only the new module declaration: 103 old test functions and 544 assertion macros. All separately stored old tests are among the protected files.
- Every one of 6,792 unowned tracked files is byte-exact, including Cargo, clocks, transport, coordinator/early-end tests, Gemini and workflow. No inherited source was restored from an older base.
- The docs insertion reverses exactly to the original document. Original timers, select order, three Stop-return sites, real shutdown calls, shared control budget, input/output limits, source samples and errors are unchanged.
- `source-proof.json` records the full checks and hashes; `before-source-proof.json` retains all protected hashes.

## New cases and actual boundaries

20 new source test identities (82 assertion macros) are listed in `added-test-identities.json`: 13 real-channel/body/component cases under `provider_worker::tests::observation`, and seven private state/wrapper cases under `provider_worker::observation::tests`. Expected nextest binary ID is `lamp-voice` (binary name `lamp_voice`); compilation has not yet verified those identities.

Real channels exercise queued Stop before polling, the original body's explicit Stop and authority-revocation returns, an actual invalid-control error, unpolled body Drop, initial Denied authority, real permission/generation changes, error cleanup and a full 16-command slice with actual Stop retained for the next drain. The slice seeds one datagram and sends the next after each actual receive, following the existing helper pattern without assuming a kernel queue can hold 17 datagrams.

A causally withheld oneshot separates the actual intake Stop receipt from a component future's later return. Separate owned task tests await requested cancellation and preserve a real panic payload. Held in-memory connector attempts cannot create an Actor; their actual future-Drop signal is checked separately and never called a private task join. Every asynchronous operation that can wait uses the test's one immutable 10-second host deadline on an unpaused runtime. Assertion-failure guards request cancellation but do not claim that as a join.

Private state tests intentionally inject malformed/duplicate/late publication; those are observation-unit tests, not reports of real worker failures. They prove that an actual component Result/error object remains unchanged even when observation fails. No original scenario was migrated and no speedup is claimed.

## Verification performed

Pinned Rust 1.96.0 rustfmt write and final check passed on exactly the three owned Rust paths, with `--edition 2024 --config skip_children=true`. Read-only `git diff --check` passed. Raw command/date/exit records are in `rustfmt-final.json` and its logs.

No Cargo command, compile, Clippy, test, native run, Git mutation, network comment or device operation was performed. Root owns independent review, the first executable gates and publication. This handoff is source qualification only.

## Frozen source hashes

| Path | SHA-256 |
|---|---|
| `lampOS/crates/voice/src/provider_worker.rs` | `56b82b75f50026cb9036ed9f88f937e9df218df7702968d0bbf7798106df1e90` |
| `lampOS/crates/voice/src/provider_worker/observation.rs` | `8a370c2a4b85bd70890d51a34259524cdf5cb236d6a8210f16abb37f6b99ef5e` |
| `lampOS/crates/voice/src/provider_worker/tests/observation.rs` | `671b70c3d4e528ce5d8c330e34acf5d5897c314b1e34a73223f7607ef090a640` |
| `lampOS/docs/voice-test-fixtures.md` | `0011049236738f2ebe8ae93fbc3269bfb2ba9952720e0a4397818992500a4dc6` |

## Worktree status

```text
 M lampOS/crates/voice/src/provider_worker.rs
 M lampOS/docs/voice-test-fixtures.md
?? lampOS/crates/voice/src/provider_worker/observation.rs
?? lampOS/crates/voice/src/provider_worker/tests/observation.rs
```
