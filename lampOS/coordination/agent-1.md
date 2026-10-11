# Agent 1 status

Owner: Agent 1 (audio). This file on `lamp-a1-reference` is the status of
`MAIN-053` and `MAIN-054`. `MAIN-050` is frozen on `lamp-a1-duplex` at
`e2aa5df1c` (tested code `bc2f1a7df`) with its own status there. Playout is
on `lamp-a1-playout` at `49dc273d9`.

- Updated: 2026-10-11
- **Checkpoint 2: the native blocker is diagnosed, reproduced and has an
  opt-in fix, corrected after your review (`MAIN-058`). Nothing has run on
  a lamp.** Write-up:
  [`docs/reference-recovery.md`](../docs/reference-recovery.md). Evidence:
  `docs/verification/reference-recovery-20261011/`.
- **Do not deploy `a39d04d84`.** It classifies a held-up capture loop as
  drift. Use the head of this branch.
- Tested code: `7ebd9af6b`; the head adds only a lab switch
  (`1d24e3288`). Base: the frozen `e2aa5df1c`, which is on your
  `6d81ab49f`.
- Your files are changed in one commit, `69e7a3521`.
- No device, no cloud. I read your run's evidence in place and copied
  none of it.

## The assignment as it reached me

`MAIN-053` and `MAIN-054` came through my terminal and are not in Git. I
took them as: make the audio reference survive what need not end a
conversation, with explicit bounded degradation and recovery, never by
widening freshness, inventing silence or losing samples quietly; and first
of all the native run that ended at 378.6 s. If the brief is ever written
down, check this against it.

## The native run: what ended it

`/private/tmp/lamp-serve-native-20261011-03`, from its own
`metadata/events-0.jsonl`.

- **Clock drift, and only clock drift.** The speaker on `4ace` runs
  157.3 ppm fast against the microphone: least squares over the 377
  once-a-second `reference_clock` reports (you got 157.235 from the same
  records). It is the drift of the two counts as the software kept them,
  not a measurement of either oscillator. The fixed margin read 2, then 3
  at 91.5 s, 4 at 146.9 s, 5 at 207.3 s, 6 at 261.6 s, 7 at 327.0 s, and 8
  with the next packet the fault at 378.6 s. Six blocks at that rate is
  382 s.
- **Nothing was late.** Reports 1.0085 s apart at the median and 1.012 s at
  worst, 100 or 101 capture blocks each. The newest packet was 8 ms old at
  the fault.
- **Nothing was lost.** Expected sequence 37,862 after 37,861 packets and
  the prime.
- **So every fixed-margin session on that lamp ends at about six
  minutes.** Idle is when this one happened to get there.

This is the limit `MAIN-050` reported from a 102 s recording as "ten to
fifteen minutes at 50 ppm to 150 ppm". The rate on this lamp is 157 ppm.

## The fix

`--reference-recovery bounded`, off by default, refused under `vad-only`.

| With | It recovers | How |
|---|---|---|
| The fixed reference, as the pilot runs | A margin the clocks used up, and nothing else | **Re-centre**: canceller reset, the newest accepted blocks given again as its lead, the count moved by what a `reference_status` says, new DSP epoch through the existing `CaptureDspReset`. Two blocks early when the room is idle |
| `--reference-clock capture` | Nothing to re-centre: its count does not drift. Loss, overflow, a speaker that paused | **Untrusted**: the canceller is not run and the capture goes on as recorded. **Reseed** on 50 ms of fresh, gapless reference. Three reseeds a minute, untrusted a second at most, then a named fault |

After either, echo evidence is withheld for three seconds of the lamp's
own sound, which is V1's wait.

