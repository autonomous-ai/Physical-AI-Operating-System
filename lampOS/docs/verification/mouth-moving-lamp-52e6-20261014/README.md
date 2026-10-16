# Whether a mouth moves, lamp-52e6, 2026-10-14 (task 39, #720)

## For the person at the lamp: about ten minutes

Sit about a metre in front of lamp-52e6, facing it, and read the big words
the Mac shows. Only numbers are kept (how much each face's mouth moves, with
the time); no picture, no sound.

1. **Talking** (30 s): speak, anything, on SPEAK; be still on QUIET.
2. **Over the lamp** (30 s): the Mac asks the lamp for a long story; speak
   over it on SPEAK.
3. **Silent** (15 s): look at the lamp and say nothing.
4. **Chewing and smiling** (20 s): chew on CHEW (gum or nothing), smile and
   laugh silently on SMILE.
5. **Two people** (25 s, only if a second person is there, `TWO=1`): A
   speaks on A, B on B.

The builder starts it from the Mac (it waits until the camera looks, which
on lamp-52e6 can take minutes after the eyes restart):

```sh
sh mouths.sh orangepi@lamp-52e6.local 9bac623906beeb40c78e3b019226a469265398491c0aa130d79c54e0bd77f8e7 ~/mouths-run
```

The build is `fifth/mouth-moving` at `5f0cb6056` (rebased on lamp-voice
`96974cd0b`, with #766's still-face filter: the mouths are read for the faces
it keeps), built on the Mac with zigbuild, installed (not run) on lamp-52e6
under `/opt/lampOS/eyes/9bac6239…/`. The lamp's own eyes unit is restored on
exit. By hand: `~/mouth-mode.sh on|off` on the lamp.


The build: `lamp-eyes` from `fifth/mouth-moving` (sha256
`5ab783951d28…` for the trial below), run with `LAMP_BUS=on
LAMP_EYES_MOUTH=on` in a drop-in for the trial only. Nobody was by the lamp
(evening); the lamp's own unit was restored after, and its file compared
equal to the backup.

## Cost (journal of `lamp-eyes`, ten-second lines)

| | Value |
|---|---|
| Reading mouths, a frame | 0.6 to 1.1 ms (grey JPEG decode and 384 points) |
| The detector, a look (unchanged) | 61 to 89 ms |
| Frames read for mouths | All the camera gives while a face is on the bus: about 7 a second; none with nobody in view |
| From a frame's taking to its mouth read | 116 to 135 ms (mostly the camera's queue) |
| The process | 22 MB resident, 15% of one core |
| The board | 51 to 52 °C |

## Smoke check with a face on the Mac's display: not shown

A public-domain video of a person speaking (a US government weekly address,
Wikimedia Commons) was played full screen on the Mac the lamp faces, muted:
15 s frozen, then 45 s of speech. Played as it is, the face was 6% of the
lamp's picture and was found in 44 of 126 looks; cropped to the face, 16 to
18%, and found in 20 of 178 looks, with confidence 0.63 to 0.79 against the
0.6 floor and a new id at most looks. A mouth needed three frames under one
face (the first build, 0.5 s): there were 10 scores in the first run (mean 0.31, all while it spoke)
and 1 in the second. That says the path works end to end on the lamp, not
whether the score parts speech from stillness: a face on a screen at this
distance is found too seldom. It is left for a person.

## What is left for a person (about ten minutes)

`mouths.sh` runs five scenes and writes, for each, the score under each
face while each prompt was shown, as numbers only:

1. **talking**: facing the lamp, speaking on SPEAK and still on QUIET.
2. **over**: the Mac asks the lamp for a long story (one real turn); the
   person speaks over it on SPEAK.
3. **silent**: facing the lamp, still, 15 s.
4. **chewing**: chewing, then smiling and laughing silently.
5. **two**: two people, one speaking at a time (only with `TWO=1`).

```sh
sh mouths.sh orangepi@lamp-52e6.local 316939531bb5c0aed303a55cc28d354710e1d9ca3106fc2e1843a651b4d4f858 ~/mouths-run
```

