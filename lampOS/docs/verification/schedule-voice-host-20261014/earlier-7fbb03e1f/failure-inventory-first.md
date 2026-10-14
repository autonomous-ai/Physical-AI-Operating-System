# Task 31 / #709: failure and verification inventory

Prepared on 2026-10-14 from the retained logs and implementing agents' command
reports. This is scratch evidence only. No source, original log, Git state,
service, or device was changed while compiling it; no new Cargo command ran.
Main owns the final gate and the eventual repository evidence. The d5 complete
gate has now failed; the rebased 7f gate and final source-copy outcome remain
pending. This inventory does **not** claim either has passed.

Unless stated otherwise, commands ran from
`/Users/d/code/autonomous-os/lampOS`, using Rust 1.96.0
(`ac68faa20`, host `x86_64-apple-darwin`). Host metadata in
`/private/tmp/lamp-third-schedule-voice-machine.json` records Intel i7-10700K
3.80 GHz, 16 logical CPUs, 64 GiB RAM, macOS 26.6.2 / Darwin 25.6.0.
No result below is a lamp, real-provider, acoustic, power-cut, or Linux/ARM64
execution result. The later ARM64 checks compile only. Times reported by Cargo
are test/build elapsed times, not UX latency.

All filenames below are under `/private/tmp/`. Original failure logs remain
intact. Every raw log referenced below was present when read. Several early
logs omit their invocation; their exact flags are explicitly left unknown.
Empty formatting logs alone do not prove exit status; their pass status comes
from the implementing agent's preserved successful tool result/report.

## Failed attempts and their boundaries

### 1. In-progress provider integration did not compile

- `lamp-third-schedule-voice-provider-check.txt`: new `Reminder` and
  `ReminderDeferred` variants were not yet handled in all fixture/coordinator
  matches; existing `Settings` and `Options` literals lacked the new fields.
  Cargo reports five lib errors and ten lib-test errors, plus two unused-variable
  warnings in `provider_terminal.rs`. No tests ran.
- `lamp-third-schedule-voice-provider-check2.txt`: remaining `Settings` literal
  in `voice/src/main.rs:53` lacked `schedule_speak` and `schedule_tools`.
  E0063; no tests ran.
- Boundary: incomplete shared source integration at compile time, not an observed
  running-service failure. All exhaustive matches/literals were updated by their
  owners. The provider agent remembers `cargo check -p lamp-voice --tests`, but
  the precise original flags, including whether `--offline` was present, are
  unavailable; do not reconstruct them as exact commands.
- Later verification: `lamp-third-schedule-consumer-voice-lib-check-first.txt`
  contains successful production-lib checking; the provider/consumer/tool suites
  and strict Clippy logs listed below prove the later coherent test build.

### 2. Reminder tool-denial test omitted a required fake-provider terminal

- `lamp-third-schedule-voice-reminders-first.txt`: nine tests, **8 passed,
  1 failed**, zero ignored, 5.31 s. Failed test:
  `provider_worker::tests::reminders::reminder_tools_are_denied_even_in_quiet_and_do_not_change_the_next_turn`.
  Panic at then-current `provider_worker.rs:4616`: `no provider output in time`.
- `lamp-third-schedule-voice-reminder-tools-red.txt`: the same test was repeated
  alone with a backtrace; **0 passed, 1 failed**, 5.48 s, same panic. The exact
  complete shell invocation/backtrace setting is not retained in the log or
  agent's current command record. Preserve both failures.
- Boundary: authored fake-Gemini protocol fixture. It omitted the separate
  asking-turn `idle_complete` after tool replies; the real `calls_turn_open`
  guard correctly withheld final completion. The fixture was corrected to send
  that terminal; no production completion guard was relaxed.
- Later verification: `lamp-third-schedule-voice-reminders-fourth.txt`, exact
  command `cargo test --offline -p lamp-voice --lib provider_worker::tests::reminders -- --test-threads=1`,
  **11 passed**, zero failed/ignored, 0.60 s. The count grew because two additional
  regressions had been added. The exact named test also passes in the post-rebase
  log, so this is not a claim based only on a changed aggregate count.

### 3. A declared test module had not yet been saved

- `lamp-third-schedule-consumer-voice-check-first.txt`, exact command
  `cargo check --offline -p lamp-voice --tests`: compiler could not read
  `crates/voice/src/tools/schedule_integration_tests.rs`, referenced at
  `tools.rs:44`. No tests ran.
- `lamp-third-schedule-voice-reminders-second.txt`: a separate provider test
  attempt encountered the same missing module; no tests ran. Its full invocation
  is unavailable from the retained raw log/agent command record.
- Boundary: shared-file sequencing during implementation. The premature test
  linkage was temporarily removed; the completed file was saved and linked again.
