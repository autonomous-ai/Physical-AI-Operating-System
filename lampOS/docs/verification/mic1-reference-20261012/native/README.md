# mic1 with the capture hold, on the lamp, against the baseline

Agent 1, 2026-10-12. Read-only analysis of two native fixed-source
recordings by Main on lamp `4ace`, `mic1`, `v1-baseline`, capture-clocked
reference, mixers unchanged. No audio is copied here.

| | Baseline | Treatment |
|---|---|---|
| Run | `lamp-micpath-mic1-executed-lw4abily` | `lamp-mic1-timing-executed-69rmqb_6` |
| `lamp-live` SHA-256 | `14fca820…8941e` | `33f2eb21…9800` (commit `f18362fa9`) |

**The canceller now has the render before its echo, and removes the
lamp's own voice: 24.7 dB over the whole reply, against 2.8 dB.**

## Held frames

`*-held-frame-timing.json`, every capture row of each bundle.

| | Baseline | Treatment |
|---|---|---|
| Rows; sequences contiguous from 1; audio offsets contiguous | 7,000; yes; yes | 7,001; yes; yes |
| Between one frame's read and the next: least, median, most | 8.0, 10.3, 12.0 ms | 8.1, 10.3, 12.0 ms |
| Processed after the frame's own read: least, median, 95th, most | 0.3, 1.0, 3.3, 6.2 ms | 39.1, 40.5, 43.0, 47.0 ms |
| Reference's count of capture blocks, minus the frame's sequence | 0 in every row | 4 in every row |
| Accepted render not yet given to the canceller, median | 1,126 frames | 1,109 frames |

Each frame carries the time of its own read. The reference recorded with
it is the reference when it was cancelled, four reads later.

## Alignment

`*-reply-starts.json`, `*-timing.txt`.

| | Baseline | Treatment |
|---|---|---|
| At the three reply starts, the first block with the lamp's voice | cancelled two blocks before the canceller had that render | two blocks after |
| The two blocks cancelled just before the render arrived | -21.7 and -11.8 dBFS: the lamp's voice | -53 to -55 dBFS: idle |
| Least-squares filter from render to microphone, peak | 16.8 ms before the cursor | 22.4 ms after it |
| The canceller's own alignment values seen | 0, 72 ms | 0, 8 ms |

## Cancellation

`*-pair.json`. Levels in dBFS.

| | Baseline | Treatment | Replay had said |
|---|---|---|---|
| Lamp sounding, whole reply, before and after | -14.1, -17.0 | -13.6, -38.4 | |
| Removed | 2.8 dB | 24.7 dB | |
| Lamp alone, copy 1: before, after, removed | -15.0, -20.2, 5.2 dB | -14.8, -42.7, 27.9 dB | 23.2 dB |
| Lamp alone, copy 3: before, after, removed | -15.5, -17.1, 1.6 dB | -15.4, -38.0, 22.6 dB | 27.9 dB |
| Lamp alone, blocks scored 0.8 or over | 114 of 430, 193 of 360 | 0 of 430, 10 of 360 | 4, 0 |
| Original request (sunset), lamp silent: level, scored | -45.5, 124 of 188 | -44.9, 133 of 188 | 124 |
| Copies repeat: correlation, gain | 0.9996, 1.000 | 0.9985, 1.000 | |

## The clip over the lamp

Four stretches of the clip's speech, after cancellation.

| | Baseline | Treatment |
|---|---|---|
| Level with the clip | -18.9, -23.2, -17.0, -21.7 | -39.6, -52.1, -37.6, -49.2 |
| Same place, copy 1 | -18.5, -24.5, -19.5, -21.6 | -42.8, -51.8, -40.3, -55.9 |
| Same place, copy 3 | -17.4, -23.2, -16.2, -21.1 | -40.3, -54.9, -35.5, -52.8 |
| Correlation with the clip | 0.05, 0.01, 0.04, 0.09 | 0.01, 0.25, 0.01, 0.18 |
| Same, copy 1 | 0.07, 0.00, 0.03, 0.09 | 0.02, 0.05, 0.01, 0.06 |
| Blocks scored 0.8 or over, with the clip | 136 of 337 | 0 of 332 |
| Same blocks, copy 1 and copy 3 | 101, 153 | 0, 10 |
| Clip alone in the lamp's pause: level, correlation, scored | -43.9, 0.71, 4 of 56 | -43.1, 0.70, 0 of 60 |

As the replay said: the lamp no longer hears itself, and the clip is still
not scored while the lamp sounds. It is 26 to 37 dB under the lamp's voice
before cancellation and 23 to 28 dB come off. Where the lamp is weaker
(second and fourth stretch) the clip shows in the output by its
correlation with the dry clip, 0.25 and 0.18 against 0.05 and 0.06 in the
copy without it; in the fourth it is 4 to 7 dB over the copies without it.
The detector scores no block.

## Limits

- One recording each. Same room and day, a different run: the clip's
  placement, the room and the canceller's start differ between them.
- The clip alone in the lamp's pause is at -43 to -44 dBFS in both runs
  and the detector scores 4 blocks in one and none in the other. That is
  not the hold's doing either way: what that voice scores at this level
  on `mic1` is not settled.
- Nothing here says anything about `mic2`, whose path this change does not
  touch.

`inputs.sha256`: digests of the bundle files read. The scripts that
produced the tables are kept outside the repository; their digests are in
`../scripts.sha256`.
