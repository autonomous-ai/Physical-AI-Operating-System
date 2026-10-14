# The builders: the running list

Since 2026-10-14 four sessions build lampOS at once, at the product
owner's request: "keep a running list of todos on github so that once
it's done it reads that list for new tasks and continue working. you
also read that list and see what's done and add more tasks. it will get
todo, build, test, merge."

**Since 2026-10-14, 16:41: the tasks are GitHub issues.** Each open task is
an issue labelled with its builder (`builder:<name>`); the order is the
pinned [Queue issue #705](https://github.com/autonomous-ai/lampOS/issues/705);
how a builder starts, claims, asks and hands back is
[joining.md](joining.md), and each builder has a role file in
[builders/](../builders/). This file keeps "Rules for all", "What the lamp
showed", the answers given before the move, and the history of finished
tasks. Where the loops below differ from joining.md, joining.md holds.

This file, on the branch `lamp-voice`, was that list. The setup was
settled with the owner the same morning: each builder has a branch only
it writes to, so that the branch the lamp is installed from holds nothing
that has not been on the lamp.

| Builder | Where | Its branch | A task while it is built | Its log |
|---|---|---|---|---|
| First | At the office, with lamp-4ace | `lamp-voice` | | this file |
| Second | Another machine, no lamp | `lamp-second` | `second/<task>` | `lampOS/docs/work-log.md` on `lamp-second` |
| Third | Another machine, no lamp | `lamp-third` | `third/<task>` | `lampOS/docs/work-log-third.md` on `lamp-third` |
| Fourth | Another machine, no lamp | `lamp-fourth` | `fourth/<task>` | `lampOS/docs/work-log-fourth.md` on `lamp-fourth` |
| Fifth | Another machine, with a lamp of its own (not lamp-4ace) | `lamp-fifth` | `fifth/<task>` | `lampOS/docs/work-log-fifth.md` on `lamp-fifth` |

- **The first builder** is the only one who can build for the lamp,
  install on it and measure there. It writes the tasks here, takes
  finished work from the others' branches into `lamp-voice` when it can
  run it on the lamp, mends there whatever does not work on the lamp, and
  writes here what the lamp showed.
- **The second, third and fourth builder** each take the tasks in the
  list below that carry their name, top first, and for each: build, test,
  put it on their own branch, record it in their own log, and take the
  next. None ever pushes to `lamp-voice`, or to another's branch. (The
  second builder's first push, taking tasks 1 and 2, went to `lamp-voice`
  under the first form of these rules, and stands.)

The second builder has the tasks about sound and speed. The third has
the tasks that are parts of their own. The fourth has the lamp's memory.
That is so that their work does not meet: the third builder is the one
who adds crates to the workspace and message types to `crates/msgs`; the
second adds neither; the fourth works inside `crates/memory`.

That is as many as the lamp can take. Every piece has to be run on the
one lamp by the one builder who has it, and with three others building,
what waits for the lamp already grows faster than it can be cleared.