That build is installed on lamp-52e6 under `/opt/lampOS/eyes/316939531bb5c0aed303a55cc28d354710e1d9ca3106fc2e1843a651b4d4f858/`
(not run), and the bus program the script uses at `~/fifth-lamp-bus`.
The person sits about a metre in front of the lamp and reads the prompts on
the Mac. The eyes run with the mouths only for the run; the unit is restored
on exit. The marks are the Mac's clock put on the lamp's with one offset
taken over ssh, good to about a tenth of a second; a score counts for a
prompt from 1.2 s after it was shown (the window; half a second in the
first run). What to read: SPEAK well
above QUIET in 1 and 2, and silent, CHEW and SMILE near QUIET. The
score's numbers and the window are set from it.

## First run with a person, 2026-10-15: the window held too little

The talking, over, silent and chewing scenes were run (`~/mouths-run` on
the Mac; 400 messages, one each 250 ms). The face was found in about 39 of
40 looks, yet:

| Scene | Messages with a mouth | Mean score, by prompt (scores) |
|---|---|---|
| talking | 32 of 122 | SPEAK 0.080 (14), QUIET 0.046 (17) |
| over | 21 of 111 | SPEAK 0.000 (7), QUIET 0.024 (13) |
| silent | 23 of 72 | QUIET 0.006 (19) |
| chewing | 23 of 95 | CHEW 0.043 (4), SMILE 0.044 (4), QUIET 0.200 (10) |

Every mouth said was `"frames":3, "window_ms":500`, with `age_ms` 167 to
215 (median 193). The window was counted back from when the score was
said, and the newest frame was already about 190 ms old then, so only
about 300 ms of frames (two or three changes at 7 a second) were in it;
three is the least that says anything, and three changes mostly gave 0.
Speech did not stand out from stillness.

The fix (`mouth.rs`): the window counts back from the newest frame read,
and is 1.2 s (about eight changes at 7 frames a second), needing five.
A test feeds the measured rates (a frame each 143 ms, a look each 250 ms,
said 190 ms after the newest frame) and checks every look after the first
second has a score of 7 to 9 frames. The recordings are to be taken again
with it; `FULL` is not changed until then.

## Second run, 2026-10-15: the score itself did not see speech

With the 1.2 s window (`~/mouths-run2` on the Mac) every look had a
score, yet SPEAK came out 0.038 against QUIET 0.082, CHEW 0, SMILE 0.176,
silent 0. The score was the lower face's mean change less the eyes', over
6 grey levels. The eyes change more than the lower face at rest, so it
was at or under 0 almost always; lips opening by 5 to 14 pixels are about
5% of the lower face, a mean change of about 2 grey levels, inside the
noise; and a chewing jaw fell below the region (1.5 times the
nose-to-lips drop below the lips).

The next build (`mouth.rs`) changes the score and says what it is made of:

- the eyes are a gate, not subtracted: a frame whose eyes changed by more
  than 10 grey levels on average is left out (`HEAD_MOVED`);
- a frame's movement is the share of lower-face points that changed by at
  least 12 grey levels (`CHANGED`), 0.15 of them reading 1 (`FULL_SHARE`);
- the region reaches 2.5 times the drop below the lips.

All three numbers are first guesses. `mouths.sh` now runs the eyes with
`LAMP_EYES_MOUTH_RAW=on`, so each message carries, for every frame in its
window, `lower_mean`, `lower_share8`, `lower_share12`, `lower_share16` and
`eyes_mean` (numbers only, with the frame's `taken_us`; count each frame
once). One more ten-minute run answers both whether the new score parts
SPEAK from QUIET and, if not, which step and gate would.

## Third run, 2026-10-16: the whole face moved between frames

With the raw changes said (build `d214fe838`, `~/mouths-run3` on the Mac;
the person followed every prompt), the medians of each frame's raw values,
each frame once (by `taken_us`), from 0.5 s into each prompt:

| Scene | Prompt | `lower_mean` | `lower_share12` | `eyes_mean` |
|---|---|---|---|---|
| talking | QUIET | 10.6 | 0.286 | 20.9 |
| talking | SPEAK | 11.8 | 0.297 | 20.3 |
| over | QUIET | 9.2 | 0.229 | 18.9 |
| over | SPEAK | 10.8 | 0.255 | 24.1 |
| silent | QUIET | 10.9 | 0.260 | 21.7 |
| chewing | QUIET | 6.6 | 0.156 | 13.2 |
| chewing | CHEW | 4.3 | 0.068 | 9.5 |
| chewing | SMILE | 6.4 | 0.161 | 14.3 |

