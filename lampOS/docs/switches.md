# Switches

Every setting that turns a behaviour on in `lamp-voice`, `lamp-body`, `lamp-eyes`,
`lamp-expression`, `lamp-skills`, `lamp-tasks` and `lamp-controls`: what it does,
what lamp-4ace and the product set, the pull request that added it with the lamp
check it named, and the product default proposed for it. Made for the owner's
question of 17 October, "why off?", and for switching lamp-4ace on group by
group: each switch that passes its check becomes the default in code before
1 November (the first builder, #705, 07:30).

**As of** `lamp-voice` `cfbd82ae0` (17 October 2026, about 07:30 +07). The code is
the source of truth; this page is a census of it, to be kept up as switches are
added, turned on or removed.

## How to read it

- **Values and default:** what the code accepts, and what happens when the
  setting is absent. Almost every switch is off unless set.
- **lamp-4ace (repo unit)** and **Product unit:** what
  `lampOS/deploy/lamp-4ace/` and `lampOS/deploy/product/` set, reading only
  lines that aren't commented out. lamp-4ace may also run drop-ins that aren't
  in the repo (see "What the census found").
- **Added by:** the merged pull request that brought the name in, or the commit
  for the ones added to `lamp-voice` directly before pull requests were the rule.
- **Lamp check it named:** what that pull request or its doc said must be seen
  on a lamp, and whether a doc records it **shown**. Where nothing is recorded,
  it says so; no lamp result here is assumed.
- **Proposed product default:** "on (already)" where both units already set it;
  "on" where the evidence is in and it should be switched on; "off until …"
  naming the check that would turn it on; "off" for a diagnostic or a switch
  turned down.
- Paths, sockets and test hooks (`LAMP_*_SOCKET`, `LAMP_*_DIR`, `LAMP_*_FILE`,
  `LAMP_*_TEST_*`, `LAMP_BUS`, `LAMP_BUS_ROOT`, `LAMP_CLOCK`) and tuning numbers
  (speeds, poses, reaches) are left out: they configure, they don't turn a
  behaviour on.

## The count

98 switches:

| | Voice: hearing, turns, ring | Voice: tasks, memory, schedule, skills, looks | Voice: its own flags | Other parts | All |
|---|---|---|---|---|---|
| On already, or right at its default | 12 | 9 | 7 | 16 | **44** |
| **Proposed to turn on now** | 1 | 2 | 0 | 0 | **3** |
| Off until its named lamp check | 20 | 12 | 0 | 5 | **37** |
| Stays off (diagnostic, turned down, or not needed) | 5 | 1 | 2 | 3 | **11** |
| A setting, not a behaviour | 1 | 2 | 0 | 0 | **3** |

**The three proposed to turn on now**, each with a recorded lamp-4ace result:
- `LAMP_SPOKEN_TO=on`: shown on lamp-4ace 16 Oct 04:38 (the budget line
  answered 0 of 8, against 11 of 12 before; named calls 6 of 6).
- `LAMP_TASKS_WHO=on`: shown on lamp-4ace 15 Oct, and nothing starts without a
  heard yes.
- `LAMP_POLICY_HOLDS_ACTIONS=on`, **with** `lamp-policy --act attention` in the
  product's policy unit: played on lamp-4ace 16 Oct 06:51 (nothing ran unnamed;
  a named "turn it up" ran in 2 ms). Without `--act attention` it does nothing.

**The quickest wins among "off until"** are the switches whose check needs no
motion, since lamp-4ace's motors stay still until Monday: `LAMP_STAFF_NOTE`
(the in-time/late count), `LAMP_SHORT_SOUNDS=quiet-room-checked`,
`LAMP_ASKER_FIRST=record`, `LAMP_STOP_TOLD`, `LAMP_WAIT_KEEPS_ANSWER`,
`LAMP_TASK_NEWS_AT_PAUSE`, `lamp-eyes --leave-still-faces` (one live night) and
`lamp-expression --offline-face`.

## The voice: hearing, turns, the ring and the backup voice

