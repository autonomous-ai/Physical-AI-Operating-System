# Independent current-base Serve integration review

Reviewed `/private/tmp/lampos-tests-serve-release` from **2026-10-16 06:07:04 +0700** to **2026-10-16 06:07:07 +0700**, using shell `date`. Base: `e7d0f673d0c4bde4a770b19783d6084a8d935ec3`. Published source compared: `8af53fd459795029f6277679ded49bf7826def73`. **No blocking integration, preservation or identity finding.** This was a read-only source/Git-object/saved-log review, not implementation or runtime qualification. No Cargo, test execution, Git mutation, network, device access or repository edit was performed.

## Source and current-base preservation

All **25 owned Rust files** are byte-identical to the actual published `8af53fd` Git blobs, and the **entire 75-file Evaluator Rust tree** matches that published source. The current-base versions of the owned paths are unchanged from the earlier `d60124f` base, so no concurrent source edit was overwritten. Every one of **6,782 nonowned tracked entries** matches its exact current-base Git blob and executable/symlink mode, including Cargo.lock, the Memory fixture changes, Voice changes, and Expression tests. All **148 historical verification files**, including ignored raw logs, match the published Git objects exactly. The actual dirty/untracked set equals the declared source/doc and historical evidence paths.

`realtime-tests.md` is exactly the complete current `e7d` document followed by the exact earlier Serve append. The complete Expression #912 section remains byte-identical and occurs once; the new Serve section occurs once. Memory's separate `memory-fixture-clock.md` and the current selftest/tasks/tools docs remain unchanged from `e7d`, along with their associated current code. The document conflict was resolved without dropping either branch's content. Actual resulting document SHA-256: `379e5b356b4e69322609ce87f44ab61fd3632b969f09afcde56f7cc8f73c4f68`; appended Serve section: `65198569e6a0fe6662f9f1ca78861534eba3143701d7bee26c65b458f7fd05de`.

## Independent identity and assertion check

I parsed the actual fresh current-base compiled list: **377 tests, zero ignored**. I then extracted new test names from the actual current source and its Rust module declarations, yielding exactly **121 additions**. This independent expected set is **498 distinct identities**, with no omitted old identity, duplicate or changed ignored flag. It matches both the provided expected JSON and the complete compiled list saved in published `8af53fd`. All ten original `physical::fixture_control::channel_closed_peer_tests::*` names remain intact; no renamed `closed_peer_tests` identity is introduced.

Source comparison against actual `e7d` Git blobs preserves **376 of the 377 old bodies byte-for-byte**. The sole compatibility adaptation remains the already-published `stop_signal_sent_us: None` field in an existing `RelayPollProgress` literal. Removing precisely that field recovers the complete original body and assertions. No additional old test adaptation was introduced by this rebase.

All **14 original Serve-session bodies and 114 assertion macro sites** remain exact, and the entire assertion-helper/scenario tail is unchanged. Exact original and final file hashes match the retained attempt audit, preserving its **22 expanded attempts**, including the unchanged five-fault loop. The original six direct Serve-relay tests and their path remain unchanged in this migration.

## Gate and evidence limits

The root-owned fresh gate was running during this review. Its recorded completed stages were **fmt, clippy, list** at the final log snapshot. The fresh after-list had completed; it independently matches all 498 expected identities. No fresh complete test pass or ARM64 result is claimed. Earlier native failures and historical timings remain retained separately; unchanged Rust source does not qualify changed inherited Voice, Memory or lockfile inputs.

The copied historical verification README still names its original `d60124f` base and prior gate. That is correct as historical evidence, and must remain distinguishable from this current-base gate in the eventual PR packaging. The source proof and this review identify the actual `e7d` integration explicitly. No source workaround or failing-head retry is inferred from the rebase.

The small companion proof is `/private/tmp/tests-823-serve-release-independent-review.json`, SHA-256 `bcf22a2cf494ce7a00d267a6d1e6c2ca0819a745b3dffeaf86a42bb9d3cc18d2`. It records exact owned hashes, selected current-base input blobs, counts, documentary preservation and raw compiled-list hashes. Current source hashes stayed stable throughout review.
