# Pilot readiness: fresh failures, 12 October 2026

**Not an acceptance pass.** Main ran two further physical trials on Lamp-4ace
using runtime `b3186e7e…73f4` and existing cached speech. All attempted stimuli,
room audio, raw diagnostic audio and original failures are retained locally.
Camera and motor output stayed off. Nothing here is a direct-human result.

## Ordinary conversation

The eight-prompt plan delivered two prompts and completed two replies. Before
prompt three was granted, another input was admitted. The controller's current
readiness check therefore refused the next prompt and stopped the test. The
third player delivered zero frames; five later steps were withheld. This was an
orderly helper-triggered stop, not a spontaneous runtime crash. The source of
the extra input remains unresolved. A weaker sound in the room recording
coincides with its accumulation window; neither echo-only nor another speaker
is established. No target raw microphone audio exists for this ordinary run.

Endpoint-to-first-ALSA-write was 1.246 and 1.305 seconds, median 1.275 seconds.
Adding the configured 1.2-second endpoint wait estimates 2.475 seconds. This is
not an acoustic latency measurement and the failed two-reply cohort does not
establish a speedup. Both completed replies report zero playback gaps.

The archive and all 3,326 metadata events verify. The separate final logging
acknowledgment confirms all events; the rolling health snapshot itself predates
that acknowledgment. The 180-second room recording has 8,640,000 contiguous
frames. Stop-to-inactive took 276.371 ms; original writable mixers and stationary
muted legacy were restored. The earlier startup-refusal attempt remains a
separate failed run; its narrow controller correction was replay-tested before
this attempt and does not excuse the later extra input.

## Grouped microphone comparison

The comparison temporarily changed mic2 AGC from on to off and capture gain from
raw 147 to 91. The runtime, source audio and speaker settings stayed the same.
This changes two controls together; it cannot attribute effects to either one.
Both the sunset prompt and the overlapping coffee prompt were played once.

Lamp yielded and cancelled the sunset answer, but it retained only the tail of
the coffee request. The source alignment proposal places the retained prefix
about 3.1 seconds into the 4.56-second prompt; its uncertainty and matching limits
are in the private analysis. The session-scoped input transcript contains only
"Why does it smell so good?" The next answer discusses cooling air and plants,
not coffee. Completing that answer is not successful topic-change handling.
A later unplanned input stopped the test. The first reply recorded a 100 ms
supply gap and 520 ms of zeros during the yield; the second reports zero gaps.

Both raw-audio completion certificates and all 22 archive members verify. The
independent 100-second room recording has 4,800,000 contiguous frames. These
integrity checks do not establish acoustic quality or a successful interruption.

## Shutdown and recovery

The saved kernel journal records a Jieli USB microphone disconnect near the
end of the trial. Its journal receipt timestamp is 29 ms after capture reported
stopped, but its separate source timestamp precedes the extra input. Both fields
are retained; the journal receipt alone cannot establish when the device
disconnected. Neither establishes the cause of the extra input or runtime stop.
The coffee answer completed about 24 seconds before either timestamp, so this
later USB event does not explain the earlier lost opening. The normal rollback
first found no microphone, then refused the returned state: AGC was on while the
test's raw91 gain remained. The disconnect's cause remains unknown.

Main separately checked the same USB identity and port, no hardware owners,
the original journal and exact two-card readbacks, then restored the one
remaining test-owned gain to raw147. The original rollback then succeeded.
That recovery is a separate receipt; the original failed diagnostic remains
failed. Legacy startup required a disclosed microphone-mute reassertion, so
continuous mute during legacy startup is not claimed. Final microphone and
speaker software mutes are true; the physical switch is open.

## Next acceptance boundary

The first user pilot remains supervised, stationary and one person at a desk.
A clean sustained ordinary conversation, correct input handling, and verified
stop/reopen are still needed before declaring it ready. Natural interruptions,
background/multiple-speaker rejection and body choreography remain unqualified.
The previous hourly target is not a booking confirmation. The corrected reusable
[Rust runner](../serve-startup-clock-20261012/README.md) is now verified offline;
it has not yet driven a native conversation. No new V1 benchmark was run.

[Exact results and artifact digests](results.json). Raw recordings are in the
local `.cache/physical-audio/20261012/` cohorts `ordinary-extra-input-0srh7fxs` and
`mic-comparison-r4-jw756u6z`; they are not published in Git or uploaded.
