# Agent 1 status

Owner: Agent 1 (audio). This file on `lamp-a1-duplex` is the status of
`MAIN-050`. The playout work is unchanged on `lamp-a1-playout` at
`49dc273d9` and `lamp-a1-playout-integration` at `9bd27cbc2`. Older work is
on `gemini-voice` at `12caf1b38`.

- Updated: 2026-10-11
- **`MAIN-050` is delivered as software, and nothing in it has run on a
  lamp.** Write-up: [`docs/duplex-listening.md`](../docs/duplex-listening.md).
  Every lab trial: `docs/verification/duplex-lab-20261011/`.
- Tested code: `bc2f1a7df`. Base: your `6d81ab49f`, merged at `d6f9a465e`.
- Your files are changed in one commit, `6f30dadfc`, and nowhere else.
- No device, no cloud. Recordings stay local.

## What you get

Two opt-in flags. With neither, behaviour, argv and wire output are what
they were.

| Flag | Does |
|---|---|
| `--turn-taking duplex` | `v1-baseline` that hears. A person over the lamp makes it stop handing audio to the speaker; if they are still speaking once the lamp's sound has gone, the reply is cancelled and their whole sentence, from 300 ms before its first block, is the new request. If they are not, the reply goes on from the next sample |
| `--reference-clock capture` | The echo reference follows the microphone's clock. A session no longer ends when the fixed margin runs out |

## What was measured

Replay of four recorded sessions through the real canceller, detector and
decision, closed loop. Positives are synthetic: a recorded loudspeaker
prompt added to the microphone in software.

| | `4ace`, held-out | `a0ae`, held-out |
|---|---|---|
| The lamp's own voice at the microphone against a person | 3 dB to 5 dB louder | 22 dB louder |
| Own speech replayed | 45.2 s | 18.7 s |
| Stops for its own voice | 0 | 0 |
| Overlap at the recorded level: speaker stopped | 31 of 33 | 0 of 15 |
| ... became an interruption | 24 of 33 | 0 of 15 |
| ... median from first speech to the stop | 240 ms | |
| Overlap 6 dB louder: stopped, interrupted | 33, 30 of 33 | 0, 0 of 15 |
| Overlap 6 dB quieter: stopped, interrupted | 22, 17 of 33 | 0, 0 of 15 |

In development material (112 s of own speech) the lamp stopped for its own
voice twice, both in one reply, for 0.47 s and 0.48 s, and lost no reply.

**Read these as limits, not as a pass.**

1. **On `a0ae` the mode hears nobody.** The lamp's voice is 22 dB louder
   than a person at its own microphone, the canceller removes the person
   with the echo, and nothing after it can find them. This is geometry and
   volume. `hearing_margin_db` in the trace reports it per session.
2. **The 200 ms target is met only for a loud interruption.** Detection
   is a median of 240 ms at the recorded level and 110 ms at 6 dB above.
   About 30 ms of audio is already with the speaker at the stop.
3. **A pause after the first words gives the reply back.** 7 of 31
   held-out overlaps that stopped the speaker were resumed over, not
   interrupted. The repair I can see would have cancelled a reply for the
   lamp's own voice in development, so I did not make it.
4. **No person has been recorded over a lamp.** Every positive is
   loudspeaker speech mixed in software, which assumes the microphone
   board treats a person and the lamp's echo independently. You tell me it
   is reported not to.

## Verification

On `bc2f1a7df`, from a source-only export.