- Intermediate production-only check:
  `cargo check --offline -p lamp-voice --lib` passed in 2.77 s, retained in
  `lamp-third-schedule-consumer-voice-lib-check-first.txt`.
- Later verification: all five new real Voice→Actions integration cases passed
  within `lamp-third-schedule-voice-tools-first.txt` (16 tools tests total).

### 4. A transport path-trap fixture used an unavailable Darwin API

- `lamp-third-schedule-voice-reminders-third.txt`: E0425 at then-current
  `reminders/transport.rs:733`, `rustix::fs::mknodat` unavailable. No tests ran.
  The full original command is not recorded; this was a provider-focused test
  build of the shared Voice lib-test target.
- Boundary: test portability, not production claim-file I/O. Common path traps
  remain portable; the FIFO fixture now uses `mkfifoat` behind Linux cfg.
- Later verification: provider fourth run and transport second run passed on
  Darwin. The Linux-only FIFO branch was **not exercised** by these Mac runs.

### 5. Daily recurrence ledger fixture queried before the actual due time

- Exact command `cargo test --offline -p lamp-schedule --test delivery_ledger`,
  first redirected to `lamp-third-schedule-delivery-first.txt`: **23 passed,
  1 failed**, zero ignored, 0.46 s.
- Failed test `exact_old_ack_replay_never_consumes_a_later_daily_occurrence`;
  panic at then-current `tests/delivery_ledger.rs:115:75`,
  `Error { code: "not_pending" }`.
- Boundary: authored calendar fixture used a guessed midnight offset before
  Engine's actual scheduled deadline. Correction derives `first_at` from the
  real Engine reply's `due_unix_ms - WALL`, then `next_at = first_at + 86_400_000`.
  The original guessed numeric literal is not recoverable from the retained
  summary; no value is invented here. Production ledger code did not change.
- Identical command, later output `lamp-third-schedule-delivery-second.txt`:
  **24 passed**, zero failed/ignored, 0.43 s. Scoped strict Clippy and formatting
  passed in `lamp-third-schedule-delivery-clippy-first.txt` and
  `lamp-third-schedule-delivery-format-check.txt`.

### 6. Fixture replay reused a stale ownership snapshot

- Exact command for both attempts:
  `cargo test --offline -p lamp-voice --lib fixture_provider::reminder_tests -- --test-threads=1`.
- `lamp-third-schedule-voice-fixture-first.txt`: **0 passed, 1 failed**,
  zero ignored. Failed test
  `fixture_provider::reminder_tests::fixture_refuses_a_reminder_once_without_audio_or_disabling_later_ordinary_input`;
  panic at then-current `fixture_provider.rs:745:44`, `StaleSnapshot`.
- Boundary: test helper resent the same Snapshot as a heartbeat. The ownership
  guard correctly rejected it. Correction supplies a fresh `owner.snapshot`
  instead of reusing the old serialized snapshot; no authority rule changed.
- `lamp-third-schedule-voice-fixture-second.txt`: **1 passed**, zero failed/ignored.

### 7. Strict Clippy caught one production expression and two test assertions

- `lamp-third-schedule-voice-provider-clippy-first.txt` came from
  `cargo clippy --offline -p lamp-voice --lib --tests -- -D warnings`.
- `consumer.rs:56`: `nonminimal_bool` on negated `checked_sub().is_some_and()`.
  Corrected to `checked_sub().is_none_or(|age| age > DUE_AGE_US)`.
- `consumer/tests.rs:924` and `:1010`: `manual_contains`; replaced
  `iter().any()` equality assertions with `contains()`.
- Boundary: lint failure; these rewrites preserved the intended semantics.
- Same strict command passed in `lamp-third-709-voice-clippy-final.txt`
  (20.28 s) and later `lamp-third-709-roundtrip-clippy-first.txt` (21.48 s).

### 8. Real production transport gap: Darwin datagram ENOBUFS

- Exact command for both attempts:
  `cargo test --offline -p lamp-voice --lib reminders::transport::tests -- --test-threads=1`.
- `lamp-third-schedule-consumer-transport-first.txt`: **16 passed, 1 failed,
  1 ignored**, 4.85 s. Failed
  `reminders::transport::tests::flood_is_backpressured_and_poll_retains_only_one_ordered_packet`;
  panic at then-current `transport.rs:741:66`, OS error 55 / `No buffer space available`.
- Boundary: production portability bug exposed by a real saturated anonymous
  datagram pair. Darwin `sendmsg` can return ENOBUFS under backpressure; the
  adapter previously treated it as fatal instead of an unsent transient.
- Production correction: `send_packet` returns `Ok(false)` for ENOBUFS as well
  as EAGAIN/EINTR. It does not spin, claim delivery, or discard the pending
  packet. The regression drains all accepted counters in order and retries
  the rejected counter exactly once.
