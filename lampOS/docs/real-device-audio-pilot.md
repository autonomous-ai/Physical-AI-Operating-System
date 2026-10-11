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
The target retained certified ALSA-resampled pre-AEC microphone, post-AEC and
accepted-render PCM; pre-AEC is not native capsule PCM.

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

## Repeated trial: premature endpoint and provider timeout

A second trial used the same source greeting, target executable and processing,
with target output volume temporarily raised from 77 to 100 for the observer.
The planned NS-on/off/on cohort stopped after its first attempt failed; the
other two stimuli were withheld. Both Lamps were restored and target volume
returned to 77. No reply audio was generated or played.

The target admitted one question as two turns. The first local endpoint was
at 145937561741 microseconds; the second input was admitted 241.215 ms later,
before an answer had arrived. The provider emitted only the session-scoped
input transcript “Hola.”, then `BarrierTimeout` under the existing `Require`
policy. Request 2 was reported lost at `AwaitingResponse`; the worker exited.
The coordinator's final error was `Connection refused`, which obscured that
provider cause. This old build used Legacy reporting; the coordinated failure
consumer was integrated later in `02148ed4d` and needs separate device checks.

All 54,240 target capture frames and 82,080 accepted render frames were
certified without diagnostic faults. Every render frame was zero. The source
recorder closed when the runtime failed and its microphone permission became
unknown: both source PCM streams were discarded, their receipts are invalid,
and no source WAV or acoustic score exists for this attempt.

Offline replay reproduces the actual Start38/End113/Start137/End263 sequence.
The recorded low-VAD interval lasted 680 ms, but about 360 ms of that region
was quiet in the waveform. Earlier phoneme energy remained. Thus 600 ms below
the speech-confidence threshold was not 600 ms of acoustic silence. Exact
post-AEC PCM recreates the scores within 0.000014; replaying pre-AEC through
the original NS-on processing matches saved PCM within one integer count.
With NS off in this offline counterfactual, AEC retained, the same input forms
one turn. This supports a matched live experiment, not a changed release
default or an acoustic latency claim. No larger silence timeout was selected.

## Isolating the provider with a clean cached greeting

The source cache was later retrieved and corroborated by local ASR and waveform
matching. Its original 24 kHz WAV SHA256 is
`89f6ba271c1dedc2f7e69b77c46d50ed13cf1ec88dd6429b41e8e76f9d937269`.
A documented, validated offline resample produces 33,206 mono 16 kHz PCM16
frames, SHA256
`408e4ded308aec059e29388e68005f5f69c9e7651a041d0daa274e66590ba09c`.
This is existing synthetic speech, with no new TTS generation. It establishes
a reusable fixture, not a retroactive per-request checksum from the speaker API.

A finite Rust example based on `51bd815e9` sent that fixture from a0ae through
the same provider configuration, bypassing VAD and audio hardware. Require
stayed enabled; there was one request, no retry and no resumption. The ARM64
example passed nine tests under QEMU, strict scoped Clippy, release build and
native fixture inspection. Its executable SHA256 is
`82b24b95e557946411aab2856f4c3da48ea81792976cd273b88efe7bd8d7a5ee`.

Gemini transcribed “Hello, Lamp. How are you doing today?” and replied:
“Hello! I'm doing quite well, thank you for asking. How have things been with
you?” It returned 129,121 audio frames. First audio arrived 1.037035 s after
the final input enqueue, with maximum input pacing lateness 1.805 ms. This
single component observation excludes endpointing, speaker playback and room
acoustics; the answer was saved, not played.

The planned uploads of recorded microphone clips were not executed: automatic
approval review rejected those payloads as potentially containing private
speech. Analysis continues locally and with fresh controlled live stimuli.
The clean synthetic control does not establish recognition quality for any
recorded microphone clip.

## Two spoken NS-off attempts and one withheld prompt — 2026-10-11

The completed source 51bd cohort used the exact same synthetic cross-Lamp greeting,
target a0ae volume 100 and NS off with AEC unchanged. Two prompts were played; both
target runs retained one admitted turn and a full retired reply, with certified
capture/render diagnostics. A third prompt was withheld when the unchanged 100 ms
readiness cue expired 9.917 ms before the confirmation check. It is a zero-input
operational failure, not a dropped Lamp answer. All attempts remain in the ledger;
rollback/source restoration validated and the outer wrapper restored the original
target volume 77.

Trial 1's independent cached local ASR agrees across both source-room microphones
and the actual accepted output PCM: “Hello, I'm here to help out. So how are you
doing today?” The 3.68 s reply has no injected playback gap. Gemini's text
transcript instead says “I'm here and ready to help”, so software transcription
must not replace the acoustic/PCM wording evidence. Trial 2's reply was “We're
looking great and ready for a productive day together.” Its input transcript was
wrong (“How many people today?”), and it contains 6,240 explicitly accepted zero
frames at 24 kHz—a 260 ms gap after only 40 ms of initial output. The complete
output remains a conversational-quality failure.

The Jieli observer recovered both prompts and reply endings without reply clipping,
but its hardware DSP strongly gates quiet phonemes. The source's C-Media observer
(capture 20 / AGC on) clipped about 9% of prompt samples. Waveform/ASR-supported
recorder-visible timing proposals were 1.85–2.01 s and 1.97–2.17 s; these are **not
accepted acoustic bounds**, and no p50/p95 or ≤2 s acceptance is claimed. The first
opening is clearer than the earlier weak pilot, but reply/provider differences
prevent isolating gain as the cause. No human listening, direct-human, double-talk,
or NS-setting winner is established.

