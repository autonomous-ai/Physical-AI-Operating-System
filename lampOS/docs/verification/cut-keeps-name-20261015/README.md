# A cut keeps its name on lamp-4ace's own path: the #809 follow-up, 2026-10-15

Fourth's evidence for the reviewer's P1 on
[#809](https://github.com/autonomous-ai/lampOS/pull/809) (part of
[#782](https://github.com/autonomous-ai/lampOS/issues/782)). Run on Fourth's
Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0). Nothing was run on a lamp.

## What was wrong

With `LAMP_CUT_KEEPS_NAME=on` and the options lamp-4ace runs
(`--endpoint smart-turn-fast --end-silence-ms 600`, kept-start, no
`--early-end-ms`), the turn ends at a comma on the model's verdict (400 ms of
quiet) or at the 600 ms window, and the words after it come as a new
admission through `commit_admitted_input`, which always sent
`continues: false`. Only `continue_after_early_end`, reached only with
`--early-end-ms`, set it from the switch. #809's tests set `continues: true`
by hand at the provider worker and never crossed the coordinator.

## What changed

- The coordinator keeps the turn it ended (`EndedTurn`: turn, the capture
  time of the block that ended it, connection, microphone generation), only
  with the switch.
- At the next admission, the Start says `continues: true`, and the
  `input_admitted` record `"continues":true`, when the request it replaces is
  that turn's, its input ended, nothing of its answer was heard
  (`!had_audio`, no playback), on the same connection and microphone, and
  the new speech's trigger block was read within 1 s of the end
  (`CONTINUES_WITHIN_US`).
- The switch is read once into the rig (`Rig::cut_keeps_name`), so tests set
  it without the environment; `continue_after_early_end` uses the same field.
- What the provider hears does not change: `continues_cut_off` (the note
  that tells the model the request goes on from one cut off, `39eb93085`)
  stays `false` on this path, as it was; only an end sent early sets it.
- `lamp-voice` at `740e1e892` did not build its tests: three Starts in the
  second voice's tests lacked `continues_cut_off`. A separate commit adds
  `continues_cut_off: false` to each (said on #705 at 19:51), so that this
  branch's gate could run.

## Tests (`gate/`)

- `coordinator::tests::endpoint_session::a_turn_ended_at_a_comma_goes_on_in_the_next_request_only_with_the_switch`:
  the coordinator's own loop on the logical clock, duplex, the 600 ms window,
  a confident model. Speech 1 s, 500 ms of quiet, speech 0.8 s. The end is
  `model_complete` at 400 ms; the provider peer sees two Starts,
  `[false, true]` with the switch and `[false, false]` without, and without
  it no `input_admitted` record has the field.
- `…::a_new_request_long_after_the_end_does_not_go_on`: the same with 1.8 s
  of quiet (the speech back 1.4 s after the end): `[false, false]`.
- `coordinator::tests::early_end::a_question_after_the_window_ended_its_name_goes_on_and_cutting_in_does_not`:
  the real Coordinated provider worker and Gemini session (kept-start)
  against the scripted service, on the host clock. "Hey Lamp," (0.5 s),
  700 ms of quiet ended by the window, the question 100 ms after the end,
  and "stop" over the answer to it 700 ms after the question's end:
  admissions `[false, true, false]` with the switch, `[false, false, false]`
  without.
- Each was run against a mutated coordinator on this Mac before the commit:
  with `continues` never set, the first and third fail; with the
  "nothing of its answer heard" condition removed, the third fails
  (`[false, true, true]`).

## What it cannot reach (`floor-holds.txt`, `floor_holds.py`)

Once the last three things the lamp heard went unanswered
(`body_link::Attention::busy`), a new sound within 1.8 s of an end, while
the answer has not begun, takes no turn (`HOLD_FLOOR_US`, the `floor_held`
record), with or without the switch: the rest of a cut line is dropped there
before it is admitted. In the voice's records the first builder copied to
this Mac on 15 Oct at 16:17 (`~/lamp-voice-data/sessions/turn-times-20261015`:
duplex, mic2, kept-start; the records do not name the lamp), read in place
and counted only:

- 932 ends, 531 floor holds in 10 sessions; 528 followed an end of the turn
  they held, 401 of them within 1 s of it (192 after `model_uncertain`, 130
  `silence_fallback`, 77 `model_complete`, 2 `model_incomplete`).
- 924 next turns admitted after an end, 46 of them within 1 s of it.

Which of the 401 were the same person going on, and which the room's other
voices, was not measured.