- `lamp-third-schedule-consumer-transport-second.txt`: **17 passed, 0 failed,
  1 ignored**, 4.83 s. Ignored `poll_cost` was run separately and passed (below).
  Supporting failure explanation and primary-source provenance are retained in
  `lamp-third-schedule-consumer-transport-failures.md`; no new web lookup was
  made for this inventory.

### 9. Restricted-sandbox socket denial in the combined reminder run

- `lamp-third-schedule-voice-reminder-path-first.txt`: **38 passed, 26 failed,
  1 ignored**, 13.68 s. All 26 panics contain OS error 1 / PermissionDenied /
  `Operation not permitted` while creating the test sockets.
- Main confirms this was the restricted-sandbox run, not an automatic approval
  review rejection. No source correction was made for these denials.
- The same source rerun with private-socket permission produced
  `lamp-third-schedule-voice-reminder-path-second.txt`: **64 passed, 0 failed,
  1 ignored**, 11.53 s.
- Main can recover only that these were serial `lamp-voice --lib` runs with a
  `reminder` filter. Exact early flags are unavailable; do not label a guessed
  command as the original. Exact post-rebase invocations are preserved below.
- The raw first log lists every failed test; its exact 26-name list is appended
  to this inventory for retention. These environment denials are distinct from
  the earlier tool-denial fixture failure, even though one test name repeats.

### 10. Scratch baseline archive preparation hit an older host Python API

- `lamp-third-schedule-rebased-baseline-prepare-first.txt` records exact command
  `python3 /private/tmp/lamp-third-schedule-rebased-baseline-prepare.py`, cwd
  `/Users/d/code/autonomous-os`, exit 1. `tarfile.extractall(..., filter='data')`
  raised `TypeError: extractall() got an unexpected keyword argument 'filter'`.
- Boundary: scratch artifact preparation, before examples or Cargo ran; no
  production Rust failure. The helper retained explicit checks rejecting links,
  nonregular entries and traversal, then omitted the unsupported keyword.
- The second preparation succeeded in
  `lamp-third-schedule-rebased-baseline-prepare-second.txt`. The first archive
  and empty scratch directory were retained.
- Later baseline capture/build logs are listed below. They are pre-change
  source-baseline evidence, **not** proof that the modified final copy matches.

## Production corrections found by read-only audit, without a retained red run

These are genuine source-level corrections. Do not invent a failing test
transcript: dedicated tests were written after the findings/fixes and passed.

1. **Camera continuation watermark.** Stored reminder text such as "Let me
   look" could otherwise enter the ordinary transcript-derived camera
   continuation path, including when a delayed transcript outlived the current
   reminder. The coordinator now records the latest reminder turn and refuses
   that turn and earlier delayed transcripts for this purpose; later ordinary
   turns preserve their existing policy. Passing regression:
   `coordinator::tests::reminders::stored_reminder_words_cannot_start_a_camera_followup_even_when_transcript_is_late`.
2. **Current admission timestamp.** The loop-start timestamp could precede a
   newly received capture observation, causing the reminder's checked age to
   appear future and preventing admission despite fresh quiet evidence. The
   enabled reminder check now samples the current monotonic time at admission
   for readiness/quiet/playback/offer qualification. Passing regression:
   `coordinator::tests::reminders::reminder_quiet_uses_current_admission_time_after_new_capture_arrived_in_loop`.
   Both exact tests pass in `lamp-third-schedule-voice-reminder-path-second.txt`
   and `lamp-third-schedule-post-rebase-reminders-first.txt`.
3. **Typed freshness sequence fence.** A greater Bus header sequence could
   previously reuse the typed sequence. The consumer now requires a strictly
   increasing typed sequence for a greater header sequence, while an unchanged
   retained message must preserve exact bytes and never refresh its age.
   Passing test:
   `reminders::consumer::tests::retained_bus_message_never_renews_age_and_reused_typed_sequence_is_refused`.
4. **Same-revision lost-Ack recovery.** An already persisted Ack intent at the
   same current pending revision was unconditionally rebased, which the ledger
   correctly rejected as not newer. Recovery now retains the exact existing
   Ack when the guard revision is unchanged and rebases only on fresh newer
   evidence. Passing test:
   `reminders::consumer::tests::uncommitted_lost_ack_at_the_same_revision_keeps_the_exact_attempt_and_recovers`.
   Both consumer regressions pass in `lamp-third-709-consumer-first.txt` and
   the post-rebase reminder log.

## Successful focused runs and exact available commands

Counts overlap between focused and combined runs; **do not add them into a
unique-test total**. All listed test successes have zero failures. `ignored`
is explicitly reported where nonzero.

