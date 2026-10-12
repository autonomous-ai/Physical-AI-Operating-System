# A level floor under the speech score

Agent 1, 2026-10-12. A proposal on `lamp-a1-speech-floor`, on
`lamp-v2-chat` `675c71c62`. Not integrated, not run on a lamp.

A capture block whose signal after cancellation is under -70 dBFS is
reported with a speech score of at most 0.5. Six such blocks no longer
start a request. Nothing else changes: the end of a request, what the
duplex hearing reads, and the samples themselves are as they were.

## What it answers

The lamp took input that nobody gave it. There are two kinds.

**Loud sounds in the room that were not meant for the lamp.** The 0.59 s
burst in the ordinary run, as loud as a prompt at the room microphone, and
the room sounds between 4 s and 17 s of the second device recording. Audio
cannot tell whom a sound is for. This proposal does nothing about them.

**Starts on near-silence.** In the first device recording
(`lamp-duplex-diagnostic-executed-fhld3dr3`, lamp `4ace`, gain control on),
25.51 s into capture, with the lamp silent and no player playing, the
detector found its six start blocks in a microphone signal at -81.8 to
-83.4 dBFS after cancellation. The quiet floor of the two seconds before
was -83.5 dBFS. The request was admitted, the service transcribed "¿Qué?"
and the lamp answered in Spanish. Ten blocks of that stretch scored 0.8 or
over, and none of them is over -75.2 dBFS.

A direct chat session (`lamp-human-chat-asi6qgae`) has one more of the
kind by room evidence only: an input at 40.6 s that the service transcribed
"Sí.", while the room recording shows no step of 8 dB over its floor. That
session recorded no lamp audio, so its level at the lamp is not known.

The speech model does not look at level. That is the cause this addresses.

## What the recordings say

A replay script runs the turn detector's rule over the score
each lamp recorded for every block, in all 29 diagnostic bundles on the
office host: 30.7 minutes of capture from both lamps (`4ace`, `a0ae`),
with the gain control on and off, software noise suppression on in six and
off in twenty-two. It reads
the lamp's own score and the lamp's own audio. It does not replay the
coordinator, so a start below is a detector start, not an admitted request.

| | |
|---|---|
| Sessions, blocks | 29, 184,289 |
| Detector starts | 208 |
| Of them with the lamp silent | 109 |
| Blocks scored 0.80 or over, lamp silent | 12,077 |
| Of those under -70 dBFS after cancellation | 12: 11 in the first device recording, 1 in an on-site session |
| Blocks scored 0.80 or over, lamp sounding | 2,344 |
| Of those under -70 dBFS after cancellation | 42 |

The starts with the lamp silent are the ones a floor has to leave alone.
Their quietest start block, lowest first:

| Quietest of the six start blocks, dBFS | Session | At |
|---|---|---|
| -83.4 | first device recording, `4ace` | 25.51 s |
| -53.4 | second device recording, `4ace` | 59.29 s |
| -50.3 | on-site exchange, `4ace` | 46.88 s |
| -38.7 | ten-prompt session, `a0ae` | 70.60 s |
| -38.6 | ten-prompt session, `a0ae` | 51.28 s |

Over all 109: 5th percentile -36.9, quartile -32.9, median -27.4 dBFS.

The second is the last 0.6 s of the second device recording, at shutdown.
The third is a 2.8 s sound in an on-site session, 40 dB over that
session's idle floor; I do not know what it was. Every other start has no
block under -38.7 dBFS among its six.

## The treatment, and the one not chosen

Held to 0.5 (`cap`), at -70 dBFS:

| | With the lamp silent | With the lamp sounding |
|---|---|---|
| Starts removed | 1: the start on near-silence above | 1: `a0ae`, 27.59 s into a session |
| Starts delayed | 0 | 1: `a0ae`, 76.65 s, by 40 ms |
| Ends moved | 0 | 0 |

The two with the lamp sounding are the lamp's own voice let through by the
canceller, with a block or two of the six cancelled to nothing: the
loudest start block is at -6.4 and -1.2 dBFS before cancellation. The
playback mask decides those in the lamp, and it is not replayed here.
The same at -75 and at -65 dBFS, except that -75 leaves the `a0ae` start
at 27.59 s in place. At -60 dBFS two more `a0ae` starts inside the lamp's
own speech are delayed, by 10 and 180 ms.

Set to zero (`zero`), which is what I meant to build first:

| Floor | Starts removed and delayed | Ends moved, 600 ms endpoint | Largest | Ends moved, 1,200 ms endpoint | Largest |
|---|---|---|---|---|---|
| -75 dBFS | 1 and 1 | 9 | 340 ms sooner | 4 | 20 ms |
| -70 dBFS | 2 and 1 | 12 | 340 ms sooner | 9 | 70 ms |
| -65 dBFS | 2 and 1 | 13 | 1,330 ms sooner | 9 | 70 ms |

The score a block gets is read in three places. Six consecutive blocks at
0.80 start a request. A run of blocks under 0.35 ends one. The duplex
hearing reads 0.5 and over as speech-like. Zero changes all three, and the
recorded ends move. 0.5 is under the first, not under the second and equal
to the third, so only the first changes.

## Why -70 dBFS

It lies between the two groups and nearer the faint one. The start it
removes has no block over -81.8 dBFS among its six and none over
-75.2 dBFS at 0.8 or over in its whole stretch. The quietest
start block of any other start with the lamp silent is -53.4 dBFS, 16.6 dB
above the floor, and of any start that is not the shutdown, -50.3 dBFS. In
samples it is ten steps of the 16-bit scale.

## Through the production code

