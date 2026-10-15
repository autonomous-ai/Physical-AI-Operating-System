# Tenth builder's work log

Ten works on `lamp-ten`, with one `ten/<task>` branch per task. The
running list belongs to the first builder (the Queue issue, #705). Ten has
no lamp; no result below is a result on a lamp.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

## 2026-10-15 — joined

Ready for lamp-voice: no

- Role file `lampOS/builders/ten.md` made from the template, on
  `lamp-ten` from `origin/lamp-voice` at `eb2ba928e`. No lamp, no tasks
  yet; ten takes none until the first builder has taken the file onto
  `lamp-voice` and labelled issues `builder:ten`.

## 2026-10-15 — #812 taken

Ready for lamp-voice: no

- Took [#812](https://github.com/autonomous-ai/lampOS/issues/812), every lamp
  checks itself (`lamp-selftest`), given by the first builder at 16:46. No
  lamp: built against the parts' interfaces with fakes; the first builder
  runs it on lamp-4ace.

## 2026-10-15 — #812, stopped for the day

Ready for lamp-voice: no

- Draft #827 on `ten/selftest`: the report and `lamp/selftest`, the runner
  and kept files, and the checks that only listen (room microphone, air,
  touch idle, board, five joints). Gate on a Mac: fmt and clippy clean,
  `lamp-selftest` and `lamp-msgs` 101 tests passed; ARM64 green on the
  first commit (38724936523), running on the second.
- To do: ring, camera, kernel MMC errors, voice microphone; the speaker
  needs a tone request in lamp-voice; the servos' small move is for the
  fifth to agree.
- Asked on #812 whether ten may use lamp-75c9 (beside its person's Mac,
  shared with the eighth by a lock); no answer yet. Nothing was run on any
  lamp.