| Switch | Values and default | lamp-4ace (repo unit) | Product unit | Added by | Lamp check it named | Proposed product default | Why (one line) |
|---|---|---|---|---|---|---|---|
| LAMP_ADDRESSED | `on`; off. Judges each request "said to the lamp from close"; needs LAMP_ATTENTION and LAMP_BUS | — | — | #792 | "Most likely wrong on lamp-4ace": sideways seat (#763), fewer than two looks; attention.md: "not on any lamp" | off until played on a lamp facing its seat | #792 names its likely faults and gives no lamp result; attention.md says not on any lamp |
| LAMP_ASKER_FIRST | `on`/`record`; off. Softer speech waits up to 5 s while the asker's answer is awaited | — | — | #1092 | Plan: `record` on lamp-4ace first to set the 6 dB margin; group-talk.md: "6 dB is not yet measured" | off until a `record` run on lamp-4ace sets the margin | #1092 and group-talk.md say the margin is unmeasured; `record` is the step named |
| LAMP_ATTENTION | `on`/`busy`/`busy-near`; off. Busy-room rule holding answers not for the lamp | busy-near | busy-near | commit 01fc900cd (direct) | attention.md: busy-near on lamp-4ace since 18:46 on 14 Oct (build 41d09542f), 19.5–26.5 dB at desk | busy-near (already) | Shown on lamp-4ace 14 Oct (attention.md "A name heard close to the lamp"); many later switches need it |
| LAMP_BUS_AUDIO | `on`; off. Puts the voice mic's audio on the bus (needs LAMP_BUS) | on | on | commit 6e1392926 (direct) | none named; name.md and selftest.md need it for lamp-name and the mic check | on (already) | lamp-name (in product) hears the name only from this topic (name.md) |
| LAMP_CUT_KEEPS_NAME | `on`; off. A request continuing an early-ended one keeps its name/call | — | — | #809 | "Most likely wrong": second voice inside a pause; the run at 400/550 ms; attention.md: "not on any lamp" | off until played on lamp-4ace | #809 waits on a played run; #918's 15 Oct records show 2 of 4 unsigned plays were such cuts |
| LAMP_EXPRESSION_BODY | `on`; never otherwise. Body adds Expression's requests to its pose (lamp-body) | on (lamp-body) | on (lamp-body) | commit fc93f8608 (direct) | expression.md: used on lamp-4ace since 2026-10-14 10:05 | on (already) | Running on lamp-4ace since 14 Oct (expression.md); the lean and rest glance need it |
| LAMP_EXPRESSION_OUT | `on`; nothing otherwise. Voice publishes voice/timing, activity, playout for Expression | on | on | commit 40e9b1c00 (direct) | expression.md: playout said ahead since 14 Oct 13:00, measured on lamp-4ace | on (already) | Expression's faces depend on it; in use on lamp-4ace since 14 Oct |
| LAMP_FAINT_FLOOR_MAX_DB | dBFS -47 to -20; unset = floor fixed at --faint-floor-db (-28) | — | — | commit 96bc22266 (direct) | lamp-4ace RUNBOOK: "off (watching)"; `room_floor` / `room_alone_dbfs` records to be watched first | unset until lamp-4ace's room_floor records support a ceiling | RUNBOOK lists it off (watching); group-talk.md says watch a room before letting the floor go |
| LAMP_FAINT_HOLD_MS | ms 0–1000; unset/0 = each block judged alone. Keeps trailing desk speech | 400 | 400 | commit de6ae60ed (direct) | not-speech.md: shown on lamp-4ace at 400 ms, 0 of 7 short phrases thrown out | 400 (already) | not-speech.md result on lamp-4ace; quiet lines at a tenth of the volume still not admitted |
| LAMP_FIRST_WORDS | `kept`; off. A yield's request starts at first words under the lamp's voice (≤1.6 s) | — | — | #936 | "Not shown: nothing was played"; a played minute of cut-ins (cutins.sh) and stories asked over | off until the played cut-in minute | #936 measured only on replayed recordings; no lamp run in PR or docs |
| LAMP_HEAD_TAP_CUE_DBFS | dBFS -60 to -20, else start refused; default -35. Level of the tap "hm?" | — | — | #849 | Hum level by ear on a lamp (#849 "Next"); attention.md: -35 "not yet measured" | unset (-35) until tuned by ear on a lamp | For tuning only; default untested against the lamp's speech (attention.md) |
| LAMP_HEAD_TOUCH | `on`; off. Head tap is a call / stops speech; hold 3 s = sleep | — | — | #849 | Tap-to-speaker under 150 ms, hum by ear, head_gap_ms recording; attention.md "not on any lamp" | off until tap latency is measured on a lamp | #849 says lamp latency unmeasured; #1176 keeps product pats in the old engine |
| LAMP_KERNEL_CLOCK | `on`; off. Says the clock is not set rather than tell a wrong time | — | on | #884 | offline.md "What was not shown": nothing run with network cut; NTPSynchronized on lamp-4ace not checked | on (already) | No battery clock (#830); only changes anything while the clock is unset |
| LAMP_MARK_WEIRD | `on`; off. "That was weird" keeps the last minute for review, 7 days | — | — | #855 | Shown on lamp-52e6 15 Oct (mark written, listed); 7-day sweep host only | off | A builder review tool that keeps session records on the lamp; #855 itself is a builder check |
| LAMP_NAME_IN_PASSING | `ignored`; off. "the lamp" spoken about is no call in a busy room | — | — | #864 | #864: "Nothing was run on a lamp"; attention.md: not on any lamp | off until played on lamp-4ace with LAMP_SPOKEN_TO | Model declined both played cases itself (14, 15 Oct); #918 run still answered 1 of 2 in-passing lines |
| LAMP_NAME_NOTE | `on`; never otherwise. Tells the model the lamp's name was heard | on | on | commit 7a07ebd9e (direct) | attention.md: played on lamp-4ace 14 Oct 07:28, note arrives and does no harm | on (already) | Shown on lamp-4ace 14 Oct (attention.md table) |
| LAMP_NAME_NOTE_BUSY | `face`; off. In a busy room the name note waits for a turned face | face | face | commit 0ca828c67 (direct) | attention.md: on lamp-4ace since 11:52 14 Oct | face (already) | From the 14 Oct meeting: 4 false name hearings in 20 min made the lamp answer the table |
| LAMP_NAME_SIGNAL | `on`; off. Measurement: model signals hearing its name | — | — | commit aabb3ac39 (direct) | attention.md: tried, heard 6 of 7 calls but answered 1 of 7; "the switch is off" | off | A measurement switch attention.md turned down: neither function kind was acceptable |
| LAMP_OFFLINE_VOICE | `on`; off. Offline line once per outage, amber breath, "back online" | — | — | #884 | offline.md: not run with network cut; silent until recordings via --failure-notice-dir (none in units) | off until the recordings exist and an outage is played | No unit provisions --failure-notice-dir, so it would be silent anyway; offline.md shows nothing run |
| LAMP_OTHER_LANGUAGE | `sorry`; off. Other-script/number-only turns in a conversation are asked again | — | — | #930 | lamp-4ace 06:42 cherry-pick: no wrong answer in 10 cut-ins, but no "Sorry?" followed | off until a replay shows "Sorry?" said | #930 left why no "Sorry?" came for the next replay's trace |
| LAMP_PRIVACY_CONTROLS | `on` exactly (Controls, Voice, Ears together); off. Hardware slide closes both mics | — | — | #795 | privacy-switch.md handoff steps (off minute, slide mute LOW/HIGH, ring red); "no lamp was used" | off until the first builder's lamp handoff passes | #795: polarity, ALSA device, 50 ms budget unshown; promotion is the first builder's call |
| LAMP_RESTORED_MUTE | `on`; off. Tells the restart guard's mute apart; head tap reopens it | — | — | #997 | kill -9 while listening, then one head tap opens it; a held mute stays; not documented as shown | off until that check, together with LAMP_HEAD_TOUCH | Needs LAMP_HEAD_TOUCH (controls.md); fixes the 16 Oct 12:20 deaf lamp seen on lamp-4ace |
| LAMP_RING_ASLEEP | `on`; off. Faint warm breathing ring while muted/asleep, not dark | — | — | #1025 | Goes on with the stop fix (#1024), sleep by a hand on the head, photographed; no result in docs | off until the photographed sleep check | Rule-8 friendly (a dark ring reads as broken) but no lamp result recorded |
| LAMP_RING_EYE | `on`/`strong`; off. Ring looks toward the person and blinks at rest | on | on | commit d3e9cc3d2 (direct) | ring-choreography.md: lean "not yet seen on a lamp"; the owner found it too subtle 15 Oct, hence `strong` | on (already) | Running on lamp-4ace; `strong` exists but is in no repo unit |
| LAMP_SAY | `on`; off. Say socket for other parts (skills) to speak through the voice | on | on | #954 | #954 steps: begun/spoken, dropped on speech-over, expired under pause; not recorded as shown | on (already) | Welcome back (lamp-skills, in product) speaks only through it (install.md) |
| LAMP_SAY_GROUP | a group name; unset = socket private to the voice's user | orangepi | lampos | #954 | Same as LAMP_SAY (socket as `orangepi` on lamp-4ace) | lampos (already) | lamp-skills runs in group lampos in product; orangepi is lamp-4ace's user |
| LAMP_SAY_SOUNDS | absolute dir; unset = /var/lib/lampOS/skills. Skill sounds only from under it | — | — | #1144 | #1144: "nothing has run on a lamp, and no sound has been heard" | unset (default matches install's skills folder) | The default is the product's skills folder; a path, not a behaviour switch |
| LAMP_SECOND_DOWN_LINE | `on`; off. Backup voice says "I can't think right now" when chat never starts | — | — | #1195 | none named (from the relay outage at 02:29, seen on lamp-52e6) | off until heard on a lamp with the relay's chat down | #1195 has host tests only; said unasked it must not read as a broken machine (second-voice.md) |
| LAMP_SECOND_LOOK | `on`; nothing otherwise. Tells model its name was heard after it stayed silent | on | on | commit 407c55f26 (direct) | none named; no lamp result for a second look found in attention.md | on (already) | Already in both units; one turn at most, only to the session that heard it |
| LAMP_SECOND_WAIT_CUE | `on`; off. Backup voice makes one soft "Hm." if its first round is slow | — | — | #1086 | "Draft until lamp-52e6": five questions with Gemini down, how often/when the sound played | off until that lamp-52e6 run | No result of the named run in the PR or second-voice.md (which notes 9 of 10 turns waited past 1.5 s on lamp-4ace) |
| LAMP_SHORT_SOUNDS | `quiet-room`/`quiet-room-checked`; off. Short sounds still go to the model in a quiet room | quiet-room | quiet-room | commit 3e3de2d7a (direct) | not-speech.md: 13 of 36 short words lost on lamp-4ace 14 Oct; #800: a room sound answered under quiet-room | quiet-room-checked once played (quiet-room meanwhile) | not-speech.md #800 shows quiet-room answers room sounds; the checked form exists but has no lamp result |
| LAMP_SPEAK_FIRST | `on`; otherwise moments only recorded. The voice's own hello on arrival | — (commented out) | — (commented out) | commit 4985caf77 (direct) | RUNBOOK: on since 15 Oct, never due in 0 of 680 sessions; now replaced by Welcome back | off | Both units leave it commented so only the Welcome back skill greets (install.md, skills.md) |
| LAMP_SPOKEN_TO | `on`; off. In every room an answer plays only when the lamp was spoken to | — | — | #918 | Shown on lamp-4ace 16 Oct 04:38: budget line 0 of 8 (was 11 of 12), named 6 of 6, day.sh 12 of 12 | on | Strongest lamp evidence of the group; answering talk between people is not lifelike (rule 8); cost: "Lamp, …" not facing |
| LAMP_STALLS | `on`; off. Logs loop stalls ≥50 ms with getrusage to /run/lampos-voice | — | — | #1179 | Next: on in lamp-4ace's drop-in, a scene across 00:00 (#1178) | off | A diagnostic for #1178, meant for lamp-4ace (serve-runtime.md) |
| LAMP_START_OFFLINE | `on`; off. Voice started without internet keeps listening past the 25 s budget | — | — | #884 | Stop Wi-Fi, restart the voice, stays up logging "listening with no link"; offline.md: not yet shown | off until the Wi-Fi-off restart check | offline.md: "Not yet shown on a host with its network cut or on a lamp" |
| LAMP_STOP_TOLD | `on`; off. First request after a hand/head stop tells the model it was stopped | — | — | #1148 | Code and not-speech.md: "off until it is played on lamp-4ace" | off until played on lamp-4ace | Its own doc says it waits on that lamp check (from 16 Oct 16:32, story restarted 4 of 6 times) |
| LAMP_STOP_WORDS | `heard`; off. Stop words heard in a yield end the reply (relay speech-to-text) | — | — | #970 | Count yield outcomes against played cut-ins; privacy accepted for lamp-4ace only (#828, 16 Oct) | off | stop-word.md: privacy "to be decided again before home lamps"; speech goes to the relay during yields |
| LAMP_VOICE_LINK | `on` (with bus); off. Publishes voice/link for lamp/online | — | — | #884 | lamp/online false 5 s after the link drops (with lamp-health --online on); not run | off until that check and lamp-health --online on in product | Only a signal; nothing in product reads it yet; offline.md shows nothing run |
| LAMP_WAIT_KEEPS_ANSWER | `on`; off. A short sound does not cut off an answer still to come | — | — | #1130 | lamp-4ace steps 1–4: cough within 2 s, real follow-up, #1123 count before/after; not documented done | off until those lamp-4ace steps | #1130 "Most likely wrong": the 1 s, and a short "No."/"Wait." let go |

