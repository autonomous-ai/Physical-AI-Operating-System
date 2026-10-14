# The builders: the running list

Since 2026-10-14 four sessions build lampOS at once, at the product
owner's request: "keep a running list of todos on github so that once
it's done it reads that list for new tasks and continue working. you
also read that list and see what's done and add more tasks. it will get
todo, build, test, merge."

This file, on the branch `lamp-voice`, is that list. The setup was
settled with the owner the same morning: each builder has a branch only
it writes to, so that the branch the lamp is installed from holds nothing
that has not been on the lamp.

| Builder | Where | Its branch | A task while it is built | Its log |
|---|---|---|---|---|
| First | At the office, with lamp-4ace | `lamp-voice` | | this file |
| Second | Another machine, no lamp | `lamp-second` | `second/<task>` | `lampOS/docs/work-log.md` on `lamp-second` |
| Third | Another machine, no lamp | `lamp-third` | `third/<task>` | `lampOS/docs/work-log-third.md` on `lamp-third` |
| Fourth | Another machine, no lamp | `lamp-fourth` | `fourth/<task>` | `lampOS/docs/work-log-fourth.md` on `lamp-fourth` |

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
6. Put it on your branch as a fast-forward, and in the same push write
   the task's entry in your log: what was built, what was tested and on
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
  `crates/gemini`, `crates/name`, `crates/audio`, `crates/body`,
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

Top first, each builder taking its own. The last column is the first
builder's: `open`; `taken` and `on its builder's branch` as that
builder's log reports them; then `on the lamp <date>`.

| # | Task | Short name | Builder | State |
|---|---|---|---|---|
| 1 | The end-of-turn model in a tenth of a second | `turn-end-fast` | Second | taken 2026-10-14 |
| 2 | Try to break last night's changes to the voice | `voice-review` | Second | taken 2026-10-14 |
| 3 | A listener for "stop" under the lamp's own voice | `stop-word` | Second | open |
| 4 | Expression: one part that authors what the lamp shows | `expression` | Third | open |
| 5 | The name, heard as a bare "Lamp" | `bare-name` | Second | open |
| 6 | Reminders and timers | `schedule` | Third | open |
| 7 | The button and the touch pad | `controls` | Third | open |
| 8 | A shared log, settings, and reports from the parts that give none | `defaults` | Third | open |
| 9 | Faster answers: what exists | `faster-answers` | Second | open |
| 10 | A reference that follows the microphone's clock | `reference-clock` | Second | open |
| 11 | One task out to an agent and back, through Harness | `tasks` | Third | open |
| 12 | Starting, setting up and updating a lamp | `surround` | Third | open |
| 13 | Memory across days | `memory-days` | Fourth | open |
| 14 | Whom is it said to: what exists | `said-to-whom` | Fourth | open |

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
badly; and the `diagnostics` reports for `lamp-memory` and `lamp-actions`
(this task may touch `crates/memory` and `crates/actions` for that
alone, as `crates/policy` does it).

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

## What the lamp showed

Newest first. Each builder's own notes are in its log, on its branch.

## Answers

