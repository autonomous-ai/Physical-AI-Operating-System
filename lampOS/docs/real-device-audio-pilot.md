# First Rust conversation and same-Lamp audio check

Date: 2026-10-11. These are finite diagnostic trials, not the requested
V1-main/V2 benchmark. One real Rust response reached the room, but that
conversation failed content quality and had a playback gap. Acoustic
speech-end to first-word latency remains unscored.

## Rust trial on a0ae

Source code: `51bd815e9`, with ARM64 executable SHA256
`d3318abe080336dbd4c50d9d04e4517eb21cd0b7cec5afaa5d5f62e990b525e7`.
The offline ARM64 release build, strict all-target live Clippy and 12 provider
flow tests under QEMU passed before staging. QEMU results are separate from
the following physical evidence.

Lamp-52e6 played “Hello Lamp, how are you doing today?” through its installed
speaker service. Lamp-a0ae ran the finite Rust `directed` runtime for 40 seconds
using its Jieli speech input, Sonora AEC/NS, real Gemini connection and physical
speaker. Its legacy HAL, os-server, Hermes, bootstrap and LED services were
stopped. No ring, camera or motor behavior was under test. Gemini used
`gemini-3.8-live-extended-thinking`, `Kore`, and thinking `LOW`, through
`campaign-api.autonomous.ai`; the runtime instruction requests English.

Source playback requested the existing TTS cache. The API does not expose
cache-hit or exact source PCM identity, so this was not a SHA-pinned stimulus.
Both source microphones recorded a continuous 74.875-second room observation.
The target retained certified raw microphone, post-AEC and accepted-render PCM.

| Observation | Result |
|---|---|
| Target input/answer count | One admitted input and one answer; no extra admitted turn |
| Source Jieli local ASR | “Hello Lamp, how are you doing today?” |
| Target before software AEC, local ASR | “I don't know, what are we doing today?” |
| Target after AEC/NS, local ASR | “Hello, Mel. How are we doing today?” |
| Gemini input transcript | “Allora, come ti chiami tu?” |
| Gemini output transcript | “Mi chiamo Lamp.” |
| Delivered speech | 33,120 accepted frames at 24 kHz |
| Playback defect | One 720-frame/30 ms inserted-zero gap during the opening voiced segment |
| Runtime/diagnostics | Exit 0; valid cue order and certified capture/render streams; zero diagnostic faults |
| Restoration | Source settings restored; target rollback verified with serial access still blocked |

Local ASR is a hypothesis, not human listening confirmation. The short target
reply matches the independent room recording, but its exact Italian words are
provider-transcript evidence only: the cached recognizer is English-only.
The normal result remains `audio_written_with_playback_gaps_unscored`.

The target prompt had no digital clipping: RMS was -25.91 dBFS before AEC and
-26.07 dBFS afterward. After allowing a 13.75 ms processing shift, a fixed
prompt window had normalized waveform correlation 0.954. This is not an
intelligibility score. The recording does not support blaming software AEC/NS
for erasing the greeting, and it does not establish why Gemini switched language.

The first substantial provider chunk contained 130 ms of audio; the next
arrived about 157 ms later. The speaker inserted 30 ms of zeros after the
initial audio ran out. This recorded pacing sequence is now a regression input
for reviewing buffering without sacrificing cancellation responsiveness.

| Software boundary, one sample | Elapsed |
|---|---:|
| Local endpoint to first provider audio | 1.023926 s |
| Local endpoint to first ALSA accepted write | 1.046792 s |

The endpoint includes a 600 ms silence wait. Neither row measures actual
speech-end to audible first word. The observer strongly attenuated the reply
opening; selecting its clearer later segment would misstate that metric.
No acoustic p50/p95 or speedup is reported.

Two preceding setup attempts failed stop-hook verification before any legacy
process stopped or prompt played. They are retained as setup failures, not
voice attempts. The successful trial used typed D-Bus verification of empty
stop hooks, a bounded supervised runtime, independent rollback and the existing
serial mask. The restored legacy HAL remains software-muted with that mask.
V2 is not yet installed as the continuously running default.

## Owner-requested same-Lamp check

To remove cross-Lamp distance, 52e6 played the same greeting into its own
Jieli and C-Media microphones at volume 60. This used installed V1 playback and
exclusive recorders, without Gemini or Rust conversation. Both 44.875-second,
mono 16 kHz PCM16 recordings passed hash, frame-count and privacy/ownership
checks. Original volume 94, mode and mutes were restored and verified.

| Microphone | Local ASR | Digital rail samples in 2.6 s prompt window |
|---|---|---:|
| Jieli speech | “Hello Lant, how are you doing today?” | 0 |
| C-Media ambient | “Hello Land, how are you doing today?” | 323 / 41,600 (0.776%) |

Every word except the name was recovered in both short and full-recording
passes. A proper-name spelling difference alone does not prove an audible
mispronunciation. In the preceding source-volume-100 recording, equivalent
windows had 3 Jieli and 3,036 C-Media rail samples. Lower volume reduced
clipping; this single comparison does not establish improved word accuracy.
The C-Media input still clipped and is not a clean independent onset reference.

This establishes usable same-device playback/capture for controlled diagnostic
work. It does not qualify natural duplex conversation: normal echo handling
must reject Lamp's own voice. Keep self-playback, recorded-file replay,
cross-Lamp loudspeaker input and direct-human speech as separate cohorts.

## Evidence and next checks

Private evidence stays outside Git:

- `lamp-rust-first-pilot-20261011T014304Z`: orchestration, room WAVs and target
  runtime/diagnostics; independent audit `lamp-rust-first-smoke-review-56fwvl2k`.
  Audit receipt SHA256:
  `87b8772745e85e4aab1b4170578d44961ffbaca5ffae53dec38040f7462ae588`.
- `lamp-same-device-recording-20261011T0200`: single-Lamp route evidence;
  independent audit `lamp-same-device-review-zai75t1_`.

Next: pin reusable stimulus PCM; isolate microphone/processing/provider
recognition using the recorded inputs; reproduce and correct the opening
playback gap; then repeat recorded conversations. A comparison against V1 main
requires matched hardware, stimulus and acoustic scoring. These trials do not
yet provide that comparison, interruption qualification, TV rejection or
multiple-speaker evidence.