Frozen evidence: `/private/tmp/lamp-nscheck-room-review-20261011/report.json` SHA
`c6bc7076763a93a87a21fac972d263674f64dcc7d4ff08dac97710e72e025b50`; input manifest
SHA `093ba63c5cffd23b36f37ef8e0db545a3dd5f4c4fd64b9258c5409c6ecd7ad91`; freeze SHA
`10742f5038ca023027a96f125ae94a105553e7cf0540beedf9509c7225eaa735`. This historical
cohort predates the new integrated Coordinated binary and must not be relabeled as
its physical qualification.

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

Next: qualify recovery and conversation continuity on the integrated runtime,
isolate recognition and unexpected interruption inputs with local analysis and
controlled fresh speech, and repeat recorded conversations. Keep recorded room clips local. A comparison against V1 main
requires matched hardware, stimulus and acoustic scoring. These trials do not
yet provide that comparison, interruption qualification, TV rejection or
multiple-speaker evidence.

## First native coordinated-runtime turn

Integrated source `02148ed4d`, binary SHA256
`fb7e11911b7b84ef7a0af40a0c0c8ec26e7fc852dcd1b4228d3b173a2b979371`,
completed one fresh cross-Lamp greeting on a0ae. This was a 40-second directed
run with NS off, AEC unchanged, a fresh stationary handover and independent
rollback. Both Lamps were restored; target volume returned to its captured
original 77 and source volume to 94.

The target admitted one question and retired one complete reply. Provider input
text was “Hello, how are you doing today?” Its output text was “Hello! I'm doing
well, thank you for asking. How about you?” Independent cached local ASR on both
the accepted output PCM and source Jieli recording recovers that complete
relevant reply, including the opening and ending. This remains automated
recognition evidence, not a human listening judgment.

There were two explicit playback gaps: 1,920 and 3,360 accepted zero frames at
24 kHz, or 80 ms and 140 ms. Their reply offsets were 0.09–0.17 s and
2.77–2.91 s. The 89,760 speech frames span 3.74 s; the timeline including gaps
is 3.96 s. The runtime correctly reports
`audio_written_with_playback_gaps_unscored`, not a gap-free success.

Local endpoint to first provider audio was 1.024276 s; local endpoint to first
ALSA acceptance was 1.025392 s. The endpoint includes a 600 ms silence wait.
Neither software timestamp establishes the first audible word. This single
turn does not qualify an acoustic p50/p95, recovery during an outage, natural
interruptions or a V1-main speedup.

Capture and render completion certificates are valid and acknowledged with
zero diagnostic faults. The source recordings and privacy/ownership receipts
passed independent integrity checks. Evidence directory:
`lamp-rust-coordinated-evidence-20261011T030937Z`. The first availability receipt
uses the new conservative source-clock interval; worker and coordinator ran
together on hardware without a protocol fault. Further runs must retain
failed and withheld attempts alongside this one.

## Private playback-candidate cohort

The seven-file 200 ms start-gate experiment used source `02148ed4d` plus patch
SHA256 `0123d6d8a892efc4b7c5f00c9b2c97d1a59296a7ae0813b82d46fc4a884d52fa`.
Native ARM binary SHA256:
`d06e795176e49615b910875bc7a67e0a622dc021fa729fb2b0d858dc20103ad7`.
NS remained off, AEC unchanged, and source/target test volumes were 100.
The candidate is not the integrated or selected release policy.

Three attempts are retained in `lamp-rust-playout-evidence-20261011T033405Z`:

| Attempt | Prompt played | Runtime result | Conversation result |
|---|---|---|---|
| 1 | Yes | Two admitted turns; first reply cancelled, second software-complete | Failed: extra input changed the answer to Spanish |
| 2 | Yes | Four admitted turns; three replies cancelled, fourth software-complete | Failed: original greeting was misheard, then repeated additional inputs |
| 3 | No | Original-readiness confirmation failed; prompt withheld | Unattempted audio, retained as an operational failure |

All six answer prefixes/replies report zero playback gaps. Four prefixes were
cancelled and therefore do not establish full-answer continuity. The two
software-complete replies do not make either conversation successful. Both
Lamps were restored; target returned to its captured original volume 77 and
source to 94. This is neither an acoustic speedup nor a policy win. Local
recording/reference analysis is complete for these attempts. The six 10 ms
high-VAD blocks preceding each of the four cancellations match part of the
preceding accepted reply. Pre-AEC absolute waveform correlations are
0.626–0.868; bounded post-AEC matches are 0.440–0.937. These coefficients
are not probabilities. One processed trigger measures −46.85 dBFS despite
six speech scores at least 0.869. Another trigger clips. Capture, DSP and
render-reference epochs stay continuous at 1 throughout.

This supports leaked playback triggering the VAD-only interruption path;
it does not establish an echo-only room or justify an amplitude threshold.
The independent audit found no distinct additional room utterance, while
the target signals alone cannot exclude overlapping speech in every case.
All completed target diagnostic hashes, acknowledgments and counters verify
with zero diagnostic faults. The third, withheld attempt has no completed
target certificate and remains an operational failure.

The independent C-Media observer clips about 9.1% of prompt samples; it
cannot yet supply accepted acoustic word boundaries. Room artifacts stay
private. Audit manifest SHA256:
`97dff9ea55cbe0eafec5b2022467e03ed54f2ec481111b8e0c79d167486e609b`.
The playback experiment is now published separately as
`lamp-playout-experiment-20261011` at `7f24ea81d`; it remains unselected.
Follow the [V1 parity plan](v1-voice-parity.md) before further policy tuning.
