# The early end taken back: study and probe

`second-P1` (#723), step 1 of 2: can the live service be made to forget a
request that was ended too early, so that an early end can be sent and
then taken back when the person goes on? This is the study and a probe of
the service for lamp-4ace. Step 2, building it behind `--early-end-ms`,
comes only if the probe shows that one way holds. Nothing here was run
against the service or on a lamp; the probe was run against a scripted
service on a laptop (Apple M2 Max).

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
`early_end_probe/trials.rs`. It reads the voice's own provider file (the
same fields as `service_probe`, which now shares its loaders with it in
`examples/shared/provider.rs`) and five recorded utterances, then runs
five trials. Each trial is a new session with three requests: 15 in all,
never more than 20. The run ends within 115 s whatever the service does.
It plays and records nothing, and prints one JSON line a trial of timings
and the service's own words; it prints no handle, credential or file path.

| File | Says |
|---|---|
| `seed.wav` | "Please remember the word orange." |
| `piece.wav` | "Also remember the word purple." (ended early) |
| `check.wav` | "Which words did I ask you to remember?" |
| `half.wav` | "What is the capital of" (ended early) |
| `whole.wav` | "What is the capital of Japan?" |

Every trial first says `seed.wav` and waits up to 3 s for a current
handle: the handle from before the piece. Then:

| Trial | Then | Answers whether |
|---|---|---|
| `control` | `piece.wav`, answered in full; then `check.wav` | The service names purple when it was told it (so its silence elsewhere means something) |
| `resume` | `piece.wav` ended early, 1 s for its answer to begin; the session given up and a new one resumed from the handle before the piece; `check.wav` | (a): the older handle resumes, how long setup takes, and whether purple is forgotten |
| `resume-late` | The same, but after the piece's whole answer and 2 s more | The same, with a newer handle surely given before resuming |
| `whole-resumed` | `half.wav` ended early, 1 s; resumed from before it; `whole.wav` | (a) with a question: Tokyo, and nothing about half a question |
| `whole-labelled` | `half.wav` ended early, 1 s; on the same session `whole.wav` with the note | (b): Tokyo, the half set aside |

The note: "The person paused and then went on: the request just before
this one was cut off by that pause, and this is the whole of it. Answer
only this." The 1 s after an early end is about when the answer began at
13:45; the probe does not end early itself, it says the piece and ends it.

Each row has `answer.text` (the service's words) and `answer_mentions`
(whether they contain orange, purple, Tokyo), the piece's answer so far
(`piece_answer_before_going_on`), `resumed` and `resume_setup_ms`, or
`resume_refused` with the reason, and `trial_ms`. The mentions only help:
the words decide.

**Tested here** (`cargo test -p lamp-gemini --example early_end_probe`,
five tests, run by `cargo test` through `test = true`), against
`lamp_gemini::testing`'s scripted service given a memory: what it was told
is kept with each handle it gives, and a session resumed from a handle
knows what was said up to then and no more. That is how the service would
behave if (a) holds.
- All five trials ran with 15 requests.
- `control` named orange and purple.
- `resume` and `resume-late` named orange only.
- Both whole trials said Tokyo.
- Every resumed setup presented the handle given after `seed.wav`, never
  the newer one.
- The note went once, inside the whole question, before its end.
- No handle or credential was printed.
- A service that refuses resumed setups gives `resumed: false` with the
  reason and no second attempt.
- A service that never answers ends the run within 115 s, at most 20
  requests.
- The dry run shows the 15 requests and the note.

`--dry-run` was also run here through the built example, with a provider
file of a placeholder key, and printed the plan without the key or the
path. None of this says what the live service does.

## What to run on the lamp

The first builder, once, on lamp-4ace:

1. **The five files.** On a Mac:

   ```sh
   for pair in "seed:Please remember the word orange." "piece:Also remember the word purple." \
       "check:Which words did I ask you to remember?" "half:What is the capital of" \
       "whole:What is the capital of Japan?"; do
     say -o "${pair%%:*}.aiff" "${pair#*:}" && afconvert -f WAVE -d LEI16@16000 "${pair%%:*}.aiff" "${pair%%:*}.wav"
   done
   ```

   (Here `say` gave files of 5 ms, from a session without the speech
   service; check that each is a second or two.) Any recording of a
   person saying the five lines does as well, 16-bit WAV at any rate.
   Copy them to a directory on the lamp.
2. **Build** for the lamp, as `service_probe` is:
   `cargo build --locked --release -p lamp-gemini --example early_end_probe`.
3. **As the voice's user** (the provider file is mode 0600), first
   `early_end_probe /etc/lampOS/voice/provider.json DIR --dry-run`, then
   the same without `--dry-run`. It needs the voice's network, not its
   microphone or speaker; the voice can keep running.

**How to read it:**

| If | Then |
|---|---|
| `control` names purple; `resume` and `resume-late` resume and name orange but not purple; `whole-resumed` says Tokyo; `resume_setup_ms` is well under the rest of a sentence (under 500 ms, say) | (a) holds: step 2 builds it |
| A resumed setup is refused (`resume_refused`), or `resume`/`resume-late` still name purple | (a) does not hold |
| `whole-labelled` says Tokyo and does not answer "the capital of" as a question of its own | (b) holds: step 2 builds it if (a) does not |
| Neither | (c): `--early-end-ms` is withdrawn |

Step 2 would measure "sooner" from the person's last word to the first
sound, as [conversation-latency.md](conversation-latency.md) does.

## Where it is most likely wrong on the lamp

1. **The relay.** lamp-4ace reaches the service through its configured
   endpoint. Whether that endpoint passes `sessionResumption` both ways
   has not been seen in this repository; a refused or silent resume would
   look like (a) failing when it is the road.
2. **Handles given while the piece is answered.** While the model
   generates, `resumable` is false. If the service gives no current
   handle within 3 s of the seed's answer, a trial says so
   (`handle_before_piece: false`) and goes no further.
3. **The model guessing.** A resumed session that does not remember
   purple might still say it, from the check question's own words; and
   the transcript may spell Tokyo or orange otherwise. The words in each
   row decide, not `answer_mentions`.
4. **Two minutes on a slow link.** At about 15 s a trial the run takes
   some 75 s; a slow service ends the last trials as `skipped`.

## Not shown

- Anything from the live service: every result above is the scripted
  service's.
- A person's own voice: the five lines are synthesized.
- How long resuming takes on the lamp's link, and whether it fits inside
  the rest of a sentence.
- What a session resumed from an earlier handle keeps of tools, the
  camera's context or the voice's own instruction beyond the probe's.
