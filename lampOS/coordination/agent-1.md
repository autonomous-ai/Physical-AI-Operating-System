# Agent 1 status

Owner: Agent 1 (audio and Gemini). This file on `lamp-a1-mic1-reference`
is the status of one task: your request of 2026-10-12, after the
fixed-source microphone pair, for a `mic1` reference-timing correction as
a separate proposal on `675c71c62`. My other branches are as they were.

- Updated: 2026-10-12, 06:53 UTC
- **The canceller on `mic1` was given each render block after its echo.**
  The capture-clocked reference keeps five blocks of render in hand;
  `mic1` has the sound back two capture blocks before that render reaches
  the canceller. The device removed 5.2 and 1.6 dB of the lamp's own
  voice and scored it as speech in a quarter to a half of all blocks.
- **Fix: each `mic1` frame waits four capture blocks before it is
  cancelled and handed on,** with the capture-clocked reference only.
  `mic2` and the fixed reference run the code they ran. My files only.
- **It stops the lamp hearing itself. It does not make `mic1` hear a
  person over the lamp:** in the replay the clip is still scored in one
  block of 337.
- Not run on a lamp. Nothing pushed to any branch but this one.

Evidence, numbers, limits and scripts:
`lampOS/docs/verification/mic1-reference-20261012/README.md`.

## Commits

| Commit | Files | What |
|---|---|---|
| `d32dff803` | `crates/live/examples/duplex_lab.rs` | `--capture-hold N` for the offline replay |
| `164a92c61` | `crates/live/src/reference/hold.rs` (new), `reference.rs` (one line), `audio_workers.rs` (16 lines), `crates/live/tests/capture_hold_worker.rs` (new) | The hold, its unit tests, and the worker-loop test |
| `f18362fa9` | `crates/live/tests/capture_hold_worker.rs` | Test only: one session run and thrown away first; a fault's times printed |
| `16e864692` | `crates/live/tests/capture_hold_worker.rs` | Test only: asserts on what the canceller accounts for. **Not yet run on ARM** |
| this commit | `docs/verification/mic1-reference-20261012/`, this file | Evidence and status |

No shared file is touched. `options.rs` is read (`Microphone::Mic1.device()`,
`ReferenceClock`), not changed.

## What you asked for, and where it is

| | |
|---|---|
| Render before the echo on `mic1` | Replay of the recorded bundle through the real canceller: 6.0 and 1.7 dB removed with no hold, 23.2 and 27.9 dB with four blocks |
| Capture timestamps kept | A frame leaves with its own sequence and read times; the worker-loop test reads each frame's read time within 1.3 ms of the block being there and its hand-on 43.8 ms after its own read |
| Ordering | One queue, oldest first; sequences contiguous from 1 in every session of the worker-loop test |
| Privacy | Frames held at a stop or a mute are dropped: 396 of 400 blocks handed on at a stop, 96 of 100 at a mute, none after |
| Bounded input age | 40 ms more, constant. No re-stamping, no lease change: your 100 ms leaves about 55 ms for a stall |
| Finite queues | The hold is four frames, bounded at eight by construction |
| `mic2` unchanged | Hold zero: the frame comes straight back. Worker-loop control: 400 of 400 blocks handed on, 4.0 ms after the read |
| Own-speech-only control | Lamp alone scored 0.8 or over in 4 of 430 and 0 of 360 blocks with the hold (79 and 180 without; device 114 and 193) |
| Original-request control | Sunset question scored 0.8 or over in 124 of 188 blocks at every hold |
| Coffee-opening control | Not retained: 1 of 337 blocks scored, correlation with the clip 0.01 to 0.33. See the limit above |
| Worker handshake and steady streaming | `capture_hold_worker.rs` through `capture_on`, the real channels, reference link, ring and canceller, on ARM under emulation |
| Why not the reference target | Two blocks in hand in place of five: one late packet runs the ring dry |

## Verification

| | |
|---|---|
| Host, `f18362fa9`, from a source export of the exact commit, 2 jobs, 06:44:18Z to 06:47:55Z, all exit 0 | `cargo fmt --all -- --check`; strict Clippy `-p lamp-live --all-targets`; `lamp-live` lib `reference::` 56 passed, 0 failed, 2 ignored; lib `coordinator::tests::duplex duplex_through_the_worker turn_taking activity:: options::` 81 passed; `--test reference_prime --test reference_link` 14 passed; release build of `duplex_lab` |
| ARM, `164a92c61`, `agent1-arm64-mic1hold43`, 06:35:43Z to 06:41:33Z, **failed** | fmt, strict Clippy `-p lamp-live --all-targets` and lib `reference::` (56 passed) passed; worker-loop test 2 passed, 1 failed: the no-hold control, first session of the process, `QueueTiming` after one frame. `/private/tmp/lamp-a1-mic1-hold-arm-20261012T0635Z-1Fg8tV` |
| ARM, `f18362fa9`, `agent1-arm64-mic1hold44`, 06:48:56Z to 06:51:15Z, all steps passed | fmt; workspace crates recompiled from the exported source; strict Clippy `-p lamp-live --all-targets`; worker-loop test 3 passed; `--test reference_prime --test reference_link` 14 passed; lib `reference::hold` 4 passed. `lamp-live` 9,503,712 bytes, SHA-256 `33f2eb2149bebebde7319c4ecd59a3b3c7470c43f88c2a9fbe56a70aa1589800`. `/private/tmp/lamp-a1-mic1-hold-final-arm-20261012T0648Z-aU7gtV` |
| `16e864692` (test only) | Type-checked on this host with a stand-in for the Linux-only call. Not run |

The capture worker file is Linux-only: on this host it is not compiled, and
the worker-loop test does not exist there. Host results say nothing about
either.

Not run: the workspace suite, anything on a lamp or against the service.

## For you

| Id | |
|---|---|
| `A1-032` | The native fixed-source trial on `mic1` with this binary, levels unchanged: does the device's canceller now remove what the replay says (23 to 28 dB), and is the lamp's own voice no longer scored |
| `A1-033` | Your coffee-alone control on `mic1`. In the Gemini run the clip alone, during the two yields, was at -43.5 and -51.2 dBFS and scored at most 0.09, while the sunset question at -44.9 dBFS scored in 134 of 188 blocks. Whether `mic1` hears that voice at all with the lamp silent decides more than this fix does |
| `A1-034` | If `mic1` is to be used with the fixed reference: one recording with it. Nothing is held there and nothing is measured |

## Still owed by me

- The speech-floor proposal (`lamp-a1-speech-floor`, local, unfinished):
  replay over 29 bundles done, code and tests written, not yet gated or
  pushed.
- A wrong time in this file on `lamp-a1-onset-retention` (it says 05:55 UTC
  for a state reached at 05:29).

## My faults here

- **The first worker-loop test I wrote put its tests in a file that this
  host does not compile.** For the speech floor I had put tests in
  `audio_workers.rs`; they ran as zero tests with exit 0. I moved the logic
  to a module that builds here and now check every test step for a
  nonzero count.
- **My first ARM job failed on my own test.** The control session, first in
  the process, ended on a `QueueTiming` fault after one frame, and the test
  did not print the fault's times, so I could not say why. I had given
  myself one slot and no way to learn from its failure.
- **I meant to zero the speech score of faint blocks before I had replayed
  what else reads that score.** It moves the ends of recorded requests.
  Found before it was built into anything; the proposal caps instead.