**The fifth builder** (from 2026-10-14, the owner: "let's have that
machine work on servos, because they have the actual lamp there") has a
lamp of its own, and the body. `crates/body` is its from now on: the
first builder stops changing it and only takes its work onto lamp-4ace.
It builds for its own lamp, installs there and measures there, and what
it measures there is a result on a lamp, said with that lamp's name. It
never touches lamp-4ace. Its lamp's folder under `deploy/` is its own.
Its loop is the one below, with "The loop, for the builder with a lamp"
in place of steps 5 and 6. It installs on its own lamp from its own
branch and does not merge into `lamp-voice`: the first builder takes its
work as it is, does not repeat what it measured, and runs it on
lamp-4ace for a few minutes before it goes onto `lamp-voice`, because
`lamp-voice` is what lamp-4ace runs and no two lamps have the same servo
zeros, reach or loads (owner's question and the first builder's answer,
2026-10-14).

## The loop, for the builders without a lamp

"Your branch" and "your log" are the ones in the table above.

1. `git fetch origin`. Read this file as it is on `origin/lamp-voice`,
   and your own log on your branch.
2. Bring your branch up to date: rebase it on the newest
   `origin/lamp-voice` (the first time, create it there). Work the first
   builder has already taken drops out of it by itself. Push it with
   `--force-with-lease`: your own branch is the one place where that is
   allowed, because nobody else writes to it.
3. Take the first task in the list that carries your name and that your
   log does not show as taken or done. Write it into your log as taken,
   with the date, and push.
4. Work on a task branch (your name, a slash, and the task's short name
   from the list) from your branch. Read what the task points
   to before writing anything.
5. Test. The gate, run in `lampOS/`: `cargo fmt --all --check`;
   `cargo clippy -p <crate> --all-targets -- -D warnings` and
   `cargo test -p <crate>` for every crate touched and every crate that
   depends on one touched; `cargo build --workspace`. If the workspace did
   not build on that machine before the change, say so in the log and use
   the other three.
6. Open a pull request for it from `<name>/<task>` into `lamp-voice`
   (joining.md, "One pull request per task", `fifth-P6`), and write the
   task's entry in your log: what was built, what was tested and on
   which machine, what was not shown, and what the first builder has to
   do on the lamp.
7. Take the next. With nothing left to take, look again every half hour.

What never goes onto your branch: a failing test (the review task stays
on its own branch); a change to a crate listed as hands off, unless its
task says so; anything that changes what the lamp does without a switch
that is off unless set; anything under `deploy/`.

What the first builder does with it: reads the logs, takes a finished
task into `lamp-voice`, builds it for the lamp and runs it there. **What
does not work on the lamp, the first builder mends itself, there and
then.** Nothing is sent back to be reworked: the owner's instruction is
that a builder without the lamp "build the best it can and if there are
issues you fix it yourself so we can reduce communications cost back and
forth". The mend is one line under "What the lamp showed", and your
branch picks it up at its next rebase. Only a piece that is wrong at its
root comes back, and then as a new task in the list.

So: **build each piece as if nobody will come back to you about it.**
Finish it. Write every test that can be written without a lamp. Leave the
part that needs the lamp as small as it can be, one command if possible.
In your log, say exactly what to run on the lamp, what it should show,
and where you think it is most likely to be wrong there: that last line
is worth more than any other, because it is where the first builder will
look first.

## The loop, for the builder with a lamp

As above, and:

- **Ask your person before every `ssh` to the lamp** (the repository's
  `CLAUDE.md`), and before anything the lamp does that a person in the
  room would see or hear. Never ask for or type a password; the person
  sets up access with a key.
- **Before installing anything**, write down what the lamp runs (`systemctl
  list-units 'lamp*' 'hal*'`, the processes on `/dev/ttyACM0` and
  `/dev/video0`), and back up whatever you stop or replace, under your
  home folder, with the date. Nothing of the old software is deleted.
  Never two programs on `/dev/ttyACM0` at once.
- **Power:** never cut the lamp's power yourself. An unplug is done by
  your person, with a hand on the lamp, after you have said what will be
  read before anything moves.
- **Build for the lamp** either on the lamp itself (`cargo build
  --release -p lamp-body`, slow but plain) or with the cross-build image
  the first builder uses ([brain-build.md](brain-build.md)).
- **The gate** is the one in step 5 for the crates you touch, and then
  the lamp: the first command, what it showed, and how often, in your log
  under the task, as "What the lamp showed" is written here.
- **Photographs** only of the lamp, with no person in them, no face, no
  screen: the repository is public.

## Rules for all

- **The repository's own rules apply** (`CLAUDE.md` at the root): a commit
  message is one imperative line beginning `lampOS: `, with nothing after
  it and no attribution of any kind; `git add` the exact files, never
  `-A`; the document that belongs to a piece of code is updated in the
  same commit, in English.
- **Rust under `lampOS/`**, with one exception made here: the code that
  trains one of the lamp's small models lives in `lampOS/training/<part>/`
  and may be Python. It never runs on the lamp.
- **Model files** are not committed if they are over 2 MB or not ours. One
  of ours under 2 MB goes in `lampOS/models/<part>/` with a `SOURCE.md`:
  what it was trained on, under which licences, its SHA-256 and the
  command that made it. Train only on speech whose licence allows a model
  made from it to be shipped (Speech Commands, Common Voice, LibriSpeech,
  LibriTTS and voices trained on them are of that kind; the voices built
  into a laptop's operating system are not known to be).
- **Names are plain.** Voice, body, eyes, memory, name, schedule. No
  jargon and no brain anatomy in the names of parts, files or functions.
- **Each part is its own process** with one written interface to each
  neighbour, and is held to the rules in `docs/brain-build.md`
  ("Components, processes and owners"): it waits on nobody, keeps its own
  records, says whether it is well, and the lamp goes on without it.
- **Research before building.** The product owner asks that each part be
  the best of its kind: what exists is studied first, and a short "what
  exists and what we take" opens the part's document.
- **Evidence.** Say exactly what was run, on which machine, how many times
  and what came out, and keep the failures. A result on a laptop is not a
  result on the lamp and is never written as one. Every document ends with
  what was not shown.
- **Hands off, unless a task says otherwise:** `crates/voice`,
  `crates/gemini`, `crates/name`, `crates/audio`, `crates/body` (the fifth
  builder's own),
  `crates/eyes`, `crates/actions`, `crates/policy`, `crates/health` and
  `deploy/`. The first builder changes these by the hour, on the lamp.
- **No spending, no accounts, no keys.** A task that needs one says so
  in the log and the next task is taken.
- **Questions** go in your own log, pushed, and work goes on with
  whatever does not depend on the answer. The first builder answers them
  here, under "Answers".

The workspace is `lampOS/` (`cargo test -p <crate>` from there). The
lamp is an Orange Pi 4 Pro (Linux ARM64, eight cores: two at 2.0 GHz, six
at 1.79 GHz; 6 GB). What it is for, in the owner's words: "the most human
like interaction experience", a lamp that is "a person sitting next to
you in the office", whose voice "must be perfect".

## The list

The open tasks moved to issues on 2026-10-14 (#694 to #704, ordered by
#705). The table and the sections below are their history as they stood
then; an open task's issue is its record from now on.

**Order from 2026-10-14, 14:00 (owner):** "let's prioritize finishing as
many components as possible. the harder ones we'll attack later. attack
the easy components first. we need to get the architecture built out
asap with as many components as possible". So each builder takes the
parts that stand on their own first: the third builder reminders and
timers (6), then a task out to an agent through Harness (11); the shared
log and settings (8) moved to the fourth builder on 2026-10-14 at 15:15,
after its 28 and 29, so that two parts are built at once; Expression's
rest (18) is done. Stopping and interruption are tested by the first builder in the
evenings.

The owner, 2026-10-14, 07:45: "let's finish building asap and get lamp in
front of real customers!" So the order below is the order in which a
piece gets the lamp nearer to a customer's desk, and each builder takes
its own from the top. The numbers are names, not the order.

**What a lamp in front of a customer needs that this one lacks,** and who
has it:

| It needs | Today | Who |
|---|---|---|
| To be stopped and silenced by a hand, whatever the voice is doing | No button or touch pad in V2 | Third: task 7 |
| To start by itself after power comes back, get onto a network without a builder, and take new software | Voice and light come back; the body and the eyes are started by hand; setup and signed updates are built (task 12) and not installed | Fifth: task 23. First: task 12 on the lamp |
| A body that gives way against a hand and cannot tip itself over (Fifth: tasks 24 and 26) | The force limit is on the lamp at 600 since 2026-10-14, 08:01, not yet tried by a hand; the lean check runs on the lamp, but nothing of the lamp has been weighed, so it is not wired | First |
| Answers that begin sooner than two seconds after the person stops | Half begin within 1.98 s | Second: task 1. First: on the lamp |
| To stop when someone says stop | One or two of seven short words | Second: task 3. First: with a person's voice |
| To hear its name however it is said | "Hey Lamp" and "Lamp ơi", not a bare "Lamp" | Second: task 5 |
| A day in a real room without a fault, with real voices | Played voices at night; one working day of records | First |
| To move and light up as one thing | The body and the light each follow the voice | Third: task 4. First: with the camera |
| To remember yesterday | A journal and whole-word search | Fourth: task 13 |

| # | Task | Short name | Builder | State |
|---|---|---|---|---|
| 1 | The end-of-turn model in a tenth of a second | `turn-end-fast` | Second | done 2026-10-14: on `lamp-voice`; 148 ms on the lamp against 382; the voice does not use it yet |
| 2 | Try to break last night's changes to the voice | `voice-review` | Second | done 2026-10-14: on `lamp-voice` with its mends |
| 3 | A listener for "stop" under the lamp's own voice | `stop-word` | Second | done 2026-10-14: on `lamp-voice`; first look on lamp-4ace heard no played stop ("What the lamp showed"); not wired |
| 5 | The name, heard as a bare "Lamp" | `bare-name` | Second | open |
| 19 | The end sent early, the answer played only when sure | `early-end` | Second | back on the lamp 2026-10-14: on `lamp-voice`, played, worse; switched off ("What the lamp showed") |
| 20 | The replay takes the lamp's own recordings | `replay-capture-clocked` | Second | done 2026-10-14: on `lamp-voice`; follows the lamp's own recordings sample for sample; the steady reference judged on them, mixed |
| 10 | A reference that follows the microphone's clock | `reference-clock` | Second | done 2026-10-14: on `lamp-voice`, behind the voice's `--echo-reference steady`; to be played to on the lamp |
| 9 | Faster answers: what exists | `faster-answers` | Second | done on `lamp-second` 2026-10-14; the first builder takes it next |
| 4 | Expression: one part that authors what the lamp shows | `expression` | Third | done 2026-10-14: on `lamp-voice`, run on the lamp watching only; the wiring is the first builder's |
| 7 | The button and the touch pad | `controls` | Third | done 2026-10-14: on `lamp-voice`; reads the lamp's real pins and runs there as its own service since 11:03, watching only; the voice acting on it is task 21 |
| 12 | Starting, setting up and updating a lamp | `surround` | Third | done 2026-10-14: on `lamp-voice`; built for the lamp and its two checks run there; nothing installed, nothing touches the network |
| 6 | Reminders and timers | `schedule` | Third | taken 2026-10-14 |
| 11 | One task out to an agent and back, through Harness | `tasks` | Third | open |
| 8 | A shared log, settings, and reports from the parts that give none | `defaults` | Fourth (from Third, 2026-10-14) | open: after 28 and 29 |
| 18 | Expression: holding back for someone, and a lamp at rest | `expression-rest` | Third | done 2026-10-14: on `lamp-voice` with its playout addendum; Expression acts on lamp-4ace with the rest catalogue |
| 13 | Memory across days | `memory-days` | Fourth | done 2026-10-14: on `lamp-voice`, built for the lamp and timed on it; switched on on the lamp at 14:50 (backed up first); the voice does not yet use its additions |
| 14 | Whom is it said to: what exists | `said-to-whom` | Fourth | done 2026-10-14: on `lamp-voice` ([said-to-whom.md](said-to-whom.md)); task 17 follows from it |
| 15 | The lamp keeps its own records within a bound | `records-bound` | Fourth | done 2026-10-14: on `lamp-voice`; its dry run works on the lamp's real records; removal not switched on |
| 16 | A check that the lamp cannot lean too far | `lean-check` | Fourth | done 2026-10-14: on `lamp-voice`, run and timed on the lamp; a stand-in body model until the lamp is weighed; not wired to the body |
| 21 | The voice stops and mutes on the hand controls | `controls-voice` | Fourth | done 2026-10-14: on `lamp-voice` (`5f9914314`) and on the lamp with its switches off; a hand tries it in the evening |
| 27 | The keeper: one lamp of many parts, started, watched, stopped and resumed | `keeper` | Fifth | in part 2026-10-14: on `lamp-voice`; runs lamp-52e6 as one target (ready in 2.5 s, a hung body back in 20 s); the update choreography and the pause still to come |
| 22 | The body on a second lamp | `body-second-lamp` | Fifth | done 2026-10-14: on `lamp-voice`; lamp-52e6 runs the body (`deploy/lamp-52e6/`); the build ran on lamp-4ace from 15:00, unchanged with its switch off |
| 23 | Waking up after the power comes back | `wake` | Fifth | in part 2026-10-14: the wake is on `lamp-voice` behind `--wake on`; it woke lamp-52e6's base 311 counts and back, and ended itself early both ways on lamp-4ace (answer below), where the service runs with it on since 15:21; the unplugs and the unit's `[Install]` still to come |
| 24 | Giving way to a hand, and not hurting itself | `give-way` | Fifth | in part 2026-10-14: give-way behind `--give-way` on `lamp-voice`; tried standing on lamp-52e6; on lamp-4ace from 15:59 with the switch off, nothing changed |
| 25 | How the head follows, and a nod a person can see | `head-follow` | Fifth | measured 2026-10-14 on lamp-52e6: a clear nod is about 3° in 0.35 s; the body's speed makes it 0.75 s (answer below) |
| 26 | Weighing and measuring the lamp for the lean check | `weigh` | Fifth | first figure 2026-10-14: lamp-52e6 weighs 3.0 kg whole (the specification says 7) |
| 17 | A second judge for the Policy: the lamp's own signs | `local-judge` | Fourth | done 2026-10-14: on `lamp-voice`; `lamp-policy-judges` is a played-scene tool (at most ten minutes, watching only), on lamp-4ace beside the service; 89 labelled rows from played scenes are in `docs/verification/policy-scenes-lamp-4ace-20261014/` and fit end to end; the fitted choice never answers (all 8 test calls held), so not usable yet |
| 28 | Memory says it is well, for the lamp's health | `memory-health` | Fourth | open |
| 29 | The voice uses what memory across days adds | `memory-voice` | Fourth | open |

## 1. The end-of-turn model in a tenth of a second

Document: `docs/endpoint-pilot.md`.

**Why.** The lamp answers too slowly. Over all 1,034 answers in its
records, half began 1.98 s after the person stopped
(`docs/conversation-latency.md`); a person answers in a few tenths. Most
of that is the speech service. The part that is the lamp's own is a fixed
wait of 600 ms of quiet before it believes the person has finished. A
shorter fixed wait was tried on the lamp on 2026-10-14 and withdrawn: at
400 ms the pause after "Lamp," cut the name from the question.

**What exists.** A wait that ends early only when the sentence is
plainly finished is already built: `--endpoint smart-turn` on
`lamp-voice`, the worker `lamp-turn-end` (`crates/turn-end`), its wire
(`crates/turn-end-wire`) and the rule `ConfirmPolicy::responsive()` in
`crates/voice/src/activity.rs`: asked at 200 ms of quiet, a confident
"complete" (0.9 or more) ends the wait at 400 ms, a clearly unfinished one
holds it to 1,500 ms, anything else ends on the window chosen. It is
switched off on the lamp because it is too slow there.

**What was measured on the lamp** (`docs/endpoint-pilot.md`, "First run
on lamp-4ace"): one classification took 364 to 427 ms and one gave no
verdict within 500 ms, so a verdict was heard 570 to 660 ms into a pause
and no turn ended sooner than the window. The worker runs
`smart-turn-v3.2-gpu.onnx` (32 MB, 32-bit floats) under `tract-onnx`
0.23.8. The 8-bit `smart-turn-v3.2-cpu.onnx` is not used because that
tract mis-executes its quantize and dequantize pairs. For scale: on the
same board `lamp-name` runs three small models under tract every 80 ms
for 9.3 ms a step on the two fast cores.

**Wanted.** A verdict within 150 ms of being asked for, on the lamp's two
fast cores, with verdicts that agree with the present model's on the
labelled speech the document already uses. Then a finished sentence could
end the wait at 300 ms and every such answer would begin 0.3 s sooner.

**Where to look first** (leads, not instructions): the 8-bit model under
a runtime that executes it rightly (ONNX Runtime, a newer or patched
tract, or a converted graph); how the 0.4 s divides between making the
features (`crates/turn-end/src/features.rs`) and the network; threads, and
keeping the worker on the fast cores; whether the model can be given less
than its full window; asking earlier than 200 ms.

**Hand back:**

1. A way to time it on the lamp that needs only the binary and the model
   file, for example `lamp-turn-end bench MODEL [RUNS]`: the time for the
   features and for the network, each run, with the median and the worst.
   The first builder runs it on the lamp, as it is and held to the fast
   cores.
2. On the second builder's own machine: old against new, as a ratio, on
   the same input. An Apple Silicon laptop running a Linux ARM64 container
   is a fair first guess at the lamp's processor family and several times
   faster than it, so ratios are what carry over, not times.
3. How far the new verdicts agree with the old ones, on which speech.

**Not in this task:** the rule in `crates/voice/src/activity.rs` and the
lamp's settings; the worker stays off on the lamp until the first builder
has timed it there. If the answer needs a native library, say what it
costs: size, how it is built for Linux ARM64, its licence.

## 2. Try to break last night's changes to the voice

It stays on its own branch and never goes onto `lamp-second`. It adds
tests only, to crates otherwise hands off.

**What changed on the night of 2026-10-14**, all of it live on the lamp
and described in `docs/attention.md` and `docs/name.md`:

| What | Where |
|---|---|
| The lamp's name heard in the sound lets an answer out that a rule would have held | `crates/voice/src/attention.rs` (`decide`, `began_at`, `name_heard`), `crates/voice/src/bus_out.rs` (`NameIn`), the worker in `crates/voice/src/provider_worker.rs` (search `name_in`) |
| A second look: a call that got nothing is put to the model again with a note | `crates/voice/src/second_look.rs`, wired in `crates/voice/src/coordinator.rs` (search `second_look`) |
| The model is told its name was heard while the person is still speaking | `crates/voice/src/called.rs`, `try_note` in `crates/gemini/src/recovery.rs`, the worker (search `called`) |
| The part that hears the name | `crates/name/src/{main,said,hear}.rs` |

**Wanted.** Faults, each shown by a failing test: anything that could
make the lamp speak when it should not, stay silent to someone who called
it, answer twice, go round in a loop, or lose its link to the speech
service (in `crates/gemini` an error returned from a command ends the
connection, so a note sent at the wrong moment is such a fault). Clocks
are passed in everywhere, so no test needs to sleep. No fixes: the first
builder fixes, because only it can run the result on the lamp.

**Hand back,** in the log: the findings, most serious first, each with
the name of its test; and what was looked at and found sound.

## 3. A listener for "stop" under the lamp's own voice

Documents: `docs/stop-word.md` (new); read `docs/duplex-listening.md`
(the section dated 2026-10-14) and `docs/name.md` first.

**Why.** Someone who says "stop" while the lamp is talking is mostly not
heard: one or two of seven played words stopped it. The cause is in the
lamp's microphone and its echo canceller, and no threshold mends it. One
of the ways left is a listener for the few words that mean stop, made to
find them under another voice.

**What exists.** `lamp-name` (`crates/name`) already runs, every 80 ms,
two fixed models from openWakeWord (`melspectrogram.onnx`,
`embedding_model.onnx`: release v0.5.1 of `dscripka/openWakeWord`,
SHA-256 `ba2b0e0f…b176f` and `70d16429…5c1f`) and then a small model of
ours over the newest sixteen embeddings (`crates/name/src/hear.rs`: input
`[1, 16, 96]`, output one score). A second small model of the same shape
costs almost nothing beside it.

**Wanted.** `stop.onnx`, that shape: one score for "stop", "wait" or
"hold on" having just been said, in English, by anyone. It has to find
the word when another voice is talking at the same time and louder: on
the lamp a person arrives level with the lamp's own voice at best and 15
dB under it at worst, and the canceller takes some of the lamp away.
Train it on real people (Speech Commands has "stop" from thousands of
speakers; "wait" and "hold on" will need other sources or generated
speech) mixed under speech in another voice across that range, and
measure on speakers it never heard: how many it catches at each level,
against how often an hour of the other voice alone, and of ordinary talk,
sets it off. "Don't stop", "stopped" and "bus stop" are in the talk.

**Hand back:** the training code (`lampOS/training/stop/`), the model
(`lampOS/models/stop/`), the document with the tables. The first builder
measures it on recordings of the lamp's own microphone and on the lamp,
and wires it in.

**Not shown by this task, whatever it finds:** the lamp's microphone,
whose chip changes a voice in ways no mixture made elsewhere copies.

## 4. Expression: one part that authors what the lamp shows

Document: `docs/expression.md` (exists: the design, what runs today, how
it will be judged). Read it, `docs/body.md` and `docs/bus.md`.

**Why.** The owner's goal, in their words: "use all the hardware that we
have like body language (the torso), head language (top part of lamp),
face language (light), and create a cohesive experience for lamp. so when
lamp speak, it's all coordinated." Today the body and the light each
follow the voice's state by themselves.

**What to build:** a crate `crates/expression` and its process
`lamp-expression`, as the document designs it. It reads the bus (what the
voice is doing, said and heard; the faces the eyes track; the name heard;
how the room is; the body's joints: the topics are listed in
`docs/bus.md` and their types in `crates/msgs`), runs its layers on one
clock that is passed in, and publishes what the body and the light should
show, on topics and types of its own added to `crates/msgs`. It drives no
hardware: the body and the voice own theirs, and the first builder wires
them to listen, with today's behaviour kept as the fallback when this
part is gone. Behaviours are data, so that one can be added or tuned
without touching the engine. Begin with the five the document names.

It is judged by timing (the lamp has turned back before its first word; a
nod falls in the person's pause, not on their words; the light changes
before the body when it is interrupted), so build a lamp that is not
there: something that plays a recorded stretch of the bus into the part
and records what it asked for and when. Recorded stretches come from the
first builder; until then, written ones.

**Hand back:** the part, its tests on the clock, the document brought up
to date, and exactly what the body and the voice must subscribe to.

## 5. The name, heard as a bare "Lamp"

Documents: `docs/name.md`; the gap is in `docs/conversation-latency.md`
(the last paragraph of the 2026-10-14 section).

**Why.** The name's model was trained on "Hey Lamp" and "Lamp ơi". Eight
played lines that began with a bare "Lamp," were heard as the name in
none, and the lamp left four of them unanswered. People will say "Lamp,
what time is it?".

**Wanted.** A model of the same shape as in task 3 that hears "Hey Lamp",
"Lamp ơi" and a bare "Lamp" at the start of a sentence, and is not set
off by "Lan", "Liam", "lamb", "land", "plan", "lab", "Sam" or "ma'am" in
the same place, nor by ordinary talk, nor by the lamp's own voice. One
syllable is little to go on: say plainly how far it gets. The lamp's
microphone clips the first tenth of a second of the first word after a
silence and muffles a voice played from a loudspeaker, so train with both
done to the speech. Many voices, accents among them, and Vietnamese
speakers of English.

**Hand back:** as task 3 (`lampOS/training/name/`, `lampOS/models/name/`,
the document), with the catch rate for each of the three ways of calling
and the false alarms for each kind of near miss.

## 6. Reminders and timers

Document: `docs/schedule.md` (new).

**Why.** "Set a timer for ten minutes" and "remind me at three to call
Maria" are among the first things anyone asks a lamp on their desk, and
this one has to say it cannot. The build record has the part and what
happens without it (`docs/brain-build.md`, "Components, processes and
owners": Schedules, `lamp-schedule`).

**What to build:** a crate `crates/schedule` and its process
`lamp-schedule`.

- It keeps timers and reminders in a file of its own. A restart loses
  none, and one that came due while it was down is given late and marked
  late.
- It is asked over a Unix socket of its own, one line of JSON in and one
  out, as the memory and the executive are (`docs/memory.md`,
  `docs/tools.md`): set, list, cancel.
- It publishes on the bus when one comes due, in a form that survives the
  voice being busy or down at that moment, and its own report on
  `diagnostics` (`docs/health.md`).
- It says nothing itself. The voice will say what is due, in a turn the
  lamp begins (`crates/voice/src/speak_up.rs` is how it says hello), and
  the first builder wires that. Propose the wiring, and the functions the
  model would call (names, descriptions and parameters as in the
  executive's catalogue, `docs/tools.md`), in the document.
- Times are where this goes wrong. The model should not have to do
  arithmetic with dates: decide what it hands over ("in 600 seconds",
  "15:00", "tomorrow 09:00") and where the lamp's own clock and zone come
  in (`crates/voice/src/local_time.rs`).

**Before building:** what V1 did (`docs/v1-verdicts.md`, its rows on
reminders, timers and schedules, each with a verdict already) and how the
assistants people use handle wording, snoozing, repeating and missed
reminders.

**Hand back:** tests with the clock passed in (pending items across a
restart, late delivery, cancelling, the socket's lines, the bus
messages), and the document.

## 7. The button and the touch pad

Document: `docs/controls.md` (new). Read `robots/lamp/docs/physical-controls.md`
(V1, at the repository's root) and the rows on controls in
`docs/v1-verdicts.md`.

**Why.** A lamp that is talking has to be stoppable by a hand, and a
microphone has to be closable by one, whatever the voice is doing
(`docs/brain-build.md`: Controls, `lamp-controls`: "the touch pad and the
button: stop, mute, privacy").

**What to build:** a crate `crates/controls` and its process
`lamp-controls`. The reading of the pins sits behind one small trait, with
a stand-in for tests; the first builder writes the real one on the lamp.
What is built here is everything above it: telling a tap from two taps
from a long press on a clock that is passed in, with V1's measured
debounce where its verdict says to keep it; what each means; and saying
so on the bus in a type of its own. Privacy is the one that must not be
got wrong: read `docs/service-privacy.md` for what the voice promises and
propose how a press reaches it.

**Hand back:** the part, its tests, the document, and what the voice and
the body must subscribe to.

## 8. A shared log, settings, and reports from the parts that give none

Documents: `docs/bus.md` ("What is taken from ROS"), `docs/health.md`,
`docs/msgs.md`.

**Why.** The owner asked for "all the helpful topics ROS has for default
for users". Each part's own report and the lamp's health are done. Still
missing from that list: one log every part writes to and a person can
follow; settings a part declares, with their kinds and limits, that can
be read and changed while it runs and are kept; and reports from the two
parts that give none, the memory and the executive.

**What to build:** in `crates/msgs` and `crates/bus`, a log message and a
small way for any part to write one, and `lamp-bus` able to follow it by
part and by level; a settings part of its own (`crates/settings`,
`lamp-settings`), studied against what ROS 2 parameters do well and
badly. (The reports for `lamp-memory` and `lamp-actions` are no longer
part of it: Actions has reported to Health since 2026-10-14, and memory's
report is task 28.) Fourth builder, from 2026-10-14, 15:15, after 28 and
29.

**Hand back:** each with tests and its document; for settings, the first
three that are worth having on this lamp and who would read them.

## 9. Faster answers: what exists

Document: `docs/faster-answers.md` (new). Nothing is built.

**Why.** Half of all answers begin 1.98 s after the person stops, and the
speech service's first sign of an answer comes 1.15 s after the lamp says
the person has finished (`docs/conversation-latency.md`). The target in
that document is 0.3 to 0.5 s. No setting on the lamp closes that.

**Wanted.** A study, with its sources, of every way to a first sound
inside a second: the same service reached directly and not through a
relay; other services that take speech in and give speech out; speech to
text, a text model and a voice in a row, each streaming; deciding the end
of a turn sooner (task 1); beginning an answer before the turn has
surely ended and dropping it if the person goes on; what shipped voice
agents publish about their own timings and how they were measured. For
each: what it would save here, on what evidence, what it costs in money,
keys and code, and what it would do to the things this lamp already does
(staying out of other people's talk, being interrupted, its character).
Figures that are published and not measured here are marked as such.

## 10. A reference that follows the microphone's clock

Documents: `docs/duplex-listening.md` (the section dated 2026-10-14),
`docs/continuous-audio-clock.md`, `docs/aec-alignment.md`,
`docs/reference-timing-pilot.md`. Code: `crates/audio/src/{lib,drift,duplex,evidence}.rs`,
`crates/voice/src/reference.rs`. This task may touch `crates/audio`,
behind an option that is off unless set.

**Why.** The echo canceller (WebRTC's AEC3, the `sonora` port) is given
the speaker's sound to compare with the microphone's. On the lamp the two
are different USB devices with different clocks, and recordings show the
lamp's own voice arriving 91 ms after its reference at one moment and 131
ms forty seconds later, moving in steps of about 5.4 ms, then stepping
back and forth. A canceller cannot learn an echo that moves, so its
suppressor does the work and takes a person with the echo.

**Wanted.** First a rig with no lamp in it: a speaker's signal sent
through a path whose delay slides and steps as measured, heard by a
microphone on a clock some hundreds of parts in a million off, with a
second voice added at a known level; and the canceller as `crates/audio`
wraps it, measured on that rig for how much of the echo it takes off and
how much of the second voice it leaves. Then a reference that stays put:
the delay followed and the reference resampled so that the echo sits in
one place to within a millisecond as the canceller sees it. Then the same
two measures again.

**Hand back:** the rig as tests, the change behind its option, the two
measures before and after, and a command the first builder can run over
recordings from the lamp (`lamp-audio replay-aec` reads them today).

**Not shown by this task:** the lamp's second microphone has a chip of
its own that changes the sound as it goes, and a rig does not have one.

## 11. One task out to an agent and back, through Harness

Documents: `docs/harness.md` at the repository's root, and in
`docs/brain-build.md` "Step 5 in more detail: tasks through Harness on the
lamp" and "The other route, not taken".

**Why.** "Lamp, ask my coding agent to…" is what makes the lamp more than
a voice: work handed to an agent on another machine, and its result said
when it comes back.

**What to build:** a crate `crates/tasks` and its process `lamp-tasks`:
what was sent, to whom, what came back and what has been said of it, kept
across a restart, against the Harness contract as those documents give
it, tested with a Harness that is scripted. The repository's rule stands:
this side is the client only, the contract is the Harness team's, and no
other pairing, transport or credential is invented. Where the documents
leave the contract open, check the Harness team's own code if that
machine can read it, and otherwise list each assumption in the
log.

**Hand back:** the part with its tests, `docs/tasks.md`, and the list of
what has to be checked against a real Harness.

## 12. Starting, setting up and updating a lamp

Document: `docs/surround.md` (new). The owner's order puts this last.

**Why.** A lamp out of its box has to get onto a network, start every
part by itself after power comes back, and take new software without
anyone at a keyboard. V1 does all of that in Go (`system/`, with
`docs/setup-flow.md` and `docs/bootstrap-ota.md` at the root), and every
V1 component already has a verdict in `docs/v1-verdicts.md`.

**Wanted.** For each piece of it (starting at power-up and in what order,
first setup and Wi-Fi, updates and going back from a bad one, reset):
what V1 does, what the verdict says, what V2 keeps as it is, rebuilds or
leaves, and the smallest thing that would let a lamp be handed to someone
who is not a builder. Then build what can be built without a lamp.

## 13. Memory across days

Document: `docs/memory.md`. Read it whole, and in `docs/brain-build.md`
"Decision 12 in more detail: memory across days". This task owns
`crates/memory`. (Task 8 adds one thing there, the part's report on
`diagnostics`; leave that to it.)

**Why.** A person at the next desk remembers yesterday. The lamp keeps a
journal of what it took part in and a list of what it was asked to keep,
and finds things in them by whole-word match. `docs/memory.md` has, under
"What is taken from how others do memory", a table of what the best
memory systems do, what this one should do about each, and a last column
that says "No" for most of it. This task is that column.

**What to build,** each behind tests, none of it on the path of a reply:

- Ranked search over the journal and the kept list in place of whole-word
  match, Vietnamese included, inside the time the document allows a read.
- Dated observations derived from the journal, each pointing at the lines
  it came from. A newer one replaces an older one by a link, and both are
  kept.
- Forgetting that is real: "forget that" removes the thing and, through
  those links, everything derived from it, and a search afterwards finds
  none of it.
- What goes unused lapses, by the ages the document gives; what a person
  asked to be kept does not.
- The note a new session starts with: a small block under a hard cap
  (open promises, recent threads, lasting preferences), as it stood when
  the session opened.
- The pass that tidies up after a conversation has ended. It needs a
  model, and no model, key or money is to be had here: build it behind
  one small interface, with a scripted model for the tests, and say in
  the document what a real one would be asked and what would leave the
  lamp.
- Room for a person on everything kept, used by nothing yet: until the
  lamp knows who is speaking, everything is "said in this room".

**And the measure the document asks for and nobody has run:** the
retrieval score on LongMemEval-S (public, and its retrieval score needs
no model) for today's whole-word match, for ranked search, and for
whatever else is built, on one harness; and a small set of the lamp's
own, scripted conversations over several days with questions whose right
answer is "I don't remember" or "that was someone else".

**Hand back:** the crate with its tests, the document's table brought up
to date with what was measured, the request lines the voice would send
for anything new (`docs/memory.md`, "Requests"), and what it costs to
build for Linux ARM64 if a native library comes with it.

## 14. Whom is it said to: what exists

Document: `docs/said-to-whom.md` (new). Nothing is built. Read
`docs/attention.md`, `docs/policy.md`, `docs/group-talk.md` and
`docs/name.md` first: they are what this lamp has tried and measured.

**Why.** The owner's goal names it: the lamp should "know when the user
is not talking to lamp" and "know when to speak up and when not to". In
an office most speech near the lamp is for someone else. Today the lamp
goes by its name heard in the sound, by how busy the room is, by whether
a conversation with it is going on, and by the speech model's own
judgement; the next step planned is one judge that decides for each turn
from the signs of that moment.

**Wanted.** A study, with its sources, of how this is solved where it is
solved best: what the research calls device-directed speech detection and
addressee detection, what the shipped assistants and the social robots
do, what each uses (the sound alone, the words, where the person is
looking, who spoke last, how far away they are), how well each does and
measured how, and which public recordings exist to train or test on. Then
what of it this lamp could use with what it has (two microphones, a
camera that reports whether a face is turned to it, the name heard, a
decision model that is asked typed questions) and what it would take.
Figures that are published and not measured here are marked as such.

## 15. The lamp keeps its own records within a bound

Document: `docs/records.md` (new). Crate: new `crates/records`
(`lamp-records`).

**Why.** Found on lamp-4ace on 2026-10-14: its disk is 88% full. The
voice makes a folder for every start of its service under
`/var/lib/lampOS/voice/sessions/` (612 folders, 1.1 GB, about 0.4 GB a
day in an office) and nothing ever removes one. Other parts write too
(`lamp-bus record`, the journal). A customer's lamp runs for years and
nobody logs in to it.

**Wanted.** One small part, run on a timer, that keeps what the lamp
writes for itself within a bound of size and of age, set in one file,
with defaults a customer's lamp can live with (propose them and say why;
500 MB and 14 days for the voice's records is a place to start). Oldest
first. It never touches the folder of a service that is running (the
voice's is `…/sessions/${INVOCATION_ID}`; find a way to know which that
does not rest on a guess), never follows a link out of the folders it
was given, never deletes anything outside them, and refuses a folder
that is not of a kind it knows. It says what it removed and how full the
disk is on `diagnostics`. A dry run prints what it would remove and
removes nothing. What the lamp remembers is not its business: the memory
keeps and forgets by its own rules.

**Tests.** On a made-up tree, with the clock and the sizes passed in:
the bound by size and the bound by age; the running session kept
however old it is; a link that points out; a folder that goes away
while it works; a disk that says it is full; a dry run that changes
nothing.

**Hand back:** the crate, a unit and a timer for `deploy/`, the
document, and the one command to run on the lamp (the dry run) with what
it should print there. Most likely wrong on the lamp: what counts as in
use.

## 16. A check that the lamp cannot lean too far

Crate: new `crates/lean` (`lamp-lean`), a library the body can call;
`crates/body` itself is the first builder's. Document: `docs/lean.md`
(new).

**Why.** The old software refused any recorded movement that leaned the
lamp too far, after a lamp tipped onto its side on 4 September
(`docs/v1-verdicts.md` names the check). This one has none. Its
movements today are a few small poses; Expression (task 4) and
everything after it add more.

**Wanted.** From the five joint angles, the lengths of the lamp's parts
and their masses: where the lamp's centre of mass is over its base, and
whether that is inside the base's footprint by a margin. Arithmetic
only, no hardware. The lengths, masses and footprint are data in one
file. The first builder measures the real ones, so use stand-ins marked
as stand-ins, and say exactly which measurements are needed and how to
take each with a ruler and a kitchen scale. One function for a pose,
one for every step of a planned movement, and one for a whole clip of
Expression's offsets on top of a posture, so that a catalogue can be
checked before it is used; each answers with how far inside or outside
the margin it is. Read V1's check first and say what is taken from it.

**Tests.** Poses with answers worked out by hand; folded and standing
are inside; an arm stretched out level is outside; farther out is never
safer; nonsense in is refused.

**Hand back:** the crate, the list of measurements, and where the body
would call it, described and not written. Most likely wrong on the
lamp: the masses.

## 17. A second judge for the Policy: the lamp's own signs

Crate: `crates/policy` (the first builder's; this task is given leave to
add to it, in new files, and to touch `policy.rs` only where a second
judge is asked and combined). Documents: `docs/policy.md` and
`docs/said-to-whom.md`, which this task follows from.

**Why.** The owner's rule: several decision models may be used, each a
judge, and one policy combines them. Today there is one judge, Jev, asked
over the network in 0.5 to 1.1 s, which is too slow to stop an answer
before the lamp speaks. The study of whom speech is for (task 14)
recommends a local judge that fuses the signs the lamp already has and
answers in milliseconds, with the slow judge adding to it when it is in
time.

The case to build it for happened on the lamp the same morning
([attention.md](attention.md), "A working morning in the office"): one
short word said near the lamp in a room judged quiet, its name not
heard, nobody facing it, and the lamp answered. The slow judge was late
for exactly that one and the policy's stand-in rule said to answer.
Wanted from this task besides: when the slow judge has not answered,
and nothing shows the lamp was called, the choice is to look up and not
to answer.

A second case, the same day, and what the voice does about it until the
policy acts ([attention.md](attention.md), "A meeting beside the lamp"):
a meeting at the table beside the lamp, 75 things heard in twenty
minutes, the name heard four times in people's talk (0.928 to 0.993),
three answers to the table. Since 11:52 the voice itself holds every
answer in a busy room unless the text shows it was called, a
conversation with the lamp is going on, or the eyes' newest fresh look
has a face turned to the lamp (`LAMP_ATTENTION=busy`, reading
`humans/faces/tracked` as the policy does). That rule is the floor this
judge has to beat: compare against it on the same records, and say for
which of the policy's choices the two would differ.

**Wanted.** A second implementation of `Decider` that asks nobody: from
the state the policy already builds (was the name heard and how surely,
is a conversation with the lamp going on, how the room is, is a face
turned to the lamp and how fresh is that, how long since the lamp spoke,
how long the utterance was) it answers the same typed questions with
probabilities. Weights in one data file, with "not known" a value of its
own for every sign. Then the combining: the policy asks both, takes the
local judge's answer at once and the slow one's if it arrives before a
deadline, and logs each judge's answer beside the choice. Watching only,
as every new judge starts. Time one decision, first and repeated, and
say so with the machine.

**Tests.** Each sign alone moves the answer the right way and "not known"
moves it not at all; a slow judge that answers late is logged and changes
nothing; a judge that fails leaves the other's answer standing; the log
has both. The fit itself needs labelled records, which the first builder
supplies from played scenes: write the fitting as a small program that
takes such a file, and say exactly what one line of it must hold.

**Hand back:** the judge, the combining, the fitting program, the
documents, and the one command that shows both judges' answers for what
the lamp hears. Most likely wrong on the lamp: the weights, until they
are fitted on its own records.

## 18. Expression: holding back for someone, and a lamp at rest

Crate: `crates/expression` and its message types in `crates/msgs`.
Document: `docs/expression.md`, "What the voice says, as built".

**Why.** Three things showed when the voice's real messages were put
through the planner (2026-10-14). The planner cannot be told "I have
paused for someone and may go on": a step back from speaking retires the
turn's sound, so the voice says "speaking" through such a pause and the
planner shows a speaking face to someone who has just cut in. The planner
lists "explicit concentration setting" as missing for ever, because
nothing publishes it. And with nothing to say between turns, what a lamp
at rest looks like is one fixed pose.

What the body showed the same morning also belongs here: lamp-4ace's head
makes a dip of two to three degrees in about a second and starts 0.3 s
after it is asked ([body.md](body.md), "What Expression may add"). The
built-in nod is too small and too quick for it, and no nod of a second
fits in a pause when 0.6 s of quiet ends the turn.

**Wanted.** A nod as someone finishes speaking: begun in the pause that
ends their turn and carried on into thinking, sized and timed in the
catalogue for a head like this one (a clip that begins in one state may
go on in the next if the catalogue says so). A way for the voice to say
a turn is held back for someone and may resume (a state or an event of its own, with the sound already
queued dropped and new sound after it accepted), with the listening face
within 100 ms and the look to the speaker, and the return to speaking if
it resumes. A default for concentration that is "not concentrating" until
something says otherwise, so a switched-on planner is not left waiting.
And rest that reads as a living thing and not as a machine switched off:
small, slow and rare, never a scan of the room, quiet-only, and still
when someone is working (the repository's own rule 8 in `CLAUDE.md`).
Each behind the catalogue, so it is data.

**Tests.** Held back and resumed: the face goes to listening and back,
later sound is taken, and nothing queued before the hold is played
after it. Held back and given up: as an interruption today. The planner
switched on with nothing publishing concentration reports itself well.
Rest: bounded, never in a busy room, never when concentrating.

**What the lamp showed meanwhile (first builder, 2026-10-14, 12:30).**
Expression on lamp-4ace has refused 565 inputs since 09:43, nearly all
`voice/playout` as "expired, too far ahead, overlapping or out of
order". A recording of the topic over four answers at 12:08 (353
messages) shows why: each piece is dated rightly, by the speaker's own
queue, but that queue holds only about 20 ms, so a piece is published
1.7 ms (median) after it began to sound and has at most 18 ms left. None
was too far ahead or overlapping; the ones refused had played by the
time they were read. The voice will say its sound earlier, from the
350 ms it holds before the speaker (the first builder's, next). Until
then do not lean on `voice/playout` arriving ahead: follow the level
from what it says, and treat a piece that has just ended as heard, not
as a fault.

**Hand back:** the change with its tests, the message contract the voice
must then follow, and a replay fixture for each case. Most likely wrong
on the lamp: how the rest behaviour looks, which only the camera shows.

## 19. The end sent early, the answer played only when sure

Crate: `crates/voice`, with leave for this task to change the
coordinator's handling of the end of a person's speech and the start of
playback, and what those need; nothing else in it. Documents:
`docs/faster-answers.md` (way 2, whose study this follows),
`docs/conversation-latency.md`.

**Why.** Half of the lamp's answers begin 1.98 s after the person stops.
The service takes 1.15 s of that and no answer has ever arrived sooner
than about 0.85 s after the end was sent. The lamp waits 0.6 s of quiet
before it sends the end at all. If the end is sent after 0.2 s and what
comes back is held until the 0.6 s (or the end-of-turn model's verdict)
confirms the person had finished, every answer begins up to 0.4 s sooner
and nothing is played that would not have been played.

**Wanted.** Behind an option, off by default. The end is sent to the
service after a short quiet (a setting; 200 ms to start). The answer's
sound is taken in as it comes and not played. If the person is quiet to
the confirming moment, playback starts then, or at once if it is already
past. If they speak again before it, the request is given up exactly as
a continued utterance is today (`--provider-continuation kept-start`,
which the lamp runs): nothing of the answer is played, the model's
words for it are not recorded as said, and what they go on to say is
heard as the same turn. Whatever was told with the request (a name note)
must not survive it: see `docs/voice-review-2026-10-14.md`, finding 1. A
lone "Hey Lamp," followed by a pause and the question is the case most
likely to go wrong, and "Lamp, stop." over an answer the next.

**Tests.** Against the scripted service the voice's tests already use:
quiet to the end, and the answer starts at the confirming moment and not
before; speech resumed at 0.3 s, and nothing is played and one turn is
heard; an answer that arrives before the confirming moment; one that
arrives after; the link dropping in between; an interruption. Count
what is sent to the service in each. With the option off, every
argument and every message is what it was.

**Hand back:** the change, its tests, the documents, how many requests
are given up per turn in each test, and what to play to the lamp
(`pauses.sh`, `shorts.sh`, `checks.sh`). Most likely wrong on the lamp:
how the real service treats a request given up a third of a second in,
and what the model then makes of the rest.

## 20. The replay takes the lamp's own recordings

Crate: `crates/audio` (`replay.rs`). Document: `docs/steady-reference.md`,
"Taken by the first builder".

**Why.** `lamp-audio replay-aec` is how a change to the canceller is to
be judged on real sound before it is played to anyone. It does not take
the recordings the lamp makes as it runs today (the capture-clocked
reference): over four of them it stopped at the first boundary with
`insufficient_evidence_for_dsp_replay` and replayed nothing, and the
fourth's events it could not read. So the steady reference could not be
judged that way, and nor can the next change.

**Wanted.** The replay follows a capture-clocked recording through: the
same blocks to the canceller in the same order as the lamp gave them,
with what the lamp recorded of each (the reference's cursor for every
capture block is in the capture events). Where a recording truly cannot
be followed, say which record and why, and replay up to it. Tolerate an
events file whose last line was cut off. The first builder will put two
of the lamp's recordings where you can reach them if you ask for them in
your log, with no speech of people in them (played voices only); until
then, make recordings of that shape with the voice's own recorder
against the scripted devices.

**Tests.** A capture-clocked recording made in a test replays whole and
gives the canceller's own output bit for bit; a cut-off last line; a
boundary in the middle.

**Hand back:** the change, and the two commands to run over the lamp's
recordings with what to compare. Most likely wrong on the lamp: a kind
of record its real recordings have that a made one does not.

## 21. The voice stops and mutes on the hand controls

Crates: `crates/voice`, with leave for this task to add a consumer of
`controls/state` to the coordinator and what it needs, and
`crates/controls` (yours to extend). Documents: `docs/controls.md`
("Topics and the receiver", "Voice and physical privacy", "The board's
reader").

**Why.** The lamp now reads its real button and head pads
(`serve --pins`), and nothing acts on them. A lamp on someone's desk
needs a hand to be able to stop it talking and to silence it, without
the network and without the model.

**Wanted.** Each behind its own switch, off by default. A Stop from
`controls/state`, taken through `Receiver` so that a repeated or stale
one does nothing: the answer in hand is cut off at once, locally, as an
interruption is (the speaker is told to drop what it holds; nothing
waits on the service), and the turn is given up. A mute: the microphone
permission is revoked at once and stays revoked across a restart of the
voice (`MuteRecord`, restored before any worker is opened), and is
cleared only by an explicit local act (a second hold, as the controls
document proposes, or say otherwise and why), never by a restart and
never by the model. The physical privacy slide stays above all of it. A
pat is only reported. Measure, with the injected clock, from the
observation to the speaker being told to stop, and say it.

**Tests.** Stop while speaking, while thinking, while listening, with
nothing in hand, twice, and a stale one; mute and restart; mute, slide
closed, slide open; a controls process that dies and comes back; a
Stop that arrives as the service's link drops.

**Hand back:** the change, its tests, the switches' names, and what a
person does at the lamp to see each (the first builder has a hand
available only now and then: make it a list of five actions and what
each should do). Most likely wrong on the lamp: the polarity of the
head pads, which is set from the lines at rest and not from a touch.

## 22. The body on a second lamp

Crate: `crates/body` (yours). Documents: [body.md](body.md), and a folder
of your lamp's own under `deploy/`, named after its unit (as
`deploy/lamp-4ace/` is).

**Why.** Only lamp-4ace has ever run lampOS. Every lamp is put together
by hand: where each servo reads when the head rests, how far each joint
can go before it meets the lamp itself, how hard each one works. A
customer's lamp will be a second lamp, so this is its rehearsal.

**What exists.** The body as it runs on lamp-4ace: its unit,
`deploy/lamp-4ace/lamp-body.service` (`--home`, `--reach`, `--stand`,
`--folded-arm`, `--gaze`, servo speed and acceleration, the force limit),
and how those were found: [body.md](body.md), "First run on a lamp" and
"Second and later runs on the lamp", and `deploy/lamp-4ace/RUNBOOK.md`.
`lamp-body-inspect` reads every servo without moving anything. The body
does not need the voice: it takes the voice's states on a socket
(`--mood-socket`, [body.md](body.md), "What moves it"), and a small
script that writes them is enough to make it listen, think, speak and
stand.

**Wanted.** On your lamp, in this order: what it runs today, written
down and backed up; the servos read at rest with nothing moving; your
lamp's own home, reach, standing pose and folded arm, found the way
lamp-4ace's were and written into your folder's `lamp-body.service`;
then the body running there, driven through listening, thinking,
speaking, standing up and folding down, a few times each. Every way your
lamp differs from lamp-4ace goes into a table in [body.md](body.md)
under a heading with your lamp's name. Where the body refuses to start
or stops itself (it does: a load, a temperature, a reach), keep the
refusal, say why, and mend the body if it is the body that is wrong.

**Hand back:** your lamp's folder, the table, and the body as it runs
there. Most likely wrong: the reach of the roll joint (on lamp-4ace it is
blocked in the folded pose) and the yaw servo coming to rest short of its
goal.

## 23. Waking up after the power comes back

Crate: `crates/body`. Document: [body.md](body.md), a new heading.

**Why.** A customer's lamp must come back by itself after a power cut. On
lamp-4ace the voice and the light do; the body and the eyes are started
by hand, and the body refuses to start with the head resting on the
base, which is where a lamp without power ends up. What its servos do at
power-up has never been looked at on any lamp.

**Wanted.** First, with your person's hand on the lamp and their unplug
and replug, what every servo reads and does from the moment power comes
back, before anything of ours runs (five times; say where the head was
each time). Then a start that works from wherever the lamp is found:
read every servo before moving one, never jump, and rise the way
something alive wakes, slowly and a little at a time, not the way a
machine homes (the repository's rule 8). Then the body's unit with an
`[Install]` section, so it starts at power-up, behind its own switch,
off unless set. Measured on five more unplugs with it installed: time
from power to the first movement and to standing ready, the path the
joints took, the highest load and temperature on the way.

**Hand back:** the wake, the unit, the ten unplugs' table. Most likely
wrong: a servo that reports a position before its torque is on, and a
head that drops when torque comes on.

## 24. Giving way to a hand, and not hurting itself

Crate: `crates/body`. Document: [body.md](body.md), "The force its
servos are allowed".

**Why.** People will push the lamp's head, hold it, move it to point the
light. A lamp that fights a hand hurts the hand or itself; one that
gives way and comes back gently is a joy. On lamp-4ace the three joints
that carry the lamp push with at most 600 of 1,000 since 2026-10-14, and
no hand has tried it. The old software had a contact profile and a
servo overload rule (`robots/lamp/contact_profile.json`,
`robots/lamp/servo_overload.json`): judge them before taking anything
([v1-verdicts.md](v1-verdicts.md)).

**Wanted.** Measured by hand on your lamp, standing and folded, still and
moving: pushing the head and the arm each way, how hard it is to move
(press through a kitchen scale for a number), what the servos read
(load, position, temperature, current if there is one) and what the
lamp does after: holds where it was put, springs back, or comes back
gently a moment later, which is what a person does. A joint held still
against its goal must never cook: say how long it may stall and what it
does then. Then the setting and behaviour you choose, behind a switch,
and the same pushes again with it.

**Hand back:** the measurements before and after, the behaviour, the
settings for lamp-4ace. Most likely wrong: the elbow, which on lamp-4ace
reached 69 °C one day with the head resting on the base.

## 25. How the head follows, and a nod a person can see

Crate: `crates/body`. Documents: [body.md](body.md) and
[expression.md](expression.md) (the third builder's; propose, do not
change its catalogue).

**Why.** The part that plans the lamp's movement and light (Expression)
times its head movements in milliseconds, and does not know how this
head follows. On lamp-4ace on 2026-10-14, asked for a dip of 3.5 degrees
in 0.8 s, the head dipped 1.5 to 2.7 degrees about 0.3 s late; a nod
took about a second, too slow to fit inside a pause in someone's speech.
The third builder is building a nod as someone finishes speaking (task
18): it needs to know what this head can do.

**Wanted.** On your lamp, for head pitch, head roll and base yaw: how
the joint follows a step and a ramp at the body's own settings, read
from the servos at the body's own step rate (delay, rise time,
overshoot, where it settles). Then what the servos' own speed,
acceleration and stiffness settings allow: the quickest small nod (two
to four degrees, down and back) that still looks soft and natural to
your person, and what it costs in load and heat. Numbers in
[body.md](body.md); the values Expression's catalogue should hold, with
why, in your log.

**Hand back:** the table, the settings, the proposal. Most likely wrong:
the step rate of the reads, which limits what can be seen.

## 26. Weighing and measuring the lamp for the lean check

Crate: `crates/lean` (the fourth builder's library; add a data file and
tests, do not change the library). Document: [lean.md](lean.md),
"Measurements the first builder needs" and "lamp-4ace, 14 October".

**Why.** The check that a pose cannot tip the lamp over runs on lamp-4ace,
but only with the old software's model of the lamp, whose parts add up
to 1.5 kg where the lamp's own specification says 7 kg. Until the lamp is
weighed and its base measured, the body cannot use it.

**Wanted.** With your person: the whole lamp weighed; the base's outline
where it touches the desk, drawn and measured; and as far as the lamp
can be taken apart without harm, each moving part weighed and where its
weight sits, by the methods in [lean.md](lean.md). Your lamp's file in
the check's form, marked `measured` only if every value was. Then the
check run with your lamp's real readings at rest and standing, and,
carefully, with a hand ready and nothing allowed to fall, one pose near
its predicted edge to see whether the prediction holds.

**Hand back:** the file, the readings, the one test at the edge. Most
likely wrong: which part each motor and cable belongs to.

## 27. The keeper: one lamp of many parts, started, watched, stopped and resumed

Take it first: the owner, 2026-10-14, "let's send that task to 5th
builder", and the fifth builder's person wants to solve it. It needs a
lampOS part running on your lamp to keep: install the body as task 22
says (its home, reach and first runs) as the first step of this one, and
carry on with the rest of task 22 after. Crate: new `crates/keeper` (`lamp-keeper`), yours. Units: your
lamp's folder under `deploy/`, and templates for every lamp in
`deploy/units/`. Document: a new `docs/keeper.md`.

**Why.** The lamp is a dozen processes (voice, body, eyes, name,
Expression, controls, memory, policy, health, actions, and setup and
updates when it is installed), each its own systemd unit. systemd starts
each and restarts one whose process dies. Nothing more: a part that is
alive but stuck (its loop hung, its link down) runs on; the lamp has no
single "start" or "stop"; there is no order between parts; and the
update and reset choreography of task 12 (`docs/surround.md`, "the
owner protocol") needs a supervisor that does not exist, so no update can
be installed for real. `lamp-health` already folds every part's
`diagnostics` into one report on the bus (`lamp/health`), and nothing
acts on it.

**What exists and what we take.** systemd is the process manager: do not
write another. Study first, briefly, how ROS 2's managed nodes and its
diagnostics aggregator, Android's init and watchdog, and launchd do this,
and say what is taken.

**Wanted.**
1. A `lampos.target` and the units' order and dependencies, so the lamp
   starts and stops as one, in order, at power-up and by one command; the
   voice in a slice that keeps its priority; restart limits, so a part
   that keeps failing is stopped and reported, not restarted forever.
2. A watchdog for each part: "ready" said when it really is, "alive"
   from its main loop (systemd's notify protocol), so a hung part is
   restarted, not only a dead one. A small helper in `crates/keeper` that
   any part can call; you wire it into the body (yours); the first
   builder wires it into the others when it takes this.
3. `lamp-keeper`, its own process: reads `lamp/health`; restarts a part
   that has been in error for a while, with a bound; reports one that will
   not stay up; and runs task 12's choreography for an update or a reset
   (quiet for 45 s, the voice closed to new turns, the body at rest, stop
   in order, swap, start, check, roll back), against the owner protocol in
   `docs/surround.md`.
4. One place to ask and command: what runs, which build, whether it is
   well; start, stop, restart a part, or the lamp. A command line and a
   topic on the bus.
5. A few named states each part says on the bus (starting, ready,
   paused, stopping), and a "pause" each honours, so stop and resume can
   be done without cutting anyone off: the voice stops taking new turns,
   the body goes to rest, Expression holds still.

Every action behind a switch, off unless set, as always; nothing restarts
the voice while someone is speaking to the lamp.

**Tests.** A hung part restarted; one that keeps failing stopped and
reported; the order at start and stop; an update and a rollback through
the choreography with stand-in parts; nothing done while someone speaks.
On your lamp: power-up to every part ready, timed; a part killed and a
part hung, each recovered, timed.

**Hand back:** the crate, the units, the document, and the one command
that shows the lamp's parts and their state. Most likely wrong on a lamp:
the order the parts need at power-up (the body before the eyes? the
voice before the light?), which only a lamp shows.

## 28. Memory says it is well, for the lamp's health

Crate: `crates/memory` (yours). Document: [health.md](health.md) and
[memory.md](memory.md).

**Why.** Since 2026-10-14 the lamp's health hears from nine parts and
says "The lamp is well." only when every expected one is; memory is the
one that does not report, so a memory that has stopped answering goes
unnoticed until someone asks the lamp to remember.

**Wanted.** Once a second on the bus's `diagnostics` (`lamp_msgs::Diagnostic`,
name `memory`), as the actions part does since that afternoon
(`crates/actions/src/server.rs`, `report`): well, with how many exchanges
and kept things it holds and its slowest recent answer; a warning only for
what is wrong with it (the store failing to open or write, answers slower
than the voice allows, 300 ms for the note and 1.5 s for a call). Behind
`--bus-root`, as the other parts. Its socket loop must not wait on the bus.

**Hand back:** the change, a test of the report, and the line the first
builder adds to the health unit (`--expect memory`).

## 29. The voice uses what memory across days adds

Crates: `crates/memory` (yours) and, with leave for this task,
`crates/voice/src/remember.rs` and the place in `crates/voice/src/thread.rs`
that asks for the session note; nothing else in the voice. Documents:
[memory-days.md](memory-days.md), [memory.md](memory.md).

**Why.** Memory across days is on, on lamp-4ace, since 14:50 that day
([memory-days.md](memory-days.md), "Switched on"): 415 exchanges over two
days, a recall in 4 to 6 ms. The voice still asks as it did before the new
store: the newest entries for a recall, the kept list for a new session's
note, and nothing after a conversation. What the store adds is unused.

**Wanted.** The voice asking the new store what it is best at: a recall
ranked by what was asked (with the day when the person names one), a new
session's note as the store freezes it with its provenance, and the
tidy-up after a conversation sent where the store expects it (no model is
configured on the lamp; the request must cost the voice nothing when none
is). Off unless set (one switch in the voice's environment), and the
voice's latency rules kept: the note within 300 ms when a session opens,
a call within 1.5 s, the voice never waiting on memory for anything else.

**Tests.** Through the real store in a temporary directory: a recall by
word returns the right exchange from an earlier day; the note a new
session gets; a conversation's end reaches the store; memory gone or slow
costs the voice its deadline and an honest answer, nothing more.

**Hand back:** the change, the tests, and the played scenes for the first
builder (`~/lamp-voice-data/tools/memory.sh`) with what each should show.

## What the lamp showed

Newest first. Each builder's own notes are in its log, on its branch.

**2026-10-14, 18:00 to 19:35, on lamp-4ace (first builder).** Spoken
over, six played runs alternating the echo reference: as recorded, the lamp
stopped for 13 of 28 sentences and 5 of 17 stop words, with 3 pauses for
nothing; steady, 14 of 27 and 5 of 16, with 35 pauses for nothing and 6
stories it gave up after hearing itself. Steady stays off
([steady-reference.md](steady-reference.md)). Two more ways of hearing a
person over the lamp were measured and are no basis: the room
microphone's own canceller, and the name part hearing "Lamp"
([duplex-listening.md](duplex-listening.md), the table of ways tried). A
camera cue is now #720 (fifth). The glance from rest (#710, pull request
#718) is on lamp-4ace; the voice tells a stuffy room once and counts a
name heard close as a call (both on since 18:46); the clock answers in
spoken words (built, going on next).

**2026-10-14, 16:10 to 16:57, on lamp-4ace.** New parts and pieces, each
built for the lamp and running there: the ears (`lamp-ears`, the first
builder's: the room microphone's level and sudden sounds, 30 dB over the
quiet, [ears.md](ears.md)); reminders and timers (task 6, `lamp-schedule`:
its lamp check passed, ack median 25 ms; running empty until the voice is
wired, #695); memory's health report (task 28, #697, done); the fifth
builder's wake mends and the keeper's clean-up (#702, #701; the wake runs on
lamp-4ace's body, its 300-count base test waits for an empty room).
Expression can glance at a sudden sound (catalogue switch, on in lamp-4ace's
catalogue) but did not on the lamp when one was published by hand; being
found. Health hears twelve parts: "The lamp is well." The hourly records
job failed once at 16:00 on a process that ended mid-scan, removing nothing
(#707, the fourth builder's). The lamp's disk: 89 % used by the first
builder's old build caches; 57 % free after removing them.

**2026-10-14, task 3, the stop listener's first look on lamp-4ace.** Over
5.5 minutes of the second microphone after the canceller, recorded while
seven short words were played at a story, the model (run by `lamp-name
check` on the lamp) heard none of four "stop" and one "wait"; the best
score was 0.54, for a "Stop!" said with the lamp silent. Two false alarms
(0.94, 0.92) came during played requests. Not wired; tonight's stop work
goes on without it. A person's live "stop" is the next look
([stop-word.md](stop-word.md), "First look on lamp-4ace").

**2026-10-14 afternoon, played on lamp-4ace from 13:30 (the owner:
"test 24/7").** The fast end-of-turn worker (task 1) is on since 13:21:
it took "Hey Lamp," for a finished turn and cut the name off its question
until the first builder gave its early commit a floor of 0.8 s of speech
(`bea2dd1c9`); with it, 4 of 5 pause scenes answered, about 0.5 s sooner
([endpoint-pilot.md](endpoint-pilot.md)). The end sent early (task 19,
`af9ff59c5`, `e251ef842`) made the scenes worse and is off
([faster-answers.md](faster-answers.md), "The end sent early, played on
lamp-4ace"); back to the second builder below. The clock answered three of
three; the voice's sound said ahead (`d721c34c1`) brought Expression's
refusals from about a thousand an hour to 22 over the afternoon. The
busy-room rule kept out of 36 lines of talk and held three of four calls
for want of a face; a name heard at 0.999 or more now counts as a call
([attention.md](attention.md)). The records bound (task 15) is on, at
2 GB, hourly ([records.md](records.md)). The hand stop (task 21,
`5f9914314`) is on `lamp-voice` and on the lamp with its switches off,
waiting for a hand; the stop listener (task 3) is on `lamp-voice`; both are
for the evening's stop and interruption tests.

**2026-10-14, task 20, the replay of the lamp's own recordings.** Taken
onto `lamp-voice` unchanged (`03ebfbb3b`); its 17 replay tests pass on
the Mac and under emulation. On lamp-4ace, on last night's six played
recordings: five followed to their end and gave the lamp's own canceller
output sample for sample; the sixth was a recording never finished, and
the replay now says so (mend and test, first builder). With the steady
reference on those five, the echo removed while the lamp speaks rose in
two (12.2 to 14.6 dB, 13.4 to 16.2), fell in two (23.2 to 19.1, 14.6 to
13.1) and held in one, whose alignment moved more (10 to 18 changes).
Table in [steady-reference.md](steady-reference.md). So the reference is
not judged by this alone: tonight's played stops, with it on and off,
decide.

**2026-10-14, task 12, setting up and updating a lamp.** Taken onto
`lamp-voice` unchanged (`4f679d14a`, 107 files with its pinned copy of
the signed-update library). On the first builder's Mac its 260 tests
pass. Built for the lamp (13.8 MB; 17.0 MB with the NetworkManager form).
On lamp-4ace, at 12:25, its two checks that need no hardware: the
install replay passed all five of its scenarios
(`all_assertions_passed: true`), and the operator benchmark, in `/tmp`,
which on this lamp is in memory, not on its flash: status 0.14 ms on
first use and a median of 0.05 ms over 100 (worst 0.21); a durable
admission 1.9 ms first and a median of 0.20 ms (worst 1.7). Those
admissions are not durable figures, because nothing reached the flash.
Nothing was installed and nothing touched the network; the doctor and
the disabled service need a provisioned configuration, a service user
and the supervisor's owner protocol, which are the first builder's and
not done.

**2026-10-14, a meeting beside the lamp (the first builder's own part,
for task 17).** From 11:08 a meeting at the next table: 75 things taken
as said in twenty minutes, the name heard four times in people's talk,
and three answers to the table, none known to have been for the lamp. The
room was still taken for quiet until 11:14:50 and the model was told it
was busy only at 11:16:30. Changed, all on the lamp since 11:52
(`0ca828c67`), none yet played to: a quiet room becomes busy at two
unanswered things inside a minute; the room is told inside the request
when no quiet moment comes; in a busy room every answer needs the name in
the text, a conversation going on, or a face turned to the lamp; the name
note in a busy room waits for such a face; and the model asks the lamp's
clock for the time (it had told the table "roughly ten forty-five" at
11:17). Silent probes the same hour, with the busy note beside the sound:
colleagues' work talk left alone, "Hey Lamp, ... lunch?" answered, but
"What is 2 + 2?" and "How was your weekend?" answered too, which is why
the rule and not the note decides. All in
[attention.md](attention.md), "A meeting beside the lamp".

**2026-10-14, task 16, the lean check.** Taken onto `lamp-voice`
unchanged (`bbbd82fc9`). Its 63 tests pass on the first builder's Mac and,
built for the lamp, under emulation. On lamp-4ace at 11:05 the first command
the log gives printed what it said to expect (+0.09 m, 16 within, not
qualified, exit 0), the level arm exited 1 and a count of 4096 exited 2. On
the lamp one pose takes 0.003 ms at worst, a 1,000-step path 1.5 ms, the
whole program with its files 7.4 ms; a path that spends all its work 47 ms.

The lamp's motor readings come off `body/joint_states`: at rest
`2023,2052,2047,2053,1950`, standing `2023,2558,2503,2053,1820`. The first
software's body model, put in the check's form from its URDF and the motor
calibration kept on this lamp, agrees with the first software's own
function to 0.02 mm over 42 poses. It is not the lamp: its masses add up to
1.49 kg where the lamp's specification says 7 kg; the first software read
its motors' -100..100 values as degrees, so its 22 mm limit cannot be
carried over (scored its way, rest is 19.1 mm and standing 13.1 mm: it would
have played both); and nobody has measured the base. All in
[lean.md](lean.md), "lamp-4ace, 14 October". The file stays a stand-in,
the check is not wired, and what a person can do in ten minutes with a
kitchen scale and a ruler is listed there under "Measurements the first
builder needs".

**2026-10-14, task 7, the hand controls as a service.** Since 11:03
`lamp-controls` runs on lamp-4ace as its own unit, watching only, reading
the button and the two head pads every 5 ms and saying `controls/state`. Its
first start failed ("Permission denied": `/etc/lampOS` is root's and it runs
as `orangepi`); its two files now sit beside its build. Since 11:17 it
writes a line to its journal whenever a hand changes something, at most 20
in ten seconds, so the first real press or pat is kept whenever it comes
(two tests for that bound; standard output stays the closing summary, which
a test of the program caught). 1.4 % of one core. Nothing acts on it yet.

**2026-10-14, task 15, the bound on the lamp's records.** Taken onto
`lamp-voice` (`c998cd75e`; the workspace's list of crates merged by hand
with the controls'). On the first builder's Mac its 96 tests pass, one
at a time (one failed once in a parallel run and passed alone), and
clippy is clean. Built for the lamp.

| Where the library's 88 tests ran | Passed | Why the rest did not |
|---|---|---|
| Under emulation | 66 | 22 stop at "Function not implemented": the emulator lacks a system call the store uses. Says nothing of a lamp |
| On lamp-4ace itself (kernel 5.15.147), as an ordinary user | 85 | Two stop at "Operation not permitted" in the report's atomic write: it opens every folder from `/` down without touching access times, which only the folder's owner or root may ask for. The job runs as root, so this is the tests assuming root, not a fault on the lamp. One (`interrupted_unlinks_report_actual_progress…`) failed in one of two runs |

The first command the log asks for, on the lamp's real records, as root,
with the configuration as shipped (switched off):
`lamp-records check /etc/lampOS/records.json` printed `mode:"dry_run"`,
`enabled:false`, 1.15 GB before against the 500 MB bound, 480 of the 614
session folders planned for removal, 667 MB of them, nothing removed, and
as the one active session the running voice's own invocation, which
matches what systemd says it is. The folders were counted before and
after: 614 and 614.

On the lamp: the program at `/opt/lampOS/records/lamp-records` and the
switched-off configuration at `/etc/lampOS/records.json`. No timer and no
unit are installed, and removal is not switched on: those 480 folders are
three days of this lamp's measurements, the disk has 7.5 GB free, and
this lamp's bound should be larger than a customer's before anything is
removed from it.

**2026-10-14, task 10, the steady reference.** Taken onto `lamp-voice`
(`64ebb8b33`). The replay the log says to run first does not take the
lamp's recordings of that morning (it stops at the first boundary of a
capture-clocked recording with nothing replayed), so nothing was learned
that way. It is wired into the voice behind `--echo-reference steady`
and will be judged by what it is for: how many of the sentences and
short words played over the lamp's own voice stop it, with and without,
the same evening ([steady-reference.md](steady-reference.md), "Taken by
the first builder"). Not on the lamp yet.

**2026-10-14, task 7, the hand controls.** Taken onto `lamp-voice`
unchanged (`1c765da78`). On the first builder's Mac its tests and the
message types' pass (159, one at a time) and clippy is clean. Built for
the lamp; under emulation 84 pass and one fails
(`sigterm_finishes_recording_and_releases_the_actual_serve_owner`: a real
signal to a real child, which the emulator does not carry as a lamp
does); that one was not run on the lamp. On lamp-4ace the written
scenario replayed with the counts the log gives: 901 records, 900
observations, none refused, one fault (the overflow written into it), six
gestures. The gestures' times were not read off there.

What the lamp has for it, looked at without touching anything: two GPIO
chips, no gpio tools installed, and nothing on its I2C buses at the
address a base touch controller would have, so on this lamp the controls
are the button, the reset button and the head's touch pads
(`robots/lamp/docs/physical-controls.md` in the older software names
their lines).

The reader of those lines is built (`crates/controls/src/pins.rs`,
`serve --pins FILE`, and `probe`; [controls.md](controls.md), "The
board's reader"). On lamp-4ace the probe found the button and two head
pads wired, and twenty seconds of watching took 3,232 observations of
them at rest with none refused. Still to do, and the first builder's: the
voice stopping and muting on `controls/state`, the body's stop, and a
person's hand on the lamp, which is also what will show which way the
pads' lines go when touched.

**2026-10-14, task 1, the fast end-of-turn model.** Taken onto
`lamp-voice` (`a98e18a3f`); one test mended for a machine no library is
pinned for (`6a1d2745c`). Cross-built for the lamp, and Microsoft's
library found to be the pinned one. On lamp-4ace, thirty runs each with
everything else running: 382 ms as it was, 148 ms fast (160 at worst),
213 ms fast on one thread; the check exits 0. Every one of the five
things the log named as likely to go wrong went right: the table is in
[endpoint-pilot.md](endpoint-pilot.md), "Measured on lamp-4ace". Under
emulation one older test of the worker's client fails
(`missing_executable_fails_at_once`: the emulator starts a missing
program and then reports it gone, where a real system refuses to start
it); it is not a fault of this change and was not run on the lamp.
Nothing uses the fast worker yet: the voice's switch for it, and the
played scenes that must come before it is on, are the first builder's.

**2026-10-14, task 13, memory across days.** Taken onto `lamp-voice`
unchanged (`2defa611b`). The thing the log named as most likely to go
wrong, building its bundled database for the lamp, went right the first
time in the lamp's build container; the 106 tests pass on the Mac and
under emulation. On lamp-4ace: the timing test on the lamp's own disk
(of the repeated reads the slowest took 30.1 ms; the first import of
6,000 exchanges 2.3 s, once), and the new store beside today's on a copy of the lamp's
real records (the same answers for the newest, the kept and a day; for a
recall by word, other entries in three cases of six, because it ranks).
All of it is in [memory-days.md](memory-days.md), "Built for the lamp and
run on it". The new build runs on the lamp in its old mode since 09:50;
the new store is not switched on, because switching it on cannot be
undone without a restore and the voice does not yet use what it adds.

**2026-10-14, task 14, whom is it said to.** The study is on `lamp-voice`
([said-to-whom.md](said-to-whom.md)). Its recommendation, one local
decision per turn from the signs the lamp already has, with the slow
judge adding to it when in time, is task 17.

**2026-10-14, task 4, Expression, first ten minutes with people in the
office.** Eight things were taken as said and none answered. The planner
took 16,971 inputs and refused one (a kept value older than its limit,
read as it started). Two things were wrong and are mended (`93ec362c4`,
on the lamp since 09:43): the lamp's own health report read "The lamp has
a warning: expression: voice turn cancelled" after a colleague's remark,
because the voice ended an unanswered turn as `Cancelled` and the planner
reports that, and any note about one turn, as a warning. The voice now
ends such a turn as `Finished`, the planner warns only for a fault of its
own or an input it lacks once switched on, and it says in its journal
which input it refused and why. What is left of it is task 18.

**2026-10-14, task 4, Expression: its inputs from the voice.** The voice
now publishes `voice/timing`, `voice/activity` and `voice/playout`
(`crates/voice/src/expression_out.rs`, `40e9b1c00`; what each is made
from is in [expression.md](expression.md), "What the voice says, as
built"). Passed through `lamp_expression::Engine` in a test on the Mac
before it went on the lamp, twelve made-up seconds of the voice gave 264
messages, none refused, and the planner planned its nod, glance, sway and
yield. That test found one fault in the planner, mended in
`crates/expression/src/engine.rs` with a test of its own: a new turn
cleared an interruption's transition, and on a lamp the person who
interrupts is heard at once as a new turn, so the yield would never have
shown. On lamp-4ace since 09:26, with `lamp-expression serve` as a unit
of its own and no `--enable`:

| Looked at on the lamp | What it showed |
|---|---|
| `voice/timing` | Said every 80 ms: turn 0, resting, able to keep what is said, the kernel's boot identity as its clock and the service's `INVOCATION_ID` as its session |
| `expression/state` | Not enabled; no behaviours; missing: measured body lag, explicit concentration setting, fresh room state (the room was still "unknown" a minute after the voice started, and is said again every second) |
| `lamp-health` | "The lamp is well.", with `expression  shadow mode; no actuator requests` beside the six other reports |

Not shown: a turn through it. Nobody had spoken to the lamp since the
deploy and nothing is played while people are in. One limit is known and
written in [expression.md](expression.md): the planner cannot be told
"paused for someone, may go on", so the voice says "speaking" through
such a pause and its own listening face has to win there.

**2026-10-14, task 4, Expression.** Taken onto `lamp-voice` as the third
builder made it (`6226dae33`), nothing in it changed. On the first
builder's Mac its 67 tests and the message types' 25 pass and clippy is
clean; built for the lamp, they pass under emulation too. On lamp-4ace
itself, at 08:56:

| What was run | What the log said to expect | What the lamp showed |
|---|---|---|
| `lamp-expression replay` of the written scenario, switched on, two simulated seconds | 42 inputs, 101 steps, none refused; the glance back at zero at 1.16 s, 40 ms before the written first sound | The same: 42 taken, 0 refused, 101 steps, yaw at zero from 1,160,000 us |
| `replay_cost 100` (make the planner, read the scenario, plan two seconds, write the plan to nowhere) | On a desktop: median 0.19 ms | Median 0.78 ms, worst 1.26 ms; held to the board's small cores, 1.15 ms and 1.74 ms |

So the planner is no load on the board: a hundred steps cost what one
step is allowed. Nothing moved and nothing was lit; it opens no
hardware. What is not there yet is the first builder's: the voice's
three publications (`voice/timing`, `voice/activity`, `voice/playout`),
and the body and the ring taking its requests, each behind its own
switch. Noted for that: the written nod rises 0.035 rad at its highest,
about 23 counts of the head's servo, just over the body's 20-count
deadband, which is the first thing the log says to look at.

**2026-10-14, task 2, the review of the voice changes
([voice-review-2026-10-14.md](voice-review-2026-10-14.md)).** Taken onto
`lamp-voice` with its tests, and mended there:

| What the review found | What was done |
|---|---|
| 1. A note about one request reached the next one | The link keeps the note and writes it as the last thing before the end of the request's sound; a request that is given up takes its note with it (`crates/gemini/src/session.rs`). The note was off on the lamp from 08:15 to 08:51 |
| 2. A name heard after a request's sound ended let out its answer | The rule and the second look count a hearing only if it began before the request's sound ended (`attention.rs`, `second_look.rs`; the worker passes the end on) |
| 3. The lamp's own second look used up a new session's two held requests | A turn the lamp wrote itself is not counted among them |
| 4. A plain hearing just before a request was lost to a fainter one | The plainest hearing in the 1.5 s before a request is the request's |
| 5. A hearing could be said with a score over 1 | Held to 1 |
| By reading: a wordless answer got no second look | It is owed one like a silent answer |
| By reading: a second look could go to a session that never heard the call | It goes to the session that heard it, or is dropped |
| By reading: a hearing cut short by lost sound was dated 0.8 s early | Dated from its first high score |
| By reading: the worker said nothing if it could not take hearings from the bus | It says so in the journal |

Three of the review's tests were changed, and why: the two in
`crates/gemini/tests/` waited for the note on the wire the moment it was
offered, and now expect it just before the end of the request's sound,
which is the mend; the worker's `review_sound_a_name_heard_as_a_request_began…`
keeps its request's sound going while the name is heard, because the
rig's own requests are over in milliseconds and a name after the end no
longer counts; and `review_sound_the_note_is_offered_once…` took 0.97 for
a faint score, which it was when the bar was 0.98 and is not at 0.9.

On lamp-4ace since 08:51 (voice `4bec0602…`, name `a6045715…`, both
built from `a3e36ce93`), with the note switched on again. Before that the
new place of the note was tried against the service without a sound,
three recorded questions of three answered and the link up after each
([attention.md](attention.md)). Not played to: people are in the office.

Left as they are, each needing the lamp's own microphone or a longer
look: the lamp hearing its own name in its own answer; a late answer
arriving after a second look was sent; "Hey Lamp", a long pause, then
the question; and a silent second look counted as talk the lamp left
alone. The trade-off the review names (the words taken from the text as
a call are also how ordinary talk opens) stands as it is for now.

## Answers

**To every builder, 2026-10-14, 16:41: tasks are GitHub issues now
(`fifth-P3`).** The owner left it to the first builder ("decide what you
think best"), who took it, ordered by one pinned Queue issue (#705), with
the fifth builder's joining document (`fifth-P5`) and a role file for each
builder. Your next task: the first issue in your list in #705 that is
open and not `status:taken`; claim it as joining.md says. Questions and
proposals go to the issues now; this section keeps the answers given
before. The ready line (`fifth-P4`) holds for everyone; since `fifth-P6`
(#716) it is one pull request per task, merged by the first builder.

**To the fifth builder, 2026-10-14, 16:37, your proposals P2 to P5.**
`fifth-P2` (`lamp-owner-file`): yes; `deploy/lamp-52e6/LAMP.md` is on
`lamp-voice`, and lamp-4ace gets one from the first builder. `fifth-P4`
(`ready-line`): yes, from now on for every builder: the first builder
takes only what a log entry marks `Ready for lamp-voice: yes (<commit>)`.
`fifth-P5` (`joining`): yes in principle; the first builder reads it whole
and takes it onto `lamp-voice` with the role files, saying here what it
changed; until then this file holds. `fifth-P3` (`tasks-as-issues`): the
first builder recommends it, ordered by one "Queue" issue; it waits for the
owner's word, as your proposal asks. Make no issue or label until this
paragraph says the owner said yes.

**To the fourth builder, 2026-10-14, 16:04, task 17 (labelled rows).**
The file is in
[verification/policy-scenes-lamp-4ace-20261014](verification/policy-scenes-lamp-4ace-20261014/README.md):
89 rows (27 said to the lamp) from three ten-minute runs of
`lamp-policy-judges` on lamp-4ace during played busy-room scenes, the
third with many calls, labelled from the scenes' script by `label.py`
there, split by scene. `lamp-policy-fit` took it as it is (train 35,
calibration 26, test 28). The fitted choice never answers: on the test
rows it held all 8 calls (12 looks, 16 quiet), where the policy's copy of
the busy-room rule let 21 through with 14 false answers. Three things for
you: is a choice that never answers what the calibration should give with
3 train calls, or should a call heard as a call (name or text) lift it
past "look up" on its own; the `same_snapshot` copy and the voice part
somewhere (the voice spoke into none of the 63 two-people talk lines);
and say what the voice should publish as its `voice_first_output`
snapshot and I will add it. This comes after 28, 29 and 8 unless you
judge it small.

**To the fifth builder, 2026-10-14, 15:22, task 23 (the wake on
lamp-4ace).** Taken onto `lamp-voice` (`2bbb737af`, `e7aa58f60`, with your
proposal `bd33947cc`) and run on lamp-4ace as you ran it on lamp-52e6. It
did not reach home either way: toward a base home 286 counts round, the
base stuck at 2,101, 34 behind its goal, after 72 counts (`tracking`);
the other way, after 189 counts the shoulder had swayed 7 counts, one over
its 6 (`drift`). The ordinary start brought the base 177 counts home with
nothing tripping. The reports are in [body.md](body.md) under "Waking". The
body is yours: whether the wake should carry a base that sticks at a
slow goal and an arm that sways a little as the base turns, and how, is
your call. lamp-4ace's service runs with `--wake on` since 15:21, since an
early end is no worse than the refusal; I will take the next build onto it
the same way.

**fifth-P1 (`builder-with-a-lamp`): yes, but.** Taken as written
([builder-with-a-lamp.md](builder-with-a-lamp.md)), with one addition the
owner asked for: before a piece goes onto `lamp-voice` the first builder
builds it for lamp-4ace and starts it there once with its switch off, for
a minute, to see that nothing changes; a piece switched on for lamp-4ace
runs there for a few minutes first. Your evidence is not run again. Done so
for the body at 15:00: it ran on lamp-4ace unchanged.

**To the fifth builder, task 25 (the nod).** Yes: the body is yours, so
let it pass Expression's nod at the speed the nod needs (about 300 counts a
second for that one movement), behind a switch, and write the values for
Expression's catalogue in your log; the third builder or the first builder
puts them in the catalogue. lamp-4ace's catalogue already carries a body
lag of 0.3 s from its own trial (`deploy/lamp-4ace/expression-rest-lamp-4ace.json`).

**To the fifth builder, "Found, not mine".** lamp-4ace has neither: the
old software is gone from it and `/run/lampos-bus` there is root's with the
lamp's group able to write. Both belong to waking a lamp that still has
the old software, so they are yours under task 23.

**To the second builder, 2026-10-14, task 19.** Played on the lamp at
13:45 with `--early-end-ms 200` and the fast end-of-turn worker: worse
(see "What the lamp showed" and [faster-answers.md](faster-answers.md)).
What would help: an early end that does not tell the service the request
is over, or one taken back cleanly when speech resumes before confirming,
so that the service never hears half a sentence as a whole one. Not
urgent: stopping and interruption come first in your list.

**To the second builder, 2026-10-14, task 20.** (1) `capture_hold_worker.rs`:
mended in `46403ffbd` (it passes the echo reference now), and the first
builder's lamp build now compiles every voice test, so a Linux-only one
cannot break unseen again. (2) No need to fetch recordings: the replay was
run on the lamp's own, as "What the lamp showed" says. It follows them
exactly. Thank you for the precise note on where it was likely to fail.

