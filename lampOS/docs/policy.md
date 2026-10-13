# The policy: deciding what the lamp does

`lamp-policy` (`crates/policy`) is the part that decides what the lamp
does. This is its first version. It makes one decision, "is this said to the
lamp?", and it makes it in shadow mode: it says what it would do, on
[the bus](bus.md) and in a log, and nothing acts on it.

## What it is for

Today the model that holds the conversation also decides, by answering or
not, whether it was spoken to, and on the lamp it has answered people who
were talking to each other ([attention.md](attention.md)). A decision model
is another kind of thing: it is shown a `state`, one JSON value that says
how things stand, and typed questions about it, and it answers with
probabilities and never with text. The first one is TypeSafe's Jev. The
owner's rules for this part:

- Several decision models may be used. Each is a judge.
- One policy combines their judgements and makes the single choice. No
  judge acts by itself.
- Every model sits behind one common way of asking, so that a second model
  is a second implementation and not a second design.
- A new model or a new decision starts in shadow mode: it decides and is
  written down, and does not act.
- Every decision is logged with what was known, what the model said and
  what was chosen. That log is how the thresholds are tuned again, and how
  a model of the lamp's own will be trained later.

## The process

    lamp-policy --key-file ABSOLUTE_PATH --log-dir ABSOLUTE_DIR [--endpoint URL] [--model NAME] [--deadline-ms N] [--bus-root ABSOLUTE_DIR]

| Option | What it is | If left out |
|---|---|---|
| `--key-file` | A file whose one line is the key for the decision model's service. Space around the line is taken off. The key is never printed | required |
| `--log-dir` | Where the decisions log is kept. Made with mode 0700 if it is not there | required |
| `--endpoint` | Where the decision model is asked: `https://`, or `http://` for a server on the same machine in a test (127.0.0.1, ::1 or localhost). Plain http to any other machine is refused, because the key and what was heard would cross the network in the clear | `https://openrouter.ai/api/alpha/decisions` |
| `--model` | The model asked for | `typesafe/jev-1.13` |
| `--deadline-ms` | How long one asking may take, from 1 to 60000 | 3000 |
| `--bus-root` | The bus's directory | `LAMP_BUS_ROOT`, or `/run/lampos-bus` |

A wrong or missing option prints the usage and exits with 2. So does a
`--log-dir` that cannot be made, or that its group or everyone can enter. A
key file that cannot be read or does not hold one key, and a bus that
cannot be subscribed to, exit with 1 before anything is listened to.
`--help` prints the options.

It is its own process: with it gone the lamp talks on as before. It runs
until it is killed. It cannot catch a signal (the standard library has no
way and the crate takes in nothing else), so SIGTERM ends it where it
stands: a question that was out is not decided, and its sockets on the bus
are left for the voice's next message to remove.

It subscribes, each with its type from [lamp-msgs](msgs.md), to:

| Topic | What it takes from it |
|---|---|
| `voice/heard` | What was heard, a piece at a time |
| `voice/said` | What the lamp said: the pieces of its newest answer, put end to end |
| `voice/state` | Whether the lamp is speaking |

It publishes `policy/decisions` (`lamp_msgs/Decision`) under the name
`policy`, and says once a second that it is alive there, because the topic
is silent between decisions for longer than the bus's lease.

Nothing publishes `room/state` yet, so the room is `unknown` to it. Where
that subscription goes is marked in `crates/policy/src/policy.rs`.

## The decision: is this said to the lamp?

A lamp on a desk in a shared office hears much that is not meant for it.
Before it answers it has to judge whether what it heard was said to it.

**What is judged.** The voice publishes what it hears in pieces, as the
recogniser writes them. The pieces are put end to end into one utterance,
which is over when a piece is marked `finished` or when 300 ms have passed
with no further piece. Today the voice marks none, so the 300 ms end every
utterance. An utterance is judged once. One that reaches 2,000 bytes is
judged at that length and the rest begins another, so that talk which
never pauses is still judged, in parts.

**What is known.** With each utterance goes a context:

| Field | What it says |
|---|---|
| `heard` | The utterance |
| `room` | `quiet`, `busy` or `unknown` |
| `lamp_is_speaking` | Whether the lamp was speaking |
| `lamp_last_said` | The end of what the lamp had last said: at most its last 200 bytes, begun at a whole character |
| `seconds_since_lamp_spoke` | How long before the utterance the lamp had last spoken |

