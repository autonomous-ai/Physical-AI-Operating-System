# Process simulation: real lamp-live on simulated hardware (Agent 2 draft)

**Status:** draft on branch `lamp-v2-agent2-sim`, not for integration.
**Evidence boundary:** a Linux ARM64 VM on the Mac. The audio is a digital
loopback room. Nothing here is acoustic, nothing ran on a device, and the VM
says nothing about Lamp scheduling. Results are labelled `process_simulation`
and are never pooled with physical strata.

## What runs

| Part | In the simulation |
|---|---|
| Runtime | Unmodified `lamp-live directed-fixture`, started through the real `lamp-voice-eval lamp-session` relay, with `--diagnostics` and `--cue-socket` |
| Speaker | `plug:device_speaker` → ALSA `snd-aloop` Loopback 0,0 |
| Microphone | `plug:device_micro2` ← Loopback 1,1 |
| Privacy switch | `gpio-sim` chip `/dev/gpiochip1`, line 9 (pull-up = open) |
| Room | `lamp-voice-eval sim-room`: reads the runtime's speaker from Loopback 1,0 and writes the microphone mix (stimulus plus optional echo) to Loopback 0,1, one 10 ms block at 16 kHz per iteration. It records `speaker.wav` and `microphone.wav` and accepts play, cancel, echo, privacy, signal and stop commands on a private socket. |
| Runner | `lamp-voice-eval physical-run` with `room_control` set uses `RoomPlayer`. The stratum is `process_simulation` and the stimulus source is `digital_mix`. |

Measured route during a trial: every loopback substream in use runs at
16000 Hz mono S16_LE, period 160 frames, buffer 640 frames (40 ms). The room
opens its end of each cable first, so `snd-aloop` holds the speaker cable at
16 kHz. lamp-live therefore writes 24 kHz into `plug:`, which converts to
16 kHz. The VM kernel uses `CONFIG_HZ=1000`, and `snd-aloop` uses its default
jiffies timer.

The VM's own `virtio_snd` device is blacklisted, so no simulation audio can
reach a host speaker.

## Setup and exact commands

The files are in `crates/voice-eval/sim-harness/`.

1. **Once:** run `vm-setup.sh`. It creates the Colima VM `lampos` (6 CPU,
   12 GB, vz) with Ubuntu 24.04.1 and kernel `6.8.0-146-generic` with
   `linux-modules-extra`. It loads `snd-aloop` and `gpio-sim`, blacklists
   `virtio_snd` and builds the `lampos-dev:1.96` image from `Dockerfile`.
2. **After every VM boot:** run `vm-boot.sh` to create the gpio-sim chip.
3. **Data directory:** use `~/.lampos-sim`, mounted at `/sim`.
   - Generate the fixtures with `python3 -I make-fixtures.py ~/.lampos-sim`.
     Expected `ask.wav` SHA-256: `529ce6c4…919815c`.
     Expected `reply.wav` SHA-256: `d9afb80e…14132421`.
   - For end-to-end runs, also render the stimulus cache on macOS with
     `cargo run -p lamp-voice-eval -- render-stimuli ~/.lampos-sim/acoustic ~/.lampos-sim/render-1.json`.
     It writes files only and plays nothing.
4. **Run:** use `run-sim.sh REF 'COMMANDS'`. It extracts exactly the committed
   `lampOS` tree of `REF`; working-tree edits are never included.

```sh
H=crates/voice-eval/sim-harness
# Interleaved trials: echo gain 0.3 and 0, N pairs (one 10 s session each)
$H/run-sim.sh REF 'N=20 OUT=/sim/repro/REF sh crates/voice-eval/sim-harness/batch.sh'
# The real physical runner against the real runtime (three fixed-reply scenarios)
$H/run-sim.sh REF 'OUT=/sim/e2e/NAME SCENARIOS="fixed-reply-echo-only fixed-reply-acknowledgment fixed-reply-topic-change" REPS=1 sh crates/voice-eval/sim-harness/e2e.sh'
# VM wakeup latency while trials run
$H/run-sim.sh REF 'sh crates/voice-eval/sim-harness/loaded-jitter.sh'
```

A trial counts only if the room started, the stimulus was delivered and the
runner's commands were accepted. Otherwise it is `INVALID` and excluded.

## Results

All results below use Rust source `0b00407fc8a49ac470db1e19967c6435feb66a3e`.
That is `lamp-v2-agent2` `b7fbe352` plus this draft. lamp-live is unmodified
from `b7fbe352`. The harness scripts are the committed ones, with only paths
changed.

### 1. The directed runner against the real cue emitter

The `e2e.sh` run `0b00407f-1` covered three fixed-reply scenarios, one
repetition each.

- **Cue validation:** 0 protocol errors in all three attempts. The runner
  validated every cue type the real emitter sent: `listening_ready`,
  `input_admitted`, `local_endpoint`, `speaker_first_write`,
  `speech_retired`, `turn_completed`, `cancelled` and `run_end`.
- **Triggers:** the triggers fired from real cues. `listening_ready` started
  the first stimulus. `speaker_first_write` started the overlap
  (acknowledgment) and observation steps. The Lamp-to-runner clock mapping
  was within ±1.1 to ±2.8 ms.
- **Software latencies:** these are software latencies only, with no
  acoustic claim.
  - Runner start request after the intended stimulus time: p50 11 ms,
    max 18 ms (n = 5).
  - Local endpoint to first accepted reply write: p50 9 ms, max 15 ms
    (n = 2).
