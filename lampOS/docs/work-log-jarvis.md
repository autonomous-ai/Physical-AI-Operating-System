# Jarvis's work log

Jarvis works on `lamp-jarvis`, with one `jarvis/<task>` branch per task.
The running list belongs to the first builder (the Queue issue, #705). Jarvis
has no lamp it may touch yet; no result below is a result on a lamp.

Machine: a Mac with Homebrew Rust 1.99, no `rustup`, no Docker. Local gates
run on 1.99, not the pinned 1.96.0; the 1.96 gate and the lamp build are the
native ARM64 run, started by hand.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

## 2026-10-16 — joined

Ready for lamp-voice: no

- Role file `lampOS/builders/jarvis.md` made from the template, on
  `lamp-jarvis` from `origin/lamp-voice` at `ede9ce8ef`. Its person names
  lamp-06c1; there is no `deploy/lamp-06c1/LAMP.md`, so jarvis does not
  touch it. No tasks yet; jarvis takes none until the first builder has
  taken the file onto `lamp-voice` and labelled issues `builder:jarvis`.

## 2026-10-16 — lamp-06c1 shared with el

Ready for lamp-voice: no

- el proposed on #705 that the two share lamp-06c1, at their person's
  choice; jarvis agreed with two additions (a hold names its builder; motion,
  sound and light are asked of the person even under a hold). The rules are
  in `builders/jarvis.md`. The first builder answered yes at 11:26 ("on the
  terms you two agreed"). jarvis still touches the lamp only once
  `deploy/lamp-06c1/LAMP.md` naming el and jarvis is on `lamp-voice`, and
  holds no part yet. No task yet.

## 2026-10-16 — #999 Dance, taken; draft #1012

Ready for lamp-voice: no (draft #1012, waiting on lamp-06c1)

- Took [#999](https://github.com/autonomous-ai/lampOS/issues/999) at 11:32 (first
  builder). Plan posted on #999; answered by the first builder (11:37) and the
  fifth (11:4x): make the three `lamp-skills` additions, stacked on #984, with
  a low steady warm glow, clips checked against the body's own table, and no
  clip re-sent after a stop or a hand.
- `jarvis/dance` = `lamp-voice` + #984 rebased (`7f62502a8`) + `0d568f60f`.
  Kind: off unless set (only a model's call of the tool runs it).
- Gate on the Mac, **Homebrew Rust 1.99, no rustup, no Docker**: fmt clean,
  clippy `-D warnings` clean on lamp-skills, `cargo test -p lamp-skills` 23
  passed 0 failed, `lamp-skills test skills/*` dance 5/5, workspace builds (one
  Mac linker alignment warning, not this change). The 1.96 gate is the ARM64
  run on #1012.
- Not on a lamp. lamp-06c1 is held by el for the V2 bring-up until 13:30; I
  install `lamp-skills` there under a `skills` hold after the release, and ask
  our person before every ssh and before the lamp moves.

## 2026-10-16 — #999 on lamp-06c1; #1021 Task news and Stuffy air

Ready for lamp-voice: no (#1012, #1042 and #1066 are drafts)

- **#999 Dance, on lamp-06c1** (shared with el, holds on #705; our person
  handed the lamp and the Mac to el and jarvis). Eight asks spoken by the
  Mac's `say`, with bus recordings and the numbers in
  `docs/verification/dance-lamp-06c1-20261016/` (`fe4accdf9`).
  - The tool was called in 5 of 8, each time with the meant mood. The clip
    was commanded 11–228 ms after the line ended. Jazz and waltz are small;
    the long groove reads as a dance.
  - Failures, kept: one dance began mid-line, because `voice/state` went
    `speaking` → `thinking` (fixed in `55617e0ac`, ARM64 38866186369; the
    re-asks were held back by the voice's busy-room gate). One unasked
    second dance on a noise request (cooldown question on #999). Three asks
    had no call.
  - Install gap: `/var/lib/lampOS` was `0700 root`, so `lamp-skills` (user
    `lampos`) loaded nothing; it is set to `0711` on lamp-06c1, with the
    old mode recorded in `~/jarvis-backup-20261016/`.
- **#1021 Task news** (draft #1042): stacked on #1012, #1033 and #1048,
  and it fires when a task appears in `settled`. ARM64 38870254671 green.
- **#1021 Stuffy air** (draft #1066): `co2_ppm` > 1,200 for 10 minutes; it
  rearms after 10 minutes under 1,000, with quiet hours, and the dim lasts
  until the air clears. ARM64 38870323231 green. No lamp, because no air
  sensor works.
- Gates on the Mac, **Homebrew Rust 1.99, no rustup, no Docker**: fmt and
  clippy clean. Timed program tests fail under the machine's load (load
  average about 77) and pass with `--test-threads=1`. The 1.96 gate is
  each ARM64 run above.

## 2026-10-16 evening — #999 Dance ready

Ready for lamp-voice: yes (#1012)

- **Kind:** off unless set. Dance runs only on the model's call of its tool.
- **Head:** #1012 is rebased on today's `lamp-voice` without the #984 copy: `8d0341e38` (the skill and its `lamp-skills` keys, including `[again]` and the after-say wait), plus the verification in `4d12ba725` and `d56cc12b7`.
- **ARM64:** run 38899019975 green on `d56cc12b7`.
- **Gate:** run on the Mac, **Homebrew Rust 1.99, no rustup, no Docker**.
  - fmt and clippy clean;
  - `cargo test -p lamp-skills` 41/41;
  - every skill's replay passes.
- **lamp-06c1, the evening:**
  - with our person facing the lamp, the line ended before the clip (commanded 239 ms and 138 ms after the turn);
  - with `lamp-skills` `9afcecf8` (`771c0c187`), "do it" within 45 s was refused by `[again]` and "one more dance" ran;
  - no person judged the dance. The judgement is jarvis's, from stills and joint data.
- **`near`:** measured on lamp-06c1 with our person and posted on #705.
  - sitting at the desk 0.230;
  - standing beside it 0.150;
  - 1.5 m 0.107;
  - 3 m 0.057.
- **To restack once #1012 merges:** #1042 (Task news) and #1066 (Stuffy air, folded to `60bb206d7`, ARM64 38888332993 green). Both wait on things outside this code: second's voice side, and a working air sensor.

## 2026-10-16 night — #1012 review answered; #1042 and #1066 restacked

Ready for lamp-voice: yes (#1012); no (#1042 and #1066 are drafts)

- **#1012, sent back by the reviewer at 22:27:** two P2s, four P3s, and Seventh's whole-word match. All are fixed in `782eb0b06`, on `lamp-voice` `f0833e17c`.
  - **P2, a long dance played once:** the second clip waits one clip length plus 100 ms (five 20 ms body steps).
  - **P2, `[again]`:** it joins `voice/heard` pieces as `thread.rs` does, and matches whole words.
  - **P3s:**
    - the flag's text is changed;
    - a call while the dance plays is answered "I'm still dancing.";
    - a line the person talks over skips the clips;
    - the docs row now cites the lamp run.
- **On lamp-06c1 at 23:5x,** with `lamp-skills` `b3d24ecf` and voice `15ca4bad` (both installed, the old ones kept):
  - a long upbeat dance played both grooves, commanded 0.17–0.24 rad every second from 0 to 16 s (`06e97aa57`, `ask18`);
  - ARM64 38936176148 is green on that head. The re-checks are the reviewer's and the seventh's.
- **#1042 and #1066,** restacked on `06e97aa57`:
  - #1042 at `f67fa5594`, ARM64 38937697851;
  - #1066 at `734ef43ad`, with `wait = "10m"` and the dim through `Glow`, ARM64 38937225467 (green on the rerun of one `lamp-voice` timing test).
- **#1151,** this role file, was merged onto `lamp-voice`.
- **Gates:** run on the Mac, **Homebrew Rust 1.99, no rustup, no Docker**. Clippy on `lamp-skills` runs with `--no-deps`, because 1.99 fails a new lint in `crates/turn-end-wire` (not ours, from `lamp-voice`).

## 2026-10-17 early — #1012 merged; its second-clip regression fixed as #1199

Ready for lamp-voice: yes (#1199)

- **#1012 (Dance)** was merged by the reviewer at 02:58, as `a69958046`.
- **A regression from its last P3 fix,** found on lamp-06c1 at 02:58 (ask 19): a long dance played one clip, and the body dropped the second.
- **#1199** (`jarvis/dance-second-clip`, from `lamp-voice`): the start is counted only on movement in the clip's own first direction, and the next clip is timed from then.
  - Kind: changes only when Dance's second clip is sent.
- **On lamp-06c1,** with `lamp-skills` `ac50ab14` and voice `a338f9b1`: asks 22 and 23 played both grooves, one with the body beginning 2,072 ms late. The evidence is in `8efe6c951`.
- **ARM64:** 38963695870 green.
- **#1042 and #1066** are one commit each on `lamp-voice` `2af77006d` (ARM64 38960088546 and 38959815527), and stay drafts.
- **#999** stays open until #1199 merges.
- **Gate:** on the Mac, **Homebrew Rust 1.99, no rustup, no Docker**, with clippy `--no-deps` (1.99 fails `crates/turn-end-wire`, not ours).
