# Requested reminders in every room state

Ready for lamp-voice: no

This checkpoint implements [First's answer on #709](https://github.com/autonomous-ai/lampOS/issues/709#issuecomment-6176285386):
due timers and reminders are requested work. Unknown, Busy and Quiet all
permit them at the next qualified local silence, with normal privacy,
readiness and ownership. The previous five-minute cold-start classification
wait and mid-answer room veto are removed. The ordinary conversation policy
and reminder tool refusal remain. The new policy is still behind the exact
`LAMP_SCHEDULE_SPEAK=on` switch.

The [frozen map](source.json), SHA-256
`e0c5985563535bf46d6af76d3bae71634ab3f1f03d8c713d55f202f95972ca7f`,
contains 697 Rust/manifest paths based on integration
`6573ce6c1b594bf9c8cf7836bbd0e162c09e4cd1`. The map's Git HEAD identifies
the pre-amend checkpoint; its individual file hashes identify the tested
working-tree revision. The [rebase](rebase.json) preserves the nine policy
edits and incorporates First's four-line early-end speech timestamp fix.
The [format record](format-record.json) identifies the two test files changed
by formatting. The [provider edit audit](provider-edit-audit-before-format.json)
precedes formatting and is retained with that narrower identity.

On Third's Intel i7-10700K x86_64 Mac, macOS 26.6.2 / Darwin 25.6.0,
64 GiB RAM, Rust/Cargo 1.96.0, the following sequence passed once with
`RUST_BACKTRACE=1`. Each command checked that all 697 source hashes remained
unchanged. Cargo times are externally measured command durations, not
interaction latency.

| Command, from `lampOS/` | Result | Seconds |
|---|---|---:|
| `cargo fmt --all --check` | Passed | 3.829060797 |
| `cargo clippy --offline --locked -p lamp-actions -p lamp-schedule -p lamp-voice -p lamp-voice-eval --all-targets -- -D warnings` | Passed | 36.387117448 |
| `cargo test --offline --locked -p lamp-voice --lib reminder -- --test-threads=1` | 74 passed, 0 failed, 2 ignored timing probes | 55.380337457 |
| `cargo test --offline --locked -p lamp-voice --lib tools:: -- --test-threads=1` | 18 passed, 0 failed | 3.405814460 |
| `cargo test --offline --locked -p lamp-voice --lib task_news::tests -- --test-threads=1` | 5 passed, 0 failed | 1.421234874 |
| `cargo test --offline --locked -p lamp-voice --lib agent_tasks::tests -- --test-threads=1` | 5 passed, 0 failed | 1.427169301 |
| `cargo test --offline --locked -p lamp-voice --lib coordinator::tests::early_end -- --test-threads=1` | 11 passed, 0 failed | 63.165346565 |
| `cargo build --offline --locked --workspace` | Passed | 18.605191566 |

The total is **113 test executions / 0 failed / 2 ignored** across five
filtered harnesses. The [original report](focused.json), unedited logs in
`raw/`, and [retention index](focused-retention.json) preserve commands,
boundaries and hashes. These are focused checks, not a complete affected-package
gate. No failed prior gate is cleared by this result.
The [whitespace check](whitespace-check.json) records raw-log whitespace
diagnostics separately from the passing check of code and authored documents;
no retained log was reformatted.

The reminder checks cover all three actual RoomTold classifications with
attention enabled and disabled, classifications changing before or during
audio/transcript/tool output, rejection of every due-turn tool, and the next
ordinary Busy-room answer retaining its normal policy. Disabled/invalid
requests never reach the input sink; current microphone authority and the
750 ms lease remain enforced. Two subprocess tests additionally prove that
the persistent claim stays held through Transport Drop after a live child
ignores TERM or after wait ownership is lost. Only the containing fixture's
exit releases the claim. These tests use the deliberate `retain_claim=false`
test seam to exercise fallback retention; production's unconditional retention
has a separate existing test. No motor fixture, assertion or deadline changed.

First assigned the untouched motor startup/reap fixtures to Fourth's #739.
The [answer snapshot](issue-709-answers.json) and
[ownership snapshot](issue-739-ownership.json) retain that decision. A full
suite failure that passes alone may be listed in the eventual PR under that
decision; this does not authorize marking a failed run passed. The complete
gate is running at this checkpoint. A fresh independent source-copy build
and default-off comparison are prepared but have not run. Later results and
readiness belong in [Third's log](https://github.com/autonomous-ai/lampOS/blob/lamp-third/lampOS/docs/work-log-third.md)
with their exact source and task-head identities.

The preceding c0 [native-boundary review](review-native-boundary-c0.md) and
[ledger/recovery review](review-ledger-recovery-c0.md) retain their read-only
scope. The former identified the missing ambiguous-cleanup tests now added
above; neither review is a test execution. The
[pre-validation retention index](retention-pre-validation.json) maps all
decision/review originals without changing their bytes.

## Not shown

No lamp, live provider, physical audio, real privacy switch or power-cut test
was used. GitHub ARM64 results for the preceding c0 checkpoint do not qualify
this revision. Actual RR20-to-Other/nice-19 transition needs a capable Linux
process or lamp; an ordinary-user host fixture does not establish it. Real
provider wording, busy-office due-to-audio latency, ALSA retirement and flash
stalls remain First's lamp cases in [schedule.md](../../../schedule.md#task-31-lamp-qualification).
