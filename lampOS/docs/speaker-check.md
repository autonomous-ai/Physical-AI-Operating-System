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

A request that cannot be read, or an answer that cannot be written, is
said once in the voice's log and switches the check off until the voice
restarts; the conversation goes on. Failures of the turn owner or the
speaker are not the check's and still end the service as before.

## How it is played

As the lamp's own turn with no provider request (`coordinator/speaker_tone.rs`):
admitted and planned like any speech, ended at once, its cloud request
complete from the start, its PCM the tone, dispatched in 10 ms chunks
through `dispatch_speaker_chunk` as the failure notice is
(`SpeechOrigin::SelfTest`). So every chunk reaches `SpeakerChunk` under an
output permit and becomes the echo canceller's reference, and the turn
settles once its last chunk retires. The ring is told to show nothing before the
turn is admitted, so not even the pass that admits it prepares a frame. The
head tap's "hm?" (#711) is played the same way, through the same
`commit_sound_turn`, as `SpeechOrigin::HeadCue`
([attention.md](attention.md#a-tap-on-its-head-is-a-call-711)). The row for
the first chunk reserved is named for what plays: `selftest_tone_attempted`
for the tone (it was `failure_notice_attempted` before 2026-10-16), as its
`_first_write` and `_final_sample_retired` rows always were.

## What it measures

Only a tone played to its end (its final sample retired by the speaker) is
judged. One whose turn was cancelled first (a person speaking, a hand, the
microphone closing) is `not_checked` and says so.

The cleaned microphone sound cannot carry the tone: the canceller takes out
what the lamp itself plays. So each capture block's `EchoEvidence` is kept
(`speaker_check::heard`):

- **heard:** the level before the canceller rises 10 dB or more over the
  mean of the half second before the speaker first accepted frames of it,
  within 0.8 s. Not heard is `fail` (speaker or voice microphone);
- **delay:** from that first acceptance to the first block over the rise: the
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
