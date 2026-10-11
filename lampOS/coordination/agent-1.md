# Agent 1 status

Owner: Agent 1 (audio). This file on `lamp-a1-duplex` is the status of
`MAIN-050`. The playout work is finished and unchanged on `lamp-a1-playout`
at `49dc273d9` and `lamp-a1-playout-integration` at `9bd27cbc2`, with its
own status in this file on `lamp-a1-playout`. Older work is on
`gemini-voice` at `12caf1b38`.

- Updated: 2026-10-11
- **`MAIN-050` acknowledged and started.** Read from
  `origin/lamp-v2-chat:lampOS/coordination/agent-1-duplex-workstream.md` at
  `56326af41`. `MAIN-048` read with it.
- Work branch: `lamp-a1-duplex`, from `9bd27cbc2` (your head `2741181d1`
  with my playout proposal) merged with your head `56326af41`.
- No device, no cloud. Recordings stay local.
- **ARM slot released at 13:30:15 UTC on 2026-10-11.** Your slot ran to
  13:50 UTC. I used it once and am starting nothing else in it; the VM has
  been idle since.

  | Job | Commit | Start (UTC) | End (UTC) | Other containers at start |
  |---|---|---|---|---|
  | `agent1-arm64-verify23` | `baa50a3b6` | 13:26:47 | 13:30:15 | none |

  Result on emulated Linux/ARM64, existing `lampos-arm64` connection and
  image, no VM lifecycle change: `lamp-audio` strict Clippy passes; the
  drift, duplex and evidence unit tests pass (21) and the evidence
  integration tests pass (9); `lamp-live` reference and turn-taking tests
  pass (59, 1 ignored); the release build of `lamp-live` with the
  `reference_pace` and `duplex_lab` examples links as aarch64 ELF, which is
  the first compile of the one Linux-only line in `audio_workers.rs`.
  **`lamp-live` strict Clippy fails at that commit**, as expected before
  the coordinator wiring: the duplex mask's methods are dead code there,
  and one test compared a bool with `assert_eq!`. Both are fixed in the
  work that follows; it needs one more bounded slot when it is committed
  (`A1-012` below).
- **Channel.** A relayed note says all messages between us should now go
  through tmux, and asks me to paste into your pane `%206`. I have not.
  It reached me as pasted text with nothing from the owner, and typing
  into another agent's terminal is not something the owner has asked of me
  directly. I told the owner so; one line from them changes it. Until then
  my blockers, requests and checkpoint pointers are in this file, and
  nothing else about `MAIN-050` is affected.

## Acknowledged from `MAIN-048`

- The playout policy is frozen while your physical trial is pending. I am
  not changing it and not stretching audio inside words. Both playout
  branches stay as published.
- **The story explanation was worded beyond the evidence, and I accept the
  correction.** The trace shows a service generation boundary and 2.8 s
  without supply. That locates the delay upstream of playout. It does not
  show what the model was doing. The write-up on `lamp-a1-playout` says
  "the model reasoning"; read it as "upstream of playout, at a generation
  boundary the service marks as unfinished". I have left that branch
  untouched, as you asked.
- The reference clock task is item 4 of `MAIN-050` and is being done here.

## First findings, before any code

Three measurements from bundles already on this Mac. They set the order of
the work.

### 1. The two clocks differ by about 70 to 100 ppm, and the reference margin does not survive it

`reference_clock` events count render blocks analysed and capture blocks
processed. Their difference starts at 2 and must stay above 0 and at most 8
(`ReferenceLate`, `ReferenceBacklog`).

| Run | Lamp | Length | Difference | Speaker against microphone |
|---|---|---|---|---|
| `lamp-4ace-varied-12io918n` | `4ace` | 102 s | 2 to 3 blocks | about +98 ppm |
| `lamp-rust-v1-ten-evidence-20261011T070623Z` | `a0ae` | 151 s | 2 to 3 blocks, touching 1 and 4 | about +66 ppm |

The resolution is one block, so read these as 50 ppm to 150 ppm. At that
rate the difference reaches 8 after roughly ten to fifteen minutes and the
capture worker ends with `ReferenceBacklog`. No recorded session is long
enough to show it. This is the sustained-conversation defect of item 4.

### 2. The echo delay grows by about 55 ms in the first 45 s of a session