The last three are taken when the utterance's first piece comes, not when
the utterance ends. By its end the lamp may have begun to answer it, and
whether to answer must not be judged from the answer. The log is what the
weights are fitted from, and a context that held the lamp's own reaction
would give away what is to be learned. "Last spoken" is the later of the
last piece of what the lamp said and the last moment its state was
`speaking`, because the text of an answer can come before its sound has
been played to the end.

The lamp is **in conversation** when it is speaking, or when it said
something with a letter or a digit in it within the last 20 seconds. An
answer that was only "..." is the lamp keeping quiet aloud and starts
nothing.

**What the model is shown.** The state, with its fields in this order:

    {"heard":"...","lamp":{"what_it_is":"A small desk lamp robot that can talk. It sits on a desk in a shared office.","name":"Lamp","people_call_it_with":["Lamp","Hey Lamp","Lamp ơi"],"the_recogniser_often_writes_its_name_as":["Milan","Weiland","Wei Lan","Weimar","Helen","Liam","Lam","Lan","lamb","plan","Man","Hey man"]},"room":"quiet: nobody else has been talking","lamp_is_speaking_now":false,"lamp_last_said":null,"seconds_since_lamp_spoke":null}

`room` is `"quiet: nobody else has been talking"`, `"busy: people are
talking to each other"` or `"unknown"`. `lamp_last_said` and
`seconds_since_lamp_spoke` are null unless the lamp is in conversation, and
the seconds are given to a tenth.

**What the model is asked.** Seven questions, each answered with the
probability of yes. Every one begins "state.heard is what a speech
recogniser wrote for something just said near the lamp; it can mis-write
names and words." and goes on:

| Name | The question |
|---|---|
| `calls_lamp` | Does state.heard call the lamp: its name, one of state.lamp.people_call_it_with, or one of the mis-writings in state.lamp.the_recogniser_often_writes_its_name_as, at the start or the end of what was said? |
| `names_someone_else` | Does state.heard address some other person or device by name (for example 'Tom,', 'Samantha,', 'Hey Siri'), a name that is not the lamp's and is not in state.lamp.the_recogniser_often_writes_its_name_as? |
| `others_business` | Is state.heard part of people's own work talk with each other: reports, invoices, clients, meetings, calls, printers, lunch plans, who did or sent or told what, or a reply to something another person said? |
| `lamp_could_answer` | Could a friendly talking desk lamp sensibly answer state.heard: a general-knowledge question, the time or the weather, a joke or a story, small talk about how it is, a greeting or a good night, or a request about its own light, voice or volume? |
| `follows_lamp` | state.lamp_last_said is what the lamp itself said state.seconds_since_lamp_spoke seconds ago, or null if it has said nothing lately. Is state.heard said back to the lamp about that: a reply, a follow-up question on the same subject, or an instruction such as stop, wait, go on or something else instead? If state.lamp_last_said is null the answer is no. |
| `listening_noise` | Is state.heard only a listening noise or a scrap: mm-hm, uh-huh, yeah, okay, ha, a cough, one meaningless word, with no question and no request in it? |
| `asks_to_stop` | Does state.heard ask whoever is speaking to stop, wait, hold on or be quiet? |

The model is not shown the names. The wording, the order of the questions
and the order of the state's fields are all part of what was measured: the
weights below belong to them, and a test holds each to the letter.

## How the answers are combined

The seven answers, the room and the conversation become one probability by
a weighted sum put through the logistic curve:

    sum = 0.263
        + 0.722 x calls_lamp
        - 0.249 x names_someone_else
        - 2.042 x others_business
        + 1.132 x lamp_could_answer
        - 0.136 x follows_lamp
        - 0.645 x listening_noise
        + 0.031 x asks_to_stop
        + 1.771 if the room is quiet
        - 0.206 if the lamp is in conversation

    probability = 1 / (1 + exp(-sum))

| Probability | Choice | What it means |
|---|---|---|
| 0.8 or more | `answer` | Answer it |
| 0.4 to under 0.8 | `look_up` | Turn to whoever spoke and wait, without speaking |
| under 0.4 | `stay_quiet` | Go on as it was |

