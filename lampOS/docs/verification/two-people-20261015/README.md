# Two people at the lamp: #829, 2026-10-15

Fourth's evidence for [#829](https://github.com/autonomous-ai/lampOS/issues/829).
Read and run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0).
Nothing was run on a lamp. The records are the first builder's, on the Mac
the two builders share (`~/lamp-voice-data/`). Counts only; words appear
only where they are the synthetic played lines.

## What the lamp has today (`lamp-voice`, read only)

- **Whether to answer**: the busy-room rule (`crates/voice/src/attention.rs`).
  In a busy room it plays for any of these: a conversation going on, a call
  in the text, any face turned to the lamp, the name heard. With
  `LAMP_ATTENTION=busy-near`, the name close to the lamp also counts; with
  `LAMP_ADDRESSED=on`, near and facing.
- **A conversation going on** (`thread.rs`, `going_on`): the lamp spoke in
  a kept exchange within 45 s. No speaker, face or direction is checked,
  and it comes before every other sign.
- **Turned**: any face turned to the lamp, not the speaker's.
- **The text's call** (`called`): the word "lamp" anywhere.
- **No direction or active speaker**: no direction-of-arrival or
  mouth-movement code. The two microphones are separate USB devices with
  their own clocks, and `Face` has no "speaking".
- **A session per person** (#775): follows the tallest known face, not the
  voice.
- **The gaze**: follows the largest face once still for 0.4 s, never the
  voice.

## What was played (`two-people-scene.txt`, `scripts/answered.py`)

46 runs on lamp-4ace, 14 and 15 October, of one scene
(`~/lamp-voice-data/tools/twopeople.sh`): nine lines between Daniel and
Samantha, then Daniel's "Hey Lamp, what do you think we should have for
lunch?" and his follow-up. In all of them:

- the lamp spoke into the two people's talk 21 times in 414 lines, 11 of
  them in one family of runs (floor1);
- it answered the call and the follow-up 68 times in 92.

None of #829's other three cases has been played.

## What the lamp decided (`first-output-decisions.tsv`, `going-on.tsv`)

lamp-4ace's first-output records of 15 October
(`~/lamp-voice-data/sessions/turn-times-20261015/first-output`): 69
decisions. Nine answers were let out in a busy room because a conversation
was going on:

- 6 with nobody facing, all in one live exchange (session 1, 15:25 to
  15:33), with no played line near them (`scripts/going_on.py`);
- 3 with someone facing.

A rule that "going on" needs a face turned to the lamp would have held 6
of the 9. So the follow-up needs the same voice, not a face ("Plan", step
2).

## What others do (sources)

- Facing the device is the veto, not the call. Alexa+ on Echo Show takes a
  wordless interruption only from someone in view and facing the screen
  ([Amazon](https://digprjsurvey.amazon.com/csad/help/node/TtQI6drnxX9fpAYrc1)).
  Google's Look and Talk uses face, gaze, distance and lip movement, on
  device ([Google](https://research.google/blog/look-and-talk-natural-conversations-with-google-assistant/)).
- Facing is necessary but not sufficient: in Microsoft's study people
  looked at the device while talking to each other; all signals together
  gave 9.8% error
  ([Tsai 2015](https://microsoft.com/en-us/research/wp-content/uploads/2016/02/ieee-preprint-07153545.pdf)).
- Directedness from the transcript and the previous query: equal error 5.2%
  combined against 9.3% for the best single signal
  ([Mallidi 2018](https://arxiv.org/abs/1808.02504)), and 20 to 40% fewer
  false alarms with the first query as context
  ([Apple](https://arxiv.org/abs/2411.00023)).
- Which face is speaking: Light-ASD 94.1 mAP with 1.0 M parameters
  ([2303.04439](https://arxiv.org/abs/2303.04439)); no public ARM timings.
- Multi-party: head pose alone F1 0.72, voice activity 0.73, both
  0.85 to 0.88 ([Furhat](https://aclanthology.org/W15-4642.pdf)).
- Follow-ups: Google ends Continued Conversation when you "start to talk to
  someone else", and non-owners need Voice Match
  ([Google](https://support.google.com/googlenest/answer/7685981?hl=en)).
  No product documents refusing a second person who faces it.
- Two-microphone direction is the weakest signal here: at 16 kHz, a pair
  10 to 15 cm apart resolves 8 to 12 degrees straight ahead at best, and
  two USB clocks drift.

## Plan, each step behind a switch

1. Scenes for the four cases (`scenes.sh`, for the first builder's
   `scenario.py`, which plays every voice from one loudspeaker).
2. A follow-up within 45 s goes on only for the same voice; a different
   voice needs the name, a call or a face turned to the lamp. Needs a small
   speaker-embedding model; researched next.
3. **Built**: "the lamp" spoken of is no call, `LAMP_NAME_IN_PASSING=ignored`
   ([attention.md](../../attention.md), "The lamp spoken of is not a call").
4. Which face is speaking (`Face.speaking`), for facing to mean the
   speaker's and the gaze to follow the voice: asked of the first builder
   (it touches `crates/eyes` and `crates/msgs`).
