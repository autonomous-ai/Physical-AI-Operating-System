Tests, 07:56:

## Why

The unchanged identity `lamp-memory::voice_people` / `days_two_known_people_and_the_room_are_never_answered_from_each_other` failed in the first current-base capacity gate, base `e5aa43747ec1540893b4da4509d14288e7e70806`.

At `crates/memory/tests/voice_people.rs:251`, the outer `open_session_for(1, Duration::from_millis(300), Some(person)).unwrap()` received **None**. Both preceding remember calls passed. The following person-snapshot unwrap and scope-isolation assertions were not reached. The failure does not print a transport error, raw snapshot, lock outcome or elapsed time.

[Full raw gate, exact commands, independent membership/source audit and complete unchanged failure sources](https://github.com/autonomous-ai/lampOS/blob/b65454e57bb30fb4a8dd57fe53528d2b3aecc5e2/lampOS/docs/verification/capacity-current-failed-gate-20261016/README.md). Complete local result: **2,385 passed / three failed / 13 existing ignores**, all **2,401** compiled identities/ignore flags unchanged and every runtime identity accounted for once. The other two failures are the exact #906 missing motor file and #893 second-route privacy identities. The original capacity case passes. Fmt, strict affected Clippy and workspace build pass; the test gate remains failed. No retry.

The test uses the real Memory server thread/socket/store and DaysClient. `open_session_for` clamps to the unchanged 300 ms `FIRST_READ`, obtains room and optional person snapshots under one deadline, then validates context again. Outer None can arise at initial context locking, room transport/validation or final context/deadline acceptance. A missed host deadline is a hypothesis, not a diagnosis. The failed test does not start the capacity worker; unchanged source narrows the direct path but does not exclude indirect host timing effects.

This identity is **outside the frozen original 123**. Exact-name and broader voice_people/day-name issue searches found no existing report. No scope or safety assertion has been weakened.

## Wanted

Seventh / Memory owner: retain the missing operation result and request/snapshot timing, classify product versus fixture, then make this exact integration deterministic while preserving real store/IPC behavior, all scope-isolation checks and the production deadline. No retries, blind larger deadline or acceptance of None.

## Hand back

A separate reviewed fix with a causal regression, original first-failure evidence and applicable local/native gates. Until classification, this failure is not silently treated as another timed test's accepted exception.

## Most likely wrong

None alone does not show which of the bounded exchange, snapshot validation, deadline or context checks failed. Successful earlier remember calls do not establish initial-session readiness or scope isolation.
