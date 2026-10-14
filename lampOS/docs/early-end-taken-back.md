# The early end taken back

`second-P1` (#723): can the service be made to forget, or set aside, a
request that was ended too early, so that an early end can be sent and
taken back when the person goes on?

- **Step 1, the study and the probe.** The probe ran once on lamp-4ace on
  2026-10-14 at 23:10. Resuming from before the piece (a) does not make
  the service forget it. A note on the whole question (b) does make it
  answered as one.
- **Step 2, built.** The voice's early end (`--early-end-ms`, off unless
  given) now sends that note with what the person goes on to say. The
  probe has a trial for exactly that note on the rest of a question,
  which lamp-4ace has not run yet. Nothing in step 2 was run against the
  service or on a lamp; it was built and tested on a laptop (Apple M2 Max).

## Why

The end sent early (task 19, `--early-end-ms`) made the pause scenes
worse on lamp-4ace at 13:45 on 2026-10-14
([faster-answers.md](faster-answers.md), "The end sent early, played on
lamp-4ace"):
- a question came apart into four pieces;
- two pieces were written as Spanish;
- the first half of one question was answered before the person went on;
- nothing came sooner than with the fast end-of-turn worker alone.

The first builder's reading is that each early end is a finished request
to the service, which then keeps half a sentence as a whole one. It asked
for "an early end that does not tell the service the request is over, or
one taken back cleanly when speech resumes before confirming".

## What exists

**The service has no way to take a request back.** In the Live API, the
voice's `activityEnd` "marks the end of user activity", and client
content "is unconditionally appended to the conversation history"; no
message removes either ([Live API reference](https://ai.google.dev/api/live)).
The voice ends each request with `activityEnd`
(`crates/gemini`, manual activity detection).

**Resumption handles.** A session asked for them sends a
`SessionResumptionUpdate` now and then, whose `newHandle` "represents a
state that can be resumed". While the model is generating or calling a
tool, `resumable` is false and the handle empty. A handle is valid "for
2 hr after the last session's termination", and the guide only ever
speaks of passing "the last resumption token"
([session management](https://ai.google.dev/gemini-api/docs/live-session)).
Neither page says whether an earlier handle still resumes once later ones
were given, nor what a session resumed from it remembers.

**The lamp keeps one handle.** `crates/gemini` keeps the latest handle
(`Resumption::Retain` or `Request`) and resumes from it after a failed
connection ([resumption-provenance.md](resumption-provenance.md), which
notes that resuming from an earlier handle can lose later data). It keeps
no older one.

- Taken: the handle as the one thing that might make the service forget.
  The probe keeps its own copy of the handle given before the piece,
  since the library keeps only the newest.

## Three ways, and what decides between them

| | How | It holds if |
|---|---|---|
| (a) Resumed from before the piece | When the person goes on, give the session up and open a new one resumed from the handle the service gave before the early end; then send the whole utterance | The handle from before the piece still resumes after the piece; what the resumed session remembers does not include the piece; and resuming is quick enough to finish while the person is still talking |
| (b) A labelled continuation | On the same session, send the whole utterance as the next request, with a written note that the request before was cut off and this is the whole of it | The answer is to the whole utterance, not to the piece and not a reply to both |
| (c) Neither | The early end is withdrawn and this document says why | Neither holds |

(a) could make the service truly forget the piece. (b) cannot: the piece
stays in the conversation, and the note only asks the model to set it
aside.

## The probe

`crates/gemini/examples/early_end_probe.rs`, with its trials in
`early_end_probe/trials.rs`. It reads the voice's own provider file as it
is (fields it does not use are ignored; loaders shared with
`service_probe` in `examples/shared/provider.rs`) and six recorded
utterances. Then it runs five trials, each a new session with three
requests: 15 in all, never more than 20. The run ends within 115 s
whatever the service does. It plays and records nothing, and prints one
JSON line a trial of timings and the service's own words; it prints no
handle, credential or file path.

| File | Says |
|---|---|
| `seed.wav` | "Please remember the word orange." |
| `piece.wav` | "Also remember the word purple." (ended early) |
| `check.wav` | "Which words did I ask you to remember?" |
| `half.wav` | "What is the capital of" (ended early) |
| `whole.wav` | "What is the capital of Japan?" |
| `rest.wav` | "Japan?" |

Every trial first says `seed.wav` and waits up to 3 s for a current
handle: the handle from before the piece. Then:

| Trial | Then | Answers whether |
|---|---|---|
| `control` | `piece.wav`, answered in full; then `check.wav` | The service names purple when it was told it (so its silence elsewhere means something) |
| `resume` | `piece.wav` ended early, 1 s for its answer to begin; the session given up and a new one resumed from the handle before the piece; `check.wav` | (a): the older handle resumes, how long setup takes, and whether purple is forgotten |
| `whole-resumed` | `half.wav` ended early, 1 s; resumed from before it; `whole.wav` | (a) with a question: Tokyo, and nothing about half a question |
| `whole-labelled` | `half.wav` ended early, 1 s; on the same session `whole.wav` with the note | (b): Tokyo, the half set aside |
| `rest-labelled` | `half.wav` ended early, 1 s; on the same session `rest.wav` with the voice's own note | (b) as the voice does it: Tokyo from the two pieces |

`resume-late`, the same as `resume` but after the piece's whole answer
and 2 s more, ran once on lamp-4ace and is no longer asked: (a) failed
there even at once. `rest-labelled` and `rest.wav` took its place after
that run.

The two notes:
- **With the whole question** (`whole-labelled`): "The person paused and
  then went on: the request just before this one was cut off by that
  pause, and this is the whole of it. Answer only this."
- **With the rest** (`rest-labelled`, the voice's `CUT_OFF_NOTE`, the
  same words, which a test holds): "The person paused and then went on:
  the request just before this one was cut off by that pause, and this is
  the rest of it. Answer all of it as one request."

The 1 s after an early end is about when the answer began at 13:45. The
probe does not end early itself: it says the piece and ends it.

Each row has:
- `answer.text`, the service's words, and `answer_mentions` (whether they
  contain orange, purple, Tokyo);
- the piece's answer so far, `piece_answer_before_going_on`;
- `resumed` and `resume_setup_ms`, or `resume_refused` with the reason;
- `trial_ms`.

The mentions only help: the words decide.

**Tested here** (`cargo test -p lamp-gemini --example early_end_probe`,
six tests, run by `cargo test` through `test = true`), against
`lamp_gemini::testing`'s scripted service given a memory: what it was told
is kept with each handle it gives, and a session resumed from a handle
knows what was said up to then and no more.
- All five trials ran with 15 requests.
- `control` named orange and purple; `resume` orange only.
- The three Tokyo trials said Tokyo.
- Every resumed setup presented the handle given after `seed.wav`.
- Each note went once, inside its question, before its end.
- No handle or credential was printed.
- A refused resume is reported and not tried again.
- A silent service ends the run within 115 s and 20 requests.
- The dry run shows the 15 requests and both notes.
- A provider file with the voice's other fields (`instruction_file`,
  `resumption`, `search`) is read, and its credential file is still
  refused unless private.

## What the lamp showed (2026-10-14, 23:10)

The first builder ran the probe of step 1 once on lamp-4ace
([readings](verification/early-end-probe-lamp-4ace-20261014/README.md)):
15 requests in 95 s, five recordings made with `say -v Daniel`, and the
voice's provider file reduced to the six fields the probe then took.

| Trial | The answer | Resumed, and setup |
|---|---|---|
| `control` | "You asked me to remember orange and purple." | no |
| `resume` | "You asked me to remember the words orange and purple." | yes, 2,209 ms |
| `resume-late` | "You asked me to remember orange and purple." | yes, 2,360 ms |
| `whole-resumed` | "The capital of Japan is Tokyo." | yes, 2,134 ms |
| `whole-labelled` | "The capital of Japan is Tokyo." | no |

- **(a) does not hold.** A session resumed from the handle given before
  the piece still named purple. In `resume` the piece's answer had not
  even begun in its second. A resumed setup took over two seconds as well,
  longer than the rest of most sentences.
- **(b) holds.** The whole question with its note was answered as one,
  and "the capital of" was not answered as a question of its own.

One run.

## Step 2: the note, behind `--early-end-ms`

**What the voice does now.** With `--early-end-ms`, when the person
speaks again after the end was sent early, the request is given up and
what they go on to say becomes its successor, as before. Two things are
new:
- **The flag.** The successor's start carries `continues_cut_off`
  (`ProviderInput::Start`, set only in `continue_after_early_end`).
- **The note.** The provider worker writes `CUT_OFF_NOTE` beside that
  request, inside its activity just before its end, together with any
  other note it is given (the name heard, the room), once.

A chain of pieces gives each successor the note. Without `--early-end-ms`
no request is ever marked, and every start is written exactly as before:
the flag is left off the wire when false, which a test holds.

**Why the rest and not the whole.** Sending the whole utterance again, as
`whole-labelled` did, would mean sending the piece's sound a second time.
The transport takes no sound older than 2.5 s (`MAX_INPUT_AGE`), and the
pizza question at 13:45 came in four pieces. So the successor carries what
the person goes on to say, as it always did, and the note says it is the
rest. `rest-labelled` tries exactly that on the service; lamp-4ace has not
run it yet.

**Tested here** (`lamp-voice`, M2 Max):
- The early-end session tests, against the scripted service: when speech
  resumes 0.3 s after the end ("Hey Lamp," then the question), the
  successor's wire is its sound and then the note, just before its end.
  The same in "Lamp, stop." over an answer. A request quiet to the end
  carries no note.
- `a_start_is_written_as_before_unless_it_goes_on_from_one_cut_off`: a
  plain start serializes exactly as before the flag, and an old one reads
  as not cut off.
- `the_cut_off_note_is_the_one_tried_on_the_service`: the voice's note is
  the probe's `REST_NOTE`, word for word.

On this Mac some of these session tests fail when the machine is loaded
by other work ("stale, reordered or wrong-boot runtime message"). They
passed each time it was not, and alone.

## What to run on the lamp

The first builder, on lamp-4ace, smallest first:

1. **The probe once more**, for `rest-labelled`.
   1. Make `rest.wav` beside the five from last time. On a Mac:
      `say -v Daniel -o rest.aiff "Japan?" && afconvert -f WAVE -d LEI16@16000 rest.aiff rest.wav`
      (check that it is not a few milliseconds long).
   2. Build: `cargo build --locked --release -p lamp-gemini --example early_end_probe`.
   3. As the voice's user, with the voice's own provider file:
      `early_end_probe /etc/lampOS/voice/provider.json DIR --dry-run`, then
      the same without `--dry-run`.
   4. Read `rest-labelled`. It holds if its answer is Tokyo and "the
      capital of" was not answered as a question of its own. If it does
      not hold, `--early-end-ms` stays off.
2. **The five pause scenes** (`pauses.sh`), with `--early-end-ms 200`
   added to the voice's unit beside `--endpoint smart-turn-fast` and
   `--provider-continuation kept-start`, once, against a run without it.
   The scenes' own words are on the office iMac; the documents name them
   by their subjects.

| Scene | Expected with the note | It is wrong if |
|---|---|---|
| The question about Tokyo, said without a pause | One answer, about Tokyo, its first word sooner than without the early end by up to the time between the 200 ms early end and the lamp's own end of speech (about 0.2 s with the fast worker, which ended these at about 0.4 s of quiet) | Any note on its wire, or a later first word than without |
| The question about spiders, said without a pause | The same | The same |
| The beach thought, 450 ms of silence inside | One answer to the whole thought, or none if the busy-room rule holds it, as without the early end | An answer to "we could go to the beach" alone, or two answers |
| The pizza question, 650 ms inside | One answer, to the pizza question; a successor's note in the journal ("the model is told it goes on from the request given up") | The question in pieces with none answered, a provider failure, or an answer to a piece |
| The moon thought, 900 ms inside | One answer, about the moon | An answer to the first half ("Tell me about the movie"), or two answers |

"Sooner" is measured from the person's last word to the first sound, as
[conversation-latency.md](conversation-latency.md) does: for the plain
questions against the run without the early end, for the thoughts only
that they are answered once and whole.

## Where it is most likely wrong on the lamp

1. **The rest alone.** Step 1 showed the whole question with its note
   answered as one. Whether the rest with its note is, the rest being
   "Japan?" alone, is what `rest-labelled` asks first.
2. **A note after the answer has begun.** With
   `--provider-continuation kept-start` a successor's start waits for the
   given-up request's answer to begin and stops it. The model then has
   begun to answer the piece before it hears the rest and the note. At
   13:45 that is where the moon thought was answered by its first half.
3. **Several pieces.** Each successor is told only that the request just
   before it was cut off. A thought in four pieces relies on the model
   joining all four from that.
4. **Short pieces heard as another language.** At 13:45 the service wrote
   two pieces as Spanish. A note in English beside the rest says nothing
   of that; whether it still matters shows in the pizza scene.

## Not shown

- `rest-labelled`, or any of step 2, against the live service or on a
  lamp.
- A person's own voice and pauses: every recording is synthesized.
- How much sooner an answer comes with the early end and the note on the
  lamp.