**Where the numbers come from.** They were fitted on 2026-10-13 on 346
labelled examples from the lamp's own records. On the half held out, 164
examples, a rule alone was right 74% of the time, and the model's answers
with these weights 87%. Where the combined probability was 0.8 or more it
was right 99% of the time, which is why `answer` begins there. They are a
first fit, to be made again as the log grows. That measurement was made
before this program existed, and not with it.

What the model cannot do: it cannot tell that the lamp was called when the
recogniser dropped the name from the text. So the next input is a sign from
the device itself that it was called.

**When no model answers.** The rule that was the baseline decides:
`answer` if the room is quiet or the lamp is in conversation, otherwise
`stay_quiet`. It gives the probability 0.74 for the first and 0.26 for the
second, which is how often it was right, and the decision says that the
rule made it.

**While the room is unknown.** A room nobody has judged is not taken for a
quiet one, so the 1.771 is never added. The most the sum can then reach is
2.148, a probability of 0.90. `answer` is then chosen only when
`lamp_could_answer` is near 1 and next to nothing counts against, or when
the lamp is plainly called as well: with `lamp_could_answer` at 0.9 and
every other answer at 0 the probability is 0.78, which is `look_up`. The
rule, for its part, then answers only in conversation. Decisions made now
will read as more reserved than the measurement, which had the room.

## Shadow mode

Every decision is published with `shadow: true` and nothing subscribes to
act on it. The voice answers or does not as it did before. What the policy
would have done is in the log, to be laid beside what the lamp did.

What is published on `policy/decisions`:

| Field | What it says |
|---|---|
| `about` | `attention` |
| `choice` | `answer`, `look_up` or `stay_quiet` |
| `probability` | What the choice was made from |
| `answers` | The model's answer to each question, by its name. Empty when the rule decided |
| `by` | The model as its service names it (`typesafe/jev-1.13-20260917`), or `rule` |
| `took_ms` | How long the asking took, answered or not |
| `shadow` | `true` |
| `request` | The request's number, if a piece of the utterance carried one. Today none does |

What was heard is not in the message.

## The log

One line of JSON for each decision, in `<log-dir>/decisions-YYYY-MM-DD.jsonl`,
the day by UTC. Its shape, with numbers made up:

    {"at_unix_ms":1791936000123,"about":"attention","shadow":true,"context":{"heard":"Hey Lamp, what time is it?","room":"unknown","lamp_is_speaking":false,"lamp_last_said":"It will rain this afternoon.","seconds_since_lamp_spoke":41.5},"in_conversation":false,"state":{...},"asked":"typesafe/jev-1.13","answers":{"asks_to_stop":0.09,"calls_lamp":0.07,...},"judgement":{"probability":0.6261,"choice":"look_up","from_rule":false},"by":"typesafe/jev-1.13-20260917","took_ms":655,"waited_ms":302,"usage":{"input_tokens":1066,"output_tokens":134,"cost":4.4772e-5},"pieces":3,"cut":false}

| Field | What it says |
|---|---|
| `at_unix_ms` | When the decision was made |
| `context` | Everything that was known, as above, whether or not the model was shown it |
| `in_conversation` | Whether the lamp was in conversation |
| `state` | What the model was shown. Its fields are here in the order of the alphabet; what was sent had them in the order above |
| `asked` | The model that was asked for |
| `answers` | Its seven answers. Empty when the rule decided |
| `judgement` | The probability, the choice, and whether the rule made it |
| `by` | Who decided: the model as its service names it, or `rule` |
| `took_ms`, `waited_ms` | How long the asking took, and how long it was from the utterance's last piece to the asking. The pause that ends an utterance is part of the second |
| `failed`, `failed_why` | Only when the model's answers were not used: `unreachable`, `timed_out`, `refused`, `unreadable` or `missing`, and one sentence |
| `usage` | What the asking cost, if the service said |
| `request` | The request's number, if known |
| `pieces`, `cut` | How many pieces the utterance came in, and whether it was ended by its length |

A decision is published first and written second, so that whoever comes to
act on one does not wait for a disk. Each line is one write. A line that a
failed write left unfinished is ended before the next one is added: a
reader passes over a line that is not JSON. Nothing removes old days.

