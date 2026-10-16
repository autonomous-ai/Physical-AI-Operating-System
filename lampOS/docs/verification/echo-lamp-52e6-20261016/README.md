# Echo on lamp-52e6, 2026-10-16 (#705)

How much of the lamp's own voice the echo canceller removes while it
speaks, and whether a person speaking over it survives. Measured from a Mac
in earshot (`say -v Samantha`), numbers only: no audio left the lamp, no
binary or drop-in was left on it.

Both read the voice's `duplex_hearing` trails
(`/var/lib/lampOS/voice/sessions/<InvocationID>/metadata/events-*.jsonl`):
one letter per 10 ms capture block while a reply could be yielded,
`capture_level` before cancellation and `residual_level` after, 3 dB a
letter down from full scale. `z` is -75 dBFS, the lowest the trail says,
so a residual there is "at the floor" and a figure taken against it is a
lower bound.

## Echo removed while the lamp tells a story (`aec-mics.sh`)

One story turn per microphone (voice e4f1d32b, first builder's run). Per
block while the lamp spoke, capture minus residual, counting only blocks
within 20 dB of the trail's loudest:

| Microphone | Blocks | Removed, median (p25–p75) | Capture | Residual |
|---|---|---|---|---|
| mic2 (as run) | 749 | **54 dB** (42–60) | about -15 dBFS | about -75 dBFS; 443 of 749 at the floor, so 54 is a lower bound |
| mic1 | 762 | 27 dB (6–39) | about -6 dBFS, near clipping | about -36 dBFS |

`aec-mics.sh` restarts lamp-voice, and for mic1 stops lamp-ears and adds a
drop-in for one turn; its exit trap removes the drop-in and restores both.

The self-test's 600 Hz tone (#895) measured 3 to 19 dB on the same lamp.
That was the probe: AEC3 does not adapt on a narrowband reference, and a
sound after 45 s of quiet resets its filters. The probes tried after it
are below; none worked, so `selftest/speaker` now plays nothing and reads
the lamp's own last speaking turn ([speaker-check.md](../../speaker-check.md)).

`selftest/speaker` on lamp-52e6 (`lamp-selftest --occasion on-demand`,
the voice with `LAMP_SELFTEST_SPEAKER=on` in a drop-in, put back after):

| Sound | Runs played | Result |
|---|---|---|
| pink noise, -30 dBFS (10:11–10:20, voice `0722e745`) | 2 of 11 (9 `not_checked`: something heard within 45 s) | both fail, not heard: capture before the canceller at -75 dBFS through the 3 s |
| synthetic voice, -30 dBFS (10:28–10:33, voice `f18cea13`) | none | the voice exited 8 times ("authority snapshot has expired"): the sound was built on the first request, in the loop. Now built when the check is switched on |
| synthetic voice, -30 dBFS, 3 s quiet (10:38–10:39, voice `b903163a`) | 3 of 3 | fail, fail, fail: not heard (-36 and -51 dB at most), and 2.7 dB cancelled (rise 33.7 dB at 792 ms); the capture stayed at the floor but for room sound |

So mic2's chip gates steady noise and a quiet synthetic voice alike.

### From the lamp's own speech (the check as merged)

The voice with `LAMP_SELFTEST_SPEAKER=on`, one "Lamp! tell me a short
story" from the Mac each (11:55 and 11:57), then `lamp-selftest
--occasion on-demand`:

| Voice | Before any turn | After the story | `aec-mics.sh` on the same turn |
|---|---|---|---|
| `49e21b21` (blocks within 20 dB of the turn's loudest) | `not_checked`: "the lamp has not spoken for 4 s at a stretch" | pass, rise 72.8 dB, cancelled 30.7 dB | 39 dB (666 blocks) |
| `70b4341c` (within 20 dB of the loudest of each second, as the trails) | — | **pass, rise 67.6 dB, cancelled 42.8 dB**, no residual at the floor | 39 dB (479 blocks, 260 at the trail's -75 floor) |
| `34299662` (the same rule, rebased onto lamp-voice; one story at 14:27, the answer about 9.5 s) | — | **pass, rise 70.7 dB, cancelled 37.4 dB**, twice (14:28:18 and 14:28:27, the same turn), no residual at the floor | — |
| `dfe6f54d` (the build of `24fe57cb9`, `cancelled_db` at the loudest block; run at 16:15) | — | **pass, rise 68.2 dB, `cancelled_db` 44.6 dB, `cancelled_median_db` 43.4 dB**, `turn_age_s` 63 | — |

The second rule is the script's; its figure reads lower only because the
trail's letters stop at -75 dBFS where the voice's evidence goes to -96.
The voice did not restart in any run. The first two builds were made
before the branch was rebased onto lamp-voice's head cue (#711); the
rebased build `34299662` was run at 14:28. In those three runs
`cancelled_db` was the median; it is now `cancelled_median_db`, and
`cancelled_db` is the figure at the turn's loudest block, as the tone's
was at its peak. The 16:15 run of `dfe6f54d` is the one with both figures.

## A sentence spoken over the story

The Mac asks for a long story, then about 9 s into it says "Stop! Lamp,
stop." The sentence is found by the voice's own speech detector (the first
run of 10 or more blocks at 0.5 or over), not by the Mac's clock. The Mac's
time is bracketed by `CLOCK_MONOTONIC` read over ssh before and after
`say` (the trails' `at_us` is that clock); a trail is written as its last
block is read, which matches `duplex_yield`'s `last_block_host_read_us`
within 4 ms. Voice session 0760cef3. The trails were read by a script
that is not kept here (lampOS commits no Python); the numbers are.

| Trial | Residual in the second before | Residual over the sentence | Rise | Gated blocks | Lamp |
|---|---|---|---|---|---|
| 09:31 | median -70.5 dBFS, 48 of 100 at the floor | median -18 dBFS (max -15), 21 blocks | **+52 dB** | 0 of 21 | yielded 60 ms after the onset, committed 0.57 s later; `turn_finished user_interrupted`, `barrier Settled: superseded=1 successor=2 interrupted=1` |
| 09:46 | median -75 dBFS, 65 of 100 at the floor | median -18 dBFS (max -15), 24 blocks | **+57 dB** (at least) | 0 of 24 | yielded 150 ms after the onset, committed 0.62 s later; `turn_finished user_interrupted`, `barrier Settled: superseded=66 successor=67 interrupted=1` |
| 09:49 | — | — | — | — | not a trial: a person was at the lamp for another run, and the story was cut before the sentence |
| 10:05 | — | — | — | — | not a trial: the story was held back ("a busy room ... nobody is turned to the lamp"), so nothing was spoken over |

So the residual that sits at the floor while the lamp speaks is the
canceller's suppressor (or mic2's chip) on the lamp's own voice, not a
gate on a person: a sentence over the story comes through at about -18
dBFS, 50 dB or more over the residual before it, in every block, and the
lamp stopped both times.
