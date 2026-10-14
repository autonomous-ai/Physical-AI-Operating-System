# Whether a mouth moves, lamp-52e6, 2026-10-14 (task 39, #720)

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
0.6 floor and a new id at most looks. A mouth needs three frames under one
face: there were 10 scores in the first run (mean 0.31, all while it spoke)
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
prompt from half a second after it was shown. What to read: SPEAK well
above QUIET in 1 and 2, and silent, CHEW and SMILE near QUIET. `FULL` (six
grey levels) and the window are set from it.