In virtual time through the production calls at this lamp's rate:
without the flag `ReferenceBacklog` at 41,322 blocks (the lamp: 37,855;
its packets jitter, the model's do not); with it an hour, eight
re-centres 445 s apart, no block bypassed and no counter rewound.

**For the pilot there are three candidates, and the choice is yours.**

| | Flags, with `v1-baseline` | Drift | A hold-up |
|---|---|---|---|
| Capture-strict | `--reference-clock capture` | Does not drift. No canceller resets | A fault, as now |
| Fixed, re-centred | `--reference-recovery bounded` | A canceller reset about every four minutes in a quiet room, seven and a half otherwise | A fault, as now |
| Capture, recovered | both flags | Does not drift | Short of breaking a device: untrusted, reseeded, warmed |

You are considering capture-strict first. I know no reason against it and
have two measurements for it since I first wrote this: in replay of four
recorded sessions the resampled reference cancels as well as the recorded
interleave (better on two, worse on one, level on one), and its ring
stayed between 960 and 1,490 frames of 2,400. What to read in the soak:
`reference_clock.details.drift` once a second, `learned_ppm` settling
near 157 within about two minutes and `unread_frames` between roughly 950
and 1,900.

What all three share: the starvation check ends a session when the newest
queue observation is older than the queue it saw, about 19 ms at 456
frames. The lamps' worst so far is 10.8 ms.

## What this does not fix

- **A stall long enough to break a device.** On a lamp the microphone
  buffer is 640 frames, 40 ms, and the speaker's at most 960 with about
  456 queued. Longer than that and the device itself overruns or
  underruns, and `capture.rs` and `pcm.rs` end the worker. Recovering
  that is a device restart in ALSA code I cannot run. In 453 s of
  recordings from two lamps the worst gap between capture reads was
  12.6 ms and between reference packets 20.9 ms, so I have no evidence it
  happens. My paced runs died of 40 ms to 65 ms stalls because this Mac
  has them.
- **Tests of owner change and shutdown through the coordinator during
  recovery.** Not written. Recovery lives entirely inside the capture
  worker's reference and touches no ownership or stop state; that is by
  construction, not by test.

## Your review, `MAIN-058`

All three findings were real and mine. Your `audit_*` tests are in
`recovery_tests.rs` as written, reformatted only, and pass.

| Finding | Fix, in `5df4248c3` |
|---|---|
| A capture loop held up 20 ms at a high margin was re-centred as drift, on the previous block's backlog | A packet past the bound is taken in, four blocks at most, and nothing is decided on it. The next capture block decides on its own backlog and on the read before it being within 20 ms. Held up is `ReferenceBacklog`, no reseed. The slow side needs the newest packet within 20 ms |
| Fresh reference at the one-second deadline reseeded an expired interval | The deadline is checked before any reseed: `RecoveryTimedOut` |
| A queue observation from the future counted as late | From the future, or before the last one: `QueueTiming` |

One number moved with the first fix: re-centres at the bound are 445 s
apart at this lamp's rate, not 382 s, because the count now goes from nine
ahead back to two.

## Paced, since checkpoint 1

`crates/live/examples/reference_recovery_pace.rs`, two host processes.
Ten scripted scenarios behaved as designed; the table is in the write-up.
Three ten-minute runs at 157.3 ppm, together, with the ARM gate and the
host suite running beside them:

- **Capture-clocked with recovery ran all 600 s.** The host held the
  speaker process up once, 43 ms at 520 s: untrusted for 13 ms, four
  blocks bypassed, trusted again three seconds later. It is the first
  paced run on this Mac to finish.
- Fixed with recovery re-centred early at 139.6 s, then ended at 160.4 s
  in `QueueTiming` when the host held the capture loop for 31 ms. Not
  relabelled as drift.
- Fixed without ended at the same block in `ReferenceBacklog`.

## Verification

| Where | On | What | Result |
|---|---|---|---|
| Host | `7ebd9af6b`, source-only export | `cargo fmt --all -- --check` | clean |
| Host | same | `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | clean |
| Host | same | `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 1,401 passed, 0 failed, 7 ignored |
| Host | same | `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | built |
| Host | working tree of `5df4248c3` | The hour-long re-centre test, release, `--ignored` | passed, eight re-centres |
| Host | `1d24e3288` | fmt and strict Clippy for `lamp-live`, all targets | clean. It changes `examples/duplex_lab.rs` only |
| Emulated Linux/ARM64 | `5df4248c3` | Strict Clippy for `lamp-audio` and `lamp-live`, all targets; focused tests in release; release build with examples | clean; 21 and 127 passed, 2 ignored; aarch64 ELF |

The workspace suite ran on the host only. The two examples added after
`5df4248c3` were not compiled under emulation. The Linux-only capture
worker was compiled only there. None of it is device time.

### ARM jobs

Existing `lampos-arm64` connection and image, no VM lifecycle change.

| Job | Commit | Start (UTC) | End (UTC) | Other containers at start | Result |
|---|---|---|---|---|---|
| `agent1-arm64-verify26` | `7b9715d83` | 15:42:47 | 15:47:28 | none | Worker wiring compiled and linked; 21 audio and 124 live focused tests; **strict Clippy failed**: an eighth argument on `capture_loop`. Fixed in `a39d04d84` |
| `agent1-arm64-verify27` | `a39d04d84` | 15:48:03 | 15:52:44 | none | Strict Clippy clean, 21 and 124 tests, aarch64 ELF. Superseded by your review |
| `agent1-arm64-verify28` | `5df4248c3` | 16:11:31 | 16:16:21 | none | Strict Clippy for `lamp-audio` and `lamp-live`, all targets, clean; 21 audio and 127 live focused tests, your three among them, 2 ignored; release build with examples is aarch64 ELF |

## Commits

| Commit | Whose files | What |
|---|---|---|
| **`69e7a3521`** | **yours** | `options.rs`: `--reference-recovery`. `wire.rs`: `ReferenceStatus`, `ReferenceTrust`, `ReferenceCause`, `RecoveryCounts`, two fault kinds, `MicrophoneFrame.reference`, `ReferenceTiming.recovery`. `coordinator.rs`: the option to the worker, `reference_status` in the trace. `admission.rs`: one test. `main.rs`: usage |
| `7b9715d83`, `a39d04d84` | mine | `reference/recovery.rs` and its tests, hooks in `reference.rs`, the capture worker, `Controller::hold` |
| `5df4248c3` | mine | The three corrections, with your regressions |
| `7ebd9af6b`, `1d24e3288` | mine | The paced recovery check; `duplex_lab --capture-clock` |

`69e7a3521` needs `7b9715d83` on Linux: the worker fills the frame field
the wire gained.

## For you

| Id | |
|---|---|
| `A1-018` | **A device soak**, whichever candidate you take first. For the re-centre, pass is no `reference_fault` and a `reference_status` with cause `margin_exhausted` about every four to seven minutes. For capture-strict, no `reference_fault` and the `drift` figures above |
| `A1-019` | **Should a first prime warm too?** After a reseed I withhold evidence for three seconds of sound. The canceller is just as cold at the start of a session, and `MAIN-050`'s duplex mode does not wait there. I left the frozen behaviour alone |
| `A1-020` | **The host clock on the lamps reads 3 % fast against both audio clocks.** Capture reads and reference packets are 10.305 ms apart at the median on both lamps, for nominal 10 ms blocks. Both streams agree, so nothing here depends on it, but `before_capture` interpolates the speaker queue at the nominal rate against that clock. I have not looked for the reason |
| `A1-017` | Recording a yield so an audit can tell it from starvation: not done yet. It is next after the paced tests |

## Accepted from your review of `MAIN-050`

- The hold and queue stay behind duplex; the two flags stay separate.
- **"The canceller removes the person" and "hears nobody" are results of
  a software-mixed replay**, not findings about a person's voice on this
  microphone board. `docs/duplex-listening.md` on this branch now says so
  where it said more. The frozen checkpoint is untouched.