| Raw log | Exact command or recorded scope | Observed result |
|---|---|---|
| `lamp-third-709-client-first.txt` | `cargo test --offline -p lamp-schedule --test client -- --test-threads=1 --nocapture` | 7 passed, 0 ignored, 2.17 s; Linux-only full-accept-queue case not compiled on this Mac |
| `lamp-third-709-actions-first.txt` | `cargo test --offline -p lamp-actions --all-targets -- --test-threads=1 --nocapture` | 33 passed across 14 lib + 1 main + 14 Schedule + 4 server tests; 0 ignored |
| `lamp-third-709-actions-fixture-second.txt` | `cargo test --offline -p lamp-actions --test schedule -- --test-threads=1 --nocapture` | 14 passed, 0 ignored, 5.60 s; after explicit accepted-stream blocking mode in fixture |
| `lamp-third-709-consumer-first.txt` | `cargo test --offline -p lamp-voice --lib reminders::consumer::tests:: -- --test-threads=1 --nocapture` | 20 passed, 0 ignored, 2.41 s |
| `lamp-third-schedule-voice-tools-first.txt` | `cargo test --offline -p lamp-voice --lib tools:: -- --test-threads=1` | 16 passed, including five Voice→real Actions→Schedule fixture integration cases; 0 ignored, 1.87 s |
| `lamp-third-709-roundtrip-first.txt` | `cargo test --offline -p lamp-voice --lib coordinator::tests::reminder_roundtrip:: -- --test-threads=1 --nocapture` | 4 passed, 0 ignored, 3.29 s; first attempt |
| `lamp-third-schedule-post-rebase-reminders-first.txt` | `cargo test --offline --locked -p lamp-voice --lib reminder -- --test-threads=1 --nocapture` | 68 passed, 2 ignored, 15.17 s |
| `lamp-third-schedule-post-rebase-flags-first.txt` | `cargo test --offline --locked -p lamp-voice --lib tools::tests::tasks_then_schedule_flags_keep_both_owners_and_their_declarations -- --exact --test-threads=1 --nocapture` | 1 passed, 0 ignored |
| `lamp-third-schedule-consumer-transport-cost-first.txt` | `cargo test --offline -p lamp-voice --lib reminders::transport::tests::poll_cost -- --ignored --exact --nocapture --test-threads=1` | 1 passed separately, 0 ignored, 0.01 s |
| `lamp-third-schedule-parent-cost-first.txt` | `cargo test --offline --locked -p lamp-voice --lib reminders::tests::parent_poll_cost -- --ignored --exact --test-threads=1 --nocapture` | 1 passed separately, 0 ignored, 0.03 s |

The two ignored tests in the post-rebase run are exactly the parent and
transport polling measurements above. Their separate passes do not turn them
into acoustic latency tests. The post-rebase command arrays, exit codes and
wall durations are in `lamp-third-schedule-post-rebase-focused.json`.

The round-trip tests use a real Schedule Engine/Store, real durable delivery
Store, real Bus reads, claim descriptor transfer/datagrams, actual Reminders
manager and coordinator, and actual speaker control handling of authored
partial PCM/retirement receipts. They do not fabricate `QualifiedCompletion`.
They prove no Ack after the first generation/final PCM acceptance, pending after
interruption or mute, and no re-speech after durable completion/lost Ack/restart.
Authored speaker receipts still do not establish physical acoustic completion.

Strict Clippy passes worth retaining:

- `lamp-third-709-actions-clippy-first.txt`:
  `cargo clippy --offline -p lamp-actions --all-targets -- -D warnings`.
- `lamp-third-709-client-clippy-first.txt`:
  `cargo clippy --offline -p lamp-schedule --lib --test client -- -D warnings`.
- `lamp-third-schedule-delivery-clippy-first.txt`: scoped Schedule ledger Clippy
  passed; exact original flags are not repeated here without a command record.
- `lamp-third-schedule-consumer-transport-clippy-first.txt`:
  `cargo clippy --offline -p lamp-voice --lib --tests -- -D warnings`, 12.09 s.
- `lamp-third-709-voice-clippy-final.txt` and
  `lamp-third-709-roundtrip-clippy-first.txt`: same Voice lib/tests command,
  20.28 s and 21.48 s respectively.
- Successful scoped-format reports: `lamp-third-709-scoped-format-final.txt`,
  `lamp-third-709-roundtrip-format-first.txt`,
  `lamp-third-schedule-delivery-format-check.txt`, and
  `lamp-third-schedule-consumer-transport-format-check.txt`.

## Timing and pre-change baseline artifacts worth retaining