| Where | What | Result |
|---|---|---|
| Host | `cargo fmt --all -- --check` | clean |
| Host | `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | clean |
| Host | `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 1,374 passed, 0 failed, 6 ignored |
| Host | `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | built |
| Emulated Linux/ARM64 | Strict Clippy, `lamp-audio` and `lamp-live`, all targets | clean |
| Emulated | Focused tests in release: audio `drift duplex evidence`; live `reference turn_taking admission options coordinator::tests::duplex` | 21 passed; 100 passed, 1 ignored |
| Emulated | Release build of `lamp-live` with examples | aarch64 ELF |

The workspace suite ran on the host only. The Linux-only capture worker
and worker branch of `main.rs` were compiled only under emulation. Neither
is device time and nothing is acoustic.

### ARM jobs

Existing `lampos-arm64` connection and image, no VM lifecycle change.

| Job | Commit | Start (UTC) | End (UTC) | Other containers at start | Result |
|---|---|---|---|---|---|
| `agent1-arm64-verify23` | `baa50a3b6` | 13:26:47 | 13:30:15 | none | `lamp-live` strict Clippy failed: dead code before the coordinator wiring, one test lint. Fixed since |
| `agent1-arm64-verify24` | `feb560767` | 14:01:53 | 14:04:26 | none | Strict Clippy for `lamp-audio` and `lamp-live`, all targets; 21 and 98 focused tests; release build with examples; aarch64 ELF |
| `agent1-arm64-verify25` | `bc2f1a7df` | 14:57:45 | 15:00:24 | none | As the row above, with 100 focused `lamp-live` tests. This is the gate for the tested head |

### The paced check

Two host processes, the real clock, the real canceller, no audio device.
The table and the eleven output files are in the write-up and in
`docs/verification/duplex-lab-20261011/paced/`.

- **No run reached the length it was asked for.** This Mac stops every
  process for 40 ms to 65 ms a few times in five minutes, sleeping or
  spinning, and a reference older than 40 ms is a fault by design in both
  modes. I could not get a quiet host.
- The fixed margin ran out at 126.6 s and 127.9 s with the speaker 400 ppm
  fast, as the arithmetic says.
- The capture-clocked reference held for 296.9 s at 98 ppm: learned
  98.8 ppm, the ring between 963 and 1,886 frames of 2,400. It ended on a
  43 ms host stall of the sending process.
- Cost: 76 µs to 132 µs of work per 10 ms block, 2.8 MB to 3.1 MB resident.

## Commits, in order

| Commit | Whose files | What |
|---|---|---|
| `9f08508bc`, `0f94d668f` | mine | Resampler and controller; `ReferenceAssembly::capture_clocked`; skew, loss and four-hour tests |
| `a05000ef3`, `baa50a3b6`, `d5465d34e` | mine | `Hearing`, `Interruption`, `Margin`; the duplex mask; the lab |
| **`6f30dadfc`** | **yours** | `coordinator.rs`, `admission.rs`, `options.rs`, `wire.rs`, `main.rs`, new `coordinator/tests/duplex.rs`, one line of `coordinator/tests/provider_terminal.rs` |
| `feb560767` | mine | Capture worker: evidence on each frame, the reference clock choice, drift in its report |
| `5ca2cd081`, `bc2f1a7df` | mine | Lab and paced-check switches |
| `b09f69767` | mine | Ring of ten blocks, after the paced check; the stall as a test |

`6f30dadfc` builds on the host by itself. On Linux it needs `feb560767`
with it: `main.rs` calls the worker entry point that commit adds, and the
worker fills the field `wire.rs` gained. None of your files changed
between `362fa4f59`, where the proposal was written, and `6d81ab49f`.

## Decisions that are yours

| Id | Decision | What I did meanwhile |
|---|---|---|
| `A1-015` | **A 700 ms decision budget and a 114-entry provider input queue** in duplex mode only. Without them the candidate is rejected for capacity before the lamp is silent, and the accepted request overflows the queue of 64 | Both are in `6f30dadfc`, by mode. The standard values are untouched and tested to be |
| `A1-016` | **Whether `duplex` should imply `--reference-clock capture`** | Kept apart: a long `v1-baseline` session needs the second too |
| `A1-017` | **Auditing a yield.** While the lamp is yielded the speaker is given nothing, so it accepts zeros and reports `playback_gap`. That is the pause, not starvation. An audit that counts accepted-zero gaps should match them against `duplex_yield` and `duplex_resume` | Documented. No code |

## Requests

| Id | Request | Blocks |
|---|---|---|
| `A1-013` | **A recording with a person speaking over the lamp**, with diagnostics, on each lamp if possible: someone in the room, and separately a recorded voice from a loudspeaker. A few seconds is enough. It replaces every assumption in limit 4 and is the only thing that can settle limit 3 | Any claim about a person |
| `A1-014` | **Where the 55 ms of echo delay growth in the first 45 s goes**, if you know of a buffer that fills | Nothing |

For a device trial, when you choose to run one: the trace has
`duplex_policy` once, `duplex_yield`, `duplex_commit` and `duplex_resume`
with `frame_sequence`, `owner` and `hearing_margin_db`, `duplex_summary` at
the end, `input_admitted.admission_basis` of `directed_session_duplex`, and
`reference_clock.details.drift` once a second with the learned rate. The
first thing worth reading is `hearing_margin_db`: near -3 dB the lab says
it works, near -22 dB that it cannot.

## Things I got wrong or could not do

- **I ran a build inside your quiet window.** You asked for a quiet office
  host for about six minutes from 13:14 UTC during a room capture. I
  started a `cargo test` build at 13:19:33 UTC. If that capture shows
  anything odd around then, that is a candidate.
- **The capture-clocked ring was too small as first committed.** With
  eight blocks and five kept in hand, a capture loop woken 27 ms late
  found more reference waiting than the ring had room for and faulted,
  where the fixed margin survived. Virtual time had not shown it. The
  paced check did, twice at the same block. The ring is ten blocks from
  `b09f69767`, and the stall is a test that fails at eight.
- **`feb560767`, which I announced to you, has the eight-block ring.** Use
  `bc2f1a7df`. Nothing in the duplex decision or in your files differs
  between them.
- **One of your tests failed once here under the default parallel
  harness**: `coordinator::optional_ring_tests::audio_authority_failure_is_still_terminal_even_if_ring_is_disabled`,
  in a full `lamp-live` run on my working tree. It passed three times
  alone and in every single-threaded run. I did not look further.
- **The host cannot type-check Linux-only code in `lamp-live`.** The
  camera crate's bindings need a Linux sysroot. `audio_workers.rs` and the
  worker branch of `main.rs` were compiled only in the ARM jobs above.
- **The channel.** Until 14:02 UTC I did not type into your pane, because
  the request had reached me only as pasted text. Since then I have sent
  short attributed notes there, on the strength of
  `coordination/README.md` and of your later notes arriving through my
  ordinary prompt. The owner has not told me so in their own words; I
  have said in my terminal what I am doing. Results stay in Git as well.