## Privacy

- **What is heard is sent to the decision model's service.** Every
  utterance goes, as text, to the service at `--endpoint`, with the end of
  what the lamp last said: unless told otherwise, to OpenRouter, for
  TypeSafe's model. This program must not be run on real speech in the
  office until the owner has said so. It is for played tests.
- **The log holds what was heard, in words.** That is deliberate: a
  judgement cannot be fitted again without what it was a judgement of. It
  is why the directory is the owner's alone. It is made with mode 0700 and
  its files with 0600, and a directory that its group or everyone can enter
  is refused at the start.
- **What the program prints never holds what was heard or said.** Its
  lines are counts, times, choices and kinds of failure. The loop cannot
  print at all: what it has to say leaves it as a value that can hold no
  text made while running, and a line printed from inside the library
  fails the check every change has to pass. A message on the bus that
  cannot be read is counted and nothing of it is repeated, because the
  reason it cannot be read may quote it.
- **The key** is read once from its file and sent in one header. It is in
  no error, no log line and no log file. A server whose certificate is not
  trusted is never sent it.
- The message on the bus holds no text that was heard.

## When something around it fails

None of these stops it. Each is counted, said once when it begins and once
when it ends, and the loop goes on.

| What | What it does meanwhile | What it prints |
|---|---|---|
| Nobody publishes a topic it listens to | Waits. A voice that has gone is taken not to be speaking | `voice/heard has no publisher (times=1)`, then `voice/heard has a publisher again` |
| The model cannot be reached, is late, refuses or answers something unusable | The rule decides | `the decision model is not answering (timed_out): the rule decides until it does`, then `the decision model answers again after failures=3` |
| The log cannot be written | Decides and publishes without it | `the decisions log cannot be written (no storage space): decisions go on without it`, then `the decisions log is written again; missing=7` |
| The bus cannot be published to | Decides and writes the log without it, and tries again each second | `decisions cannot be published on the bus (...)`, then `decisions are published again; unpublished=2` |
| A message is not of its topic's type | Passes it over | `a message on voice/heard is not of its type: such messages are passed over`, then `voice/heard is read again after passed_over=3` |
| Utterances end faster than they are judged | One question is out at a time and eight utterances may wait. When a ninth ends, the one that has waited longest is let go unjudged | `more than 8 utterances wait to be judged: the oldest are let go`, then `no utterance waits any more; dropped=2` |
| A topic's socket cannot be read | Goes on with the other topics | `voice/state cannot be received from (...)`, then `voice/state is received from again` |
| The thread that asks the model ends, which only a fault in a judge can cause | The rule decides from then on. It does not come back until the program is started again | `the thread that asks the decision model has ended: the rule decides from now on` |

For each decision it prints one line:

    lamp-policy: decision 12 about=attention choice=look_up probability=0.626 by=model took_ms=655 waited_ms=302 pieces=3 bytes=26

## Asking a decision model

`crates/policy/src/decide.rs` is the one way of asking, and knows nothing
of lamps. A judge is anything that takes a state, questions and a deadline,
and returns every answer or one failure:

| Failure | When |
|---|---|
| `unreachable` | The name was not found, the connection was refused or broke, or the certificate was not trusted |
| `timed_out` | The deadline passed first |
| `refused` | The service answered with a status other than 200. Its body is not read |
| `unreadable` | What came back is not a set of answers |
| `missing` | The answers leave out a question that was asked |

A question is yes-or-no (`noul`, answered with the probability of yes) or a
choice among named options (answered with a probability for each). The
state is handed over as the text that is sent, because the order of its
fields is part of what a model reads and a JSON value keeps none.

Jev is asked with one request on one connection:

    POST https://openrouter.ai/api/alpha/decisions
    Authorization: Bearer <key>
    Content-Type: application/json

    {"model":"typesafe/jev-1.13","state":{...},"questions":{"calls_lamp":{"type":"noul","instructions":"..."},...}}

and answers:

    {"model":"typesafe/jev-1.13-20260917","answers":{"calls_lamp":{"type":"noul","noul":0.88},...},"usage":{"input_tokens":1066,"output_tokens":134,"cost":4.4772e-05}}

