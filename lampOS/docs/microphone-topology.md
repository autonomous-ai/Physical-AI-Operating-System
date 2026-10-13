# Microphone topology and processing qualification

Updated 2026-10-11. Kyle's layout and listening report were relayed again by the
owner; they agree with the hardware-team information already recorded here.
Other evidence is the pinned parts list, manufacturer material and a read-only
check on lamp-4ace. This update adds attribution, not new hardware measurements.

## Three physical capsules, two active capture devices

The supplied front-view diagram identifies one central capsule as `mic1` and
the two outer capsules as `mic2`. These are two input roles, not two physical
capsules. The team reports that the outer pair feeds a Jieli board with onboard
processing. Its mono USB stream does not mean that the board contains only one
microphone. Earlier channel-count reasoning must not be used to deny its
internal two-microphone arrangement.

| Role | Team-reported hardware and placement | Observed host interface |
|---|---|---|
| Environmental sound / mic1 | Center capsule through C-Media / Unitek Y-247A | `plug:device_micro1`, C-Media capture: mono PCM16 at 44.1/48 kHz; separate from its stereo playback endpoint. |
| Conversation / mic2 | Left and right capsules through a Jieli dual-microphone board | `plug:device_micro2`, Jieli capture: mono PCM16 at 48 kHz; one USB input endpoint, no playback endpoint in the observed ALSA stream descriptor. |

The manufacturer's [Y-247A page](https://www.unitek-products.com/products/usb-2-0-to-stereo-audio-converter)
describes a USB sound adapter with analog microphone input, supporting 16-bit
48 kHz conversion. This identifies the adapter, not the center capsule's
sensitivity, frequency response, directivity or self-noise.

