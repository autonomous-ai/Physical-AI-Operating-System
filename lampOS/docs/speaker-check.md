# The speaker check

Part of the self-test ([#812](https://github.com/autonomous-ai/lampOS/issues/812),
[selftest.md](selftest.md)). `lamp-voice` says what its microphone made of
the lamp's own last speaking turn: whether it heard the lamp speak, and how
much of that the echo canceller removed. **Nothing is played for it**: a
person near the lamp hears nothing, sees nothing, and no turn is opened.
Off unless `LAMP_SELFTEST_SPEAKER=on` is in the voice's unit (with the bus,
`LAMP_BUS_ROOT`).

## Asking

`lamp-selftest` publishes `selftest/speaker/request`
(`lamp_msgs/SpeakerCheckRequest`: `id`, `occasion`). The voice looks once a
loop and answers at once on `selftest/speaker` (`lamp_msgs/SpeakerCheck`:
the `id`, `verdict` `pass`, `fail` or `not_checked`, `reason`, and from a
measured turn `rise_db`, `cancelled_db`, `cancelled_median_db`,
`cancelled_lower_bound` and `turn_age_s`; `delay_ms` is no longer given). A request not taken within 10 s is stale.
The self-test waits 2 s for the answer.

A request whose occasion is not `factory` or `on_demand` is answered
`not_checked`. A request that cannot be read, or an answer that cannot be
written, is said once in the voice's log and switches the check off until
the voice restarts; the conversation goes on.

## What it measures

The voice keeps each capture block's `EchoEvidence`
(`coordinator/speaker_tone.rs`, `speaker_check::Turns`). A block whose
reference was not silent (`render_quiet_blocks` 0) is the lamp speaking; a
turn ends once the speaker has been given nothing for 64 blocks (0.64 s).
A turn of 4 s of speaking blocks or more is measured; the last one
measured is kept, and a request is answered from it:

- **heard:** the loudest level before the canceller, over the mean of the
  half second before the turn (`rise_db`), is 10 dB or more. Less is
  `fail` (speaker or voice microphone);
- **cancelled:** over the blocks within 20 dB of the loudest of their
  second (so quieter passages count as much as loud ones), the
  median of the level before the canceller minus the level after
  (`cancelled_median_db`). Under 20 dB is `fail`. `cancelled_db` is the
  same difference at the turn's loudest block, as the tone's was at its
  peak until #968, so earlier reports stay comparable; it does not decide.
  When any of those blocks has the residual at the evidence's floor
  (-96 dBFS), the true figures are at least these
  (`cancelled_lower_bound`, a value of its own in the self-test's report);
- **age:** `turn_age_s` is how long before the answer that turn ended. A
  turn from hours ago is answered the same as one from a minute ago, and a
  person or a TV talking over the lamp during it lowers the median toward
  a false `fail`: read the age with the verdict;
- no turn of 4 s yet since the voice started is `not_checked`, and says so.

Levels are over 120 ms windows, one per 10 ms block. This is the rule
`aec-mics.sh` reads from the voice's `duplex_hearing` trails
([verification/echo-lamp-52e6-20261016](verification/echo-lamp-52e6-20261016/README.md));
the trails' letters stop at -75 dBFS, so the script's figure from the same
turn can read lower.

## Why not a sound

Until #968 the check played its own sound and measured that. None of the
sounds gave a figure for the canceller on lamp-52e6 (#705):

- a 600 Hz tone, 0.3 s (#895): 3 to 19 dB, falling within a run. AEC3 does
  not adapt its filters on a narrowband reference, and a sound after a long
  quiet triggers a fresh delay estimate that resets them;
- 3 s of pink noise at -30 dBFS: never reached the canceller. mic2's chip
  suppresses steady noise; the level before cancellation stayed at -75 dBFS;
- 3 s of a synthetic voice at -30 dBFS: gated by the chip too (not heard
  twice, 2.7 dB once).

A sound at the lamp's speaking level would pass the chip, but the lamp is
not to make one by itself near people. The lamp's own speech already
reaches the canceller (54 dB median on mic2, 27 dB on mic1, with the same
rule), so the check reads it.

## Not shown yet

The thresholds (10 dB rise, 20 dB median, 4 s turn) are from that
measurement, not from many lamps.
