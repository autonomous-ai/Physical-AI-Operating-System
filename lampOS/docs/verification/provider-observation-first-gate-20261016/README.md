# Provider observations: failed first gate retained

Recorded from `date`: 2026-10-16 06:51:04 +0700. Shared Mac debug build, Rust 1.96.0; no native or lamp qualification is claimed. This is the first frozen source on base `e7d0f673d0c4bde4a770b19783d6084a8d935ec3`. Raw commands, dates, logs, exits, compiled lists, source snapshots and failure triage are copied without editing. The original handoff records the state at its own timestamp; the reviewed new-test correction was applied later and its fresh gate is separate and pending.

Fmt, strict Clippy, compiled identity comparison and workspace build pass. All **2,257 prior identities** and existing ignore flags remain, plus **20 new cases**. First focused gate is **19 passed / 1 failed**; full Voice/Memory/Evaluator/Surround serial gate is **2,262 passed / 2 failed / 13 existing ignores**. Each stage ran once. The new failed case runs once in focused and once in the full gate; both failures are retained, with no retry to obtain a green result.

## New socket-test correction

`provider_worker::tests::observation::unpolled_real_worker_drop_never_claims_a_queued_stop_or_return` wrongly unwrapped the result after peer Drop: macOS returned OS 54 ConnectionReset. The preceding DroppedBeforePoll and no-Stop assertions passed; the final attempt-Drop await was not reached. The reviewed correction is confined to this new test: an empty successful receive is required before Drop; afterward only None or the exact ConnectionReset is accepted, while every event and other error fails. The body/no-Stop assertions and actual attempt-future Drop receipt stay required. No original test or observer/worker implementation changes. The original and proposed sources/diff remain distinct. The corrected-source gate is pending; nothing here labels the first gate green.

## Separate original reminder failure

Original `coordinator::tests::reminder_roundtrip::actual_consumer_ipc_coordinator_and_two_retirements_ack_only_after_whole_completion` fails at `coordinator/tests/reminder_roundtrip.rs:338`: its resolution does not match `Some(Resolution::AckConfirmed { .. })`. Earlier assertions established **pending false** and **exactly one Ack request**. The actual resolution was not printed; it must not be called None, timeout, or any other specific value. The raw failure block and unchanged complete original source are included.

This identity is outside the frozen original 123 and differs from the consumer test already accepted on #739. Its fixture authors Provider PCM and drives the real coordinator/consumer IPC; it does not launch the newly observed Provider worker. That narrows the path, but unchanged source and separate native passes do not prove a cause or exclude indirect timing effects on the shared host.

The exact identity passes once in the separately retained normal native runs for #909 (0.031 s), #931 (0.047 s), #925 (0.030 s) and #927 (0.031 s). Membership and line references are in `reminder-native-membership.json`; authoritative native logs are in [Serve/Records](../serve-records-native-20261016/README.md), [Gemini/Moments](../gemini-moments-arm64-20261016/README.md), and [Serve/transport](../serve-transport-native-20261016/README.md). These observations do not qualify this preparation or establish a fix.

The new original-identity failure is reported separately to Eighth for diagnosis and a deterministic fix, keeping the assertion and bounds. No unchanged failed case is retried for qualification. The new preparation remains unready until its own corrected gates and native review are complete.
