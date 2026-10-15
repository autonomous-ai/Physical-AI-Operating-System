# Seventh builder's work log

Seventh works on `lamp-seventh`, with one `seventh/<task>` branch per task.
The running list belongs to the first builder (the Queue issue, #705).
Seventh's lamp is to be lamp-52e6 once its owner file names seventh; every
result on a lamp below names it. Seventh never touches lamp-4ace.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

- The ARM64 workflow (`.github/workflows/lampos-arm64.yml`) runs on push
  only for `lamp-*` and `second/**` to `fifth/**`, not `sixth/**` or
  `seventh/**`. Seventh starts it by hand (`gh workflow run lampos-arm64.yml
  --ref seventh/<task>`).

## 2026-10-15 — who-gets-the-work (#733) — taken

Ready for lamp-voice: no

- Task: #733, the person names who gets the work. Given to seventh by the
  first builder, through seventh's person; the second built it and left it
  at `ec2fb2a4d`, green on ARM64 (run 38666494649), without its pull
  request.
- Base: `origin/lamp-voice` at `6569a414c`.
- Branch: `seventh/who-gets-the-work`, the second's one commit rebased on
  that base without conflict (`931b00ebe`). Gate and ARM64 run
  38673112146 under way.
- Questions: on #733, the label is still `builder:second`.

## 2026-10-15 — who-gets-the-work (#733) — pull request

Ready for lamp-voice: yes (#769)

- Kind: off unless set (`LAMP_TASKS_WHO=on`, `--tasks-who`). The second's
  commit, rebased twice on `lamp-voice` (now `465e68fd0` on `5798f73d3`);
  the second rebase kept the first builder's two new tool wordings and
  carried them into the `who` declaration.
- ARM64: run 38673112146 on `931b00ebe` green on its second attempt (the
  first failed only in `lamp-policy`'s program test, a known flake); run
  38679700066 on `465e68fd0` green.
- Gate on a Mac at load average 100–134: fmt, clippy clean; `lamp-tasks`
  107, `lamp-voice-eval` 284 passed; `lamp-voice` lib 928 passed, 33 timed
  tests failed (#739); `agent_tasks::` alone 14/14 five times.
- Not shown by seventh: any lamp run. The first builder's minute on
  lamp-4ace began 11:36; rebase after #751 merges (one test hunk in
  `crates/tasks/src/protocol.rs`).

## 2026-10-15 — memory-per-person (#764) — pull request

Ready for lamp-voice: yes (#771)

- Kind: off unless set: nothing changes without a memory, an eyes socket
  and a known person at the lamp; `with_nobody_known_every_line_to_the_memory_is_as_before`
  pins the bytes.
- Commit `19e94334c`; ARM64 run 38679434978 green.
- Gate: `lamp-memory` 149 passed (two new host end-to-end tests against the
  real store, both memory modes); `lamp-voice-eval` 284; `lamp-voice` lib
  930 passed, 26 failed: 23 timed (#739), 3 `provider_worker::tests::reminders`
  that pass with `--test-threads=1` (also failed on #733's gate, unchanged
  code).
- Not shown: any lamp run; the first builder plays it on lamp-4ace with two
  people.