- Both Actions timing samples are inside the corresponding first/second
  Actions test logs above: 21 successful calls each, zero failures. First sample
  first 135.160 ms, repeated-20 p50 88.388 ms / p95 143.321 ms / max 143.420 ms.
  Second sample first 134.044 ms, repeated p50 89.074 ms / p95 146.202 ms /
  max 146.297 ms. Boundary: client entry before real Actions connect through
  parsed durable Schedule response, with real SQLite FULL persistence and an
  injected Schedule clock. Startup, provider, audio and lamp excluded. The
  250 ms target was not a functional timing assertion or filesystem guarantee.
- `lamp-third-schedule-consumer-transport-cost-first.txt` and `.json`: actual
  `Transport::poll` elapsed samples for authored idle/incoming/outgoing queues;
  excludes parent JSON/coordinator work, process/claim startup and all I/O to
  Schedule, storage, provider or audio.
- `lamp-third-schedule-parent-cost-first.txt` and `.json`: actual
  `Reminders::poll` elapsed samples including its bounded JSON and settlement
  work, with in-process datagram peers. Excludes Schedule/store, full coordinator
  iteration, child startup, provider and audio. No hard threshold assertion.
- `lamp-third-schedule-rebased-baseline-prepare-{first,second}.txt`,
  `lamp-third-schedule-rebased-baseline-build-first.txt`,
  `lamp-third-schedule-rebased-baseline-capture-first.txt`,
  `lamp-third-schedule-rebased-baseline.json`, and
  `lamp-third-schedule-rebased-baseline-compare.md`: pinned pre-change
  `3773c1fb88190fb5289bf1378eaa1f5fe5d134cb` source copy and 72 pure metadata/API
  cases (36 ordinary + 36 with existing Tasks). Its offline locked scratch
  helper build passed. The later final-copy comparison remains Main's gate.

For a compact retained failure set, keep **every first/red/check2/second/third
failure filename in sections 1–10**, paired with its named successful run,
plus the post-rebase-focused JSON and the two audit-regression passing logs.
Do not substitute a green summary for a raw red log. Earlier Task6 calendar,
storage, runtime, or unrelated Touch/Motor failures are outside this inventory.

## Exact failed names from the restricted-sandbox attempt

The following list is copied from the final `failures:` block in
`lamp-third-schedule-voice-reminder-path-first.txt`; it is one run, not 26
independent production defects.

```text
coordinator::tests::reminders::interruption_after_first_retirement_never_acknowledges_later_audio
coordinator::tests::reminders::no_audio_or_provider_failure_stays_pending_and_selects_no_failure_recording
coordinator::tests::reminders::only_a_fenced_attempt_without_any_provider_pcm_can_retry
coordinator::tests::reminders::reset_or_playback_gap_cannot_be_relabeled_whole_reminder_completion
coordinator::tests::reminders::whole_reminder_joins_two_real_retirements_and_successful_provider_terminal
provider_worker::tests::reminders::becoming_busy_before_audio_transcript_or_tool_output_defers_and_suppresses_terminal
provider_worker::tests::reminders::busy_refusal_is_independent_of_attention_and_the_next_ordinary_turn_works
provider_worker::tests::reminders::disabled_and_unknown_refusals_keep_the_worker_alive_without_model_submission
provider_worker::tests::reminders::reminder_tools_are_denied_even_in_quiet_and_do_not_change_the_next_turn
provider_worker::tests::reminders::unanswered_reminders_are_not_room_activity_but_their_actual_audio_is
reminders::consumer::tests::abandon_keeps_pending_and_restart_never_grants_the_uncertain_audio_again
reminders::consumer::tests::an_offer_is_committed_before_handoff_and_fresh_renewals_keep_the_same_token
reminders::consumer::tests::cancel_snooze_store_switch_and_missing_due_withdraw_existing_offer
reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech
reminders::consumer::tests::delayed_durable_prepare_cannot_publish_an_already_expired_offer
reminders::consumer::tests::evicted_ack_receipt_resolves_by_current_absence_without_claiming_its_cause
reminders::consumer::tests::full_ledger_refuses_new_admission_without_evicting_any_uncertain_delivery
reminders::consumer::tests::lost_committed_ack_is_recovered_after_restart_without_another_offer
reminders::consumer::tests::offered_tuple_must_match_current_private_get_store_delivery_and_revision
reminders::consumer::tests::old_uncertain_item_does_not_starve_the_next_pending_occurrence
reminders::consumer::tests::only_exact_qualified_completion_can_change_the_durable_prepared_record
reminders::consumer::tests::outgoing_queue_is_bounded_and_replacing_an_offer_never_discards_settlement
reminders::consumer::tests::qualified_pre_audio_retirement_retries_once_with_a_new_durable_correlation
reminders::consumer::tests::slow_get_withdraws_current_authority_without_refreshing_its_observation
reminders::consumer::tests::storage_failure_before_prepare_withholds_the_offer_and_latches_failure
reminders::consumer::tests::uncommitted_lost_ack_at_the_same_revision_keeps_the_exact_attempt_and_recovers
```