What comes back is held strictly to what was asked. Every question has to
be answered, as its own type, with a probability from 0 to 1. A choice has
to be one of its options, and its probabilities have to add up to 1 within
0.02. Anything else fails the whole asking: the policy is never handed half
an answer. An answer is read by its stated length or in chunks, up to
256 KiB.

The deadline is over everything together: looking the name up, connecting,
making the connection secure, sending and reading. The system's own lookup
of a name takes no deadline, so it runs on a thread of its own and is
waited for only as long as the asking has left. Measured from the office's
network, outside this program, the service answered in 0.66 s in the
middle and under 0.76 s nine times in ten, and now and then took seconds.

## How long a decision takes

From the last piece of an utterance to the decision on the bus:

    the pause (300 ms)  +  up to 20 ms to notice it  +  the asking  +  up to 5 ms to take the answer up

Measured on the development Mac, ten times each, with a stand-in model that
answers at once: 314 to 324 ms after the last piece, 319 in the middle, and
7 to 9 ms after a piece marked `finished`. With the real model the asking
is added, and each asking makes a new secure connection first. That has not
been measured from this program.

## Not shown

Everything here was run on one development Mac (x86_64), and nowhere else.

**It has never run on a lamp, and it has never asked the real service.**
No request from this program has left the machine. The request and the
answer have the form that was checked against the real service the night
this was written, outside this program. Whether the service takes this
program's own request is not shown: its headers, one connection for each
asking, and the text of the state as this program writes it, without
spaces and with letters outside ASCII as they are. Whether the roots it is
built with trust the service's certificate is not shown either.

What was run: 60 tests of the crate. They passed 30 times in a row, and
then in the three runs with `lamp-msgs` and `lamp-bus` that every change
here is held to. Before that, one test of the log failed once in fifteen
runs: it cleared the log's way before the program had finished with the
decision before, which was then written after all. The test now waits. The
program was not changed for it.

- 22 of asking, against a server on 127.0.0.1 that plays a script. Three
  of them are over a secure connection, with a certificate made for the
  test and trusted in place of the built-in roots.
- 10 of the decision, 7 of putting utterances together, 5 of the log, 2 of
  the options.
- 11 of the loop on a real bus in a directory of its own, with a stand-in
  for the model and the test as the voice.
- 3 of the built program, started, fed and killed.

And the 13 tests of `lamp-msgs`, one of them for the new type.

Not exercised:

- **The weights against real answers.** The four measured examples are
  checked as arithmetic. No labelled example has been put through this
  program and the real model, so the 87% is the measurement's and not this
  program's.
- **Whether this context is the measurement's.** When the lamp's side is
  taken (the first piece), what "last spoke" is measured from, and the
  seconds to a tenth are this program's choices. How the 346 examples had
  them is not known here. If they differ, the weights fit less well than
  measured.
- **The pause of 300 ms on a lamp.** It was given, not tuned. How far
  apart the voice's pieces come inside one utterance on a lamp has not
  been looked at: one utterance may be judged as two, or two as one.
- **A piece marked `finished`.** No publisher sends one today. It is taken
  as the last piece of its utterance, added to the others, and not as the
  whole of it again.
- **That an answer's text comes before its sound has ended.** It is the
  reason for the two moments "last spoke" is taken from. It was not
  measured here.
- **The lamp's processor.** `cargo check` for `aarch64-unknown-linux-gnu`
  did not complete on this Mac: the target is installed, and `ring`, which
  the secure connection is built on, needs a C compiler for ARM that is
  not. `lamp-msgs` with the new type does check for it. Nothing of the
  policy has been built for Linux or run on it.
- **The system's own lookup of a name.** The tests connect by address, or
  with a lookup of their own in its place. An IPv6 address is read from an
  endpoint and has never been connected to.
- **A bus that cannot be used at all.** A topic that cannot be published
  was played with a file in its way. A root that cannot be subscribed to
  ends the program at the start, and that was not played.
- **A disk that fills.** The log failing was played with a file in the
  way, not a full disk.
- **SIGTERM.** The built program was killed with SIGKILL.
- **Hours of running.** The longest run was a few seconds. The lease of
  3 s and the once-a-second sign of life were not watched for longer.
- **A unit.** There is no systemd unit and nothing in `deploy/` for it.
