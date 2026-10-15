# The speaker check

Part of the self-test ([#812](https://github.com/autonomous-ai/lampOS/issues/812),
[selftest.md](selftest.md)). `lamp-voice` plays one short tone through its
own speaker path and says what its microphone made of it. Off unless
`LAMP_SELFTEST_SPEAKER=on` is in the voice's unit (with the bus,
`LAMP_BUS_ROOT`).

## What a person sees and hears

One tone: 600 Hz, 0.3 s, -30 dBFS, faded in and out over 10 ms. Nothing
else: the ring shows no face for it, no Expression or body cue is sent, the
head does not move, and `voice/state` says nothing of it.

## Asking

`lamp-selftest` publishes `selftest/speaker/request`
(`lamp_msgs/SpeakerCheckRequest`: `id`, `occasion`). The voice looks once a
loop and answers on `selftest/speaker` (`lamp_msgs/SpeakerCheck`: the `id`,
`verdict` `pass`, `fail` or `not_checked`, `reason`, and when heard
`rise_db`, `delay_ms`, `cancelled_db`). A request not taken within 10 s is
stale.

It plays only when all of these hold, and otherwise answers `not_checked`
with which one did not:

- the occasion is `factory` or `on_demand`: never at first boot or on a
  schedule, so a lamp in a home never beeps by itself;
- the microphone is allowed by the privacy switch;
- the lamp is at rest: no turn, nothing queued for the provider or the
  speaker, nothing being heard, not paused by the keeper;
- nothing was heard (voice activity) or said for 45 s
  (`speaker_check::QUIET_US`);
- it is not between 12:00 and 14:00 local time, and the local time is known.

## How it is played

As the lamp's own turn with no provider request (`coordinator/speaker_tone.rs`):
admitted and planned like any speech, ended at once, its cloud request
complete from the start, its PCM the tone, dispatched in 10 ms chunks
through `dispatch_speaker_chunk` as the failure notice is
(`SpeechOrigin::SelfTest`). So every chunk reaches `SpeakerChunk` under an
output permit and becomes the echo canceller's reference, and the turn
settles once its last chunk retires.

## What it measures

The cleaned microphone sound cannot carry the tone: the canceller takes out
what the lamp itself plays. So each capture block's `EchoEvidence` is kept
(`speaker_check::heard`):

- **heard:** the level before the canceller rises 10 dB or more over the
  mean of the half second before the first chunk was handed to the speaker,
  within 0.8 s. Not heard is `fail` (speaker or voice microphone);
- **delay:** from that first chunk to the first block over the rise: the
  echo path the canceller relies on. The level is over 120 ms windows, so
  the onset is to a few tens of milliseconds;
- **cancelled:** at the tone's loudest block, the level after the
  canceller is at least 6 dB under the level before it. Less is `fail`;
- too little evidence before the tone (the reference not trusted, say) is
  `not_checked`.

The room microphone (lamp-ears) gives only 100 ms levels on `ears/level`;
the self-test reads its rise itself.

## Not shown yet

Not run on a lamp. The thresholds (10 dB rise, 6 dB cancelled, 0.8 s) are
from the canceller's design, not measured; the first run on lamp-4ace sets
them.