## The voice: tasks, memory, schedule, skills and looks

| Switch | Values and default | lamp-4ace (repo unit) | Product unit | Added by | Lamp check it named | Proposed product default | Why (one line) |
|---|---|---|---|---|---|---|---|
| LAMP_AIR_FACTS | `on`; off unless set. Tells the model, once per 3 h, that the air has been stuffy for 10 min | on | on | commit 56f25ed9d (direct) | none named in a PR; air.md: on lamp-4ace since 14 Oct 18:46, first told 19:45 with a window suggestion (shown) | on (already) | air.md records it shown on lamp-4ace 14 Oct; one quiet remark, only while a person is already talking |
| LAMP_HELLO_ASKS_POLICY | `on`; off. A due voice hello waits up to 2 s for the Policy's `lamp-policy --hello` answer | — | — | #883 | Shadow run on lamp-4ace: read `hello_due`/`hello_policy` waited_ms after the 11:00 and 16:00 sessions; speaking-first.md: no real person has yet made a hello due (not shown) | off | Product hello is the Welcome back skill and the voice's own hello is off (install.md), so it is moot; neither policy unit has `--hello` |
| LAMP_LIGHT_REQUESTS | `on`; off. Ring takes lapsing light requests from skills, the person's light returning by itself | — | — | #993 | Publish a 10 s green `light/request`: ring eases green, back to the person's light, "make your light blue" ends it; no record of it shown | off until the #993 ring check is played (on both lamp-voice and lamp-skills) | Without it skills give the light back by hand (skills.md); #993 left the run to the first builder and no doc records it |
| LAMP_LOOK_FULL | `on`; off. "Let me look" asks the eyes for one larger settled still | — | — | #842 | lamp-52e6 run shown 15 Oct: 6/10 objects named vs 4/10; not shown: objects held by a person, head turning, lamp-4ace | off until played on lamp-4ace with a person and eyes `--full-look` in a unit | #842 "Not shown" names lamp-4ace; needs eyes `--full-look`, which no deploy unit sets |
| LAMP_LOOK_FULL_SIDE | integer 320 to 4096 px; unset = 1024 across / 64 KiB. Still's side up to 184 KiB, for handwriting | — | — | #987 | none named (PR body is only "Part of #941") | off (unset) until a handwriting look is shown | Only useful with LAMP_LOOK_FULL and a camera run large (eyes.md #941); no lamp result recorded |
| LAMP_MEMORY_DAYS | `on` only; off. Voice uses the Days memory client across days | on | on | #743 | Played scenes for the first builder (memory-days.md); "Switched on, lamp-4ace" 14 Oct 14:50 (shown), loud-office replay not counted | on (already) | memory-days.md records it switched on lamp-4ace 14 Oct; install.md takes lamp-4ace's set as the product's |
| LAMP_MEMORY_RULES | `on` only, with Days memory; off. Keeps a person's corrections as rules for later sessions | — | — | #822 | Shown on lamp-52e6 twice 15 Oct (rules kept, told, taken back by number); not shown: Policy reading rules, seventh rule on a lamp | off until its lamp-4ace minute | Shown only on lamp-52e6 with synthesized speech (#822); the Policy half of #813 not shown |
| LAMP_NAMES | `checked` only; off. Name kept only if said/spelled; one name per face | — | — | #1057 | #1057 "Not shown": a played name in a busy room, model following the note; "For lamp-4ace: LAMP_NAMES=checked" | off until played on lamp-4ace | Fixes a real 16 Oct lamp-4ace failure (two names for one face), but #1057 says nothing shown on a lamp yet |
| LAMP_ONBOARDING | `on` plus absolute LAMP_ONBOARDING_FILE; off. Walks a new person through seat, name, face, preferences | — | — | #858 | A lamp-52e6 run with a person: seconds per step, redone/skipped steps, framing, setup_step reliability; onboarding.md "Not shown" | off until the lamp-52e6 run with a person | onboarding.md and #858 both list the person run as not shown |
| LAMP_POLICY_HOLDS_ACTIONS | `on` only; off. Actions wait up to 2 s; not run on stay_quiet without the name | — | — | #908 | Busy-room lines without the name, then a named "turn it up"; played on lamp-4ace 16 Oct 06:51 (shown: nothing run unnamed, named ran 2 ms) | on, together with `lamp-policy --act attention` in the product policy unit | #908 played on lamp-4ace; attention.md says on lamp-4ace since 16 Oct; no effect without `--act attention`, absent from both repo policy units |
| LAMP_PROGRESS_NEWS | `on`; off. Voice tells a fresh Policy `say_now` progress step as news | — | — | #765 | tasks.md "Playing progress news on lamp-4ace": a few-minute task, expect `say_now` then one spoken sentence; PR says nothing run on a lamp | off until that play is done, with `lamp-policy --act progress` | #765 "nothing was run on a lamp"; tasks.md most likely wrong: the Policy may `wait` on everything |
| LAMP_ROOM_FACTS | `on` only; off. Model told when the room goes quiet or busy | on | on | commit f7598521b (direct) | none named in a PR; room_told.rs and attention.md cite lamp-4ace scenes 13 to 16 Oct | on (already) | On in both units; the rule behind lamp-4ace's busy-office fixes (attention.md) |
| LAMP_SCHEDULE_SPEAK | `on` or `skill` (waits up to 10 s for a skill's ask); off. Speaks due reminders | on | on | #747 | schedule.md task 3.1 lamp qualification (seven spoken cases); for `skill`, #1038's reminder-in-two-minutes with/without lamp-skills | on (already); `skill` off until #1038's lamp check | Installed on lamp-4ace (install.md); the `skill` value's 10 s wait is #1038's "most likely wrong", no lamp result recorded |
| LAMP_SCHEDULE_TOOLS | `on` only; off. Permits the six Schedule tools (voice and actions) | on (voice, actions) | on (voice, actions) | #747 | schedule.md task 3.1 lamp qualification: off-state minute, scheduling checks, seven spoken cases | on (already) | On lamp-4ace and listed by install.md as product default |
| LAMP_SESSION_FACT | `on` only; off. Tells each new session nobody is talking to the lamp | on | on | commit 0b042f8fd (direct) | none named; RUNBOOK: on since 13 Oct 17:29, "unmeasured" | on (already) | On since 13 Oct; session_fact.rs: its effect on a session's first hearing still unmeasured |
| LAMP_SKILLS_ASK | `policy` (Policy's gate) or `skip` (act unasked); unset = nothing self-started runs (lamp-skills) | skip | skip | #950 | #950: nothing ran on a lamp; device run the first builder's on lamp-4ace 17 Oct | skip (already) until `lamp-policy --gate` is in the product unit and #1155 lands, then policy | skills.md: with `policy` a hello at the first lull is refused; product greets in a busy room only while it skips |
| LAMP_STAFF_NOTE | `on`; off. A request whose heard words name one of the staff gets a hand-over note | — | — | #1173 | lamp-4ace with TASKS_WHO/ASKED: Codex haiku, Claude Code poem, code X; count "in time" vs "after its input was closed"; not shown | off until the first builder's in-time/late count on lamp-4ace | tasks.md "Not shown on a lamp yet"; most likely wrong: transcript arriving after the request's end |
| LAMP_TASKS_ASKED | `on` only; off. Work asked of the staff always handed over; unclear names asked | — | — | #859 | Shown on lamp-4ace 15 Oct 21:22 to 21:59 (hand-overs and name question); reopened 16 Oct; `handovers.sh` (#903) not shown yet | off until `handovers.sh` passes (told 4/4, offered 0, handed 3/3) | Shown 15 Oct, but with it on stories were offered to Claude Code on 16 Oct; the #903 fix is "Not shown yet" (tasks.md) |
| LAMP_TASKS_NEEDS | file path; unset = a missing part is told as not running. Light "not set up" words | — | /usr/local/bin/harness | #1063 | none named on a lamp (#1063 host tests and gate only) | /usr/local/bin/harness (already) | A home lamp without Harness gets plain words (tasks.md, #811); lamp-4ace has Harness so does not need it |
| LAMP_TASKS_TARGET | alias string; default `office`. The tasks target alias | — | — | commit c2ac775f5 (direct) | none named; tasks.md "Who gets the work" suggests `office-claude` beside LAMP_TASKS_WHO | unset (`office`) | A per-lamp target name, not a behaviour; only needed when targets.json names differ |
| LAMP_TASKS_TOLD | file path; default /var/lib/lampOS/voice/tasks-told. Record of task news already told | — | — | commit 55e92e3d8 (direct) | none named | unset (default path) | A storage location, not a behaviour switch (tasks.md, setup.md) |
| LAMP_TASKS_WHO | `on` only; off. The person may name Claude Code, Codex or OpenCode | — | — | #769 | tasks.md "Who gets the work" played run; shown on lamp-4ace 15 Oct with TASKS_ASKED (name read back, yes required) | on | tasks.md: shown on lamp-4ace 15 Oct and kept on from 21:30; nothing starts without a heard yes |
| LAMP_TASK_NEWS | `skill`; otherwise as before. Finished/stopped/asking news waits up to 10 s for a skill's ask | — | — | #1038 | #1038: lamp-tasks --progress, LAMP_SAY, Task news skill folder; news said once via the skill; no record shown | off until #1038's task-news-through-the-skill check is played, after #1237 and #1042 send `task` | Needs the Task news skill (#1042); until a news ask names its task (#1237), a look or dim can go with the wrong task when two settle together (the Second) |
| LAMP_TASK_NEWS_AT_PAUSE | `on`; off. News at the first 1 s pause instead of after 15 s quiet | — | — | #1161 | lamp-4ace: talk while a task runs; news at first ~1 s pause, 3 s after the lamp's words, under 20 s; not shown | off until #1161's lamp-4ace check, which also covers #1217, #1223 and #1231 (one play: news at the first 1 s pause; `since_turn_ms` about 3,000 after an unanswered reply; two tasks within a minute told in one turn) | #1161 names rule 8 (news landing between two people talking) as most likely wrong; no lamp result recorded |
| LAMP_TOOLS | `on` only; off. Model may call not_for_me instead of speaking | on | on | commit 995240456 (direct) | none named | on (already) | On in both units since the function-call routing landed (tools.md) |
| LAMP_TOOL_LEAD_IN | `on` only; off. Says the read-back/start of answer before an agent-task call | — | — | #810 | lamp-4ace: provider-check --tool-turns with/without, then played hand-over and `turn-times`; 16 Oct 00:03 run failed (since mended), not re-shown | off until the #799 turn-times measurement on lamp-4ace | tools.md: 16 Oct run lost all functions (description over 1,024 B); #799 stays open until measured |

## The voice's own flags (`lamp-voice serve`)

`lamp-voice`'s command line in its unit, beside its environment. Here the code's
default and the units often differ: both units already pass the lamp's choice, so
"make it the code default" is the proposal (the first builder's 07:30: a switch
that passes becomes the default in code). Missing from the first version of this
page; the Second's 08:0x on #1238.

| Switch | Values and default | lamp-4ace (repo unit) | Product unit | Added by | Lamp check it named | Proposed product default | Why (one line) |
|---|---|---|---|---|---|---|---|
| `--turn-taking` | `vad-only` (default) / `v1-baseline` / `duplex`. `duplex`: a person heard over the lamp makes it yield | duplex | duplex | commit 32c279d4c (direct, 13 Oct) | Runs on lamp-4ace in every scene since 13 Oct (duplex-listening.md) | duplex (already); make it the code default | Both units pass it; `vad-only` is the experiment's first form |
| `--endpoint` | `silence` (default) / `smart-turn` / `smart-turn-fast`. The end-of-turn model judges each pause | smart-turn-fast | smart-turn-fast | commit 32c279d4c (direct, 13 Oct) | endpoint-pilot.md: the fast model's verdict in 148 ms on lamp-4ace, before the silence window ends the turn | smart-turn-fast (already); make it the code default | Both units pass it; without the model every pause is the window's |
| `--noise-suppression` | `on` (default) / `off`. The capture's software noise suppression | off | off | commit 32c279d4c (direct, 13 Oct) | Off on lamp-4ace since the voice came in | off (already); make it the code default | Both units turn it off; the code default disagrees with every lamp |
| `--reference-clock` | `fixed` (default) / `capture`. The echo reference follows the two USB clocks' difference | capture | capture | commit 32c279d4c (direct, 13 Oct) | Runs on lamp-4ace since 13 Oct | capture (already); make it the code default | `fixed` is a margin a long session uses up (options.rs) |
| `--reference-recovery` | `off` (default) / `bounded`. Re-centres a reference whose margin was used up | off | off | commit 32c279d4c (direct, 13 Oct) | none recorded on a lamp for `bounded` | off (already) | Both units and the code agree |
| `--echo-reference` | `as-given` (default) / `steady`. A reference that follows the microphone's clock | — | — | commit 8a494575f (direct, 14 Oct) | Five of lamp-4ace's recordings: two better, two worse, one held; no lamp run (the Second) | off (`as-given`) | Its own doc calls it "an experiment, and never a default" (options.rs, steady-reference.md) |
| `--provider-continuation` | `off` (default) / `kept-start`. A next request's start kept back while an answer may still come | kept-start | kept-start | commit 32c279d4c (direct, 13 Oct) | Runs on lamp-4ace since 13 Oct | kept-start (already); make it the code default | Both units pass it; `--early-end-ms` and the cut-off note need it |
| `--controls-stop` | flag; off. A hand on the button or the head stops the lamp's speech | set | set | commit 5f9914314 (direct, 14 Oct) | On lamp-4ace since 14 Oct (controls.md) | on (already) | Both units pass it; a stop by hand is the lamp's quickest control |
| `--early-end-ms` | 100–1000 ms with `kept-start`; off. Sends the end early after that much quiet, plays only when sure | — | — | commit af9ff59c5 (direct, 14 Oct) | Played on lamp-4ace at 200 ms: worse, so switched off (the Second's work log, row 19) | off | The end-of-turn model (`--endpoint smart-turn-fast`) took its place |

## The other parts: body, eyes, expression, tasks and controls

Body flags all say "keep as is until #1192's guard is on the lamp": on 17 Oct a
body exit wrote a bad elbow goal on lamp-4ace, #1192 now refuses such goals, and
its lamp-4ace run is Monday's, with a person there.

| Switch | Values and default | lamp-4ace (repo unit) | Product unit | Added by | Lamp check it named | Proposed product default | Why (one line) |
|---|---|---|---|---|---|---|---|
| `lamp-body --idle` | `on`/`off`; on. A lamp standing at rest shifts a little, every 40 s to 3 min at random | on | on | commit 44d8fea57 (direct; as the crate `motor`) | Ran on lamp-4ace (every 18 s before the random waits); body.md counts it as passing its minute, shown | on (already); keep as is until #1192's guard is on the lamp | Folded it never shifts, and since 16 Oct the waits are random, so it reads as alive, not on a clock (body.md "With --idle on") |
| `lamp-body --wake` | `on`/`off`; on. Brings a joint found off home back slowly, never away from home | on | on | commit ce40ff5ce (direct) | Shown on lamp-52e6 (base woke 311 and 283 counts); lamp-4ace's own pressed-head case not tried | on (already); keep as is until #1192's guard is on the lamp | Without it a lamp found off home refuses to start and has no body (body.md "Waking") |
| `lamp-body --rest-glance` | `on`/`off`; on. A resting head takes Expression's small lift, e.g. at a sound | on | on | #718 | Shown on lamp-52e6 with hand-published requests (head rose 41 counts); lamp-4ace's room folded left to its run | on (already); keep as is until #1192's guard is on the lamp | A resting lamp that lifts its head at a sudden sound and lets it go follows rule 8 (body.md) |
| `lamp-body --look-at-name` | `on`/`off`; on. When its name is heard it rises, looks up, glances once and lets go | on | on | #724 | Shown on lamp-52e6 (name heard 4 of 6, glance 135–204 ms); with a person in view not yet shown | on (already); keep as is until #1192's guard is on the lamp | Rule 8's own example: it glances to one side and the other and lets it go, at most once in 10 s |
| `lamp-body --give-way` | `return`/`stay`/`off`; return. A pushed joint gives way and stops pushing a stalled goal | — (default return) | — (default return) | commit 396c31642 (direct) | Shown by hand on lamp-52e6 15 Oct; runs on lamp-4ace since 15 Oct (#703) | return (default); keep as is until #1192's guard is on the lamp | It stops a held joint from cooking against its goal and feeds Controls' push signal (controls.md) |
| `lamp-body --limp` | `on`/`off`; on while giving way. A hand forcing a joint makes the lamp slump, as in V1 | — (default on) | — (default on) | #780 | lamp-52e6 run 6 showed only the first version; the merged build was not yet shown with a hand | on (default); a hand check of this build once #1192's guard is on the lamp | The person asked for V1's slump, but the merged version has no recorded hand run |
| `lamp-body --overload` | `release` or not given; off. Torque off on all five at sustained 80% load, retaken 120 s later | — | — | commit e14d22b89 (direct) | none named: body.md says "Not yet tried on any lamp" | off until a lamp check | Only simulated servos so far; a slump nobody has seen should not ship by default |
| `lamp-body --torque` | `on`/`off`; off. Takes hold of a joint found with torque off before starting | — | — (a lamp's `lamp.env` may pass it) | commit 64529dee4 (direct) | none named on lamp-4ace; lamp-52e6 on 14 Oct showed stale goals 18–39 counts off | off; keep as is until #1192's guard is on the lamp | It writes torque to a joint holding an old goal, the same class of risk as the 17 Oct elbow goal |
| `lamp-body --moves` | absolute folder; none. Loads NAME.json moves, played by name as gestures | — | — | #994 | Add --moves, check the start log, send a gesture on a standing lamp; not yet shown | off (no folder ships) | No move files ship, and who may ask for a move is not yet decided (#994) |
| `LAMP_EXPRESSION_BODY` (lamp-body) | `on`; off. Body adds Expression's head requests to its pose | on | on | commit fc93f8608 (direct) | Used on lamp-4ace since 14 Oct 10:05 (expression.md), shown | on (already); keep as is until #1192's guard is on the lamp | `--rest-glance` and Expression's alive layer do nothing without it |
| `lamp-eyes --full-look` | `WxH` with `--full-look-file`; off. One larger still when asked, after the head settles | — | — | #842 | Shown on lamp-52e6 (ten objects); lamp-4ace and objects held by a person not shown | off | Needs the voice's LAMP_LOOK_FULL too, which no voice unit sets |
| `lamp-eyes --steady-rate` | `on`/`off`; off. Looks 2/s instead of 4/s while one face sits still | — | — | #1114 | Only an offline cost estimate (about 21% vs 11% of a core on lamp-4ace); lamp-52e6 still to measure | off until measured on a lamp | It halves the eyes' CPU but slows gaze correction (1.5 s vs 0.75 s), not yet seen on a lamp |
| `lamp-eyes --leave-still-faces` | `on`/`off`; off. Leaves out a small, faint face at a fixed spot (a false face) | — | — | #766 | Host replay of lamp-4ace's night: 55 of 55 false faces left out, 0 people; not yet live | off now; on after one live night on lamp-4ace | The replay is strong, but a run on a lamp is still owed before it becomes the default |
| `lamp-expression --enable` | flag; off (shadow). Publishes body and light requests instead of only planning | set | set | commit 6226dae33 (direct) | On lamp-4ace with the alive catalogue since 14 Oct, shown | on (already) | Without it Expression only plans, and nothing moves or lights |
| `lamp-expression --head-touch` | flag; off. Leans into a held hand, sleeps on a 3 s hold, wakes on a tap | — | — | #849 | None shown: ARM64 binaries listed for a lamp run, no result recorded | off until shown, with the voice's LAMP_HEAD_TOUCH | Product Controls publish head touches (`--touch`), but nothing has been seen on a lamp |
| `lamp-expression --kernel-clock` | flag; off. Time of day only once the kernel says the clock is set | — | set | #884 | Read-only on lamp-75c9 (chrony keeps the clock); nothing run with the network cut | on (already) | No battery clock, so without it a stale boot clock can bring a false night (install.md) |
| `lamp-expression --offline-face` | flag; off. Listening drawn as not ready while lamp/online says offline | — | — | #978 | none named | off now; on after its minute on a lamp | The light shouldn't say "go ahead" when the lamp can't answer, but no lamp has shown it |
| `lamp-tasks --enable` | flag; off. Serves tasks at all, otherwise "Tasks is off" | set | set | commit 189b744fa (direct) | Shown on lamp-4ace 14 Oct 22:24–22:39 (prepare 35 ms, confirm 38 ms) | on (already) | Handing work to Harness by voice was shown end to end on lamp-4ace |
| `lamp-tasks --watch` / `--no-watch` | watch on unless `--no-watch` (`--watch` changes nothing). Reads each task handed over | `--watch` | `--watch` | commits 45a1e4d2c, fd6c1ae7a (direct) | Shown in the first end-to-end run on lamp-4ace 15 Oct 00:49–00:56 | on (the code default; `--watch` in the units is redundant) | Already the code's default |
| `lamp-tasks --progress` | flag; off. Puts each task's progress on the bus as tasks/status | — | set | #751 | Five lamp-4ace steps named; CPU cost measured on lamp-75c9 | on (already in product) | About 1 point of the lamp's CPU on lamp-75c9; #1156's wake and the Policy's progress need it |
| `lamp-tasks --push-wait` | flag; off. Holds Harness's wait, so a task's end is known in seconds | — | set | #1157 | Shown on lamp-75c9: idle to settled 8.3–10.2 s vs 38.9–41.2 s without | on (already in product) | Four times faster for about 1 point of CPU and a held ~130 MB Node process |
| `lamp-tasks --harness-as-person` | flag; off. Calls Harness as the person's own call, without agent markers | set | set | commit b73dfe6fa (direct) | Shown on lamp-4ace 14 Oct 22:24 | on (already) | Otherwise `harness new` is refused unless an account-wide agent setting is on; the owner chose this |
| `lamp-controls --enable` | flag; off (shadow). Gestures act, not only watched | set | set | commit 1c765da78 (direct) | Watching only on lamp-4ace from 14 Oct 11:03, then enabled; shown | on (already) | Without it the button and pads say `enabled: false` and nothing acts |
| `lamp-controls --touch` | flag; off. The touch path: head and strip pads, and the reset button held 5 s, on controls/touch | — | set | #728 | Per-pad thresholds, faulty E4/E8, latency listed; not yet shown | on (already in product) | The product needs it for the 5 s reset hold (#1167); without it the reset did nothing |

## What the census found

Gaps between the docs, the units and the code, for whoever owns each:

- **lamp-4ace's repo unit sets less than the docs say lamp-4ace runs.**
  `docs/tasks.md` says `LAMP_TASKS_WHO` and `LAMP_TASKS_ASKED` stay on on
  lamp-4ace (15 Oct 21:30), `docs/attention.md` says `LAMP_POLICY_HOLDS_ACTIONS`
  and `LAMP_SPOKEN_TO` are on there, and #997 and #1148 describe head taps
  (`LAMP_HEAD_TOUCH`). `deploy/lamp-4ace/lamp-voice.service` sets none of them,
  so they live in drop-ins on the lamp that the repo doesn't have. Before
  switching groups on, the lamp's own drop-ins should be read and copied here.
- **No repo policy unit passes `--act attention`, `--act progress`, `--hello` or
  `--gate`**, so `LAMP_POLICY_HOLDS_ACTIONS`, `LAMP_PROGRESS_NEWS`,
  `LAMP_HELLO_ASKS_POLICY` and `LAMP_SKILLS_ASK=policy` do nothing from the repo
  units alone.
- **lamp-tasks:** the product unit passes `--progress --push-wait`; the
  lamp-4ace unit doesn't (reverted in `4b8922bbc`, its build being older than
  `--push-wait`), while `docs/tasks.md` says lamp-4ace runs the same. Both
  units pass `--watch`, which changes nothing.
- **Head touch:** product Controls run `--touch`, but product Expression has no
  `--head-touch` and the voice no `LAMP_HEAD_TOUCH`, so on a product lamp the
  head pads reach only the old engine's pat. lamp-4ace's repo units have none
  of the three.
- **`LAMP_SHORT_SOUNDS=quiet-room`** is in both units, the form #800 showed
  answering a room sound; `quiet-room-checked`, which fixes it, is in neither.
- **Stale text:** `deploy/lamp-4ace/RUNBOOK.md` says `LAMP_ROOM_FACTS` is "off;
  not yet heard" while the unit sets it on; the "Switch" section of
  `docs/attention.md` says `busy-near` and `LAMP_SPOKEN_TO` aren't on lamp-4ace
  while the same file records both on it; test comments in `lamp-body.rs` say
  `--wake`, `--rest-glance` and `--look-at-name` are off unless asked for, while
  the code defaults them on; the units' comments say the Policy's gate "is not
  built yet (#939)" while `docs/skills.md` describes it built (#958).
- **Five of the voice's own flags default in code to what no lamp runs** (`--turn-taking`, `--endpoint`, `--noise-suppression`, `--reference-clock` and `--provider-continuation`): both units pass the lamp's choice, so a lamp started without the unit's line gets the experiment's first form.
- **A product body's motion flags are partly per lamp:** `BODY_POSES` in a
  lamp's `/etc/lampOS/body/lamp.env` can carry flags, and lamp-06c1's passes
  `--torque on`.
- **Small ones:** `LAMP_SCHEDULE_SPEAK` is defined twice (`reminders.rs` and
  `tools.rs`); `LAMP_ONBOARDING` has no doc comment; #987
  (`LAMP_LOOK_FULL_SIDE`) names no lamp check.