- **Failures:** all three attempts failed through runtime faults (section 2).
  - Topic change failed 2.3 s after ready, during the stimulus.
  - Acknowledgment failed at 7.8 s, while idle after the reply.
  - Echo-only failed at 20.8 s, while idle. Its run ended with
    `ReferenceLate`.

The repeat run `0b00407f-2` covered the same three scenarios with two
repetitions, 6 attempts in all.

- **Cue validation:** 0 protocol errors across 43 real cues.
- **Passed (3):** acknowledgment ×2 and echo-only ×1.
- **Failed (1):** an echo-only attempt hit `ReferenceLate` 0.3 s after
  ready. The runner marked its first step `session_ended` and injected
  nothing after it.
- **Incomplete (2):** both topic-change attempts. This is correct evaluator
  behaviour, not a runner fault.
  - The 2 s fixture reply ended at 6.09 s, and the interruption was
    admitted only at 6.79 s.
  - The evaluator withheld the step as `overlap not achieved`.
  - Interruption scenarios need a fixture reply longer than the trigger
    delay plus admission time.

**Across both runs:** 9 runner attempts and 0 protocol errors. 4 attempts
ended on the runtime faults in section 2.

### 2. Session-ending speaker underruns (EPIPE, ReferenceLate)

**Observation.**
- A single playback discontinuity ends the whole lamp-live session. The
  speaker worker fails `snd_pcm_avail_delay` with EPIPE, and the run ends
  `failed`. When the underrun stops the reference stream, the capture
  side reports `ReferenceLate`. The reply in progress is lost (in one
  case, the trace showed `runtime_failed` 620 ms into a 2 s reply).
- This is by design in `crates/audio/src/pcm.rs`, `poll_available`: any
  availability error outside the expected end state faults the stream.
  Whether that is the intended product behaviour is Main's decision.
- The speaker asks for a period of 240 frames and a buffer of 960 frames
  (40 ms) at 24 kHz. There is no real-time scheduling class.

**Batch `0b00407f`** (`batch.sh`, N = 20 pairs, interleaved, VM otherwise
idle; one host compile overlapped trials 10 to 12):

| Echo gain | ok | failed | invalid | Failure |
|---|---|---|---|---|
| 0.3 | 19 | 1 | 0 | trial 15: speaker EPIPE after the turn finished |
| 0 | 19 | 1 | 0 | trial 12: speaker EPIPE 620 ms into the reply (`runtime_failed`) |

The room process itself (a trivial 10 ms read/write loop with the same 40 ms
buffer) recorded at least one xrun in 10 of the 40 trials. Linux CPU pressure
(`/proc/pressure/cpu` some avg10) was 0.00 at both failures.

**VM wakeup latency** (`jitter.py`, a 1 ms sleep loop):

| Condition | p50 | p99.9 | Max | Over 30 ms |
|---|---|---|---|---|
| Idle, 60 s | 0.29 ms | 5.1 ms | 12.6 ms | 0 |
| During 6 trials, 75 s | 0.25 ms | 3.4 ms | 13.6 ms | 0 |

Two of those six trials failed with `ReferenceLate`. Steal time was 0.

**What this establishes.**
- Echo does not explain the faults: 1 of 20 failed at each gain.
- A generic thread never waited long enough to empty a 40 ms buffer, even
  in failing trials. Whole-VM stalls are therefore not the explanation.
- The room's single xrun in run `0b00407f-1` (at 4252.66 s) does not
  coincide with lamp-live's three faults (4249.69, 4276.03 and 4317.22 s).
- In the earlier invalid batch, about 100 s of sessions ran with no room on
  the cables, and the speaker never underran (one different failure; see
  section 3). This is a clue, not a result.

**What it does not establish.**
- Whether the cause is the simulated audio path or a lamp-live speaker
  stall. The candidates are `snd-aloop` jiffies timing, the 24-to-16 kHz
  `plug` conversion, or cable coupling with the room.
- Whether real Lamp hardware underruns, and how often. If the physical pilot
  shows `run_end failed` with this EPIPE or `ReferenceLate`, this is the
  matching simulated reproduction.

**Retracted earlier claims.**
- "Echo 0.3 failed 5/20": concurrent containers clobbered the shared source.
- "3/8 vs 0/8 with echo off": too small a sample, and not reproduced above.
- Batch `86c97723`: invalid. The room's control socket was never unlinked,
  so most trials had no room. Fixed in `0b00407f`; trials are now classified
  as valid or invalid.

**Next checks (after the pilot).**
- Hold the room at 24 kHz so the speaker cable needs no `plug` conversion.
- Switch `snd-aloop` to an hrtimer source.
- Record per-write timing in the speaker worker to locate the stall.

### 3. Stale runtime messages (one occurrence, room absent)

Seen once in the invalid batch, in a trial where the room had failed to
start:
- 3.6 s after ready, the speaker and capture workers both faulted with
  `stale, reordered or wrong-boot runtime message`.
- The provider and privacy workers exited, and the run ended with
  `physical privacy closed or lost freshness`.

It has not recurred in 40 valid trials and 9 runner attempts. It is
unexplained and not reproduced.

## Retained evidence (local, not committed)

| Path under `~/.lampos-sim` | Contents |
|---|---|
| `repro/0b00407f/` | `summary.txt` and both failing trials: `events.jsonl`, `live.err`, `room-events.jsonl`, `speaker.wav`, `microphone.wav` |
| `e2e/0b00407f-1/` | `ledger.jsonl`, the evaluation report and attempt pages, and the room recordings |
| `repro/86c97723-invalid-stale-socket/` | The invalid batch, kept for the section 3 trace |
| `fails/` | The retracted 5/20 batch (snapshot `e973944f`) |

Ask Agent 2 for any of these files.