The pinned parts list names the voice board `USB dual mic HK JZMIC v1.0`, although
its row calls it Mic 1; the wiring document and actual ALSA roles call voice
`device_micro2`. Use stable device roles rather than the inconsistent row label.
The [HK-JZMIC v1.0 manufacturer page](https://haokaimic.com/products/hk-jzmic-v1-0)
lists two analog microphones, voice enhancement, noise suppression, automatic
gain control, and 48 kHz/16-bit capture. The older manufacturer's
[product listing](https://www.haokaisz.com/en_US/proshow/1/HK-JZMIC-V1-1)
also identifies dual microphones and voice enhancement/noise reduction.
Those claims and the BOM do not identify the fitted board's firmware or verify
its active processing mode. Do not apply another board's advertised functions.

## Explicit microphone comparison

`lamp-voice directed` and `serve` accept `--microphone mic1|mic2`. The default
is still `mic2` (`plug:device_micro2`, Jieli). An explicit `mic1` selects
`plug:device_micro1` (C-Media capture); it does not change the speaker route,
mixer gain, AGC, software AEC, privacy checks, reference policy or turn-taking.
The option adds no processing stage, device probe, polling or startup delay.
Worker startup uses the selected capture device directly and session metadata
records both the requested microphone and named PCM. Those names depend on the
board's installed ALSA routing; they are not proof of a USB device identity.

The physical voice-eval configuration accepts `"microphone": "mic1"` or `"mic2"`
and passes it through the relay's typed `--microphone` option. Omission preserves
the original child arguments. Unknown and repeated values are refused before
starting a child. Fixture runtime-option forwarding retains its existing refusal.

This is an opt-in hardware diagnostic, not a new recommended default. Before a
mic1 run, verify its device mapping and exclusive ownership: the same capture
endpoint cannot also be the ambient observer. Use the independent iMac recorder
for that comparison. Keep mic1 and mic2 cohorts separate, retain the full request
prefix and response, and record actual gains and processing. The first native
checks must cover startup, mic/speaker continuity, privacy closure and echo during
overlap. Compare speech-end to first audible reply on retained room audio; the
selection alone establishes neither latency nor speech-quality improvement.

## Echo cancellation is still unqualified

The team reports echo cancellation and beamforming as well as environmental
noise cancellation. The inspected model-specific manufacturer pages do not
establish a usable acoustic echo-cancellation reference input, raw left/right
channels, direction-of-arrival output, or controls to select those functions.
Their absence from those pages does not prove the hardware lacks them.

Lamp's current software speaker route ends at the C-Media DAC and amplifier.
No ordinary USB playback endpoint was observed on the Jieli capture device.
Therefore a playback reference reaching Jieli through that USB audio route is
not established; an additional board-level connection or proprietary interface
would need separate evidence. Noise suppression/ENC must not be treated as proof
that Lamp's own speech will be removed during overlapping human speech.

The new software playback-clock defect remains a separate demonstrated bug:
a speech-chunk gap stopped the accepted PCM reference. Fixing that scheduling
bug neither proves nor disproves the hardware DSP's echo performance.

## Measurement names and intended use

`pre_aec.pcm16le` means ALSA-resampled input before lampOS's **software** Sonora
processing. On this voice input it can already include the board's processing;
it is not raw, independent microphone-capsule audio. `post_aec.pcm16le` is the
output after lampOS processing. Record mixer/AGC and any verified firmware modes
with both files. A clean-looking silent waveform can reflect suppression or a
noise gate, not a quiet room or successful speech preservation.

Keep the conversation input on Jieli by default for current stationary pilots.
Outside an explicit mic1 comparison, retain mic1 for environmental sensing and
the temporary continuous benchmark observer.
The observer shares Lamp's host and C-Media card with playback; it is not an
external microphone. Environmental levels remain uncalibrated digital levels.
Do not treat independent Jieli and C-Media USB clocks as a synchronized array.
The mono voice output does not expose separate left/right timing for a software
direction estimate. A vendor direction output or accessible synchronized raw
channels would require verification before use. Vision and fixed desk calibration
remain separate evidence for attention and orientation.

## Direct speech versus loudspeaker replay

The owner relayed Kyle's listening observation: direct human speech
recorded through mic2 sounded clear, while speech played through a loudspeaker
and recorded again through mic2 sounded muffled. No paired WAVs, matching gain,
placement, loudspeaker response or DSP configuration accompanied that report.
It is a useful hypothesis and measurement warning, not a quantified comparison
or proof of a human-versus-recording detector.

Loudspeaker, room, microphone geometry, existing source processing, AGC and board
DSP may all contribute. Do not attribute the effect to ENC alone or call the
board a liveness detector. The current iMac-to-Lamp pilots remain physical
**loudspeaker-replay** tests. They exercise cloud interaction, actual audio I/O,
playback, cancellation and repeated regressions, but cannot alone establish
latency or comfort for direct human conversation. Keep replay and direct-human
results in separate cohorts; repeat the primary acoustic boundaries with a real
speaker before manual acceptance.

Compare the original direct-mic files with the replayed files if those source
recordings become available. Sending a direct-mic recording into an offline or
provider harness is useful for isolating software behavior, but bypasses live
acoustics and is not itself an end-to-end human trial. Existing speaker-only
recordings and pre/post-software-AEC diagnostics remain useful with their limits.

## Required controlled comparisons

1. Verify the exact fitted board/firmware, exposed DSP controls, playback-reference
   wiring and any direction/raw-channel interface without guessing vendor writes.
2. Use cached Lamp-only speech, human-only speech, genuine overlap, pauses,
   near/far speech and fan/keyboard/media noise at fixed gains and positions.
3. Compare board-processed USB input alone with additional software AEC and noise
   suppression as separately declared treatments. First inspect matched recorded
   signals; only promote a treatment after live full-duplex tests. Do not disable
   software AEC globally based on a feature label.
4. Measure preserved opening words, echo-only false admissions, overlap speech
   intelligibility, interruption-to-silence, final speech-to-answer, clipping and
   processing/queue delay. Quiet output alone is not a success criterion.
5. The owner has deferred V1 benchmarking. For any later V1/V2 comparison, keep
   the hardware treatment identical; report microphone/DSP A/B experiments
   separately. Current work remains focused on V2.

Multiple synthetic voices from one iMac loudspeaker do not qualify direction
finding or spatial separation. The team drawing supplies relative placement,
not measured capsule spacing or a qualified beam pattern. No enclosure was
opened or movement commanded. The later finite [microphone control experiments](microphone-control-experiments.md) changed only advertised Jieli gain/AGC and restored both; no new runtime setting was promoted.
Evidence: `artifacts/microphone-layout-20261010/`.

## Fixed-source microphone-path pair, 12 October 2026

The [retained microphone-path pair](verification/microphone-path-pair-20261012/README.md)
completed one MIC1 diagnostic; MIC2 remains failed/incomplete because the helper
stopped on additional admissions. Both paths accepted the exact same full
337,680-frame fixed reply and retired its original owner with zero reported
speech gaps. Both software diagnostic streams and 100-second room recordings
passed the independent integrity checks, and both rollback receipts verified
stationary muted restoration with unchanged final mixers.

This was physical iMac loudspeaker replay under V1-baseline masking, not a
human or cloud conversation test. Captured admission sources remain unclassified.
No listening, ASR, acoustic score, causal gain/AGC explanation, or default
microphone promotion follows from it. The original copied plan contains inherited
mixer-trial fields; verified executed evidence records no mixer writes. Exact
hashes, commands, outcomes and preservation limits are in the linked report.

The subsequent [MIC1 Gemini duplex diagnostic](verification/mic1-gemini-duplex-20261012/README.md)
answered the initial question but did not admit the interruption. A1's separate
recorded-signal analysis identifies a late playback reference on MIC1; that
correction must be tested before judging MIC1's duplex capability. No default
microphone or gain change is selected.
