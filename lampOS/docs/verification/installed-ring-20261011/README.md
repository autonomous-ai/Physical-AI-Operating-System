# Installed-service ring check on 4ace

The unchanged installed V2 service completed **2/2 cached iMac-to-Lamp exchanges**
with ring ceiling 12, then stopped cleanly and restored stationary muted legacy
services. Both replies report zero playback gaps; all **877 ring requests** have
matching original-frame presentation receipts. Two local camera stills show the
ring illuminated during the second reply and visibly unilluminated after Stop.
No concrete defect was found in the independent artifact review.

This supplements the prior [installed-service check](../installed-service-20261011/README.md)
on the same runtime. It qualifies one finite voice/ring lifecycle, not natural
human interaction, physical mute/reopen, continuous optical choreography or
all-day reliability. Raw room audio and photos remain local.

## Exact measured runtime

The device ran source `55d7af98e149591eed7882b9ace7741486daa842`, binary
`738503a803d3718e52ca9e8886a669c0e41c007f7fe35873642afd4a83f5b8d0`.
The installed unit and provider pins are unchanged. The sole runtime argv addition
was `--ring-channel-ceiling 12` through a fresh instance-only override, removed
before legacy restoration. Camera-context and motors were not enabled.

Integration `e9bdcea282be7b787abb0568eb46a5c7958a6cce` and composed binary
`b1c61a224ea9958eeaa5f06b87fdc7591128da3daf01c21e2f3c851a7643edbd`
were **not deployed in this run**. Its newer Gemini Stop fence and optional
failure-notice work must not inherit these physical results. See the separate
[source/emulation gate](../../gemini-session-reliability.md#integration-verification-2026-10-11).

## Conversation and software timing

The two existing Daniel clips ran once each at 190/200 words per minute. Prompt 2
required the original predecessor's whole-turn completion, final retirement and
a fresh check; no retry or deliberate overlap occurred.

| Turn | Prompt | Complete provider reply |
|---|---|---|
| 1 | Lamp, how are you doing today? | I'm doing well, ready to help with whatever you need. How about you? |
| 2 | Lamp, tell me one interesting fact about the ocean. | More than eighty percent of the ocean is unmapped and unexplored. |

| Software boundary | Greeting | Ocean fact |
|---|---:|---:|
| Local endpoint → first provider audio | 890.627 ms | 939.050 ms |
| Local endpoint → first accepted ALSA write | 1,255.596 ms | 1,113.529 ms |
| First provider audio → first accepted ALSA write | 364.969 ms | 174.479 ms |
| Fixed endpoint wait, excluded above | 1,200 ms | 1,200 ms |

These are individual software observations, not speech-end-to-first-word timing
or a causal buffer comparison. Both retain the 200 ms start horizon. Owners and
playback sequences 1/2 match epoch-1 final retirement cursors 263760/517680;
there is no extra admission, cancellation, reference reset or runtime fault.
`serve` retains metadata rather than target PCM: retirement is receipt-verified,
not a byte-level PCM comparison. `complete_evidence=false` remains truthful.

One cached local Whisper `small.en` pass with two CPU threads corroborates both
full exchanges, spelling “Lamp” as “Lam” and changing punctuation/number format.
Only the two 9.995/14.720-second exchange windows were transcribed; no extra words
appear in these hypotheses. The ocean statement is retained as spoken, not a
factual acceptance oracle. `human_listened=false`; acoustic scores remain null.

## Ring, images and shutdown

All 877 original request/permit/owner/frame pairs match, with zero rejected or
pending frames. Request → SPI-return spans **1.407–12.166 ms**; actual write calls
span 1.317–1.967 ms. Six stale-presentation blanks and one verified shutdown-black
receipt are retained. These are local software boundaries, not optical latency.

| First phase transition → SPI return | Greeting | Ocean fact |
|---|---:|---:|
| Input admitted → listening | 2.803 ms | 2.618 ms |
| Local endpoint → waiting | 5.095 ms | 4.697 ms |
| First accepted ALSA write → speaking | 6.001 ms | 4.004 ms |

The active JPEG's LaunchServices bracket is 19:31:36.458993–19:31:40.369271 UTC;
the after-Stop bracket is 19:31:41.457022–19:31:45.348300 UTC. Both approved-app
wrappers exited 0 and were reaped. The active trigger retains turn 2's original
speaking receipt; the second retains the verified Stop and precedes ordinary
legacy rollback. Independent image review sees a bright near-white ring in the
first frame and a visibly unilluminated ring surface in the second. Exposure,
color, optical onset/extinction timing and full-motion clearance are unqualified;
launch/completion brackets are not shutter timestamps.

- Invocation `ca42b26b5f744782ac370ae54a637292`; 2,055 contiguous metadata events,
  healthy matching finish ACK, no dropped or unconfirmed events.
- One observed in-cgroup SPI ring owner; all six observed PID/start identities
  gone after Stop, empty cgroup/hardware-owner sets, inactive successful unit.
- Administrative Stop → verified clean state: **229.963 ms**; coordinator SIGTERM
  → `run_end`: **22.215 ms**. Shutdown-black write returned 2.806 ms after the
  coordinator Stop observation.
- Exact owned override removed after resource release; effective drop-ins empty
  and installed template unchanged. Same-boot stationary rollback, muted readback
  and both original mixer texts verify separately. The motor guard remained.

## Recording and provenance

The approved iMac observer retained all 8,640,000 mono 48 kHz float32 frames over
180 seconds: 16,875 contiguous callbacks, two checked tail callbacks, zero clips,
nonfinite samples, timestamp gaps, queue losses or recorded backend faults.
Recorded host coverage encloses the runtime window. Both cached-player receipts
retain the explicit iMac Speakers route and complete, error-free submission.
Mac prelaunch settings were output 65/input 27/unmuted. CPAL, host and target
clock domains remain distinct; no target-to-Mac acoustic transform is claimed.

[Numerical evidence and exact hashes](results.json) bind the result, 11-member
native archive, helper source, original recording/ledger, both images and the
independent audit. All archive members were verified before private extraction;
all recorded input hashes were checked again unchanged. No media or credentials
are published. Original evidence is
`/private/tmp/lamp-ring-installed-executed-1s3h0zqo`; independent review is
`/private/tmp/lamp-ring-result-audit-klliwjtt`.

The [attended pilot](../../attended-4ace-pilot.md) still requires physical
mute/reopen and desk audibility checks before facilitator handoff. Real-human
addressing, coworker/laptop/call rejection and natural duplex interruption are
not proved by these cached prompts; V1-baseline intentionally suppresses input
during own playback and its declared tail. Continuing after local End but before
a reply remains a separate provider-settlement limitation. The optional failure
notice has no approved production asset and was not enabled.