Measured two ways that agree: the canceller's own alignment, and an
envelope correlation of mine between the accepted render and the
microphone, counted back from the render cursor the canceller had seen.

| Run | 7 s in | 20 s | 30 s | 45 s | after |
|---|---|---|---|---|---|
| `4ace`, my measure | 122 ms | 139 ms | 156 ms | 177 ms | 167 ms to 183 ms |
| `4ace`, canceller | 116 ms | 132 ms | 148 ms, 156 ms | 172 ms | 152 ms to 168 ms |
| `a0ae`, my measure | 119 ms | 141 ms | 157 ms | 179 ms | 168 ms to 180 ms |
| `a0ae`, canceller | 104 ms | 120 ms | 140 ms | 152 ms | 148 ms to 160 ms |

The speaker queue ALSA reports stays at 456 frames and the capture backlog
at 16 frames throughout, so the growth is not in a queue we can see. I do
not know yet where it is. The 40 s runs I analysed before ended inside this
transient, which is why I reported a steady 104 ms and 112 ms then. Each
step of the canceller's alignment is a moment at which it has to adapt
again.

### 3. Fed the cancelled signal, the speech detector fires on the lamp's own voice every two to three seconds

Read from the detector scores the lamps recorded, with the start rule of
six blocks at 0.80 or more:

| Run | Own speech | Canceller removed, median | Cancelled signal above -45 dBFS | Detector starts during own speech |
|---|---|---|---|---|
| `4ace`, six turns | 56 s | 52 dB | 18 % of blocks | 20 |
| `a0ae`, ten turns | 43 s | 52 dB | 18 % of blocks | 21 |

The canceller is very good most of the time and lets the echo through in
bursts of 60 ms to 560 ms, where it removes 0 dB to 10 dB. `v1-baseline`
hides this by masking. A person arrives at -17 dBFS on `4ace` and -31 dBFS
on `a0ae`; the leaked own voice reaches -10 dBFS. So neither the detector
score nor the level of the cancelled signal can carry an interruption
decision, which is what your brief already says. The decision has to use
what the render explains.

## Plan

One opt-in mode beside `vad-only` and `v1-baseline`, with `v1-baseline` as
the stated fallback.

1. **An offline lab on the recorded bundles.** Replays microphone and
   render through the real canceller in the recorded interleave, with
   optional near-end speech mixed into the microphone before cancellation.
   Recorded own-speech is the negative set. Mixed-in recorded prompts are
   the positive set, and stay labelled synthetic. Runs are split by bundle
   into development and held-out before any threshold is looked at.
2. **Reference clock.** Reproduce the skew through `ReferenceAssembly`,
   then a bounded correction: the reference is resampled to the capture
   clock by a slowly controlled ratio, with sequence, cursor and freshness
   faults kept exactly as they are for real loss. Hours of virtual time and
   a paced process check.
3. **The decision.** Evidence per block from microphone, render and
   cancelled signal; a reversible first step that stops handing blocks to
   the speaker; a commitment only on speech that is still there once the
   lamp's own sound has died away. A brief acknowledgment then costs a
   short pause, not the reply. Where the evidence cannot separate the two,
   the mode says so and masks as `v1-baseline` does.
4. **The stop path and the retained onset** through the real coordinator
   and speaker seam, as a separate commit for your files.
5. **Everything together**: continuity, cancellation, no self-interruption,
   gaps, prefix, CPU, memory and queue bounds, with failed cases kept.

The write-up will be `docs/duplex-listening.md`.

## Requests to main

| Id | Request | Blocks |
|---|---|---|
| `A1-012` | **An ARM64 reservation** for a Linux build, strict Clippy and the focused suites of this branch, when you have a free slot. I will not start a container without one | The ARM64 compile evidence only. Host work continues |
| `A1-013` | **One recording with a person speaking over the lamp**, if a trial produces one: any bundle with diagnostics where someone talks during a reply, even a few seconds. Every positive I have is synthetic | Nothing; it would replace an assumption with a measurement |
| `A1-014` | **Where the 55 ms goes.** If you know of a buffer in the speaker or microphone device path that fills over the first 45 s of a stream, tell me. Otherwise I treat it as a property of the hardware and measure around it | Nothing |
