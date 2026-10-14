# Task 31 checkpoint: bounded ledger/recovery source review

Ready for lamp-voice: no

Read-only inspection of commit `c0bd596ac15ca64d7dc09534a65165e2be1eab03` on Third's development Mac. No source change, process exercise, new test execution or lamp access. This inspection does not clear the failed complete host gate.

Scope: `crates/voice/src/reminders/consumer.rs` command, offer, tick and recover_one; `crates/schedule/src/delivery.rs` prepare, retry_interrupted, complete, plan_ack, rebase_ack, reconcile and compact_resolved, with the corresponding named ledger tests.

No concrete defect was found in this bounded inspection. The following are source observations, not additional test results:

- Consumer commands require the exact current token (consumer.rs:176). Completion is persisted before Settled. A mismatched token is rejected; Abandon does not manufacture completion.
- Offer construction re-reads Get and binds the exact store/id/delivery and a revision at least as recent as the due notice (consumer.rs:215). Its lease begins before I/O and is checked after durable prepare, so storage delay cannot refresh the observation.
- Recovery attempts the exact recorded Ack and matching status receipt before new Get-based reconciliation (consumer.rs:362). A new revision guard is durably recorded while retaining the same logical request id. Uncertain entries without a completion are not automatically replayed; the bounded pending page can reach other items.
- Ledger reconciliation separates matching Ack, superseded delivery and identity-bound absence (delivery.rs:758). Compaction removes only resolved entries and retains the revision fence/counters (delivery.rs:840). Ambiguous commit errors poison the in-memory instance (delivery.rs:879).

Existing test coverage read, not rerun: lost_ack_reply_survives_server_restart_and_due_revision_change; unrelated_mutation_before_ack_requires_explicit_persisted_rebase_of_same_logical_id; overwritten_committed_ack_receipt_resolves_absence_as_gone_not_ack_confirmed; exact_old_ack_replay_never_consumes_a_later_daily_occurrence; full_unresolved_ledger_never_evicts_and_only_durable_resolution_frees_capacity; every_durable_transition_poisoned_on_before_or_after_commit_error_recovers_only_from_disk.

Limits: no new fault injection, exhaustive state-space proof, filesystem crash test, live provider/audio or scheduler observation. The task's earlier execution records retain their own exact source identities. Native CI is pending; the unrelated motor failure and Unknown-room questions remain open.