## Addendum: first complete-package gate failed; unresolved

Main reported the first full serial gate complete after the initial inventory
was written. **This gate is red; its narrower reruns are recorded in the next
addendum, while the replacement complete gate remains unresolved.** The original raw test
output and result metadata are preserved as:

- `/private/tmp/lamp-third-schedule-voice-gate-first-tests.txt`
- `/private/tmp/lamp-third-schedule-voice-gate-first-results.json`
- Supporting read-only source preflight:
  `/private/tmp/lamp-third-schedule-motor-failure-preflight.md`

Exact test command, from `/Users/d/code/autonomous-os/lampOS`:

```sh
cargo test --offline --locked -p lamp-actions -p lamp-schedule -p lamp-voice -p lamp-voice-eval -- --test-threads=1
```

Exit code **101**, measured command time **656.960286296 s**. Sixteen completed
test harnesses reported **1,111 passed, 1 failed, 8 ignored**. Voice lib itself
reported **918 passed, 1 failed, 8 ignored**, 492.96 s. This is a partial suite
execution stopped by failure, not a completed green four-package gate. No
Schedule test failed in this attempt.

Exact failed test:

```text
coordinator::motor::tests::frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted
```

The panic is at `crates/voice/src/coordinator/motor/tests.rs:441:9`:
`owned child timed out: None`. The helper has a two-second real-clock wait;
the production supervisor's startup allowance is three seconds. These source
deadlines do not establish why this particular run missed readiness. `None`
means the helper printed no stored motor failure at timeout; it does not prove
healthy startup, successful cleanup, or a harmless scheduling delay.

The retained preflight records an empty diff against upstream `3773c1fb` for
the motor fixture/preparation, process helper, Body supervisor/worker/channel/
config, and IPC files. The fixture directly launches the current unit-test
binary's exact `coordinator::motor::tests::inert_child` entry, with no normal
Voice startup or reminder-consumer path. This supports classifying the failure
as an **inherited motor fixture/worker boundary outside the directly changed
Schedule path**, but does not establish a root cause or exclude an indirect
shared-host/startup effect. The shared test binary is newly linked; source
equality is not binary equality. Do not relabel this as a proven flake or erase
the failure because narrower tests passed earlier.

The same result metadata records `cargo fmt --all --check` passing (3.6977 s)
and strict offline/locked all-target Clippy for the four named packages passing
(22.2404 s), with source hashes unchanged throughout the gate. Those passes do
not supersede the failed test stage. Main owns all further investigation and
reruns; no repository edit, Cargo run, process action, or device access was
performed while adding this note.

## Addendum: focused motor reruns passed; complete gate still unresolved

The unchanged-source focused reruns are now recorded in
`/private/tmp/lamp-third-schedule-motor-retry.json`. Both commands ran from
`/Users/d/code/autonomous-os/lampOS`; the raw logs were read to confirm the
test totals. These are host results, not lamp measurements.

```sh
cargo test --offline --locked -p lamp-voice --lib coordinator::motor::tests::frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted -- --test-threads=1 --nocapture --exact
```

Raw: `/private/tmp/lamp-third-schedule-motor-exact-first.txt`.
Exit 0; **1 passed, 0 failed, 0 ignored**; test time **1.77 s**;
command time **3.67886018 s**.

```sh
cargo test --offline --locked -p lamp-voice --lib coordinator::motor::tests:: -- --test-threads=1 --nocapture
```

Raw: `/private/tmp/lamp-third-schedule-motor-module-first.txt`.
Exit 0; **28 passed, 0 failed, 0 ignored**; test time **37.20 s**;
command time **38.499790905 s**.

The metadata explicitly records `source_unchanged: true` for each run. No
production fix was made for the original timeout, and these narrower passes
do not establish its cause or turn the earlier complete-package gate green.
Main then reported a rebase onto `d5d765862` at 16:24, resolving only a
documentation/reuse conflict while preserving Mood and Task31. The resulting
source also includes an upstream nine-line `short_sound`/`not_counted`
coordinator change. The later d5 gate is recorded below as failed; these
earlier narrow passes do not qualify it. No Cargo command or repository edit was run
to add this evidence update.

## Addendum: inherited realtime scheduling found by source audit

This is a production correction found by read-only source/API review, with no
retained failing execution to invent. The checked-in lamp-4ace Voice unit
selects SCHED_RR priority20. Schedule's internal child used Command with a new
process group, which does not reset scheduler policy. Linux preserves the
spawning thread's policy across fork/exec unless reset-on-fork is selected.
Consequently the optional persistence child could inherit RR20 and compete
with urgent audio. This is a source-contract finding, not a lamp measurement
or a demonstrated cause of the earlier unrelated motor fixture timeout.

