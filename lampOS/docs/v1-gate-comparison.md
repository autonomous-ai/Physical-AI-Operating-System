# What the V1 gate would have done with these recordings

Scope: a bounded offline comparison for `MAIN-037`. The V1 main gate,
`AdaptiveLiveGate`, exactly as it is at `origin/main`
`d5efe9d7b73cc529b34cd4abe97624682a82ca94`
(`hal/drivers/voice/_internal/live_gate.py`, SHA-256 `e388e4a0…563c861`),
was run unmodified over the recorded bundles of the day. Host only. Nothing
here is in the tree but this note, nothing was tuned, and **it is not a
validated replacement for anything**: every real clip is one where nobody was
meant to interrupt.

## What V1 does

Read from its source, not inferred:

- **Local speech only ducks.** `voice_service.py` answers the gate's
  `barge_in` with `live_playback.duck(True)` and the comment that energy can
  be residual speaker echo and only the provider may cancel a reply. The
  Rust coordinator cancels on its own detector.
- **During playback the microphone has to beat the output.** The threshold is
  the largest of -27 dB, five times the tracked noise, and the output
  envelope of the last 500 ms raised by a learned coupling plus 8 dB. The
  coupling starts at -8 dB and is never allowed above 0 dB.
- **240 ms** above the threshold during playback, 160 ms otherwise.
- **A warm-up over the first three seconds of accumulated playback in a live
  session**, in which a detected onset is counted and refused, and stays
  refused until that burst ends. After those three seconds it plays no part.
