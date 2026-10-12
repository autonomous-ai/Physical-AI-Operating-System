# mic1: the canceller was given the render after its echo

Agent 1, 2026-10-12. A proposal on `lamp-a1-mic1-reference`, on
`lamp-v2-chat` `675c71c62`. Not integrated, not run on a lamp.

With `--microphone mic1` and `--reference-clock capture`, each captured
frame now waits four capture blocks (40 ms) before it is cancelled and
handed on. Nothing changes for `mic2` or for the fixed reference.

## What was found

Main recorded the same fixed reply through each microphone
(`lamp-micpath-mic1-executed-lw4abily`, `lamp-micpath-mic2-executed-lw4abily`):
one 4.49 s recording played three times, a cached clip arriving in the
second copy. `mic1-pair.json` and `mic2-pair.json` hold the comparison.

| | mic1 | mic2 |
|---|---|---|
| Lamp alone, before cancellation (copy 1, copy 3), dBFS | -15.0, -15.5 | -13.6, -14.6 |
| Lamp alone, after the device's canceller | -20.2, -17.1 | -34.1, -46.0 |
| Removed | 5.2 dB, 1.6 dB | 20.5 dB, 31.4 dB |
| Lamp alone, blocks the detector scored 0.8 or over | 114 of 430, 193 of 360 | 9 of 430, 5 of 360 |

The reason is in the bundle (`mic1-reply-starts.json`, `mic1-timing.txt`).
At each of the three starts of the reply on `mic1`, the capture block that
already holds the lamp's voice at -21.7 dBFS, and the next at -11.8 dBFS,
were cancelled while the canceller's render cursor was still 458 and 217
frames short of the first loud render frame. A fixed least-squares filter
from render to microphone peaks 16.8 ms before the cursor and removes
13.1 dB. On `mic2` it peaks 80.2 ms after the cursor.

The capture-clocked reference keeps five blocks of accepted render in hand,
so that four packets in a row can be late. The canceller is therefore given
a render block about 49 ms after the speaker accepted it (1,172 frames in
the recording). Through `mic1` the sound is back after about 30 ms; through
`mic2`, whose path is about 97 ms slower, after about 130 ms. A canceller
cannot take out an echo that arrives before its render.

The `mic1` Gemini duplex run (`lamp-mic1-gemini-executed-o7lfc33c`) has the
same lead at both reply starts, 1.7 dB removed over the whole reply, and two
yields, each straight after the lamp's own voice.

## The change

`crates/live/src/reference/hold.rs`: `CaptureHold`, a bounded queue of
captured frames, and `blocks_for(device, clock)`: four for
`plug:device_micro1` with the capture-clocked reference, zero otherwise.
The capture worker steps the reference on the frame it has just read, as
before, and cancels and hands on the frame that has waited.

- A frame is not altered, reordered or relabelled. It leaves with its own
  sequence and its own capture times.
- Frames still held when the worker stops, or the microphone is closed, are
  dropped with the queue.
- With a hold of zero every frame comes straight back: `mic2` runs the code
  it ran.
- No option and no shared file. `mic1` is the opt-in.

Cost: a `mic1` frame is 40 ms older when it reaches the coordinator. The
coordinator refuses a frame older than 100 ms, so about 55 ms are left for
a stall where there were about 95. Speech start, admission and what the
provider is sent are 40 ms later than without it, and still about 57 ms
ahead of the same moment through `mic2`.

Keeping less render in hand would close the gap with no delay. With two
blocks in hand in place of five, one late packet runs the ring dry.

## Replay on the recorded pair

`crates/live/examples/duplex_lab.rs --capture-hold N`: the recorded
microphone signal and the recorded render schedule through the real
canceller on this host, each microphone block given to the canceller N
blocks late (`lab-hold-sweep.txt`).

| | Removed, copy 1 | Removed, copy 3 | Lamp alone scored 0.8 or over | Sunset question scored 0.8 or over |
|---|---|---|---|---|
| mic1, device | 5.2 dB | 1.6 dB | 114 of 430, 193 of 360 | 124 of 188 |
| mic1, N = 0 | 6.0 dB | 1.7 dB | 79, 180 | 125 |
| mic1, N = 2 | 7.8 dB | 6.2 dB | 92, 79 | 125 |
| mic1, N = 3 | 27.5 dB | 27.0 dB | 6, 0 | 124 |
| **mic1, N = 4** | **23.2 dB** | **27.9 dB** | **4, 0** | **124** |
| mic1, N = 5 | 22.5 dB | 27.4 dB | 6, 0 | 124 |
| mic1, N = 6 | 23.7 dB | 26.0 dB | 5, 0 | 124 |
| mic1, N = 8 | 24.1 dB | 25.6 dB | 4, 0 | 124 |
| mic2, device | 20.5 dB | 31.4 dB | 9 of 430, 5 of 360 | 149 of 188 |
| mic2, N = 0 | 19.8 dB | 22.0 dB | 1, 14 | 149 |

