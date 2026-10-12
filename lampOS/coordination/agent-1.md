# Agent 1 status

Owner: Agent 1 (audio and Gemini). This file on `lamp-a1-speech-floor` is
the status of one task: your request of 2026-10-12 to compare the
unsolicited-input class with the microphone work and to own a narrow
audio-side correction only if native signals and positive controls support
it. The branch is your `675c71c62` plus the commits below. The `mic1`
reference work is on `lamp-a1-mic1-reference` with its own status.

- Updated: 2026-10-12, 07:27 UTC
- **Two kinds of input nobody gave.** Loud sounds in the room that were
  not for the lamp: audio cannot tell, and nothing here touches them.
  And starts on near-silence: the speech model does not look at level.
- **One native start on near-silence has audio:** six start blocks at
  -81.8 to -83.4 dBFS, 2 dB over the idle floor, admitted, transcribed
  "¿Qué?", answered in Spanish. A second, "Sí.", has room evidence only.
- **Positive controls: 29 recorded sessions, 30.7 minutes, both lamps.**
  Of 109 detector starts with the lamp silent, no other has a block under
  -53.4 dBFS among its six.
- **Proposal: a capture block under -70 dBFS after cancellation is
  reported with a speech score of at most 0.5.** It cannot count towards
  a start. Ends, the duplex hearing and the samples are as they were.
- Not run on a lamp. Not activated anywhere: it is on this branch only.

Evidence, tables and limits:
`lampOS/docs/verification/speech-floor-20261012/README.md`.

## Commits

| Commit | Files | What |
|---|---|---|
| `45bed25a7` | `crates/audio/src/speech_floor.rs` (new), `crates/audio/src/lib.rs` (one line), `crates/live/src/audio_workers.rs` (three lines), `crates/live/tests/speech_floor.rs` (new) | The floor, the cap, eight tests |
| `37ea07737` | `crates/live/examples/speech_floor_replay.rs` (new) | Recorded cancelled audio through the production model, the cap and `TurnDetector` |
| this commit | `docs/verification/speech-floor-20261012/`, this file | Evidence and status |

No shared file is touched. `activity.rs` is Agent 2's and is used as it is.
This branch and `lamp-a1-mic1-reference` both change the capture loop in
`audio_workers.rs`, in different places.

## What you asked for, and where it is

| | |
|---|---|
| Native signal | The start at 25.51 s of `lamp-duplex-diagnostic-executed-fhld3dr3`: ten blocks scored 0.80 or over, none over -75.2 dBFS |
| Positive controls | 12,077 blocks scored 0.80 or over with the lamp silent in 29 sessions; 12 are under -70 dBFS, 11 of them in that one session |
| Quiet onsets preserved | No start with the lamp silent is delayed or removed other than that one. The samples are untouched, so a faint onset is in the pre-roll of the request the louder blocks start: `a_quiet_onset_stays_in_the_request_the_louder_blocks_start` |
| Not tuned on the negative alone | The floor is placed from the starts it must leave alone: 16.6 dB under the quietest of them, and nearer the faint side of the gap |
| Narrow | Only the start rule reads a different score. I first meant to set faint blocks to zero: that moves 12 recorded ends, by up to 340 ms. Held to 0.5 it moves none |
| Through the code, not only a script | The production model on this host is within 0.0038 of the lamp's score in every one of 184,289 blocks; the detector on it starts and ends where it did on the lamp in 29 of 29 sessions; with the cap, the same three starts change as in the script |

## What it does not do

- Nothing about whom a sound is for. The 0.59 s burst in the ordinary run,
  the room sounds in the second device recording, and the sound at -35 dBFS
  that became owner 2 in the `mic2` fixed-source run all pass.
- The start at the very end of the second device recording (-47.8 to
  -53.4 dBFS, at shutdown) passes.
- The level is the microphone's and its gain's. Every session is the
  default microphone. Nothing is measured through `mic1`, whose idle floor
  is near -56 dBFS: the floor would have to be measured again there.
- Nobody speaking softly or from across a room has been recorded.

## For you

| Id | |
|---|---|
| `A1-035` | What would decide it: chat sessions run with `--diagnostics`, or the level of the six start blocks written with each `input_candidate`, so that the "Sí." kind is measured at the lamp |
| `A1-036` | One soft-voice control per microphone and gain: someone speaking quietly from across the room. Its quietest start block has to stay clear of -70 dBFS |

## Verification

| | |
|---|---|
| Host, `37ea07737`, from a source export of the exact commit, 2 jobs, 07:24:35Z to 07:26:31Z, all exit 0 | fmt; strict Clippy `-p lamp-audio -p lamp-live --all-targets`; `lamp-audio` lib 63 passed; `lamp-live --test speech_floor` 4 passed; lib `coordinator::tests::duplex duplex_through_the_worker turn_taking activity::` 61 passed; release build of the example |
| ARM, `37ea07737`, `agent1-arm64-floor48`, 07:23:00Z to 07:24:12Z, all exit 0 | fmt; strict Clippy `-p lamp-audio -p lamp-live --all-targets` on aarch64; `lamp-audio` lib `speech_floor` 4 passed; `lamp-live --test speech_floor` 4 passed. `/private/tmp/lamp-a1-floor-arm-20261012T0723Z-rdTmI2` |
| ARM, `agent1-arm64-floor47`, 07:21:01Z to 07:21:33Z, **failed** | My script: stale `lamp-audio` from the shared target volume. `/private/tmp/lamp-a1-floor-arm-20261012T0721Z-xxUUUm` |

Thirteen of the 29 bundles lost their audio on this host after the replays
ran. The committed example reproduces the other sixteen exactly.

The capture worker file is Linux-only: the three lines in it are compiled
by the ARM Clippy step and not on this host. No test runs the worker loop
with the floor.

Not run: the workspace suite, anything on a lamp or against the service.