- **While playback is a risk the uplink is zeros** unless the gate has
  opened, and the 300 ms before an opening are replayed. How much of an
  onset that keeps is measured [below](#would-it-let-a-person-in-synthetic-only).

## How the recordings were mapped onto it

These are my choices, not V1's signals:

| V1 input | Taken from the Rust bundle as |
|---|---|
| Microphone frames of 64 ms from the hardware-processed device | `pre_aec.pcm16le`, the same device's raw input, in 64 ms frames. Separately `post_aec.pcm16le`, the cancelled signal, which V1 never had |
| `output_level`, the RMS of the last chunk written, times the mixer gain | The RMS of the accepted render over the 64 ms written up to that frame; mixer gain 1.0, right for volume 100 and unknown for the first pilot |
| `playback`, a speaker write in the last 250 ms | Non-silent accepted render in the last 250 ms. The Rust speaker writes zeros continuously, so a write as such means nothing |
| `playback_seconds`, accumulated since the live session began | Accumulated non-silent accepted render since the start of the run |

V1 also ran without a software canceller on a different playback path. The
comparison says what its rule does with these signals, not what V1 did.

## The two trials that interrupted themselves

Volume 100, noise suppression off. "Onset" is the gate's own 240 ms
detection during playback.

| Microphone given to the gate | Warm-up | Trial 1 | Trial 2 |
|---|---|---|---|
| Raw, as V1 had it | 3 s, as written | Prompt opens the gate. One onset in reply 1, refused | Prompt opens the gate. Four onsets, one in each reply, all refused |
| Raw | off, for analysis only | **The lamp's own voice is taken for local speech** 0.42 s into reply 1 | **And in every reply**: 0.47 s, 0.40 s, 0.41 s and 0.46 s in |
| Cancelled | 3 s | Prompt opens the gate. No onset in any reply | The same |
| Cancelled | off | No onset in any reply | No onset in any reply |

In the four windows where the Rust detector started a turn, the cancelled
signal was above the gate's threshold in 0, 2, 0 and 0 of 5 to 7 frames, and
four in a row are needed.

Replies the device did not interrupt: on the first pilot and the coordinated
run the gate found nothing either way. On the two NS-off trials at volume
100 it found one onset in each reply on the raw input, refused by the
warm-up and taken for local speech without it, and nothing on the cancelled
signal.

## What that does and does not show

- **On these clips V1's protection is the warm-up and the duck, not a gate
  that tells echo from speech.** All four false admissions fall inside the
  first three seconds of accumulated playback. Without the warm-up the rule
  takes the lamp's own voice for a person in every reply at volume 100,
  because the voice arrives 4 dB to 13 dB above the output it is compared
  with and the rule allows at most 8 dB. V1 would then have ducked, not
  cancelled.
- **The warm-up refuses every interruption during the first three seconds of
  accumulated session playback, and only then.** In these two trials every
  reply happened to end or be cancelled inside that window, so on these
  clips it refuses everything. A short reply later in a session is not
  refused by it. That is the difference you asked me to keep visible.
- **On the cancelled signal the same rule refuses all four without any
  warm-up.** It is an energy rule, and what the canceller leaves is mostly
  quiet even when the level-blind detector calls it speech.

## Would it let a person in? Synthetic only

No recording has a person speaking over the lamp. As a control I added the
prompt's own cancelled recording, 2.4 s at -23.3 dB, into the cancelled
signal during a reply. **This is synthetic and optimistic**: it puts the
talker in after the canceller, untouched, where a real canceller would
attenuate some of it. Four placements, two per trial.

| Talker level | Warm-up off: gate opens | Warm-up 3 s |
|---|---|---|
| As recorded, -23 dB | Never during playback, in 4 of 4 | Never during playback |
| +6 dB, -17 dB | In 1 of 4, after 0.41 s | Never during playback |
| +12 dB, -11 dB | In 3 of 4, after 0.35 s to 0.41 s | Never during playback |
| +18 dB, -5 dB | In 4 of 4, after 0.34 s to 0.38 s | Once, in the one placement that outlasts the warm-up, 1.86 s after the talker began |

**What reached the uplink, measured.** The gate opened during playback in 9
of the 32 runs.

- In the eight without the warm-up it opened 336 ms to 408 ms after the
  clip's first sample and replayed exactly 300 ms each time. The uplink
  therefore began between 28 ms before and 44 ms after the clip's first
  sample. In the four runs where it began after, what was dropped was 20 ms
  to 44 ms of the clip's lead-in at -56 dB to -69 dB, under a thousandth of a
  percent of its energy.
- **That says nothing about a quiet onset.** The clip is the prompt, which
  rises from -75 dB to about -18 dB within 130 ms. By the rule's own
  arithmetic at 64 ms frames, it needs four frames over the threshold and
  replays 300 ms, so it reaches 108 ms before the first frame over the
  threshold and no further. A talker who stays under the threshold for
  longer than that before crossing it loses the earlier part. No such talker
  was tested, so how much of an ordinary onset that is, I have not measured.
- In the one opening under the warm-up, the first burst was refused and the
  gate opened 1,856 ms after the talker's first sample. The first 1,492 ms
  of the talker, 93 % of the clip's energy, never reached the uplink.

I first wrote here that the onset is not lost. That held for a fast-rising
talker with no warm-up and failed under the warm-up.

At this volume the rule's threshold during playback sits at -6 dB
to -16 dB, and a talker has to be 12 dB to 18 dB louder than the recorded
prompt to cross it. So with these levels the rule on the cancelled signal
refuses the echo and refuses a person at the prompt's level alike. Whether
that changes at a lower playback level is exactly what cannot be read from
these clips.

## Limits

- Every real clip is negative. The positive control is synthetic, and
  optimistic in the direction that flatters the gate.
- One lamp, one room, volume 100 for all but one bundle.
- The mapping in the table above. V1's chunk size, mixer reading and session
  boundaries are not reproduced.
- The gate's adaptive coupling carries state from reply to reply. A
  different history gives a different threshold.
- I ran the exact Python source over NumPy arrays outside the repository.
  The scripts are not in Git; lampOS is Rust, and this is an analysis.

## Reproducing it

The scripts, their outputs and a manifest are in one local directory outside
the repository. Its path is in
[Agent 1's status](../coordination/agent-1.md), because it is a session
directory and may move. The manifest identifies the run wherever it is
copied:

| File | SHA-256 |
|---|---|
| `manifest.json` | `aa5ff6a852af08f512552f17c83b264c4da86775eeab06b0cfd74bd9698654b7` |
| `live_gate_v1.py`, from `git show d5efe9d7b:hal/drivers/voice/_internal/live_gate.py` | `e388e4a0e0a4a27c87d0f5ce5edc4c7e54cdde2ac61ecf1cc2536e2be563c861` |

From that directory, with Python 3.14.2 and NumPy 1.26.4:

```sh
python3 run.py live_gate_v1.py      # the two interrupted trials      -> run.out.txt
python3 more.py live_gate_v1.py     # other bundles, synthetic overlap -> more.out.txt
python3 onset.py live_gate_v1.py    # where the uplink starts          -> onset.out.json
```

`manifest.json` holds the SHA-256 of each script and each output, the
mapping, and the SHA-256 of `pre_aec`, `post_aec` and `render_accepted` for
the six bundles read, with their paths. The scripts hard-code those paths.
The recordings stay where they are.

**Checked under a second interpreter.** Python 3.14 with NumPy 1.26.4 is
not a combination NumPy supports, and the integrator found it giving wrong
figures in a script of its own. The three scripts above were rerun unchanged
under Python 3.12.10 with NumPy 2.2.3: all three outputs are byte for byte
the same. `verification-main-041.json` in the same directory (SHA-256
`6d3548d24b6956edd38ed204555a644d4381af8ddc70ac0f062e129b51792e82`) lists
both sets.

The fault is real and worth knowing. Under 3.14 with 1.26.4, arithmetic on
an array that is local to a function, owns its data and is 256 KiB or more
overwrites that array in place: after `np.mean(x * x)`, `x` holds its own
squares. A module-level variable, a view and a smaller array are not
touched, which is why small controls pass. These scripts escape it because
the gate works on 1,024-sample frames and the large arrays are only sliced,
converted or updated on purpose. Use 3.12 for anything new.