With no hold the replay does what the lamp did, on both microphones. Three
blocks is the fewest that cancel `mic1`. Four is what is built: one block
over that, for what two sessions do not show.

The clip over the lamp, at N = 4 (`mic1-lab-hold4-pair.json`; N = 0 and
N = 3 beside it). Four stretches of the clip's speech; the second and third
rows are the same places in the copies without the clip.

| After cancellation | 0-590 ms | 770-1140 ms | 1370-3070 ms | 3280 ms on |
|---|---|---|---|---|
| Level with the clip, dBFS | -36.4 | -52.8 | -42.6 | -46.9 |
| Copy 1, copy 3 | -30.3, -37.2 | -48.2, -56.2 | -46.6, -42.5 | -54.1, -53.2 |
| Correlation with the clip | 0.01 | 0.33 | 0.02 | 0.24 |
| Copy 1, copy 3 | 0.02, 0.01 | 0.06, 0.07 | 0.01, 0.01 | 0.05, 0.05 |
| Scored 0.8 or over, with the clip | 0 of 59 | 0 of 37 | 0 of 170 | 1 of 71 |
| Scored 0.5 or over, with the clip | 0 | 4 | 0 | 6 |

**This stops the lamp hearing itself through `mic1`. It does not make
`mic1` hear a person over the lamp.** The clip is 27 to 38 dB under the
lamp's own voice at that microphone and 23 to 28 dB come off: where the
lamp is loud the clip stays under what is left, and the detector scores one
block of 337.

## Through the capture worker

`crates/live/tests/capture_hold_worker.rs`, Linux only: the worker's own
`capture_on` loop over a scripted microphone, with the controller's side of
its channels and the speaker's side of its reference link played by the
test in real time. The microphone holds the sound of a render block 30 ms
after the speaker accepted it, two blocks before the ring gives that block
to the canceller, as on the lamp.

Run on ARM under emulation, commit `f18362fa9`, container
`agent1-arm64-mic1hold44`: three sessions, after one run and thrown away.
Medians.

| Session | Blocks the microphone held | Frames handed on | Read after the block was there | Handed on after its own read | Last second, before and after | Render accounts for |
|---|---|---|---|---|---|---|
| `mic1`, stopped | 400 | 396 | 1.3 ms | 43.8 ms | -26.8, -55.4 dBFS | 99 percent |
| `mic1`, muted | 100 | 96 | | | | |
| another microphone, stopped | 400 | 400 | 1.4 ms | 4.0 ms | -26.8, -52.3 dBFS | nothing: no alignment |

In every session the sequences are contiguous from 1, each frame's read
time is its own (not before the microphone held the block, increasing,
not after it was processed), and nothing arrives after the stop or the
mute. The four frames held at that moment are never handed on.

**The level does not separate the two streamed sessions.** On this scripted
noise the canceller's suppression lowers the output as far with no hold.
What differs is what the canceller accounts for: 99 percent of the
microphone block with the hold, no alignment at all without. Commit
`16e864692` makes the test assert that and not the level; it is
type-checked on this host and **not yet run on ARM**. What the hold does to
cancellation rests on the replay of the recorded bundle above.

The first ARM job (`agent1-arm64-mic1hold43`, `164a92c61`) failed: its
first session, the control, ended with `QueueTiming` after one frame. The
worker ends a session when the microphone has more than 40 ms unread or a
render packet is more than 40 ms old. My reading is that the first use of
the canceller in a process under emulation took longer than that; the test
did not print the fault's times, so this is not established. Since
`f18362fa9` one session is run first and thrown away, and a fault's times
are printed. In the second job the first session ran through.

## Limits

- The replay runs a canceller that starts cold on this host, with the
  render cursor rounded to whole blocks. One recording per microphone.
- The lead was 18.5 to 19.5 ms in the two `mic1` sessions recorded. Four
  blocks leave 20 ms. A session in which the sound came back more than
  40 ms before its render would not be cancelled.
- With the fixed reference every accepted block goes to the canceller as it
  arrives. Nothing is held there, and there is no `mic1` recording with it.
- The voice-eval fake does not go through the capture worker.
- Nothing here ran on a lamp.

## Files

| File | What |
|---|---|
| `mic1-pair.json`, `mic2-pair.json` | the device recordings compared |
| `mic*-timing.txt`, `mic*-reply-starts.json` | when the lamp's voice is in the microphone against when the canceller has its render |
| `lab-hold-sweep.txt` | the replay at each hold |
| `mic1-lab-hold{0,3,4}-pair.json` | the comparison run on the replay's output |
| `inputs.sha256` | digests of the bundle files, clips and lab binary used |
| `scripts/` | what produced them; `scripts/analyse_run.sh` names the order |

The bundles are local recordings and are not in the repository.