`crates/live/examples/speech_floor_replay.rs` gives each bundle's
cancelled audio to the production speech model on this host, then to
`speech_floor::reported_score` and to `TurnDetector` unchanged.

| | |
|---|---|
| Sessions, blocks | 29, 184,289 |
| Largest difference between this host's score and the lamp's, any block | 0.0038 |
| Blocks more than 0.01 apart; on different sides of 0.80 | 0; 0 |
| Sessions in which the detector on the host's score starts and ends where it does on the lamp's | 29 of 29 |
| Sessions in which that agrees with the replay over recorded scores | 29 of 29 |
| Blocks held to 0.5 | 188, the same blocks on either score |
| What the reported score changes | the three starts in the table above, and nothing else, on either score |

The model on this host is the model on the lamp to the third decimal, so
the replay over recorded scores and the replay through the code are one
result. Of the 188 blocks held, 54 had scored 0.80 or over; the others had
scored between 0.5 and 0.8, where being held changes nothing that reads
the score.

## What it does not do

- It does not know whom a sound is for. Anything loud enough to be a
  person at the lamp passes.
- The start at the end of the second device recording (-47.8 to
  -53.4 dBFS) passes.
- The duplex rule that turns a yield into an interruption is unchanged: a
  faint block the model scores 0.5 or over still counts there.
- The level is the microphone's and its gain's. Every bundle here is the
  default microphone (`plug:device_micro2`). There is no recording through
  `mic1`: the floor has to be measured again before it is used with it.
- Every positive here is a loudspeaker prompt or the lamp's own voice, and
  one unidentified sound. Nobody speaking softly, or from across a room,
  has been recorded.
- The voice-eval fake computes its own score and does not go through the
  capture worker. `crates/live/examples/duplex_lab.rs` reports the model's
  score. Neither is changed here.
- A bundle's `vad_score` is the reported score. A faint block the model
  scored higher shows exactly 0.5 there.

## What would decide it

1. The level at the lamp of the "Sí." kind: chat sessions run with
   `--diagnostics`, or the level of the six start blocks written with each
   `input_candidate` in the trace.
2. One soft-voice control per microphone and gain setting: someone
   speaking quietly from across the room. Its quietest start block has to
   stay clear of -70 dBFS.
3. A run on a lamp with the change.

## Checks

Commit `37ea07737`.

| | |
|---|---|
| Host, from a source export of the exact commit, 2 jobs, 07:24:35Z to 07:26:31Z, all exit 0 | `cargo fmt --all -- --check`; strict Clippy `-p lamp-audio -p lamp-live --all-targets`; `lamp-audio` lib 63 passed; `lamp-live --test speech_floor` 4 passed; `lamp-live` lib `coordinator::tests::duplex duplex_through_the_worker turn_taking activity::` 61 passed; release build of the example, SHA-256 `816b6c7ec2720661a3b2d4b51c2f618166a5f135983cf247a42dbd7217c9aa76` |
| ARM under emulation, `agent1-arm64-floor48`, 07:23:00Z to 07:24:12Z, all exit 0 | fmt; strict Clippy `-p lamp-audio -p lamp-live --all-targets` on aarch64; `lamp-audio` lib `speech_floor` 4 passed; `lamp-live --test speech_floor` 4 passed |
| ARM, `agent1-arm64-floor47`, 07:21:01Z to 07:21:33Z, **failed** | My job script did not remove this workspace's outputs from the shared target volume; cargo linked a `lamp-audio` built from another branch and `lamp-live` did not compile |

The capture worker file is Linux-only. The ARM Clippy step compiles the
three lines in it. No test runs the worker loop with the floor: what the
worker does with it is the one call, and the function it calls is what
the tests and the replay exercise.

**Thirteen of the 29 bundles no longer have their audio on this host.** It
was removed, not by me, after these replays had been run (they ran at
06:00Z and 06:54Z; the files were gone by 07:26Z). Their rows in
`replay.json` and `rust-replay.json` are from those runs and cannot be run
again here; the example binary then was built from this same source before
it was committed. The example built from the commit gives identical output
for the sixteen whose audio remains (`rerun-check.json`): all the `4ace`
sessions, both device recordings among them. The two `a0ae` starts in the
table above are among the thirteen.

## Files

| File | What |
|---|---|
| `crates/audio/src/speech_floor.rs` | the floor, the cap, `reported_score`, four tests |
| `crates/audio/src/lib.rs` | one line: the module |
| `crates/live/src/audio_workers.rs` | three lines: the import and the call in the capture loop |
| `crates/live/tests/speech_floor.rs` | four tests through `TurnDetector` as it is |
| `crates/live/examples/speech_floor_replay.rs` | the replay through the production code |
| `replay.json` | the output of the replay over the recorded scores |
| `scripts.sha256` | digests of the scripts that made it, kept outside the repository |
| `rust-replay.json` | the example's output for the 29 bundles, with what it agrees with |
| `rerun-check.json` | which bundles still have their audio, and that the committed example reproduces their output |

If this is integrated, `docs/live-runtime.md` and `HANDOFF.md` describe the
start rule and need the clause. I have not edited them.

## Reproduce

```sh
cargo build --release -p lamp-live --example speech_floor_replay
target/release/examples/speech_floor_replay RUNTIME_DIR [SCORES.tsv]
```

The replay over recorded scores is a Python script and is not in the
repository, which holds Rust only under `lampOS/`. It is kept on the
office host in `/private/tmp/lamp-a1-analysis-scripts-20261012T0714Z-myLgKO`; `scripts.sha256` names its digest. The Rust example
above gives the same starts and ends in all 29 sessions.

The bundles are local recordings and are not in the repository.
`replay.json` names the directory each session was read from.
