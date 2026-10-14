Task 31 final integration source review, 2026-10-15

Base and fetched integration head:
`d8e9556dea3f1867a5af5297d1ebddbd165d9102`.
Compared with prior task stash
`c834db1a37f3cb14ff374d556129dbf2c80df4dd`, whose base was
`1a38f2f3e6b8afd1e7896761a0da6149a2400f67`.
Repository: `/Users/d/code/autonomous-os`; Task 31 is still uncommitted.

No Task 31 rebase defect found in the reviewed integration seams. One
previously reported upstream task-news defect remains:

- `coordinator.rs:3640-3642` records qualifying early-ended speech in
  `said_turn` without refreshing `said_at_us`. Successful early confirmation
  returns Some at `:3784-3786`, bypassing the sole timestamp assignment at
  `:3793` inside `if sent.is_none()`. The new news-presence gate at
  `:4395-4396` can therefore remain false after genuine early-ended
  conversations when no fresh face is available. This is unchanged from
  fd1a273ee/d8, not introduced or repaired by Task 31. First's follow-up
  should cover early-ended speech, startup, short sounds and autonomous
  speech separately; no timing assertion or timeout change is indicated.

Evidence:

- Independently rehashed all 508 files in the pre-rebase record: 505 are
  byte-identical, none missing, only the three expected files differ.
  For each of those three, the complete ordered added/deleted lines from
  old task stash to working tree exactly equal the old-base-to-d8 upstream
  edit lines; source context/line offsets were excluded from that comparison.
- Cargo.lock retains Task 31's Actions/Schedule/Voice dependencies and
  adds the upstream Touch `i2cdev` 0.6.2, `nix` 0.26.4 and Controls dependency
  entries. Reuse decisions retain both the Task 31 section and upstream
  Touch provenance/qualification limits. The coordinator's only new delta
  from the old task tree is the incoming person-presence change.
- The changed upstream Body, Controls, Msgs, agent_tasks and task_news Rust
  and manifest files were compared byte-for-byte with d8 HEAD. No local
  difference was found. Task 31's tracked diff remains in Actions, Schedule,
  Voice, Cargo.lock and its docs, with no Body/Controls/Msgs modifications.
- Order remains promised camera look (`coordinator.rs:4224`), SecondLook
  (`:4262`), reminder (`:4301`), greeting (`:4348`), news (`:4389`). The shared
  selector (`:4789`) still refuses a live Reply or pending follow-up.
  `said_at_us` remains heard-person presence; `last_turn_us` remains news
  quiet time; reminder quiet/fresh-capture checks remain separate. Reminder
  PCM does not write the presence timestamp.
- Reminders still return None before spawning anything unless
  `LAMP_SCHEDULE_SPEAK=on` (`reminders.rs:212`). Tool declarations/routes
  independently require `LAMP_SCHEDULE_TOOLS=on`; CHECK stays owned by Tasks
  (`tools.rs:266`, `:345`, `agent_tasks.rs:103`). All reminder tool calls,
  including CHECK, encounter the veto before ToolDesk (`provider_worker.rs:1350`).
  Touch still selects its separate path only with `--touch`; Body's new
  `--look-at-name` remains false unless explicitly enabled.

Exact SHA-256 of the three merged files:

| File | SHA-256 |
|---|---|
| lampOS/Cargo.lock | c4470f61d935a22bb0eb7df3406d7911a4740abafcd581a5d34405dda102acd7 |
| lampOS/crates/voice/src/coordinator.rs | ce4a26e7ae552b2d308b89c70096d75a27365e43931fb6169fc8111591336791 |
| lampOS/docs/reuse-decisions.md | 2c4115f40604b02b1107efa0402f066a76d82a50fb2775376c08bf31462b10a6 |

Unchanged reviewed seam hashes:

- tools.rs: `b1becea1a7cd60fdcbda23a22616019568bdc9660bb6e933ac0cdb27bb2d32a0`
- provider_worker.rs: `69c06a80c5f1a902718bbc38ea321ef97f4994379afc799248261dc1e14b22ef`
- coordinator/tests/reminders.rs: `165af1c1a6c717c9c4b312d2c59478623595287cdb94e75f1bd407607dbcb34f`

Verification in this review: source/diff/hash comparisons and
`git diff --check` (exit 0). No Cargo, build, test, CI or lamp execution was
performed; no repository edit or git mutation was made. This review does
not make the checkpoint ready and does not attribute the prior Mac motor
wait failures. Exact-head native validation remains required.
