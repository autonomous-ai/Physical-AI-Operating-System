# Thirteen's work log

Thirteen works on `lamp-thirteen`, with one `thirteen/<task>` branch per
task. The running list belongs to the first builder (the Queue issue, #705).
Thirteen has no lamp, so no result below is a result on a lamp.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

- 2026-10-16, `provider_worker::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity`
  fails alone at `provider_worker.rs:5958` on `lamp-voice`'s `b7c309809`, on
  an Intel Mac (said on #705 for the tests builder). Not mended.
- 2026-10-16, `early_end::an_answer_that_arrives_after_the_lamp_is_sure_begins_400_ms_sooner_than_without`
  failed once in a full `lamp-voice` run on a loaded host; 3/3 alone. Not
  mended.

## 2026-10-16 — joined

Made its role file from `builders/TEMPLATE.md`, as joining.md's "Starting a
session" says: name `thirteen` (chosen by its person; no other builder,
branch or label used it), no lamp, no crates. Commented on #705 and takes no
task until the first builder has taken the role file onto `lamp-voice` and
labelled issues `builder:thirteen`.

## 2026-10-16 — the review queue, then #1071

The first builder's first task (14:38 on #705) was the review queue while the
reviewer was away; at 15:08 the reviewer came back and split it. Reviewed,
each with the head read on the pull request: #1059, #1043, #1046, #980,
#834 (and re-check), #1050, #1055, #1041 (and re-checks), #1052 (and
re-check), #1016, #1018 (and re-check), #1068, #1079, #1076 (the first
builder's quick fix for #1071), and the re-checks of #1033 and #1038.
Findings that sent a pull request back: #1050 (a rotated device key never
reached the lamp), #1052 (an older pinned model kept for good), #834 (a
`run` from a terminal held the lock with no end). Each is fixed and
re-checked.

Role file: #1073.

#1071 (`thirteen/records-worker-list`): the voice's commands, worker roles
and helpers are named once, in a new crate `lamp-voice-processes`. The voice
starts only listed roles; the records guard knows exactly the list; a refusal
names an unknown role, and after two refused cleanups in a row the records
diagnostic says so. Stacked on #1076. docs/records.md, "The voice's own
list".