Pinned rustix1.1.5 has no safe scheduler-policy setter. The approved correction
uses Linux-only thread-priority3.1.1, setting Other + Min and checking the
result as Other/static priority0/nice19. It runs immediately after the exact
Schedule speech enable gate, before configured paths, claim, storage, or
additional threads. Setter/readback failure exits only this optional child.
The parent/audio policy is untouched; non-Linux hosts retain their policy.
The low niceness priority is deliberate and no response-time improvement is
claimed. Maintained API/source references and the initial alternative analysis
are in `/private/tmp/lamp-third-schedule-child-scheduler-audit.md`.

Verification retained by Main:

- `/private/tmp/lamp-third-schedule-priority-check-first.txt`: host Voice Cargo
  check passed, Cargo-reported 32.89 s. The subsequently retained repository
  `scheduler-host-check.json` records the exact command
  `cargo check -p lamp-voice --tests`, exit 0, command 33.01705593 s. The raw
  output records exactly one new locked package, thread-priority3.1.1.
  Host cfg excludes the Linux
  implementation, so this pass alone does not validate its code path.
- `/private/tmp/lamp-third-schedule-priority-fetch-first.txt`: exact command
  `cargo fetch --locked --target aarch64-unknown-linux-gnu`, cwd
  `/Users/d/code/autonomous-os/lampOS`; exit0, command0.57425448 s.
- `/private/tmp/lamp-third-schedule-priority-arm-compile-first.txt`: exact
  `cargo +1.96.0 check --offline --target aarch64-unknown-linux-gnu --tests`;
  exit0, command3.887204222 s, Cargo3.85 s.
- `/private/tmp/lamp-third-schedule-priority-arm-clippy-first.txt`: exact
  `cargo +1.96.0 clippy --offline --locked --target aarch64-unknown-linux-gnu --all-targets -- -D warnings`;
  exit0, command0.24602394899999958 s, Cargo0.17 s.
- The two ARM-target commands ran in
  `/private/tmp/lamp-third-schedule-priority-arm-mye86tiq`, a standalone helper
  containing the exact scheduling module and its Linux-only isolated-thread
  test. `/private/tmp/lamp-third-schedule-priority-arm-check.json` records
  `copied_source_byte_identical: true` and source SHA-256
  `1e168a42221dddd2352a99ef636311156c98bcf1f693e4a31b081aae7f34caac`.
- `/private/tmp/lamp-third-schedule-child-scheduler-format.txt`: scoped
  rustfmt1.96.0 check passed, with an intentionally empty successful log.

The ARM result is **compilation and strict Clippy only for that isolated
module/test**. It is not linking, a complete ARM64 Voice build, Linux test
execution, an actual RR20-to-Other transition, or lamp evidence. The real
Linux setter/getter test is authored but has not been run in this record.
No ignored/skipped Linux test is counted as a pass. There is no new runtime
timing or audio measurement associated with this correction.

Main froze 675 source paths on base
`d5d765862ab1f73629ee7a2c5a9a8086f0246d11`, branch `third/schedule-voice`, in
`/private/tmp/lamp-third-schedule-voice-final-source-frozen.json`. The latest
complete gate was running when this addendum was first prepared. Its failed
result is recorded below; narrower motor/compile passes do not supersede
either red complete-gate record.

## Earlier scratch link/count audit

After the scheduler addendum, 64 referenced scratch files were checked,
including expanded brace filenames and the two companion timing JSON files:
**64 present, 0 missing**. Five intentionally empty format logs remain;
their pass claims rely on the preserved successful command reports, not on
file size. The result lines in all 24 referenced raw test logs were reread;
their per-run passed/failed/ignored counts agree with this inventory. The
restricted-sandbox failure list contains exactly 26 names and matches the raw
final `failures:` block in order. Overlapping suites are still not added into
a unique-test count. This audit made no repository edits and ran no Cargo or
test process; the final complete gate remains Main's responsibility.

## Addendum: d5 complete gate failed with two inherited motor timeouts

This is a second complete-gate failure, not a replacement for the earlier
3773 failure or its focused reruns. Raw evidence:

- `/private/tmp/lamp-third-schedule-voice-final-gate-results.json`.
- `/private/tmp/lamp-third-schedule-voice-final-gate-tests.txt`.
- `/private/tmp/lamp-third-schedule-voice-final-gate-source.json`: base
  `d5d765862ab1f73629ee7a2c5a9a8086f0246d11`, 675 source paths.
- `/private/tmp/lamp-third-schedule-voice-final-gate-format.txt` and
  `/private/tmp/lamp-third-schedule-voice-final-gate-clippy.txt`.

Exact command, cwd `/Users/d/code/autonomous-os/lampOS`:

```sh
cargo test --offline --locked -p lamp-actions -p lamp-schedule -p lamp-voice -p lamp-voice-eval -- --test-threads=1
```

Exit **101**, command **676.865360202 s**. Across 16 completed harnesses:
**1,111 passed, 2 failed, 8 ignored**. Voice lib: **918 passed, 2 failed,
8 ignored**, test time **491.59 s**. The eight ignored cases remain listed
in the result JSON; ignored cases are not passes. All Schedule harnesses
completed without failures. The later build stage was not reached. The
metadata records source unchanged throughout this gate. Formatting passed
in 3.910192944 s; strict offline/locked all-target Clippy for the same four
packages passed in 43.095510111 s. These successes do not override test failure.

Both failures panic at `crates/voice/src/coordinator/motor/tests.rs:441:9`,
`owned child timed out: None`:

1. `coordinator::motor::tests::crash_and_stalled_control_never_allow_replacement_or_false_cleanup`.
2. `coordinator::motor::tests::failed_reap_after_stopped_is_latched_and_reported_once`.

The previously failing
`frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted`
passed in this d5 run. No fix or cause is established by the change in failed
names. The d5 raw log has no parent backtrace, child stderr, phase timing or
crash/hang subcase label.

The read-only follow-up is in
`/private/tmp/lamp-third-schedule-d5-motor-failure-audit.md`. The second named
failure is definitely the initial readiness wait at line 1001, before any
shutdown/Stopped/reap exercise. The first is the reaped predicate at line 645;
the failed crash/hang subcase cannot be recovered from this log. The common
helper's 2 s deadline is checked before its next poll, while the supervisor
allows 3 s for startup. This makes delayed pre-Ready startup or an unserviced
parent interval consistent with `None`, without proving either occurred.

Main has moved the working base to `7fbb03e1f`, whose IPC change makes an
owned not-yet-private socket a retryable condition. The motor channel already
waited for both private peer sockets on d5; the motor fixture does not use
Voice's normal Worker startup helper. Consequently the new fix is not a
demonstrated explanation for these timeouts. Preserve the d5 failure and
qualify 7f separately. No final 7f gate, final copy result, Linux execution or
lamp result is claimed here. This update ran no Cargo or test process and
changed no repository source.

## Addendum: 7f focused motor failure also reproduced without Task 31

Two serial focused runs used the same command, with `RUST_BACKTRACE=1`:

```sh
cargo test --offline --locked -p lamp-voice --lib coordinator::motor::tests:: -- --test-threads=1 --nocapture
```

- Integrated source on `7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`, cwd
  `/Users/d/code/autonomous-os/lampOS`: exit 101, **27 passed, 1 failed,
  0 ignored**, 40.38 s tests, 70.988669401 s command, 24.35 s Cargo compilation.
  Raw `/private/tmp/lamp-third-schedule-7f-motor-first.txt`; exact metadata
  `/private/tmp/lamp-third-schedule-7f-motor-first.json`. The crash/hang test
  failed at helper441, and the backtrace identifies the initial reaped wait at
  test line645. `failed_reap...` and `frozen_config...` passed in this attempt.
  The preceding load averages were 3.26/4.09/4.63 on this 16-logical-CPU host;
  those averages do not measure per-thread startup or scheduling delays.
- Clean 7f baseline **without Task31 source**, cwd
  `/private/tmp/lamp-third-schedule-7fbb03e1f-baseline-kg0lzf0c/lampOS`:
  exit 101, **25 passed, 3 failed, 0 ignored**, 41.22 s tests,
  80.425533549 s command. `CARGO_TARGET_DIR` was explicitly the shared
  `/Users/d/code/autonomous-os/lampOS/target`; builds remained serial. Raw
  `/private/tmp/lamp-third-schedule-7f-clean-motor-first.txt`; exact metadata
  `/private/tmp/lamp-third-schedule-7f-clean-motor-first.json`, which records
  `task_source_present: false`. Main reports baseline source unchanged.
  Failed cases, all helper441/`owned child timed out: None`:
  `crash_and_stalled_control_never_allow_replacement_or_false_cleanup`
  (reaped wait645),
  `explicit_current_segment_reaches_inert_counts_without_completing_voice`
  (initial readiness905), and
  `failed_reap_after_stopped_is_latched_and_reported_once`
  (initial readiness1001). The preceding load averages were 2.67/3.52/4.27.

The clean baseline establishes that this failure occurs without Task31.
It does not determine the timing cause, validate a fix or turn either gate
green. Scratch-only diagnostic instrumentation is prepared in
`/private/tmp/lamp-third-schedule-motor-diagnostics-plan.md` and its linked
patch/manifest. It has not been compiled or executed; no root/final-copy
source or production deadline was changed. Final complete 7f qualification
and final standalone outcome remain pending here.