Every frame changed by 10 to 20 grey levels, silent or not: the whole face
shifted between frames. The lamp's head follows the face (the body's gaze
ran in the second and third runs; in the first, with the body off, the raw
changes were small), while the regions stayed where the face was found,
read at the nearest pixels, so the face slid under them.

The fix (`mouth.rs`): each frame is lined up with the one before by the
band round the eyes (the integer shift within 6 pixels each way that makes
it change least; the gaze moves the face 2.5 to 3.4 pixels a frame at
most), the lower face is read at that shift, the shifts add up so the
regions follow the face, and the head gate reads the eyes' change after
the line-up. `raw` now also says `shift_x`, `shift_y` and `eyes_after`. A
test slides a textured picture 2 pixels left and 1 up a frame (16 and 8 in
all): it reads still, with every shift found; the same slide with the lips
moving reads as a mouth. `mouths.sh` now ends by printing these medians
for every raw field (`raw-medians.txt`), so the next run reports itself.

## Fourth run, 2026-10-16: lined up, yet not a still face

Build `fd51867b2` (`~/mouths-run4` on the Mac; the person followed every
prompt). The medians, as `raw-medians.txt` printed them:

| Scene | Prompt | `lower_mean` | `lower_share12` | `eyes_mean` | `eyes_after` |
|---|---|---|---|---|---|
| talking | QUIET | 12.8 | 0.310 | 27.6 | 15.1 |
| talking | SPEAK | 16.6 | 0.427 | 31.5 | 17.1 |
| over | QUIET | 11.5 | 0.315 | 14.1 | 11.0 |
| over | SPEAK | 11.5 | 0.302 | 21.5 | 13.3 |
| silent | QUIET | 8.3 | 0.221 | 14.7 | 12.8 |
| chewing | QUIET | 5.3 | 0.120 | 9.0 | 9.0 |
| chewing | CHEW | 6.6 | 0.146 | 10.3 | 10.3 |
| chewing | SMILE | 7.7 | 0.193 | 16.5 | 11.6 |

The median shift was 0 everywhere. Frame by frame (each `taken_us` once)
the medians hid two kinds of frame:

- **Still.** Before the first prompt and in the silent scene's still
  stretches (14 frames and more in a row), both regions changed by 4 to 7
  grey a frame, `lower_share12` 0.02 to 0.10, with no shift: pixel noise,
  one pixel at a time. No flicker showed: a light flickering against the
  shutter would have lifted those stretches too.
- **Moving.** Bursts with a shift found and `eyes_mean` 20 to 40, in which
  even the lined-up eyes changed by 9 to 30 (`eyes_after`): a head turning
  or nodding, which a whole-pixel shift cannot undo. Lower and eyes went
  together in them (correlation of `eyes_after` with `lower_mean` 0.37 to
  0.85 by prompt). In talking only 13 to 16% of frames had `eyes_after`
  under 10; in silent 39%, with `lower_share12` 0.08 in them; talking
  SPEAK's still frames 0.19 to 0.25.

So the "still face changing by 8 to 15" was mostly a face in motion, and
the rest pixel noise of about 5 grey. The fix (`mouth.rs`): each point is
the mean of the 3 by 3 pixels round it (noise and part-pixel slides cut to
about a third), and each region's mean signed change is taken out before
its points are compared (a light or exposure step, not ruled out by run 4
since it kept no signed change). `raw` now also says `lower_offset`,
`eyes_offset` and `lower_share12_corr` (the share the score is made of);
`eyes_mean` and `eyes_after` have the offset taken out, `lower_mean` and
`lower_share*` keep it. A row-wise flicker band is not taken out: none
showed. Tests: a step of 30 grey every other frame reads still with no
frame left out, and with the lips moving reads as a mouth; noise of up to
12 grey a pixel reads still (one pixel at a time it read 1.0, `lower_mean`
9.2, as silent did on the lamp) and a moving mouth under it moves.
`mouths.sh` now also prints, for each prompt, the frames the head gate
keeps, the median `lower_share12_corr` in them, and the correlation of the
eyes' change with the lower face's. `HEAD_MOVED`, `CHANGED` and
`FULL_SHARE` stay until the next run, whose numbers the 3 by 3 means change.
