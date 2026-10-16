# The second builder's log

The second builder's own record, on `lamp-second`, which only it writes
to. The tasks and their briefs are in `work-split.md` on `lamp-voice`.
Dates are the lamp's (Asia/Ho_Chi_Minh). The second builder works on an
Apple M2 Max laptop (macOS) and a Linux ARM64 virtual machine on that
laptop (Colima, 6 virtual cores, Debian bookworm, Rust 1.96.0). Neither is
the lamp, and nothing here was run on a lamp.

Since the evening of 2026-10-14 the tasks are GitHub issues in the order
of the Queue issue (#705), a finished task is one pull request from its
`second/<task>` branch into `lamp-voice`, and `lamp-second` carries this
log and nothing a lamp runs ([joining.md](joining.md),
[builders/second.md](../builders/second.md)).

## Taken

| # | Task | Branch | Taken | State |
|---|---|---|---|---|
| 1 | The end-of-turn model in a tenth of a second | `second/turn-end-fast` | 2026-10-14 | on `lamp-voice`; on the lamp 148 ms against 382; the voice does not use it yet |
| 2 | Try to break last night's changes to the voice | `second/voice-review` | 2026-10-14 | on its own branch, 2026-10-14 (`1d737f8e5`); taken into `lamp-voice` with its mends |
| 3 | A listener for "stop" under the lamp's own voice | `second/stop-word` | 2026-10-14 | on `lamp-voice`: hears the word alone; under the lamp's voice only through the canceller, partly; a first look on lamp-4ace heard no played stop; not wired |
| 5 | The name, heard as a bare "Lamp" (issue #694) | `second/bare-name`, then `second/bare-name-2` | 2026-10-14 | #725's model on `lamp-voice`; on lamp-4ace at 23:56 it heard 1 of 6 played calls against the installed model's 3 to 4: not installed; the second (#750) checked on lamp-4ace at 08:55: 1 of 6 at 0.9 against the installed 2 of 6, bare "Lamp" lifted but "Sam" and "Lan" as much: not installed, #750 closed; the 14 cut `say` lines asked for as a held-out lamp test; after the MVP |
| 9 | Faster answers: what exists | `second/faster-answers` | 2026-10-14 | on `lamp-voice` (a study, done beside 5 and 10) |
| 10 | A reference that follows the microphone's clock | `second/reference-clock` | 2026-10-14 | on `lamp-voice`, behind `--echo-reference steady`; on the lamp's recordings, mixed (two up, two down, one held) |
| 20 | The replay takes the lamp's own recordings | `second/replay-capture-clocked` | 2026-10-14 | on `lamp-voice`: five of the lamp's recordings followed sample for sample |
| 19 | The end sent early, the answer played only when sure | `second/early-end` | 2026-10-14 | on `lamp-voice`, behind `--early-end-ms`; played on lamp-4ace at 200 ms: worse; switched off |
| 36 | Always alive: the idle layer, sleep and waking (issue #714) | `second/always-alive` | 2026-10-14 | on `lamp-voice` (#729, merged 23:03); on lamp-4ace since 23:07 |
| second-P1 | The early end taken back (issue #723) | `second/early-end-back`, then `second/early-end-labelled` | 2026-10-14 | step 1 on `lamp-voice` (#731, merged 23:06); on lamp-4ace at 23:10 (a) did not hold and (b) did; step 2 as pull request #738 into `lamp-voice`, 2026-10-15 |
| 45 | Updates: a lamp keeps its lampOS current by itself (issue #737, MVP) | `second/updates` | 2026-10-15 | on `lamp-voice` (#741, `78b984b69`); lamp-4ace updated itself to build 188 from a signed test channel at 03:48; the publishing proposal agreed on #737; the CI `publish:` job not added by this session (it handles the signing secret and makes releases; this session's safety check refused the edit); left to the first builder or the owner; #1124 (2026-10-16): `lamp-update`'s lookup was counted out only after its answer was handed over, which turned #1003's and #1092's ARM64 runs red once each; now counted out first (`lamp-update` 77 passed, ARM64 green, ready) |
| 44 | A yes said in two breaths still starts the task (issue #736, MVP) | `second/two-breath-yes` | 2026-10-15 | #740 closed as not needed, 2026-10-15: `d128c458a` (the model told why a call was not done) started five of five on lamp-4ace; the branch kept at `0ebd994d5`; the news crash found in its minute fixed in #756 |
| 40 | Progress news through the tasks part and Jev (issue #732, `second-P2`) | `second/progress-news`, then `second/progress-reads` | 2026-10-15 | #751 merged by the reviewer at 17:47 (`ecca16554`), after four rounds of review (stale answers, the quiet from `resting`, a run identity across restarts, a new run's first count and the cooldown across the tasks part going); the first builder's asks as #793 (a value at start, a read at once, `unread` for a task not read yet, a journal line a read) merged by the first builder at 19:42 (ARM64 green, 4,501 passed); #732 open for the follow-up |
| 41 | Codex and the others: the person names who gets the work (issue #733, `second-P3`) | `second/who-gets-the-work` | 2026-10-15 | moved to the fourth builder at 07:07; the branch at `ec2fb2a4d` on `2176cd38a`, gated, with the tests the schedule tools needed; its state said on #733 and the claim released |
| 48 | The Policy decides when the lamp speaks first: progress news, then hello (issue #752) | `second/policy-speaks`, then `second/hello-asks` | 2026-10-15 | #765 merged by the reviewer at 18:14 (`4ad0ec5c3`), #752 closed; hello asking the policy rebuilt as one commit on `lamp-voice` `a583fda28` (`0aac359c1`, carrying the keeper's pause), gated, and proposed as #856 (`second-P5`) once #757 closed |
| 52 | Task news, polish: a follow-up question is not told the news again, and a long result gets a gist (issue #757) | `second/news-polish` | 2026-10-15 | #794 merged by the first builder at 20:41 (ARM64 green, 4,587 passed); #757 closed; the first builder listens on lamp-4ace tonight |
| 59 | A turn with a function call speaks 1.8 to 2.9 s after the person stops (issue #799) | `second/tool-turn-speed` | 2026-10-15 | pull request #810: `provider-check --tool-turns`, `turn-times`, call times in the first-output record, and the lead-in behind `LAMP_TOOL_LEAD_IN=on`; measured by the first builder with `turn-times` (plain 1.38 s, with a call 2.25 s, the second generation about 1 s); the reviewer's four findings fixed (`6cc4f2c09`); its fifth (audio between the call and our answer taken for the answer's delay) fixed (`1d06e35a0`); merged by the first builder at 19:42, ARM64 green (4,572 passed); #799 open for the lead-in's lamp check; the first builder's lamp check of 16 Oct 00:12: with the lead-in on, nothing called; found at 11:5x: `start_agent_task` was 1,036 bytes with two of the staff and the link takes 1,024, and `thread::equipped` then gave the session no function at all, silently; draft pull request #1020 (`second/lead-in`, `ac461cfe8`): the lead-in is the read-back as a statement then the call (1,005 bytes at most), a function the link would refuse is left out and named in the journal (the first builder's 11:57), every combination tested; `lamp-voice` 1,519 passed, 11 fixture spawn timeouts of the #887 family that passed alone; the first builder asked at 11:57 that a refused function be left out alone and named, done; the executive's refused `tools.json` said in the journal with why (`3c4c19c8a`, `lamp-voice` 1,530 passed); ARM64 green (5,432), ready at 13:0x; #1020 merged in the afternoon; Sixth's P3 there (a `tools.json` of 33 or more valid functions refused whole, where before the first that fit were told) as `second/catalog-room`: only what the link refuses refuses the file; #1085 (`3d1138e95`, `lamp-voice` 1,595 passed, ARM64 green 5,613, ready) |
| 60 | A short room sound heard as "¿Qué?" gets an answer (issue #800) | `second/short-sound` | 2026-10-15 | pull request #818: `short-turns` counts them from the records (kinds, no words), and `LAMP_SHORT_SOUNDS=quiet-room-checked` holds a let-through sound's answer unless it was a short word, said close, or someone faced the lamp; the count on today's lamp records asked for on #800; ARM64 green, marked ready; rebased twice (onto #778's second voice, whose worker now also takes `--sounds-checked`); the reviewer's 20:11 finding fixed: the second voice holds a short sound not heard as a short word (`3076f8452`, `lamp-voice` 1,326 passed); the lamp minute is the first builder's |
| 61 | Asked to have Claude Code do something, the lamp sometimes does it itself (issue #854) | `second/hand-over` | 2026-10-15 | draft pull request #859 (`9169de55e`), off unless `LAMP_TASKS_ASKED=on`: `start_agent_task` called whenever the staff are asked for anything, a story included, even mid-story; the session told so after the persona (both routes); `who` may be "not sure", which asks "Claude Code or Codex?"; `provider-check --hand-overs` counts it per scene; `lamp-voice` 1,334 passed; ARM64 green (4,595 passed). Played on lamp-4ace at 21:22–21:33: scenes 1–4, 6 and 7 as meant, and the switch kept on there from 21:30; the unclear name was guessed and asked about twice, and "not sure" never taken, so `afe87d6d9` names the guess in the read-back, asked once; the check's `asked_again` measured a yes and is now `after_own_story`; played again at 21:58: one question naming who, nothing started without a yes; #859 merged by the reviewer at 22:44 (ARM64 green, 4,686 passed), #854 closed; lamp-4ace keeps `LAMP_TASKS_ASKED=on`; reopened 2026-10-16 02:08: some sessions offered every story asked of the lamp to Claude Code (my wording had no limit on who was asked): pull request #903 (`second/story-told`), the words both ways and `start_agent_task` taking `named` and `on_the_computer` (nothing prepared with neither); its first head told the stories 4 of 4 on lamp-4ace at 02:43 but lost both named poems ("I can't write code …"), so the hand-over is said first and in full again and the exception after it (`b1f59b2b9`, `lamp-voice` 1,371 passed, one timed test passed alone 3 of 3); its second head (two required arguments) lost them too, 0 of 3, twice claiming a hand-over with no call (03:10); the third, `3c6775bb7`, leaves `start_agent_task` as it was and changes only the session note: `handovers.sh` on lamp-4ace at 03:37, told 4 of 4 and Claude Code named 2 of 2 with the read-back; ARM64 green, ready, on lamp-4ace; reruns at 08:00 and 11:00; #903 merged by the reviewer at 05:15 (head `7eb98c818`), #854 open for the 08:00 and 11:00 reruns |
| 62 | Jev 1.14: replay the Policy's logged decisions through 1.13 and 1.14, and pictures with the questions (issue #871) | `second/jev-replay` | 2026-10-15 | draft #873 (`cbbd8c58f`, `lamp-policy` 179 passed): `lamp-policy-replay` asks each logged attention question again of each model (no words out) and counts against labels; running on lamp-4ace since 23:18 on 3,054 decisions, 193 labelled, summary at 08:00; `lamp-policy --images` asks again with the eyes' frame (the voice's rule, 640×480, TypeSafe's `assets`) and five picture questions, in shadow, counted by `--pictures`; whether OpenRouter passes pictures on is the lamp's to show; the reviewer's four findings fixed in two rounds (`9e83243fb`: nothing written once the eyes' frame is gone, each label given once to its nearest line; `4f6c4fe5b`: the picture taken at the moment its request is written, contradicting labels at one moment refused), `lamp-policy` 183 passed, ARM64 green (4,770 passed); ready; the reviewer's 01:13 P3 (a picture written whole on a connection closed unanswered, then a try withdrawn, recorded as not sent) fixed in `679535244`: each try's delivery told, `sent` if any went, both askings counted on the same lines; `a6bb08751` adds `--again --refit` (each model's weights fitted again to its own answers on half the labels, counted on the other half, 200 times), for the switch; `lamp-policy` 192 passed; a refit that cannot be made said so (`45b3ca7dd`); rebased for #881's relay fix (`49c493727`); the picture asked first whether anyone is in it, for the empty desk's near-chance answers on lamp-4ace (`6702173f2`); `--cost-ratio N` (`c4785b870`, `lamp-policy` 195 passed) for the first builder's choice between 1.13 and 1.14 (stays 1.13 since 01:30); with #883 as `second/pictures-hello` (`804651c6c`), on lamp-4ace since 02:27, OpenRouter passing the picture; the reviewer's 04:14 finding (a picture asked from a queue after the face had gone) fixed with one shared `Presence` checked at dispatch and right before the frame (`e20350516`, `lamp-policy` 196 passed, ARM64 green, 4,968 passed); no finding left at 05:09, waiting on the first builder's camera check and #871's scenes with the owner; rebased over #943 (`bc8a03d77`, `lamp-policy` 200 passed); ARM64 green on `bc8a03d77` (5,313 passed); rebased over #937's `--rules` at 11:3x (`86cbdb078`, `--rules` beside `--images`, `lamp-policy` 205 passed); #883 merged minutes later, so rebased again: `policy::Beside` carries progress, hello and pictures beside the attention question, and `lamp-policy` takes `--hello` beside `--images`; the reviewer's fresh gate at 12:25: two real-time bus tests in `tests/policy.rs` failed on their host; measured here: 40 of 40 alone at head and on `lamp-voice`, idle and under load, and the whole file 95/95 at head against 94/95 on `lamp-voice` (a third timing test): the file's own flakiness, offered as an issue; rebased onto #844's `--zone` at 14:2x (`84484c64c`, `lamp-policy` 221 passed); #1060 filed for the Policy's bus tests and fixed by #1065 (`second/policy-bus-tests`: a 20 s wait, a diagnostic and a pause; with 0.4 s stalls every second `lamp-voice` failed 7 of 20 and the branch passed 40 of 40; ARM64 green, 5,526 passed; ready at 15:2x); rebased onto #958 at 15:3x (`0e45ea022`, `--gate` beside `--images`, `lamp-policy` 227 passed); #873 merged at 15:5x; #1065 merged at 16:08, #1060 closed; #1040: the shadow judgement proposed from the first builder's 79 rows (in conversation `3 × max(0, follows_lamp − 0.55)`: 13 of 33 said to the lamp leave `stay_quiet`, 0 of 46 not; a fit with each row left out gave +3.02 at 0.55) and built as draft #1106 (`lamp-policy --attention-next`, `next` on each line, `attention_next_differs`, nothing published; `lamp-policy` 231 passed); #1106 merged at 18:2x; the reviewer's two P3s in #1142 (the fit is a ridge fit, penalty 0.5, and a plain fit gives +5.02 at 0.51 as the reviewer found; the switch-off run compared directly; `lamp-policy` 232 passed); the fit script on #1040; #1142 merged at 20:5x |
| 63 | The Policy holds the lamp's actions (issue #800): the "Thailand" `volume_up` of 15 Oct 14:49 on a `stay_quiet` | `second/hold-actions`, `second/act-attention` | 2026-10-16 | the voice behind `LAMP_POLICY_HOLDS_ACTIONS=on` (`23c26d9bf`): a volume or light call waits up to 2 s for the turn's attention decision made for real, and on `stay_quiet` without the name is not run and the turn's sound is not let out; the Policy's `--act attention` (`f8a17a0ad`, on #873 and #883 as `second/act-attention`, its pull request once they merge); pull request #908 ready (ARM64 green, 4,895 passed); played on lamp-4ace at 03:28: "someone should turn it up a little" in a busy room, `stay_quiet`, not done, 558 ms, nothing said, the volume kept at 75%; the reviewer's 03:45 and 03:48 (pairing, cancellation) fixed in `1502a26c6`, then at 05:09 and 05:11 two more: a late decision still taken for another turn, and a cancellation stuck behind full output. Fixed in `6c1dc01fa`: `voice/heard` carries the turn whose input began last, the Policy's `Decision.request` already carries it on, and decisions are paired by that number; the session notes a withdrawn call as it reads it (`call_withdrawn`), asked right before a kept call runs. The reviewer's 05:52 P3s and the first builder's 05:57 in `1ab26f986`: only `set_timer`, `set_reminder` and `cancel_reminder` of the schedule held, and the cancelled-turn test settled after the real stop with the worker healthy; `lamp-voice` 1,406 passed; pushed at 06:1x for `holds.sh` on lamp-4ace; the reviewer's 06:40 P2 (one Policy utterance across two turns, labelled the later) fixed in `27d969e05`: a piece of another turn ends the open utterance; `holds.sh` on lamp-4ace at 06:51 on `1ab26f986` (the two people's lines and the overheard "brighter" not done, named actions run in 0 to 2 ms); ARM64 green on `27d969e05` (5,058 passed); `second/lamp-holds` (`cba140ea7`) is lamp-4ace's build of it with `--act attention` and #883; for the first builder to merge; #908 merged by the first builder at 07:41; the Policy's side as pull request #943 (`second/attention-acts`, on today's `lamp-voice`, `lamp-policy` 166 passed, ARM64 green, 5,201 passed), the reviewer's only finding a doc line (a named turn's call runs at once), fixed in `c73293fd7`; #943 merged by the reviewer at 09:04 as `fca65fde5` |
| 64 | A person's kept rules about speaking first (#813, with the seventh) | `second/policy-rules` | 2026-10-16 | the interface agreed with the seventh at 02:41 (`memory/rules` gains `hello` and `news`); `lamp-policy --rules` for `quiet` and `quiet_mornings` built (`31fd05ae1`, `lamp-policy` 206 passed), held until #873 and #883 merge; the windows and "while talking" when the seventh's fields land; rebuilt on `second/pictures-hello` with the seventh's #904 limits (`5fdb9e1e0`: per kind `never`, `not_before_minute`, `not_from_minute`, `not_while_talking`, the last held while the room is not known quiet; `lamp-policy` 214 passed); its progress half on today's `lamp-voice` as pull request #937 (`second/rules-news`, `1e7f9e19a`, `lamp-policy` 167 passed) so that #813 need not wait for #873 and #883; the hello's half follows #883; #937's room checked when the answer comes, not when it was asked (the reviewer's 08:06), fixed in `b5185b121` (`lamp-policy` 168 passed, ARM64 green, 5,205 passed); the seventh closed #813's own side at 08:15; rebased over #943 (`cdc6ee1ba`, `lamp-policy` 171 passed, ARM64 green, 5,256 passed); no finding left at 08:50, waiting on the spoken keep/forget check; #937 merged in the 10:33 round; the hello's half (`second/hello-rules`, on #958): with `--rules` the hello question holds on `hello` limits, the moment question `news` on `news` and anything else on both, guard `kept_rule`, no model asked; `--rules` needs `--progress` or `--hello`; draft pull request #1003 on #958 (`994c7b932`, `lamp-policy` 191 passed); draft #1003 (`2775d4994`, rebased on #958's fix, `lamp-policy` 194 passed); #1040 filed for the attention weights (`follows_lamp` −0.136 though it tells the lamp's lines apart, median 0.60 against 0.29 on the first builder's 79); the guard #1054: a `stay_quiet` that follows the lamp at 0.6 or more keeps its timers and reminders (17 of 33 against 6 of 46), `lamp-voice` 1,574 passed; #958 merged, so #1003 retargeted to `lamp-voice` (one commit); #1003 rebased over #873 (`13e2cbf13`, `lamp-policy` 231 passed, ARM64 green 5,617, ready); peer review of Fourth's #1090 at the first builder's 16:11: two P2s (a capitalised opener before a comma, "Tomorrow, remind me…", now holds the line itself, shown by a probe on the head and the merge base; one attention test fails 3 of 3), and #1054 needs one more voice-side rule after it (a line after someone else's unanswered question in a quiet room); #1082 merged at 16:16, so #1003 rebased over it (`cb6dd64ff`: the kept rules' `hold()` and #1082's `busy()` both kept, the rule applied whether or not an answer came, both sides' tests kept whole; `lamp-policy` 233 passed; ARM64 green on the re-run); the reviewer's 18:22 on #1003: the P2 (a kept rule held a reminder) and three P3s fixed in `cedbffb1b` (rebased over #1099 and #1106; `lamp-policy` 238 passed); `never` across kinds left for the owner; peer re-check of Fourth's #1090 at `825f197ee`: no finding (`lamp-voice` 1,607, one real-clock test of mine flaky 1 of 3 alone, reported on #823); #1054's follow-up (a turn after someone's unanswered line is held as any `stay_quiet`) built on #1090 and gated (`lamp-voice` 1,612 passed), pushed when #1090 merges; rebased after #1090 merged (`2594772ee`, ARM64 5,710), ready; Fourth's peer review (P2: a cancel on a reply to the lamp is silent and cannot be undone; two P3s) fixed in `e3c566f7e`: only `set_timer` and `set_reminder` run on a following `stay_quiet` (`policy_holds::SETS`), a short sound is nobody's line, the doc slips fixed; the four changed tests fail on the old rules; `lamp-voice` 1,626 passed, ARM64 green (5,750); the reviewer's P2: the 11:20 timer scored `follows_lamp` 0.35 and stays held at 0.6, now said plainly; and a reply to the lamp only in conversation (the lamp's sound within 20 s, the Policy's own window), in `acce50bcb`, rebased onto `36b872d1c` (one test-helper conflict) as `5a253519e`, `lamp-voice` 1,692 passed; Fourth's re-check clean; #1003 merged at 23:02, the reviewer's two P3s on it folded into #1143, rebased onto `lamp-voice` (`bd655b589`, `lamp-policy` 239 passed), retargeted |
| 65 | The progress question counts a slow answer as unasked, and a turn that speaks twice twice (from #751, found on #883) | `second/progress-turns` | 2026-10-16 | the same rule as the hello's: a turn's kind known at its start, counted once at its first sound (`f120bda7f`, on `lamp-voice` `28d581215`); pull request #932, ARM64 green (5,002 passed), merged by the reviewer at 07:10 as `e5aa43747` |
| 66 | Physical skills: the say service, the Policy as the gate, Reminders and Task news as skills (issue #939) | `second/say`, `second/gate` | 2026-10-16 | the say service as pull request #954 (`LAMP_SAY=on`: `{skill, note | text, priority, expires_in_ms}` on `/run/lampos-say/say.sock`, group `LAMP_SAY_GROUP` (`orangepi` on lamp-4ace), answered `begun` then `spoken`, `dropped`, `expired` or `refused`; a reminder's gates and its proof of a whole turn; `lamp-voice` 1,450 passed, ARM64 green, 5,209 passed); the Policy's gate on the bus as pull request #958 (`--gate`, `skills/ask` answered by a `Decision` naming the ask; `hello` through #883's question, `news` and `unprompted` a rule `wait` until their `moment` question; stacked on #883, `lamp-policy` 176 passed); Reminders and Task news as skills wait on the seventh's answer on #939 (their words come with the trigger, which `do.say` cannot carry yet); #954's reviewer findings fixed: one deadline for the whole line (`719b1850f`), and the expiry counted from the line and looked at again as it begins, the line at most 4,096 bytes with its newline (`cebb2eba4`, `lamp-voice` 1,454 passed); #958 rebased with #883 (`ff38b8a7d`), #959 a draft only for its ARM64 run; #954 accepted on lamp-4ace by the first builder at 10:17 (`begun` 1.09 s, `spoken` 4.64 s as `orangepi`) and merged in the 10:33 round, as was #937; that round left `lamp-voice` unbuildable (#954's `Reply.say` and #926's `cut_in` against #830's offline turns), mended by pull request #976 (`second/fix-voice-build`, merged at 10:4x as `f5a20ba28`; voice 1,510 passed, one timed test passed alone 3 of 3); next the `moment` question for `news` and `unprompted` asks, owned by the hello question (`second/moment`); the moment question as #958's second commit (`a80e52ccb`: never while anyone talks to the lamp, then `good_moment` on its own connection, a rule after 1.5 s, let go after 2 s as `late`; `lamp-policy` 187 passed); #883 merged at 11:25, so #958 is rebased onto `lamp-voice` (`c02555c7f`, no conflict) and retargeted there, and #959 closed; ARM64 green on `a80e52ccb` (5,390 passed); #958 ready at 12:0x (ARM64 green, 5,394 passed); for Reminders and Task news, (c) proposed at 12:16 instead of (a): the skill triggers and moves, its `say = { from = "schedule" | "tasks" }` asks the voice, which keeps the words and the durable delivery and says a reminder itself if no skill asks within 10 s; the reviewer's two P2s of 12:19 on #958 (a side queue that could lose an ask's name; the 2 s checked only before asking) fixed in `2c890f074` (each ask carries its name and receipt; the model gets what is left; a later answer is let go as late; `lamp-policy` 190 passed; ARM64 green, 5,436); (c) agreed at 12:32; Task news moved to Jarvis (#1021); `say = { from }` in `crates/skills` as #1033 (`lamp-skills` 23 passed); the voice's side as #1038 (`from` in the say service, `LAMP_SCHEDULE_SPEAK=skill` and `LAMP_TASK_NEWS=skill` waiting up to 10 s for the skill; `lamp-voice` 1,557 passed); the Reminders folder as #1039 on #1033 (three recorded cases); #1033 and #1038 ready (ARM64 5,442 and 5,465); Jarvis's Task news needed the end of a task on `tasks/status`, which the watch let go before `ps` showed it idle: `settled` kept two minutes, #1048 (ARM64 5,464, ready); #958 rebased onto #844's `--zone` (`692702dc1`, the moment question on the same `LocalTime`, `lamp-policy` 193 passed), #1003 with it (`e15b0dc36`); peer review of Twelve's #1009 at the first builder's 14:05: no finding; #1038's three P2s from Fourth fixed (`d780d1443`: a `from` ask taken with the switch off and living 20 s; while a reminder is held for its skill the say service, the hello and the news wait; `lamp-voice` 1,600 passed; ARM64 green, 5,544 passed); #1033's P2 and P3s fixed (`8f9b1fe2c`: `say.from` in a skill with a tool refused, `say` read by hand with plain errors; `lamp-skills` 31 passed); #1039 rebased onto it (`7bff9c23d`); #958 merged; its two P3s (an asker gone while someone talks; `busy` without the moment question) as `second/moment-held`; #1048: Fourth's P1 closed by Jarvis's #1042 (`appears` on new items), the P2 runtime test and the P3 (only the watch's own latest read counts) in `4c8058112`; peer re-check of Sixth's #1049: no finding; #1033, #1038 merged at 15:57; #1082 ready (ARM64 5,608); #1039 retargeted and ready (ARM64 5,613); Thirteen's two P3s on #1038 answered in #1087 (docs only, ready): an ask names its source, not an item, and the loop's guards are pinned by no test, as no harness drives `run_inner` (a day's work, not taken); #1099: a skill's reminder asks the gate as `reminder`, held only while someone talks (`lamp-msgs`, `lamp-skills`, `lamp-policy` together; 105, 31 and 228 passed); #1087 merged; #1099 rebased over #1082 (`84da46b2b`, 105/230/32 passed, ARM64 green, ready); the reviewer's P2 on #1039 is #1099's fix, so #1039 merges after it; #1099 merged at 18:1x, its three P3s in #1143 (stacked on #1003: a waiting reminder decided at once even behind a question out; `lamp-policy` 239 passed); #1039 rebased onto it (`7fc1f1d77`, `lamp-skills` 33 passed); #1124 closed as the duplicate of #1118; #1048 merged at 20:56 (Fourth's re-check clean; `get_progress` answers a settled task's end, Ten and Nine's problem 3 (b), shown on lamp-75c9 by Ten; a settled task timed from its first-kept answer so #1145's follow looks never tell it twice, `d119c2ee3`); its doc P3 as #1150; peer reviews: the sixth's #1086 (three P3s, fixed, re-check clean, merged), Thirteen's #1134 (two P3s), the sixth's #1139 (two P3s: a call split across pieces leaves its tail, shown by a probe); the seventh's #1146, the voice part (two P3s: any tool's `"look": true` opens the camera, not only a skill's; the backup voice never takes the look); Fourth's #1107 (two P3s: an idle Stop opens the 4 s too; a real short word in them is lost in every mode, unsaid in the docs) and #1108 (one P3: three comments over the wrong code), named by the first builder at 21:25, each run merged with `lamp-voice` `f1e13fbe5` (#1108's P3 fixed in `13709c362`, re-check clean); the eighth's #1149 (two P3s: its ignore reason predates #1145's confirming look, so a faster look has 10 s, not 20: it passes with the watch at 5 s and fails at 15 s; only a faster look can pass it) and the sixth's #1154 (one P3: no test that a refused picture's retry stays inside the 5 s; the old 3 s bound fails its new test); #1054 ARM64 green (5,710) and ready |
| 67 | Short sounds with no words cut off answers still to come (#1123, the sixth's 16:5x, the first builder's 17:05) | `second/wait-keeps-answer` | 2026-10-16 | the decision is the coordinator's, before `commit_admitted_input`, for both voices; no words of a new sound can be had before it takes the turn, so `LAMP_WAIT_KEEPS_ANSWER=on` works on length and pauses: while an answer is awaited unheard, a new sound waits (pending, its audio kept), and 1 s after its trigger, or 100 ms before its own deadline, it is let go if quiet 300 ms, else takes the turn from its start; an ended one is let go at once (the admission would otherwise end the service on the next sound); three tests on the real admission, detector and mask with the duplex hold; the word-free count script posted on #1123 for lamp-4ace; #1130 ready (ARM64 green, 5,624); the reviewer (one P1, three P2s) and the first builder (three P2s): on the product's mask a sound that waited 1 s plus its pre-roll was rejected as evicted, and the tests' mask was switched off; rebased onto `f40a6907b` and fixed in `92d06aefa`: the mask keeps 192 blocks with the switch, a sound that began waiting keeps the rules once the answer sounds, only answers to someone and within 3 s, the name takes the turn, a sound within 1 s after one let go is taken, tests on the product's duplex mask with a full pre-roll (each new rule's test fails without it), `group-talk.md` moved and measured (0.75 s or less let go, 0.8 s or more taken); `lamp-voice` 1,664 passed, ARM64 green (5,779); the first builder's re-check (a new P2: once the answer plays the mask scores the person 0.0, so a waited follow-up looked quiet) fixed in `0caceba87`: a waited sound is judged by its pause as it stood when the answer began to sound, and a block without echo evidence does not wait; my first test of it fed "speaker quiet" evidence and could not fail, the control showed it; approved at 23:53, ARM64 green (5,876) |
| 68 | A result told within 20 s of the agent settling (#560, the first builder's bound at 20:49; Nine and the second to choose a faster look or a push) | `second/news-on-settled`, `second/news-at-pause` | 2026-10-16 | the proposal on #560 (22:00): Harness's own `wait --machine … --any` (looks every 3 s, idle on two looks at one turn; `--machine` came to `wait` with `show`'s, #1333), about 10 s, against about 19 s and a Node start every 5 s for `ps`; Nine took it in `lamp-tasks` (`serve --push-wait`), the eighth left the voice's gate to me; the voice's half: (1) #1156, it reads at once when `tasks/status` lists a task newly settled, not up to 10 s later (a watcher, `Wake::Settled`; the no-switch integration test now says what it listens to; `lamp-voice` 1,638 passed), (2) with `LAMP_TASK_NEWS_AT_PAUSE=on` news at the first pause of a second, not after 15 s with no turn (`quiet_for_news`, the eighth's case 19 s against 33 s), stacked on #1156 as #1161; the first builder's review of #1156 (P2: no `tasks/status` without `--progress`, which Nine adds to the units in #1157; five P3s) and the reviewer's two doc P3s fixed in `b37fbe7dc` (`start_with_clock`, no news lost if the watcher can't start, at most one woken read a second); Ten's peer review clean, with lamp-75c9's numbers: settled 8.3 to 10.2 s after Harness's idle with `--push-wait`, so the first word about 10 to 12 s; the disk monitor (dee's ask) had my three build folders at 190 G: two deleted and one cleared, about 167 G freed, now one folder, one build worktree, `CARGO_INCREMENTAL=0`; #1156 merged at 23:5x; #1161 retargeted, the first builder's P2 (1 s after the lamp's own words is the person's turn) fixed: after the lamp's words 3 s with nobody starting (`AFTER_LAMP_US`); a wrong claim of mine on #1156 (that #1157 put `--progress` in the units) corrected within minutes, the units come with Nine's #1174 |
| second-P1 | (kept mergeable) the note after an early end, #738 | `second/early-end-labelled` | 2026-10-14 | rebased onto `e12a6cc83` (`f84889c91`) beside #782's `continues`, each field behind its own switch; `lamp-voice` 1,241 passed, 1 timed motor test failed under load and passed alone 3 of 3; merged by the first builder at 19:42 (ARM64 green, 4,499 passed); merged beside the second voice, it broke `lamp-voice`'s test build (three of the second voice's test literals without `continues_cut_off`): mended by #851 (`second/start-fields`, merged as `426ad0991`) |
| 45 | (for #811) the install's two blockers in `lamp-update`, on nine's #815 | `second/install-gates` | 2026-10-15 | `e13001625`, two commits on #815's `73a1fffa8` for nine to fast-forward: every part in `lampos.target` carries setup's marker condition itself (a target's condition holds back nothing it pulls in), and install writes a setting only where it is missing or still the default it wrote; on a throwaway Ubuntu VM (systemd 255) with no marker, #815's units started all 14 parts at boot and the fixed ones none; the reviewer's two later rounds (a failed enable never retried; a stop between a write and its record) closed: the debt and each setting recorded before the write (`d64bae34e`), `lamp-update` 68 passed; a sixth round (two failed runs in a row) fixed (`d5c033983`, `lamp-update` 69 passed); the first builder fast-forwarded #815 to it at 22:04; draft #863 gave the ARM64 run (4,682 passed); #815 merged by the reviewer at 22:2x and #863 closed; #811 open for the bootstrap |
| 43 | The person's phone over Bluetooth (issue #735, `second-P4`) | none | 2026-10-15 | cancelled by the owner the same night before any work was kept; released on #735 |

## Proposed tasks

| Id | Proposal | Issue | State |
|---|---|---|---|
| second-P1 | The early end taken back: resumed from the session's state before the piece, or a labelled continuation, probed once against the service before anything is built | #723 | yes, but (first builder on #723, 20:20): the study and the probe first, the probe's lamp step read-only of the voice's settings, recorded speech, at most 20 requests, `--dry-run`; taken 2026-10-14 |
| second-P5 | The hello on arrival asks the Policy (the second half of #752) | #856 | filed 2026-10-15 20:4x; yes from the first builder at 00:02; pull request #883, ready (`9e7b7cbf9`, ARM64 green, 4,765 passed); in shadow on lamp-4ace from 2026-10-16 00:38 (first builder); the reviewer's 00:43 finding (a hello chosen in shadow and the voice's own greeting for it counted twice) fixed in `938a5d53b` (`lamp-policy` 172 passed); #883 and #873 both change `lamp-policy`'s `main.rs` and `policy.rs`, put together on a local branch so the second to merge follows the first at once; ARM64 green on `938a5d53b` (4,770 passed); with #873 on lamp-4ace from `second/pictures-hello`; rebased at 03:5x for `lamp-voice`'s `mod selftest` (`a16945b9b`, msgs 93, policy 172, voice 1,383 passed); the reviewer's 01:33, 04:55 and 05:22 findings (a late first sound, one own turn speaking twice, a slow answer counted as unasked) fixed in `ea38e6874` and `589e904b1`: whether a turn was asked for is known at its start, and only a turn the lamp began itself counts, once (`lamp-policy` 175 passed); for the owner's minute at 11:00; rebased three times on 16 October as `lamp-voice` moved (#943's `--act attention`, #950's `lamp-skills`, #830's `lamp/online`): `34ce7a8ed`; ARM64 green on `38a10c694` (5,296 passed); rebased for #937 and #954 (`bb0e75569`, ARM64 green, 5,384 passed); merged at 11:25 |

## Found, not mine

- **The body's glance from rest gets neither its lead nor its "from where
  it rested" on a lamp** (2026-10-14, the body's, the fifth builder's;
  said on #714). Under `--rest-glance on` both need the body to have
  counted 50 steps at rest, and every request from Expression resets that
  count; `lamp-expression serve --enable` asks every 20 ms, zero offsets at
  rest included. How to see it: the body's own `expression_run_with` with
  zero requests before and after the glance, as on a lamp (a scratch test,
  not kept), lifts the goal 50 counts, against 80 to 90 in
  `a_resting_lamp_glances_with_the_switch_and_comes_back_to_rest`, which
  asks only during the glance; lamp-4ace lifted 49 at 18:40. lamp-52e6's
  head needs the lead to start from below.

## Handed back

Newest first.

### Task 45, for #811: the install's two blockers (2026-10-15, on pull request #815)

Ready for lamp-voice: through #815 (`second/install-gates`, `e13001625`)

**Why:** the reviewer found, on nine's #815, that nothing held the lamp's
parts back before setup: systemd checks a unit's condition only when that
unit's own job runs, after the jobs it pulls in are queued, so
`ConditionPathExists` on `lampos.target` gated none of the 14 parts (the
ears would open the room microphone on a lamp never set up). And a rerun
of install, which is how the updater upgrades itself, wrote the build's
default over a lamp's own pins. Nine had paused for the night; the crate
is the updater's.

**What it is:** each unit `PartOf=lampos.target` carries
`ConditionPathExists=/var/lib/lampos-setup/done` in `[Unit]`;
`lamp-update.service` and `lamp-records.timer` stay outside, ungated.
`state.json` gains `settings`, the SHA-256 of each product setting as
install last wrote it; install writes a setting only where it is missing
or still exactly that, and keeps and says any other.

**Tests** (M2 Max): every unit in the target gated, by `PartOf` and
`WantedBy` alike, 15 in all (fails with the condition taken out of
`lamp-ears.service`); the reviewer's case, line 99 to 100 and install
again, kept and said, nothing written or asked (fails with the old
always-write); a newer default replaces only an untouched setting; a state
from before reads and writes as before. `lamp-update` 63 passed, strict
clippy clean.

**What to run on the lamp:** a lamp with the product units and no marker:
reboot; `systemctl list-units 'lamp-*' --state=active` shows only
`lamp-update`, and `systemctl status lamp-ears` says "start condition
unmet". Then the marker and `systemctl start lampos.target`: every part.

**Against real systemd** (a throwaway Ubuntu 24.04 VM on the M2 Max,
Colima, systemd 255; the lamp's bookworm has 252): #815's units with no
marker started all 14 parts, by `systemctl start lampos.target` and by a
real boot, while the target itself was skipped; the fixed units started
none of them (only `lamp-update`, outside the target), and with the marker
all 14. The table is on #815.

**Where it is most likely wrong on the lamp:** systemd 252 behaving
otherwise than 255 here (not expected: condition checks have worked so
since long before both).

### Task 52: task news, polish (2026-10-15, issue #757)

Ready for lamp-voice: yes (#794)

**Kind: a fix, on by default** (the words of two notes and of one function
answer); a note that fits is byte for byte as before (pinned).

**What it is:** asked about news already told (the key in the told file),
`check_agent_task` answers `told_before: true` and asks for "it's the one I
mentioned", with a gist only if none was given; a question or a stop
already told is asked or said again shorter. A long result's note cuts the
task's words to 100 characters first, keeping 364 characters of a long
story against 190 before (a 273-character task, just finished; 324 against
150 a while ago), and asks for its gist in one plain line before "the rest
is on their computer", never only that.

**Tests** (M2 Max): the kept characters, the gist asked first and the task
cut to 100 for long and short tasks; told before, asked twice, a question
and a stop already told; a long or truncated result asked about. **Gate:**
fmt, strict clippy, `lamp-voice` 1,166 passed, 0 failed, 8 ignored (one
timed test failed once under load in the first gate and passed alone 3 of
3); ARM64 run 38703309129 green, 4,487 passed.

**What to run on the lamp:** the 08:02 part of the day again: a 300-word
story handed over by voice, told unasked, then "How is that task going?" a
few seconds later.

**Where it is most likely wrong on the lamp:** the model still giving only
where the rest is, or announcing the news again despite `told_before`.

### Task 48: the Policy decides when the lamp speaks first, progress news (2026-10-15, issue #752)

Ready for lamp-voice: yes (#765)

**Kind: off unless set:** `LAMP_PROGRESS_NEWS=on` on the voice and
`lamp-policy --progress --act progress`. Without the voice's switch no
subscriber appears on either topic, the tasks part is asked nothing new,
and no told file is written; without `--act` every progress decision and
log line stays `shadow:true` (tests). One commit, `e06d676d8`, on #751's
`8855a2507`; built by the sub-agent that built #751 and reviewed here
(only `task_news.rs` in the voice; subscriptions only with the switch, on
a thread of its own; each decision let go says why; the note through
`fitted`; the told key `<store>:<task>:progress-<change>` so it cannot meet
an observation's).

**What it is:** the policy, acting, sends progress decisions with
`shadow:false` while "is this said to me?" stays in shadow. The voice
takes a fresh `say_now` (60 s, this store, this task still working, no
newer change), fetches the progress line and the task, and offers one
note under its own gates, let go if not told within 60 s.

**Tests** (M2 Max): the chain from a change on `tasks/status` through the
real acting policy loop (a stand-in judge) to one note offered, told once
and kept; every kind of decision let go; long notes in English and
Vietnamese fit. **Gate:** fmt, strict clippy and tests on the 22 crates
that are or use `lamp-msgs`: 3,652 passed, 1 failed (`lamp-health`, the
Mac's disk warning), 21 ignored; the workspace build. Native ARM64 run
38667569523 on `e06d676d8`: green, 4,443 passed, 0 failed, 35 ignored.

**What to run on the lamp** (`docs/tasks.md`, "Playing progress news on
lamp-4ace"): both switches on after the minute; `act=progress` in the
policy's start line and a `voice.` subscriber on both topics; a task of a
few minutes handed over by voice while sitting in view; the change, the
`say_now`, the offer, the telling and the one sentence, with the time
from decision to sound.

**Where it is most likely wrong on the lamp:** Jev answering `wait` to
everything; the 60 s life and the voice's gates not meeting; a summary
line that does not make a natural sentence; the presence gate rarely
opening.

### Found in task 44's minute: news too long for one written turn (2026-10-15, for issue #736)

Ready for lamp-voice: yes (#756)

**What happened:** in #740's first minute on lamp-4ace the voice ended
three times in three, 4 ms after committing news, with `Gemini Live
InvalidConfiguration`. The news is one written turn, refused over 1,024
bytes (`wire::MAX_NOTE_BYTES`) as `InvalidConfiguration`, which the
provider worker ends on; the note's own words are 545 to 585 bytes and it
quoted up to 300 characters of the task and 1,200 of the answer. It
happened again at 07:26 without #740, and the first builder put the fix on
lamp-4ace at 07:38.

**Kind:** a fix, no switch: it changes only a note that would have been
refused; one that fits is the same byte for byte (a test). One commit,
`158968946`, on `lamp-voice` at `d128c458a`: `task_news::fitted` cuts the
note to fit, the answer first, then the task, by whole characters, marked
"…(it goes on)".

**Tests** (M2 Max): one byte over is refused by `encode_written_turn`;
every note, for long answers and tasks in English and Vietnamese, in every
state and at both ages, fits and is taken. **Gate:** fmt; strict clippy on
voice, voice-eval, memory; the `task_news` tests. Native ARM64 run
38657718552 on `158968946`: green, 4,410 passed, 0 failed, 35 ignored.

**What to run on the lamp:** the news of a task with a long answer, told
whole with no `InvalidConfiguration` (the first builder's 300-word story).

**Where it is most likely wrong on the lamp:** a long answer whose result
is at its end, cut away before the model sees it.

### Task 40: progress news through the tasks part and Jev (2026-10-15, issue #732, `second-P2`)

Ready for lamp-voice: yes (#751)

**Kind: off unless set:** `lamp-tasks serve --progress` and `lamp-policy
--progress [--utc-offset-minutes N]`. Without them no `ps` is run, the
`tasks/status` topic is never made, the policy subscribes to nothing new,
both diagnostics are exactly as before, `get_progress` is refused with
`progress_off`, and an attention decision's bytes are unchanged (tests).
One commit, `a3d10489b`, on `lamp-voice` at `1337e7e1d`; built by a
sub-agent of this session and reviewed here (no free text on the bus; the
`about` line kept only in the tasks part's memory; `ps` bounded in time,
bytes, rows and line length; the progress judge on its own thread and
connection). Rebased here once (a `docs/tasks.md` section beside the first
builder's).

**What it is:** `tasks/status` (`lamp-msgs` `TaskStatus`), one kept value
for every watched task: engine, state, seconds working, last active, how
often the progress line changed and how much, and signs read from it
without a model (`tests_pass`, `build_works`, `fixed`, `found`, `done`,
`pull_request`; a phrase table with words that cancel a phrase). One
`harness ps --machine <id> --json` per machine on the watch's schedule. In
the policy, "say this progress now?" asked of Jev in shadow (worth telling,
good moment, too much), with hard rules: never while anyone talks or within
15 s of speech, at most one note per task per 15 minutes, a yes that comes
back once someone talks turned to `wait`. Decisions on `policy/decisions`
with `about: "progress"` and `task: <store>:<task>:<change>`.

**Tests and measures** (M2 Max): 4 new in `lamp-msgs`, 11 in `lamp-tasks`,
10 in `lamp-policy`. `parse_ps` 55 µs for 40 harnesses; one read through
the adapter against a scripted `harness`, 4.2 ms median; status published
to decision on the bus with a stand-in judge, 52.7 ms median (folder
`docs/verification/progress-news-host-20261015/`). **Gate:** on the agent's
head, fmt, strict clippy on the 20 crates that are or use `lamp-msgs`, the
workspace build, and 3,159 tests passed with one Mac-only failure
(`lamp-health`, the Mac's disk 95% full); after the rebase, fmt, the same
clippy, `lamp-msgs` 79, `lamp-tasks` 115, `lamp-policy` 154 (2 ignored),
the build; merged onto `cf2f5b43c`, `cargo check --workspace
--all-targets`. Native ARM64 run 38632666318 on `a3d10489b`: green, 4,258
passed, 0 failed, 32 ignored.

**What to run on the lamp** (`docs/tasks.md`, `docs/policy.md`): time `harness
ps --machine office --json` ten times as `orangepi`; `--progress` on the
tasks part and `lamp-bus info tasks/status` before and after; one real
task with `tasks/status` tapped for two hours, its changes and signs
counted and each sign checked against `get_progress` by hand; the read
times from diagnostics; then a day of the policy with `--progress
--utc-offset-minutes 420`, its latency and choices, the disagreements
marked.

**Where it is most likely wrong on the lamp:** `about` being the person's
ask, not the agent's progress; `ps` (Node, up to 40 agents) costly every 2
minutes; English phrases not matching the agents' own summaries; Jev
uncalibrated for this question; "spoke up unasked" read from
`voice/state`; bus permissions between `orangepi` and the policy's dynamic
user, not checked.

### Task 5, the second model: the bare "Lamp" in voices it never learned from (2026-10-15, issue #694)

Ready for lamp-voice: yes (#750)

**Kind:** a model file, off unless installed: `models/name/name.onnx`
replaced (1,233,449 bytes, `a8453aeb…`), no crate changed. Two commits
on `lamp-voice` at `78b984b69`, ending `928803b08`; built by a sub-agent of
this session and reviewed here (sources and licences in `SOURCE.md`; no
recordings kept; `say` and Kokoro voices for testing only). Training code
under `lampOS/training/`, the exception flagged on #725.

**Why the first failed, and the change:** near misses were 54% of its
other-word negatives; now 10% of each batch's other words, and three seeds
averaged in one graph. `training/name/played.py` imitates the lamp's played
check in 33 `say` speakers it never trained on. At 0.9 (pass: 3 of 6
calls, at most 1 of 8 near misses) the first passes 9/33 as said and 8/33
played; this one 22/33 and 12/33, hearing 4.0 and 3.5 calls against 2.3
and 2.0. False alarms per hour lower on every set (LibriSpeech 1.4 against
3.3); near misses in Piper voices higher (Lan 40% against 25%). Scores
under tract through `hear.rs` match onnxruntime within 4e-6. **Gate:** fmt
and the workspace build (no crate changed). Native ARM64 run 38637337785
on `928803b08`: green, 4,277 passed, 0 failed, 33 ignored.

**What to run on the lamp:** `lamp-name check DIR RECORDING.wav` on the
23:47 recording, beside the installed model; install only if it hears at
least as many calls (3 of 6 offline) with no more false hits. Asked on
#694: the installed `name.onnx` (`1d238ca7…`), and the `say` voice, speed
and full lines played.

**Where it is most likely wrong on the lamp:** colleagues called Lan or
Lâm; played through a speaker only 12 of 33 speakers pass; no real person
and not the lamp's own voice tried.

### Task 45: updates, a lamp keeps its lampOS current by itself (2026-10-15, issue #737, MVP)

Ready for lamp-voice: yes (#741)

**Kind: off unless set:** a new part, `lamp-update` (`crates/update`), its
unit template (`deploy/units/lamp-update.service`, not in `lampos.target`),
and `--apply on|off`, default `off`. With it off it writes nothing, not
even its records, and never calls systemd or the bus (a test holds every
file under the lamp root byte for byte). One commit, `01ff1faeb`, on
`lamp-voice` at `c1b8c7591`; built by a sub-agent of this session and
reviewed here (the signature checked over the exact manifest bytes before
anything is read from it; names restricted to `lamp-<plain>` and
`lamp-<plain>.service`; files' URL signed, https or `file:///` only, no
`..`; downloads only to https, bounded by the listed size, hashed as they
come; no new crate in the lock).

**One round:** the signed channel envelope read (Ed25519 with `ring`, the
lamp's key from setup), only a newer build and never one put back once;
each changed part installed beside the old in `/opt/lampOS/<part>/<sha256>/`
after a disk check (a whole second copy plus 256 MB); the units switched
at a quiet moment (voice resting, 45 s with nothing heard or said), the
voice first; 60 s of systemd and `lamp/health` watched; every changed unit
put back in reverse if a part is unwell. V1's OTA weighed in the
`v1-verdicts.md` form. The publishing proposal (a prerelease per tested
`lamp-voice` push, one channel-head release with the newest signed
envelope, the secret `LAMPOS_UPDATE_KEY` named and not added, the CI job's
YAML in `docs/updates.md`) and three questions are on #737.

**Tests** (M2 Max): 29 unit (1 more ignored, the network) and 15 whole
rounds against stand-ins (switched; a carried unit; bad signature and
checksum refused; a dying part and one in error put back; no network; not
switched while someone talks; a full disk; an older build; a switch cut
short put back; its own folders removed, hand-installed kept). The
network test run once by hand against the real `models/ambernet` asset;
`manifest` on run 144's real artifact (31 binaries, 146.95 MB) and `check`
against lamp-4ace's units (nine changes planned, nothing written).
**Gate:** fmt; strict clippy on `lamp-update`; its tests; the workspace
build. Native ARM64 run 38616519206 on `01ff1faeb`: green, 4,251 passed,
0 failed, 32 ignored.

**What to run on the lamp** (`docs/updates.md`): `df -h` and the units
written down; a throwaway key and a local `file:///var/tmp/lampos-channel/`
from the next `lamp-voice` run; `lamp-update check`; the minute with
`--apply off`; one attended `once --apply on`; optionally a deliberate
rollback.

**Where it is most likely wrong on the lamp:** a minute of health missing a
bad but running voice; carried units from the repository's
`deploy/lamp-4ace/`, which disagrees with the lamp, undoing a setting made
there; the disk (14% free on 2026-10-14); quiet needing the voice's bus;
`daemon-reload` and `restart` from inside `ProtectSystem=strict`, never
tried.

### Task 44: a yes said in two breaths still starts the task (2026-10-15, issue #736, MVP)

Ready for lamp-voice: no (#740 closed as not needed, 2026-10-15)

**Closed.** In its first minute on lamp-4ace (02:28 to 02:37) nothing was
carried: the call came while "Yes." itself was still being heard, a shape
this does not carry, and the voice ended on long news (#756). The first
builder's `d128c458a` then answered such a call as not done because the
person was still speaking, and five of five two-breath yeses started the
task (07:22 to 07:32). That answer also covers the shape below, one model
round trip later, without letting "Yes... no, wait" through, so carrying
was not worth its path in the transport. The branch is kept at `0ebd994d5`
(on `e4922a3cb`, with the call records the first builder asked for on
#736, and the window test given a barrier bound past the window, after it
failed 3 times in 5 at the 2 s default).

What follows is the hand-back as it was.

**Kind: off unless set** (`LAMP_CARRY_CALLS=on` in the voice's unit, read
by its provider worker into `lamp_gemini::SessionConfig::carry_calls`). One
commit, `df5e5f348`, on `lamp-voice` at `c1b8c7591`.

**The cause**, from the code (the 22:37 record lines are asked for on the
issue): a function call belongs to the request whose input has ended
(`crates/gemini/src/session.rs`, `tool_calls`). "Yes." was committed;
"please start it." began a moment later and "Yes." was given up as
continued, its response becoming the barrier. The model's
`confirm_agent_task` for "Yes." came while "please start it." was still
open, which can own nothing, so it was answered `{"error":"cancelled"}` and
dropped (`UnownedToolCall`), and the model said it could not start.

**The change.** With the switch, such a call (while the barrier stands for
the request the open one replaced, that request continued and not
discarded, within 3 s) is the open request's `ToolCall`, marked
`carried_from`, and its answer is written when given; every other unowned
call is cancelled as before. The voice runs it as that request's, except a
call that changes what the person sees (the executive), which is not done
and the model told the person went on. A task's yes is still checked by
the voice's own rule.

**Tests** (M2 Max): transport: the 22:37 case carried and answered once;
refused without the switch, for an abandoned request, and after the
window. Voice worker: a carried `light_on` answered "not done" with the
executive never asked; a carried call for the time answered. **Gate:**
fmt; strict clippy on gemini, voice, voice-eval, policy, surround; tests
one at a time: gemini 259 (2 ignored), voice 1,043 (6 ignored), voice-eval
284, policy 144 (2 ignored), surround 260; the workspace build. Native
ARM64 run 38616463482 on `df5e5f348`: green, 4,213 passed, 0 failed, 31
ignored.

**What to run on the lamp:** with `LAMP_CARRY_CALLS=on`, the two-breath yes
five times and a one-breath yes five times, each after a prepared task;
the tasks started, what the lamp said, and "carried over" in the journal.

**Where it is most likely wrong on the lamp:** "Yes... no, wait" for a task
still starts it (the carried yes passes the voice's check); a task call
answered after the 2.5 s kept-start patience may be withdrawn by the
service and its answer dropped as stale.

### second-P1, step 2: the note after an early end (2026-10-15, issue #723)

Ready for lamp-voice: yes (#738)

**Kind: off unless set.** Only with `--early-end-ms`, which no unit sets
since 13:46: the successor of a request whose end was sent early and given
up starts with `continues_cut_off`, and the provider worker writes
`CUT_OFF_NOTE` beside it ("... this is the rest of it. Answer all of it as
one request."), with any other note for that request. The flag is left off
the wire when false: a start without it is byte for byte as before (a
test). One commit, `9417bcfcc`, on `lamp-voice` at `dcc2e6b1e`.

**The rest, not the whole.** Lamp-4ace answered the whole question with
its note as one, but the voice cannot send a piece's sound again: the
transport takes no sound older than 2.5 s (`MAX_INPUT_AGE`), and the pizza
question came in four pieces. So the successor carries the rest with the
note, and the probe now asks exactly that (`rest-labelled`, a new
`rest.wav`, in place of `resume-late`); a test holds the voice's note and
the probe's `REST_NOTE` to the same words. The probe also reads the voice's
own provider file as it is, as the first builder asked.

**Tested, on the M2 Max:** the early-end session tests see the note once,
just before the successor's end, when speech resumes 0.3 s after the end
and in "Lamp, stop." over an answer, and none on a request quiet to the
end; the probe's six tests (the voice's provider file read; a loose
credential refused). **Gate:** `cargo fmt --all --check`; strict clippy
on `lamp-gemini`, `lamp-voice`, `lamp-voice-eval`, `lamp-policy`,
`lamp-surround`; tests one at a time: gemini 256 (2 ignored), voice 1,037
(6 ignored), voice-eval 284, policy 144 (2 ignored), surround 260; the
workspace build. Some voice session tests fail on this Mac only while it
is loaded by other work; they passed alone and on quieter runs. Native
ARM64 run 38604441393 on `9417bcfcc`: green, 3,953 passed, 0 failed, 31
ignored.

**What to run on the lamp:** `rest.wav` ("Japan?", `say -v Daniel` and
`afconvert`), the probe once more with the voice's own provider file, and
`rest-labelled` must answer Tokyo without answering "the capital of" on
its own. Only then the five pause scenes with `--early-end-ms 200`
against a run without it, by the table in `docs/early-end-taken-back.md`.

**Where it is most likely wrong on the lamp:** the rest alone being weaker
than the whole; the note arriving after the given-up answer has begun
under `kept-start` (where the moon thought went wrong at 13:45); thoughts
in several pieces; short pieces heard as another language.

### second-P1, step 1: the early end taken back, study and probe (2026-10-14, issue #723)

Ready for lamp-voice: yes (#731)

**Kind: off unless set.** No library or service code changes: an example
that runs only when started by hand (`crates/gemini/examples/early_end_probe.rs`),
`service_probe`'s provider and WAV loaders moved unchanged into
`examples/shared/provider.rs` for both probes, `[[example]] test = true` in
the gemini manifest so `cargo test` runs the probe's tests, and
`docs/early-end-taken-back.md` with a pointer from `faster-answers.md`.
One commit, `0839cfb4b`, on `lamp-voice` at `3e3de2d7a`.

**The study** (`docs/early-end-taken-back.md`): the Live API has no
message that takes a request back (an `activityEnd` ends it, client
content "is unconditionally appended to the conversation history"). A
resumption handle "represents a state that can be resumed" and is valid
2 h after the session ends, but neither Google page says whether an
earlier handle resumes once later ones were given. So (a), resuming from
the handle before the piece, and (b), a note on the whole utterance, are
both open, and only the service can say.

**The probe:** five trials of three requests each (15, never over 20,
ended within 115 s): `control`, `resume`, `resume-late`, `whole-resumed`,
`whole-labelled`. It reads the voice's provider file by path (never
printed), sends five recorded utterances from files and opens no
microphone or speaker. It prints one JSON line a trial of timings and the
service's words, and `--dry-run` prints the plan without connecting. The
gemini library keeps only the newest handle and its handles cannot be
compared outside it, so the probe keeps its own copy of the handle from
before the piece, and resumes once at once and once after the piece's
whole answer, when a newer handle has surely been given.

**Tested, on the M2 Max, against the scripted service, not the live one.**
Five tests, the service given a memory that behaves as (a) needs:
`control` names orange and purple; `resume` and `resume-late` orange
only, each presenting the handle from before the piece; both whole-question
trials say Tokyo, and the note goes once inside the whole question;
nothing private printed; a refused resume reported and not retried; a
silent service ends the run within 115 s and 20 requests; the dry run
shows 15 requests and the note. The built example's `--dry-run` also ran
here on a placeholder provider file.

**Gate:** `cargo fmt --all --check` passes; `cargo clippy -p lamp-gemini
--all-targets -- -D warnings` clean; `cargo test -p lamp-gemini` 255
passed, 2 ignored (the probe's 5 among them); `cargo test -p lamp-gemini
--examples -- --test-threads=1`: the probe's 5 and `service_probe`'s 23
passed; `cargo build --workspace` passes. Native ARM64 run 38587667135 on
`0839cfb4b`: green, 3,822 passed, 0 failed, 29 ignored.

**What to run on the lamp** (`docs/early-end-taken-back.md`): the five WAV
files (`say` and `afconvert` on a Mac; in this session `say` gave 5 ms
files, so check that each is a second or two), `cargo build --locked
--release -p lamp-gemini --example early_end_probe`, then as the voice's
user `early_end_probe /etc/lampOS/voice/provider.json DIR --dry-run` and
the same without `--dry-run`. The document's table says which way holds
from the rows; step 2 is built only on that.

**Where it is most likely wrong on the lamp:** (1) the configured endpoint
not passing `sessionResumption` both ways, so a refused resume would be
the road and not the service; (2) no current handle within 3 s of the
first answer (`handle_before_piece: false`); (3) the model guessing the
words from the check question itself: the words in each row decide, not
the `answer_mentions` flags; (4) a slow link ending the last trials as
`skipped`.

**Not shown:** anything from the live service; a person's voice; how long
resuming takes on the lamp's link; what a resumed session keeps beyond the
probe's own instruction.

### Task 36: always alive, the idle layer, sleep and waking (2026-10-14, issue #714)

Ready for lamp-voice: yes (#729)

**Kind: off unless set.** Nothing changes unless Expression is started
with a catalogue that has an `alive` part
(`crates/expression/alive-behaviours.json`, lamp-4ace's catalogue with
one). Without it the engine has no idle layer at all (`life` is `None`)
and every existing Expression test passes unchanged; the new inputs it
subscribes to (`controls/gesture`, `schedule/due`) are read and ignored.
`serve` refuses a catalogue with sleep unless `--utc-offset-minutes` is
given.

**Built** (one commit, `928ad1a57` on `second/always-alive`, on `lamp-voice`
at `b9eadf847`), all in `crates/expression` (the third builder's crate; the
task names it) and its documents:

- `src/alive.rs`: the `alive` catalogue part with its bounds, and the
  layer: a breath phase-locked to the ring's own (the same function of the
  boot clock as `ring_face.rs`, so no message between them), drifts every
  30 to 90 s from a fixed hash (no randomness), in and out over 0.35 s,
  none while a face stays close and still under the light; sleep at night
  after 15 minutes of nobody, a slow breath and a bow, no clip begins;
  waking by the name, an enabled tap or stroke, a newly due reminder, its
  own answer, or the first face of the morning, with a 2.7 s stretch that
  crosses back to the normal plan; energy as size and pace.
- `engine.rs`: three inputs (`Touch`, `Due`, `DayTime`), the layer laid
  under the plan, clips held while asleep. `runtime.rs` subscribes to the
  two topics and tells the time of day every minute (`with_local_time`).
  `main.rs`: `serve --utc-offset-minutes`, `replay --long`,
  `import --long --utc-offset-minutes`. `replay.rs`: the long bounds (an
  hour and five minutes), the time of day added to an import, and a
  summary of each behaviour's ticks and starts, the largest change in one
  tick and the farthest asked.
- `docs/alive.md` (new), `docs/expression.md` (a section, the inputs,
  the long replay).

**Measured, on the M2 Max laptop, not on a lamp.** The tests: 16 through
the engine and 6 of replay, import and the live service (in
`docs/alive.md`, "Measured"). A written hour through the built
`lamp-expression replay --long`: by day, breathing all hour and 53 drifts,
the largest change in one tick 0.0022 rad (the layer coming in); at night
from 22:45, asleep at 23:00 after 15 minutes of nobody, 14 drifts before
it, none while it nodded off, and no change in one tick larger than the
layer's coming in.

**Read in the body, not changed:** at rest lamp-4ace's head may lift 50
counts but dip only 15 and roll 1, so the resting layer only lifts the
head; with the planner asking every 20 ms the body follows small slow
lifts a count a step, and a zero stream writes nothing (a scratch test on
the body's own harness, not kept).

**Gate**, on the M2 Max: `cargo fmt --all --check` passes; `cargo clippy
-p lamp-expression -p lamp-lean -p lamp-voice --all-targets -- -D
warnings` clean; `cargo test -p lamp-expression` 203 passed (23 new);
`cargo test -p lamp-voice -- --test-threads=1` 1,027 passed, 6 ignored;
`cargo test -p lamp-lean` 61 passed, 2 ignored; `cargo build --workspace`
passes. The voice and lean tests ran before the last two changes inside
`alive.rs` (the slow fade into sleep, no drift while the layer goes out),
which neither uses. Native ARM64 run 38581248942 on `928ad1a57`: green,
3,838 tests passed, 0 failed, 29 ignored (its 255 `test result` lines).

**What to run on the lamp**, smallest first (`docs/alive.md`, "What to
run on the lamp"): (1) record a quiet hour with `lamp-bus record ...
--seconds 3600 --max-mb 128`, `lamp-expression import --long
--utc-offset-minutes 420`, `lamp-expression replay --long --enable
--catalogue alive-behaviours.json`, and read the summary: no input
refused, breathing nearly every tick, about a drift a minute, the largest
step under 0.003 rad; (2) install the catalogue and add `--catalogue ...
--utc-offset-minutes 420` to the `serve --enable` command, the body
keeping `LAMP_EXPRESSION_BODY=on` and `--rest-glance on`, and its own
`--idle on` off for one idle author; (3) ten minutes at rest standing and
folded, and once at night (or with `night` and `nobody_us` set short).

**Where it is most likely wrong on the lamp:** (1) the breath too small
for the head servo to show (20 counts asked at 12 counts a second; a
25-count ask reached 12 to 14 on lamp-52e6), or shown in steps that read
as a servo hunting: `breath.inhale` to -0.045, or to 0; (2) the servo
heard at rest by the microphones the name listener uses; (3) sleep broken
by talk: Expression holds its own plan, but the body's listening pose
answers any turn the voice takes, so a sleep that holds needs the voice or
the body to know it (#713); (4) the 15-count bow unseen; (5) the work
light's face size and 5 s guessed.

**Found, not mine** (said on #714): with `--rest-glance on` and Expression
serving, the body never counts 50 steps at rest, so the rest glance's
"from where it rested" and 40-count lift lead never engage; the glance
lifts 50 counts against 80 to 90 in the body's own test (simulated
servos), as lamp-4ace's 49 at 18:40. It matters for lamp-52e6's head.

**Not shown:** anything on a lamp; a real quiet hour (asked on #714; the
hours above are written); energy and "asleep" from the inner state
(#713); the arm's stretch (the body's `stretching` clip); the ring's look
while asleep.

### Task 5: the name, heard as a bare "Lamp" (2026-10-14, issue #694)

Ready for lamp-voice: yes (#725)

**Kind: off unless set.** No code changes. The new `models/name/name.onnx`
is a file in the repository that nothing installs: `lamp-name` reads
`name.onnx` from its `--models` directory (`/opt/lampOS/name/models/` on
lamp-4ace), and nothing under `deploy/` or `crates/` names
`models/name/`. Until someone copies it there, every lamp hears its name
exactly as before.

**Built** (one commit, `78648897c` on `second/bare-name`, on `lamp-voice`
at `a9a7262a9`): `models/name/name.onnx` (124,135 bytes, 30,913 weights,
SHA-256 `f376e964…02030`, with `SOURCE.md`), the shape of the name model
`lamp-name` already loads ([1, 16, 96] openWakeWord embeddings in, one
score out), so it replaces the lamp's file with no code change; the
document `docs/bare-name.md`, and one paragraph in `docs/name.md` pointing
to it; the training code `training/name/` (Python, `sh make.sh` end to
end, each download checked against `downloads.sha256`). It reuses task 3's
`training/stop/` by import, with three additions there that are off by
default, so task 3's results do not change: `store.py` and `train.py` take
another folder (`--inputs`), `train.py` can leave out one way of calling
(`--leave-out-way`), and `make.sh` stops after the shared sets when
`STOP_ONLY_CLIPS` is set. Trained only on speech whose licence allows
shipping the model (Piper's LibriTTS voices, MSWC English, Speech Commands,
LibriSpeech, FLEURS Vietnamese: CC BY 4.0 or MIT). EdAcc (CC BY-SA 4.0) and
AMI were used for testing only. A third of every generated line is read
with espeak's Vietnamese phonemes as a stand-in for a Vietnamese accent; no
licence-clean recording of Vietnamese speakers saying English could be
trained on (the table in the document says why each was not).

The work was done by a sub-agent of this session on the M2 Max. I
reviewed it and moved its four commits onto the newest `lamp-voice` as
one; the training code's first version read FLEURS's 32-bit floats as
silence, and that was mended and the model retrained before it was handed
over (the document says so under "What went wrong on the way").

**Measured, on the M2 Max laptop, not on a lamp** (`docs/bare-name.md`,
"Measured": voices and speakers never trained on, a score of 0.9 for one
step, the rule `lamp-name` uses now):

| Way of calling | English reading | Vietnamese-accent stand-in | Real people (MSWC "lamp", 51) | Both microphone effects (English / accent / real) |
|---|---:|---:|---:|---:|
| "Hey Lamp, ..." | 94 % | 87 % | | 95 % / 84 % |
| "Lamp ơi, ..." | 92 % | 76 % | | 89 % / 77 % |
| "Lamp, ..." | 86 % | 53 % | 51 % | 72 % / 42 % / 63 % |

A voice from another generator (Piper's British Cori, never trained on)
is heard far less: "Hey Lamp" 67 %, "Lamp ơi" 5 %, "Lamp," 16 %. The
comparison model, the same recipe without bare calls, does no better on
Cori.

What sets it off, at 0.9: "Lamb" at the start of a line in 30 % of
generated clips, "Lan" and "Lâm" 25 %, "lab" 11 to 21 %, "land" 9 to 17 %,
rhymes 3 to 14 %; "Sam", "Liam", "ma'am", "plan", "hey" and "lamp" inside a
sentence 0 to 2 %. Talk: 3.3 an hour of LibriSpeech readers, 10 of AMI
meetings, 23 of Vietnamese (FLEURS), 36 of Vietnamese speakers of English
(EdAcc). The lamp-like replies: 8.4 an hour, nearly all "Lamp here, what
can I do for you?" (24 of its 64).

Beside the same recipe trained without bare calls (the stand-in for the
lamp's own file, which is in no branch and was not on this machine): the
bare "Lamp," goes from 1 % to 65 % (all generated voices), "Hey Lamp" 90 %
to 88 %, any near miss 4.5 % to 9.4 %, readers 3.7 to 3.3 an hour, FLEURS
17 to 23, EdAcc 48 to 36, the lamp-like replies 1.4 to 8.4. At 0.98
instead of 0.9: the bare name 59 % / 31 % / 18 % (English / accent /
real), "Lamb" 5 %, readers 1.3 an hour, EdAcc 16.

**My own check of the file**, on the M2 Max: `lamp-name check` built from
`second/bare-name` (`--release`, `hear.rs` under tract 0.23.8) with
openWakeWord's two front-end files (SHA-256 as in `SOURCE.md`) and this
`name.onnx`, over the sub-agent's three test files of held-out clips laid
end to end: 18 hearings in 24 generated calls, 6 in 12 real "lamp" words,
2 in 24 near misses, at 1.4 to 1.5 ms for each 80 ms step of all three
models. That agrees with the tables; it is a laptop's time, not the
lamp's.

**Gate:** no Rust changed. `cargo fmt --all --check` passes; the native
ARM64 run on the branch's head (`78648897c`), run 38569922098: green,
3,785 tests passed, 0 failed, 29 ignored (summed over its 252 `test result`
lines). `lamp-voice` then moved nine commits that touch none of these files,
so the branch was not rebased and the run stays on its head.

**One rule read narrowly.** `builders/second.md` says the second builder
may not commit Python. The task's hand-back asks for `training/name/` "as
task 3", whose Python training code is on `lamp-voice`, and the model
cannot be made again without it. So it is in the pull request. If it is
not wanted, I take out `training/name/` and the three `training/stop/`
additions and keep the model, its source and the documents.

**What to run on the lamp**, smallest first:

1. Before installing anything, from a directory holding
   `melspectrogram.onnx`, `embedding_model.onnx` and this `name.onnx`:
   `lamp-name check THAT_DIR FILE.wav ...` over the lamp's own microphone
   recordings of calls (the 2026-10-14 ones in attention.md, or new ones:
   `lamp-bus record DIR mic2/audio --seconds 600`, then
   `training/stop/from_bus.py`), and the same files with the lamp's
   current models directory. It prints each hearing at 0.9 and the highest
   score per file: the eight bare "Lamp," lines that the current file
   heard in none are the ones to count.
2. Ten minutes of the lamp talking with nobody near, recorded the same way
   and run through `lamp-name check`: how often it hears itself.
3. Only then, as `docs/bare-name.md` "Wiring it on the lamp" says: keep
   the current file as `name-hey.onnx`, copy this one in its place,
   restart `lamp-name.service`; going back is the reverse.

**Where it is most likely to be wrong on the lamp:**

1. **A bare "Lamp" after the microphone's lost onset.** The first tenth
   of a second of "Lamp" is most of its "l", and the loss trained on is a
   guess (20 to 40 dB down for 60 to 140 ms), not measured from the lamp.
   The rows "onset lost" and "both" in the document are where to look
   first.
2. **The lamp's own voice.** "Lamp here, ..." set it off 24 times in 64.
   Gemini's voice through the lamp's loudspeaker and canceller was never
   heard. Whatever takes `name/heard` must pass over hearings while the
   lamp's own reply plays, if it does not already.
3. **Vietnamese voices, and Lan and Lâm.** Every accented call here is a
   stand-in, and Vietnamese talk was the hardest part before the bare
   name too (23 and 36 an hour). A colleague called Lan or Lâm sets it off
   about one time in four.
4. **Any real voice.** The model leans on the one generator it learned
   from (Cori: 16 % for a bare "Lamp"); real people's single word "lamp" is
   at 51 %. Real calls recorded at the lamp are what it needs next.

**Not shown:** anything on a lamp; the lamp's current `name.onnx` beside
this one (the commands to put it in the same tables are in the document,
"The lamp's current name.onnx"); a real person saying "Lamp," as a call;
more than one training run.

### Task 3: a listener for "stop" under the lamp's own voice (2026-10-14)

**Built:** `models/stop/stop.onnx` (124,135 bytes, 30,913 weights, SHA-256
`5fe07569…72d41`, with `SOURCE.md`), the same shape as the name's model:
the newest sixteen openWakeWord embeddings in, `[1, 16, 96]`, one score out,
every 80 ms. The training code is `training/stop/` (Python, `sh make.sh`
end to end, every download checked against `downloads.sha256`), and the
document `docs/stop-word.md`. Trained only on speech whose licence allows
shipping the model: Speech Commands, MLCommons' Multilingual Spoken Words
(English, from Common Voice), LibriSpeech and AMI (all CC BY 4.0), and
speech generated with Piper's `en_US-libritts-high` voice (trained from
scratch on LibriTTS; the LibriTTS-R voice openWakeWord uses was avoided,
its data being for research only). No Rust changed.

**The task is met for the word alone and not under the lamp's voice.**
Threshold 0.9, one step, speakers never trained on; "suppressed" means
through a suppressor told the other voice exactly, which the lamp's
canceller is not, so the lamp lies between the two:

| | alone | 0 dB | −5 | −10 | −15 | 0 dB, suppressed | −15 dB, suppressed |
|---|---|---|---|---|---|---|---|
| "stop", real speakers (736) | 85% | 17% | 11% | 7% | 4% | 48% | 25% |
| "wait", real speakers (263) | 84% | 13% | 8% | 4% | 3% | 48% | 18% |
| "hold on", generated (375) | 95% | 47% | 30% | 21% | 14% | 89% | 59% |

Plainly mixed under another voice the word is lost: a speech recogniser
(wav2vec 2.0) also wrote only the other voice's words in those mixtures,
so no threshold mends it. False alarms an hour at 0.9: 9.0 over 11.1 h of
read speech, 3.2 over 0.9 h of generated voices, 7.4 over 24.1 h of AMI
meetings; at 0.98 about 2.2, 1.1 and 1.0, but only 48% of words alone are
then caught. Near misses at 0.9: "don't stop" 3%, "bus stop" 3%, "stop"
inside a sentence 8%, "stopped" and "waited" cut from sentences 56% and
60%. "Stop it." counts as a stop (19 of 19 alone). In AMI meetings, said
naturally: 1 of 28 "stop", 3 of 39 "wait", 5 of 22 "hold on". Median 62 ms
from the end of the word to the hearing. On the M2 Max laptop, one
training run and one seed.

**Found on the way:** openWakeWord's mel model sets its floor 80 dB under
the loudest value of everything it is given at once, so features made in
batches differ from `hear.rs`, which gives it one chunk at a time;
training reproduces the chunk-at-a-time floor, and its embeddings match
`hear.rs` to within 4.6e-5. **Under tract:** a scratch program running
`hear.rs` unchanged under tract-onnx 0.23.8, with `stop.onnx` in place of
`name.onnx`, matched training's scores to within 6e-6 over 1,185 steps,
on macOS and in the Linux ARM64 container; the model alone takes 24 µs a
run there.

**Recommended in the document:** 0.9, one step, listened for only while
the lamp speaks; a hearing pauses the speaker and lets the duplex rules
confirm or resume. If a hearing is to stop the lamp outright, 0.98.

**Gate:** no Rust changed; `cargo fmt --all --check` clean on
`lamp-second` with these commits; `cargo build --workspace` passed on the
task's branch, and the code it builds is unchanged by them.

**On the lamp, the quickest look:** ten minutes of `lamp-bus record …
mic2/audio` while the lamp talks and nobody speaks, then `from_bus.py` and
`listen.py` from `training/stop/`: how often it fires on the lamp's own
voice through the canceller. With no code change, `lamp-name check` runs
the model on the lamp if `stop.onnx` is installed as `name.onnx` in a
directory of its own. Wiring (a second model on the same embeddings in
`lamp-name`, a topic of its own) is proposed in the document, "Wiring it
on the lamp".

**Where it is most likely to be wrong on the lamp:** the canceller. The
whole difference between 17% and 48% at equal level is how much of the
lamp's own voice is gone before the listener hears it, and the lamp's
second microphone and its chip are in no mixture made here. And it may
fire on the lamp's own voice where the canceller leaves it, which is the
first thing the ten-minute recording shows.

**Not shown:** the lamp, its microphone, its canceller, a person at the
lamp; real "hold on" (2 read clips and 22 in meetings); more than one
seed. Common Voice itself was not used (it now needs an account).

### Task 19: the end sent early, the answer played only when sure (2026-10-14)

**Built** (`crates/voice`; `docs/faster-answers.md` way 2,
`docs/conversation-latency.md`, `docs/live-runtime.md`,
`docs/conversation-thread.md`): `--early-end-ms MS`, off by default;
100 to 1,000 ms in steps of 10, shorter than the end-of-speech wait in
force, and refused without `--provider-continuation kept-start`. For the
lamp: `--early-end-ms 200`.

- At that much quiet the coordinator sends the request's end
  (`activityEnd`) and keeps listening. The authority goes on saying
  "listening" until the 600 ms wait, or the end-of-turn model's verdict,
  confirms; nothing can play before. The answer's sound is held and starts
  at the confirming moment, or at once if that is already past.
- Speech again before confirming (six blocks in a row at 0.8 or more, the
  detector's own start rule) gives the request up exactly as
  `commit_admitted_input` does: `user_interrupted`, then an Interruption
  admit with nothing published between, which kept-start treats as a
  continuation. The blocks heard since the end (at most 30) open the next
  request.
- An utterance that would count as a sound gets no early end, so a lone
  "Lamp," stays one request with its question.
- The model's words reach `voice/said`, and the body its gestures, only once
  confirmed. A "..." answer is ended unheard at the confirming moment; the
  busy-room floor waits for it.
- **Outside the coordinator, for the first builder to look at:**
  `provider_worker.rs` and `thread.rs` take a request given up after its
  end but before confirming out of the conversation thread (`thread::unsay`),
  so its words are not kept as said. It cannot happen with the option off.

**Tests against the scripted service** (real worker and session,
kept-start; "start/end" are activityStart/activityEnd):

| Case | Sent to the service | Given up | Last sound to first block to the speaker |
|---|---|---|---|
| Quiet to the end | 1 start, 145 blocks, 1 end | 0 | 604 ms, at the confirming moment and not before |
| Speech again at 0.3 s (vad-only, and duplex 600) | 2 starts, 235 blocks, 2 ends | 1 | about 1,271 ms, the second answer only; the first never played |
| Answer before confirming | 1 start, 1 end | 0 | 603 ms |
| A "..." answer before confirming | 2 starts, 1 end | 0 | nothing played |
| Answer after confirming (option on / off) | 145 / 185 blocks | 0 | 1,269 / 1,670 ms |
| The link drops between | the first link one request, the next question on a new link | 0 | the first turn `provider_failed`, nothing played; the next 603 ms |
| "Lamp, stop." over an answer | 3 starts, 3 ends | 1 | 602 ms |
| A lone "Lamp," a pause, the question | 1 request | 0 | 1,273 ms |
| Option off | end at 600 ms, no new records, the same worker arguments and `run_start` | 0 | 920 ms |

A transport test shows a name note goes only with the request ended early
and never reaches the next, continued or abandoned. Measured path from the
last sound to the first speaker block: with the injected clock at the
lamp's 1.15 s service time, 1,910 ms off and 1,510 ms on; in a scripted
session with a 900 ms service, the end sent at 604 against 203 ms and the
first block at 1,670 against 1,269 ms.

**Gate.** On the Linux ARM64 VM, from `lampOS/`, on `cd815e905`:
`cargo fmt --all --check` clean; `cargo test --locked -p lamp-voice-eval`
284 passed; `cargo test --locked -p lamp-voice` 955 passed (5 ignored), the
library and every test target but `capture_hold_worker`, which does not
compile on `lamp-voice` itself ("Questions", 1); `cargo build --locked
--workspace` builds. Clippy for `lamp-voice` and `lamp-voice-eval` lists
only the older `lamp-body` warning and that same test; with `-D warnings`
it is clean on the builder's Mac, where that test is not built. On the builder's Mac, the voice library passes test
by test, and in full parallel runs fails only on timing under load (motor
and ring timeouts, ENOBUFS; once one "Lamp, stop." session with
`ConnectionReset`, cause not found, diagnostics added): `origin/lamp-second`
fails the same motor and ring tests there in the same way.

**On the lamp:**

1. Build, deploy, add `--early-end-ms 200` to `ExecStart`.
2. `pauses.sh`: an `early_end` record on each line; "Hey Lamp," with a
   450 ms pause gives one `early_end_given_up` and is answered; a bare
   "Lamp," gives none; 650 and 900 ms pauses behave as before; the first
   sound about 0.4 s sooner.
3. `shorts.sh`: the same words stop the lamp as without the option, and
   the `not_speech` counts are unchanged.
4. `checks.sh`: all pass; give-ups only at pauses of 0.2 to 0.6 s.
5. Watch the journal for `BarrierTimeout`.

**Where it is most likely to be wrong on the lamp:** how the real service
treats a request committed and given up 0.3 s in, and what the model makes
of the rest. And a first piece the service never answers ends the link
(`BarrierTimeout`) unless `provider.json` sets `silent_response:
assume_over` with `no_reply_after_ms`; whether the lamp's file does is not
known here.

**`crates/gemini`:** nothing needed. One possible speed-up: if the real
service confirms an interruption sent before any output, `retire_active`'s
kept start could be released at once, and short continuations would be
answered sooner.

**Not shown:** anything on the lamp or with the real service; an answer
that arrives and completes whole before confirming is still written to the
memory journal (with the lamp's 600 ms wait and no end-of-turn model that
cannot happen).

### Task 20: the replay takes the lamp's own recordings (2026-10-14)

**Built** (`crates/audio/src/replay.rs`; `docs/steady-reference.md`, "The
replay takes the lamp's recordings"):

- `lamp-audio replay-aec` follows a capture-clocked recording through. It
  rebuilds the voice's ring as `ReferenceAssembly` does: the lead and the
  prime at each start or reset; before each capture block, every frame
  accepted by then (`reference.accepted_through`); one block taken; the
  rate for the next from the fill and the frames owed since the last packet
  (`analysed_at_us` less `accepted_at_us`, at most 20 ms). After every
  block its cursor must equal the lamp's (`reference.analysed_through`), so
  the canceller gets the lamp's blocks in the lamp's order.
- It knows the mode from `run_start` in `events.jsonl` (`reference_clock`),
  or, for a session too old to say, from the first capture record's
  cursor; `report.json` says which and how.
- Where it cannot follow, it stops at that capture record, names it and
  why in `limitations`, and keeps and writes out everything before it.
- `events.jsonl` is read a line at a time (up to 4 GiB, which a long serve
  session may need; it was read whole under 32 MiB before): a cut-off last
  line is passed over and reported (`events_cut_off`); a line over 64 KiB
  is counted (`events_long_lines`); any other bad line is an error naming
  its number.
- Without an option the cancellers get the reference the recorded voice
  used (`echo_reference` in `run_start`); `--steady-reference` or the new
  `--as-given-reference` chooses.

**Tested** in `crates/audio/tests/replay.rs` on recordings made in the
tests as the voice makes them: a capture-clocked one of 300 blocks (the
speaker 1,000 ppm fast, every seventh packet late, so the rate moves)
replays whole, `aec_only_mismatched_samples_vs_recorded` 0; the same
without `run_start`, known by its cursor, exact; a reset in the middle,
two segments, both exact; events cut off mid-line, read and reported; a
cursor moved by one frame at the 61st record, stopped there, the record
named, the 60 blocks before kept and exact. The 12 older replay tests pass
unchanged. Gate on the Linux ARM64 VM, on `cb4d24e61`: `cargo fmt --all --check` clean;
`cargo clippy --locked -p lamp-audio --all-targets -- -D warnings` clean;
`cargo test --locked`: `lamp-audio` 178 passed (2 ignored), `lamp-voice-eval`
284 passed, `lamp-voice` 935 passed in its library and every test target
but one; `cargo build --locked --workspace` builds. The one is
`crates/voice/tests/capture_hold_worker.rs`, which does not compile on
`lamp-voice` itself (`1766259f3`): line 161 calls `capture_on` with nine
arguments, and it takes ten since the steady reference was wired
(`--echo-reference`). This change touches nothing in `crates/voice`; see
"Questions".

**On the lamp** (any recording made with the audio diagnostics):

```sh
lamp-audio replay-aec RUN artifacts/replay-as-recorded
lamp-audio replay-aec RUN artifacts/replay-steady --steady-reference
```

The first should end `replay_complete_for_recorded_capture: true` with
`aec_only_mismatched_samples_vs_recorded: 0` in every segment (the lamp
runs without noise suppression). If so, compare the second's
`lamp_sounding_internal_alignment_ms` and `…_changes` with the first's,
and listen to `segment-*-aec_only.wav` at the words played over the lamp.

**Where it is most likely to be wrong on the lamp:** a kind of record the
lamp's recordings have and the made ones do not. The made recordings
follow the recorder's format as `replay.rs` checks it, not the voice's
recorder run against scripted devices (no test in `crates/voice` does that
in a capture-clocked session, and that crate is not this task's).
Candidates: recovery (`--reference-recovery`, off on the lamp but its
reseeds would move the ring), a privacy close and reopen mid-run, or the
fourth recording's events failing for a reason other than a cut-off or
long line (the message now names the line). The first mismatch's
`limitations` line names the record.

**Asked for:** two of the lamp's capture-clocked recordings with played
voices only (for example `cut-a` and the run of words into quiet), as a
`.tar` attached to a draft release of this repository, or wherever is
easiest; say where in "Answers". With them I will make the made
recordings' shape match whatever the lamp's have.

### Task 10: a reference that follows the microphone's clock (2026-10-14)

**Built.** `lamp_audio::steady::SteadyReference`, off unless asked for:
`EchoProcessor::with_reference(noise_suppression, Reference::Steady)`
(`EchoProcessor::new` is unchanged), and `lamp-audio replay-aec RUN OUT
--steady-reference`. A second echo-evidence observer follows where the
echo is from the sound itself; the reference is delayed, through `drift`'s
interpolator and at most 2 ms a block, so that the canceller finds the echo
20 ms after it and keeps finding it there. Everything is in
`docs/steady-reference.md`.

**Why it follows the sound and not a buffer.** Read from the code: in the
capture-clocked mode the lamp runs, a jump in the speaker's queue goes into
the ring too and cannot step the echo; the recorded steps of 5.4 ms back
and forth are most likely the second microphone's own buffering, which the
lamp cannot read; and the delay hint the voice passes leaves the speaker's
queue out in that mode (the queue less the ring's frames is always below
zero and floored), so the hint is the capture delay alone.

**The rig** (`crates/audio/tests/steady_reference.rs`): speech-like sound,
the speaker 98 ppm fast, the microphone 200 ppm slow, the delay stepping as
recorded, a person over the lamp, and the voice's capture-clocked ring as
`ReferenceAssembly` drives it. **The two measures**, before → after, with a
moving echo (M2 Max and the Linux ARM64 VM, identical figures):

| Person against the echo | Echo removed | Person kept over the lamp | Echo within 1 ms of its place |
|---|---|---|---|
| level | 46.5 → 44.6 dB | −5.9 → −3.7 dB | 7% → 76% |
| 15 dB under | 45.3 → 43.3 dB | −8.5 → −7.1 dB | 7% → 81% |
| level, half the echo not linear | 43.9 → 40.4 dB | −5.6 → −3.7 dB | 7% → 71% |

A still echo costs a person 3.8 dB; the moving one 5.9; steadied, 3.7. The
canceller's own estimate goes from 28–84 ms to 0–20 ms. The rig shows the
direction and size, not the lamp's figures: on the rig the canceller copes
with a moving echo better than on the lamp. Cost of the stage: 6 µs a 10 ms
block on the M2 Max, 12 µs on the VM (99th percentile 24 and 52 µs).

**Tested** on the Linux ARM64 VM, from `lampOS/`, on `5d8093a8e` rebased on
`lamp-second`: `cargo fmt --all --check` clean;
`cargo clippy --locked -p lamp-audio --all-targets -- -D warnings` clean;
for `lamp-voice` and `lamp-voice-eval` the only clippy warning is the same
one in `lamp-body` as before (`run` in `crates/body/src/inspection.rs`
never used), in a crate this change does not touch; `cargo test --locked`:
`lamp-audio` 173 passed (2 ignored: the release-only measures and an older
one), `lamp-voice` 937 passed, `lamp-voice-eval` every suite passed, no
failures; `cargo build --locked --workspace` builds. The measures above
come from `cargo test --release -p lamp-audio --test steady_reference --
--include-ignored --nocapture` on both machines.

**On the lamp:** first the replay, twice, over the recordings of
2026-10-14:

```sh
lamp-audio replay-aec RUN artifacts/replay-given
lamp-audio replay-aec RUN artifacts/replay-steady --steady-reference
```

Compare, per segment of `report.json`, `lamp_sounding_internal_alignment_ms`
and `..._changes` (the canceller's estimate: it should hold within a few
milliseconds and change rarely with the option, against 80 to 128 ms as
given), and listen to `segment-*-aec_only.wav` at the words played over the
lamp. `operations.jsonl` has `steady_reference.echo_ms` for every block: it
should step by about 5.4 ms where the recording's echo stepped. If it does,
wire it: where the capture worker makes `EchoProcessor::new(…)`, make it
`EchoProcessor::with_reference(…, Reference::Steady)` behind a voice option.

**Where it is most likely to be wrong on the lamp:** the follower. It
needs a linear fit at one alignment to explain at least half of the
microphone's energy, and on the second microphone little of the echo is a
linear copy; where it explains less, the reference keeps its last delay.
And a match one pitch period of Gemini's voice away may be held for a while
(about a second per step on the rig). `echo_ms` in the replay shows both.

**Not shown:** the lamp, its second microphone's chip, a person, whether
2 dB is heard, the cost on the lamp's processor, the stage inside a running
voice. The causes of the movement are read from the code, not measured.

### Task 9: faster answers, what exists (2026-10-14)

A study, `docs/faster-answers.md`. Nothing built, nothing run on a lamp,
no paid service called. Every outside figure is marked "published, not
measured here" with its source and boundary.

**What it found.** The lamp's speech service (`gemini-3.8-live`) is
published at 1.18 s from a question to its first sound; the lamp sees
1.15 s from the end it sends. So the relay and the road from Ho Chi Minh
City are probably a small part of the wait, and no setting brings this
service near 0.3 to 0.5 s. What the lamp can take off is its own: the
0.6 s it waits to be sure the person has finished, and part of the 0.35 s
it holds the first sound. That gives about 1.5 s. Near 1 s also needs a
faster service, which today means another voice, against the owner's
decision.

**The ways, in short:** (1) end a plainly finished turn sooner: task 1,
take first; (2) send the end at about 0.2 s of quiet and play only once
sure: up to 0.4 s on every answer, code in the voice, behind a switch;
(3) a shorter hold before the first sound: about 0.1 s at 200 ms; (4) the
same service without the relay: unknown, probably small, needs a key;
(5) another Gemini Live model or an Asian endpoint: up to 0.5 s by
published figures, but older, preview or retiring models; (6) another
speech-in, speech-out service: a new voice, no; (7) speech to text, a
text model and a voice: not shown faster from Vietnam; (8) shipped
agents' own figures: a yardstick only (1.0 to 1.7 s median measured from
the user's side).

**For the first builder, in the study's order of trials:** first split
the 1,034 recorded answers into wait, service and hold, and time the road
from the lamp (it changes nothing on the lamp); then the fast end-of-turn
model; then a 200 ms hold; then the end sent sooner.

**Not established:** no round trip from the lamp is recorded anywhere;
where the relay's own server is; where Google serves an AI Studio key;
whether Gemini's text-to-speech "Kore" sounds like the Live voice; the
company's bill through the relay.

### Task 1: the end-of-turn model in a tenth of a second (2026-10-14)

**Built.** `lamp-turn-end --fast`: the same pinned model
(`smart-turn-v3.2-gpu.onnx`, unchanged), made 8-bit in memory as the worker
starts and run by ONNX Runtime 1.30.0 on two threads, the worker kept on
the cores with the largest `cpu_capacity` (cpu6 and cpu7 on the A733).
Without `--fast` the worker is what it was, byte for byte in its answers.
Also `lamp-turn-end bench` (the timing the task asked for),
`lamp-turn-end agree` (fast against exact on labelled clips),
`lamp-turn-end --check-model PATH --fast`, and
`WorkerCommand::smart_turn_fast` in `crates/turn-end-wire` for the voice
to start it with. Nothing in the voice uses it. Everything is in
`docs/endpoint-pilot.md`, "The fast model": what was tried and why each
was or was not taken, the measurements, the agreement tables, the cost of
the native library, and what was not shown.

**What it showed, on the Linux ARM64 VM** (M2 Max host; not the lamp):

| | Today (exact) | Fast |
|---|---|---|
| Verdict, features and model, median of 24 warm runs | 87.0 ms | 29.5–30.3 ms (three runs) |
| Worst of the 24 | 92.8 ms | 31.3–33.2 ms |
| Peak address space | 114 MiB | 243.5 MiB (cap 544 MiB) |
| Processor time per verdict | 88 ms | 59–61 ms, two threads |
| Processor time over 3 s idle | | 0 ms |

Held-out human clips (1,167, shards 4 and 5 of Pipecat's test set), asked
200 ms into a pause as the voice asks: 3 verdicts on the other side of 0.5,
18 in another of the voice's bands (5 newly "finished"), 95.54% correct
against 95.63%. The one multiplication kept in floats was chosen on the
development shards 0 and 1, not on these. By the exact model's measured
ratio (383 ms on lamp-4ace's fast cores against 87 ms here) the fast model
would take about 130 ms on the lamp. That is an estimate, not a
measurement.

**Two findings beside it:**

- tract 0.23.8 built in a Debian bookworm container has no fast 8-bit
  kernel: its build script's assembler probe (`.cpu
  generic+fp+simd+dotprod`) fails on binutils 2.40 and clang 14 to 16, so
  the SDOT kernel is left out without a warning. It does not touch any
  float model the lamp runs today, but anyone who tries 8-bit under tract
  will find it no faster. `.arch armv8.2-a+dotprod` assembles everywhere.
- Asked 100 ms into a pause instead of 200 ms, today's model is no worse
  at what ends a turn early: finished turns judged finished 91.5% against
  90.6%, unfinished ones judged finished 3.3% against 4.0% (held-out clips,
  digital replay). With the fast model that would put a verdict near
  230 ms. That is `ConfirmPolicy` in `crates/voice/src/activity.rs`, yours.

**Tested** on the Linux ARM64 VM, from `lampOS/`, on `1968ba608` rebased on
`lamp-voice` `40e9b1c00`: `cargo fmt --all --check` clean; `cargo clippy
--locked -p lamp-turn-end -p lamp-turn-end-wire --all-targets -- -D
warnings` clean; for `-p lamp-voice` and `-p lamp-voice-eval` clippy stops
on one warning in `lamp-body` (`run` in `crates/body/src/inspection.rs` is
never used), a crate this change does not touch, and lists no other
warning; `cargo test --locked` for `lamp-turn-end` (16 + 7 + 1 + 5, model
tests ignored), `lamp-turn-end-wire` (4 + 12), `lamp-voice-eval` (all
suites) and `lamp-voice` (744 in the library and every integration suite):
no failure; `cargo build --locked --workspace` builds. With the model and
the library present, all 4 tests of `tests/fast.rs` and all 11 of
`tests/parity.rs` pass on the VM, and the 4 of `tests/fast.rs` on macOS.

**On the lamp, as the service user, in the release directory:**

```sh
curl -fsSLO https://github.com/microsoft/onnxruntime/releases/download/v1.30.0/onnxruntime-linux-aarch64-1.30.0.tgz
tar -xzf onnxruntime-linux-aarch64-1.30.0.tgz
cp onnxruntime-linux-aarch64-1.30.0/lib/libonnxruntime.so.1.30.0 .
./lamp-turn-end bench smart-turn-v3.2-gpu.onnx 30                # as it is
./lamp-turn-end bench smart-turn-v3.2-gpu.onnx 30 --cores fast   # held to the fast cores
./lamp-turn-end bench smart-turn-v3.2-gpu.onnx 30 --fast         # the fast model
./lamp-turn-end --check-model smart-turn-v3.2-gpu.onnx --fast
```

It should show `"cores":[6,7]` on the fast runs, the second run's
`total_ms` median near 383 ms, the third's under 150 ms, and the check
exiting 0. To use it: in `crates/voice/src/coordinator/endpoint.rs`
(`EndpointRig::start`), `WorkerCommand::smart_turn_fast` in place of
`WorkerCommand::smart_turn`, for example behind an `--endpoint
smart-turn-fast` beside `smart-turn`.

**Where it is most likely to be wrong on the lamp:** the time. The
130 ms is a ratio carried over from the float model, and an 8-bit
multiplication may lose more on the A76 (half the M2's vector units) than
a float one does. If `bench --fast` is over 150 ms, try `--threads 1`
too: `lamp-name` uses the same two cores every 80 ms, and two threads can
wait on each other. Next most likely: the library not loading on the
lamp's system (it needs glibc 2.28 and `GLIBCXX_3.4.22`; the check says
so at once), and no `cpu_capacity` in the lamp's kernel (the worker then
falls back to the highest frequency; `"cores"` shows what it chose).

**Not shown:** anything on the lamp; speech through its microphone; the
fast model inside a full voice session; whether a 300 or 400 ms end suits
people.

### Task 2: try to break last night's changes to the voice (2026-10-14)

On its own branch only, as the task says: `second/voice-review`, one
commit `1d737f8e5` on `e93a828d1`. It adds tests and
`docs/voice-review-2026-10-14.md`; the one line outside a test file is
`mod review_2026_10_14;` at the end of the worker's test module in
`crates/voice/src/provider_worker.rs`. Nothing was fixed.

**Faults, each shown by a test that fails today, most serious first:**

1. **Speaks when it should not.** The note sent while someone is speaking
   ("your own ears caught your name, Lamp, just now") stays with a request
   that is then given up (judged a sound, or the microphone closed): no
   `activityEnd` is written, so the next request, by anyone and however
   much later, reaches the model with the note in front of it. Fails both
   with `kept-start` and without.
   `crates/gemini/tests/review_2026_10_14.rs`:
   `review_a_note_for_a_request_given_up_is_not_carried_into_the_next_request`.
2. **Speaks when it should not.** A name heard after a request's sound has
   ended, but before its first output, lets that request's held answer
   out (a busy room: "Hey man…" just after someone else's line).
   `crates/voice/src/provider_worker/tests/review_2026_10_14.rs`:
   `review_a_name_heard_after_a_requests_sound_ended_does_not_let_out_its_answer`.
3. **Speaks when it should not.** The second look's own written turn
   counts as one of a new session's two protected requests, so the next
   line of other people's talk is not held.
   `crates/voice/tests/review_2026_10_14.rs`:
   `review_the_lamps_own_second_look_does_not_use_up_a_new_sessions_held_requests`.
4. **Silent to a caller, narrow.** Before a request begins, the second
   look keeps only the newest hearing, so a later faint one (0.91)
   replaces a plain one (1.0).
   `review_a_plain_hearing_just_before_a_request_is_not_lost_to_a_fainter_one_after_it`
   (same file).
5. **No effect on behaviour.** `crates/name/src/said.rs` can publish a
   hearing scored above 1, which `NameHeard::check` refuses.
   `crates/name/tests/review_2026_10_14.rs`:
   `review_a_hearing_is_never_said_with_a_score_over_one`.

**Seen by reading, not shown by a test** (the second look is switched on
only from the environment, so no test can turn it on without a seam that
was not added): an answer that opens with a spoken "..." ends as
`nothing_said` and never reaches `second_look.finished`, so that call gets
no second look; the second look counts a hearing until the turn is over and
can bring a held answer back out; it can go to a renewed session that
never heard the call; `lamp-name` dates a hearing 0.8 s early after a gap;
the worker ignores a failure to subscribe to `name/heard` without a word.

**Found sound, with six passing tests:** `try_note` cannot end the link;
the second look is taken once and does not loop; the note goes out once
per request; a name counts only for its own request. No path was found
that answers twice or loses the link.

**Tested on** an Apple M2 Max laptop (macOS, Rust 1.96.0), from `lampOS/`:
`cargo fmt --all --check` clean; `cargo clippy -p lamp-voice -p
lamp-gemini -p lamp-name --all-targets -- -D warnings` clean; `cargo build
--workspace` builds; `cargo test -p lamp-gemini` and `-p lamp-name` fail
only on their review test; `cargo test -p lamp-voice`: library 726 passed
and 5 failed, `tests/review_2026_10_14` 3 passed and 2 failed. Of the
library's 5, one is the review test; the other 4 are older
`coordinator::motor` tests that time out after 2 s when the whole library
runs at once on this laptop (21 of them in another full run, and the same
21 on `origin/lamp-voice` without the review line); one at a time, all 46
in those groups pass. Nothing was run on the lamp.

**For the first builder:** the five failing tests are the work list, in
that order; each passes once its fault is mended. Most likely to matter on
the lamp: the first, because a "Hey Lamp" said alone and judged a sound is
an everyday event there.

## Questions

1. **Answered 2026-10-14 (mended in `46403ffbd`).** **2026-10-14, task 20.** `cargo test -p lamp-voice` does not compile on
   `lamp-voice` at `d83834cb2`: `crates/voice/tests/capture_hold_worker.rs`
   line 161 calls `capture_on` with nine arguments; since the steady
   reference was wired it takes ten (the echo reference). Adding
   `lamp_audio::Reference::AsGiven` there is likely all it needs;
   `crates/voice` is yours, so it is left to you. That test's harness runs
   the real capture worker against scripted devices in a capture-clocked
   session; given a diagnostics directory (its sixth argument, `None`
   now) it would make exactly the kind of recording task 20 replays, from
   the voice's own recorder, and a test there could replay it.
2. **Answered 2026-10-14 (not needed: the replay was run on the lamp's own).** **2026-10-14, task 20.** Two of the lamp's capture-clocked recordings,
   with played voices only, to check the replay against: where can I
   fetch them? (A `.tar` on a draft release of this repository would do.)
