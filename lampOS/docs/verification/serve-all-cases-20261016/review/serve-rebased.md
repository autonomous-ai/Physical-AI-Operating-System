# Independent review of rebased Serve coordination (#909)

Reviewed 2026-10-16 03:58:16+0700, from `date`. Scope: read-only source, Git objects and saved raw gate evidence in `/private/tmp/lampos-tests-serve-coordination`. No builds, tests, reruns, network, devices, Git mutations or repository edits were performed by this reviewer.

**No blocking source-preservation or evidence-wording finding.** The current README correctly keeps the successful local evaluator gate separate from failed/incomplete native qualification and historical timings. This review does not make the PR ready to merge or establish an ARM64 pass.

The reviewed source head is `cce8f3cc6ca19d116800d09fa9de9bca37043dc3`, based on `48b3737d5e83efa00278172cb602d3525d368a43`. The updated verification README and raw evidence are uncommitted documentation additions at review time. README SHA-256: `5693852a47608ca4023fcb2fc3c69cd3e574ee2f0c199a9c957aa76885ebff0f`. Root's announced final checksum refresh/removal of the temporary manifest remains packaging work; I did not mutate those files.

## Source and scenario preservation

- All **19 changed source/implementation-doc files** equal both the frozen qualified `serve-seed` bytes and the supplied after-rebase hashes. The committed base-to-head delta has these 19 paths plus 55 verification artifacts. No manifest, lockfile, workflow, profile, or product-crate path differs from the new base. The implementation document preserves its entire base prefix and appends only the 27-line Serve section.
- All **68 current evaluator Rust files** exactly equal the pre-gate source snapshot. The source is unchanged after the complete local gate.
- An independently written reversal of the Serve constructor, import, mutability and header changes recreates the whole original `tests/serve_session.rs` byte-for-byte. Original/reversed SHA-256: `bb8e83a7c0cb08703852638749b01d32e88eb696b35aacb093a5462bd4ddcf49`; current: `cffbb51a9f21b03f2262099b23de526842eb098c600c02f3ec8eca187f800923`. The entire assertion-helper/scenario tail beginning at `fn statuses` is unchanged.
- All **14 test bodies**, **114 assertion sites**, **18 literal run sites**, and **22 expanded attempts** are retained. The five-variant faulty-cue loop accounts for the four additional attempts. Every per-body hash matches the earlier independent audit. Ten identities/18 attempts select the clock; three Recorder identities and one delayed-transport identity retain four host attempts. Original 100 ms checks, faults, request values and successful `run_suite` completion checks remain exact.
- The conditional constructor at `tests/serve_session.rs:139` requires both no recorder and no transport delay. Its else branch still constructs `FakePreparedRoomPlayer` and the ordinary `PhysicalBackend`. All four host cases pass in the fresh raw gate; none was silently bypassed.
- Terminal-receipt wording matches the retained implementation: `fixture_commands.rs:135` defers the final relay Ack until `run_with_controlled_clock` returns and output finishes (`:150`); `session.rs:1224` observes actual `child.try_wait`; `serve_fixture_driver.rs:242` holds time through the remaining relay rounds; `mac.rs:2890` separately waits for and evaluates the real outer transport status. The new code does not replace OS exit evidence with a logical-end flag.

## Gate and native evidence

I reparsed the raw `root-tests.log`, not only `root-result.json`: **422 passed / 0 failed / 0 ignored**, 25 summaries, and zero measured/filtered cases. All 14 original Serve IDs are present in the fresh compiled list and report `ok`. The prior compiled list contains 416 cases; the only six additions are `physical::fake_lamp::slow_transport_clock_tests::*`, with no removals. Every saved stage exit file and gate entry is zero for formatting, strict clippy, compiled listing, complete serial evaluator tests and workspace build. Their recorded end is **2026-10-16 03:44:54 +0700**. The fresh Serve binary's 133.01 s is one local gate execution, not a new paired benchmark.

All **38 newly copied rebased-local/first-native artifacts** are byte-identical to their scratch originals. The native raw logs independently yield:

| Run | Observed result | Qualification limit |
| --- | --- | --- |
| Normal `38800988605` | 305 passes, 1 failure, 130 profile exclusions; 4,633 planned tests unrun | Old head precedes the Body queue-age fix #902. |
| Full `38801026642` | 3,845 unique passes, 1 failure; one repeated final FAIL summary row deduplicated | Three successful shards; shard four stops at 101 passes/1 failure with 1,225 unrun. |

The failed full-run identity is the unchanged Body `inert_child_stop_timing_samples_are_host_ipc_only`. The logged panic is the authority send in `supervisor/tests.rs:151`; that iteration calls `ready` at `:503` before its Stop at `:505`. This locates the observed failure, not its cause. The README appropriately identifies separate #914 and does not claim #900 fixes it.

Membership was recalculated against the exact 123-entry inventory Git object at `4596d2cac:lampOS/docs/verification/realtime-inventory-20261015/inventory.json`; it also equals the original 119 local entries plus four Linux-only capture-hold cases and the later complete 123 audit. The incomplete full run observed **93/123**, including **13/14 Serve**; `delayed_readiness_is_awaited_by_event_not_by_completion` was absent. The 13 Serve durations sum to **71.114 s**, across concurrent hosts. Neither that sum nor shard wall time is a serial 123-test measurement.

## Wording and remaining limits

The updated README's lines 17–21 correctly qualify the fresh base/local gate, lines 25–29 retain source scope and the historical pair, and lines 33–39 preserve both native failures and unfinished qualification. No all-123 pass, full ARM64 pass, serial under-two-minute result or push-to-result claim is made.

The raw historical pair confirms **396.970 s → 111.763 s**, all 14 PASS in each invocation, no skipped cases or retry records. Its test source matches the current constructor migration, but it uses the separately identified historical helper. After ran first; known finite gates overlapped the entire before invocation, while comparable after load is not established. The README retains all of these limits. This pair must not be relabeled as a fresh rebased-head or ARM64 benchmark.

Suggested concise PR wording, consistent with the current README:

> Rebased onto `48b3737d5` with the qualified Serve source unchanged. The fresh complete local evaluator gate passes 422/0/0, including all 14 original Serve scenarios and 22 attempts; formatting, strict clippy and workspace build pass. The retained 396.970→111.763 s serial Mac pair is historical and load-confounded. The previous head's full ARM64 run failed separately in Body #914 and observed only 93/123 original identities and 13/14 Serve cases. Normal and full native qualification of the new head remain pending; #823 remains open.

Independent reproducible source reversal, per-body hashes, copied-artifact hashes, raw result counting and exact compiled additions are in `/private/tmp/tests-823-serve-rebased-independent-evidence/audit.py` and `proof.json`. Source review and saved-log verification do not substitute for root's native qualification.
