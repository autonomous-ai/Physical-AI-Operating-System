# V1 voice parity before V2 optimization

The owner restated this order on 2026-10-11: study what V1 already does well,
port that behavior into Rust, establish a working baseline, then improve it.
This is an implementation and qualification requirement, not a claim that V2
already matches V1. All references below use main commit
`d5efe9d7b73cc529b34cd4abe97624682a82ca94`, freshly fetched on this date.
Installed flags can select a different path; an old device observation does
not identify its configuration.

## Three different V1 paths

| Path | Protection in V1 main | Cost or limit |
|---|---|---|
| Stock automatic turn-taking (`HAL_LIVE_MODE=false`) | While TTS/music speaks, keep the microphone open but discard capture, or release it when warm capture is disabled. Recheck playback after the blocking read. After playback, skip a short echo tail, clear the input prefix and reset the speech detector. | Ordinary replies are protected from their own audio, but spoken interruptions during playback are discarded too. |
| Optional hardware-AEC live (`LIVE_MODE && !AEC_ENABLED`) | Compare microphone level with recent playback and noise; require 240 ms sustained playback-time activity, keep a 300 ms prefix, and inhibit admission during the first 3 seconds of accumulated playback. A local candidate only ducks the speaker; provider interruption cancels the reply. | This is a reference design, not proof that the installed Jieli DSP supplies the expected cancellation. Warm-up, level thresholds and ducking can hide or delay real interruptions. |
| Other live path | During playback/tail, send same-duration silence by default. In `cancelled` mode, pass data only when software cancellation had reference coverage. | Reference coverage is not a measurement that echo was removed. It does not prove that the person spoke. |

Pinned code: [stock capture suppression](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/voice/voice_service.py#L1063),
[local candidate ducks only](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/voice/voice_service.py#L1395),
[adaptive gate](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/voice/_internal/live_gate.py#L136),
[uplink policy](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/voice/voice_service.py#L1322).
The broader [microphone audit](microphone-v1-v2-audit.md) records the exact
configuration conditions and earlier device evidence.

## The current Rust difference

At integrated runtime `02148ed4d`, six consecutive 10 ms blocks with speech
probability at least 0.80 create a candidate. The directed coordinator accepts
it immediately as `DirectedSessionVadOnly`, cancels the current owner and sends
a new provider Start. The bounded candidate mechanism preserves audio and
ownership but has no active echo/addressee decision. It therefore does not yet
port V1's interruption safeguards.

Two physical prompts in the playback-candidate cohort produced four such
cancellations. Independent waveform analysis links all four trigger windows
to the preceding accepted reply. The reference/DSP epochs remained continuous;
the failure is not explained by a reset at those boundaries. This supports
self-echo involvement, without certifying that nobody else spoke in the room.
The speaker also reached the target microphone much louder than the prompt,
with clipping. More cancellation tuning alone has not resolved the failures.
See [physical evidence](real-device-audio-pilot.md).

Checkpoint `7dde135af` now provides an opt-in `v1-baseline` that suppresses
prospective input during confirmed own output and a declared 100 ms tail,
before replacing the reply. It preserves capture/reference continuity and
checks delayed output evidence against the original input intervals. Host and
ARM gates pass. After an initial recorder setup failure, a physical fixed-reply
trial completed without self-cancellation, followed by two complete real Gemini
replies with no extra admissions or cancellations. That Gemini run still failed
quality: an 80 ms accepted-zero gap and a wrong-language second reply. This is
limited lifecycle evidence; general physical parity and acoustic latency remain
pending. Default `vad-only` is unchanged. See the
[implemented baseline](turn-taking-baseline.md).

## Maintained reference designs and provider constraints

Reviewed on 2026-10-11: LiveKit documents separate activity, interruption and
false-interruption recovery behavior. Its adaptive path distinguishes attempted
interruptions from acknowledgments; its false-interruption handling can resume
speech instead of permanently discarding it. These are useful behavioral
references, not measured Lamp results or a reason to add its Python/Node runtime
to lampOS. Its own documentation says most client interruption options do not
apply when a realtime provider owns turn detection. Check the actual integration
contract before copying a configuration knob.
[LiveKit turns](https://docs.livekit.io/agents/logic/turns/).

Gemini's API specifies that manual activity signals are sent when automatic
activity detection is disabled; `START_OF_ACTIVITY_INTERRUPTS` causes activity
start to interrupt the response. Rust currently selects both. Consequently,
after Lamp has sent Start, a server interruption is not independent proof that
the microphone contained a genuine user interruption. This is an inference
from the API contract and our configured request path. Qualify the local
candidate before that irreversible boundary; a naive copy of V1's
wait-for-provider-confirmation rule would not preserve its meaning.
[Gemini Live reference](https://ai.google.dev/api/live.md).

## Implementation order and acceptance

1. Establish a clearly named V1-style turn-taking baseline in Rust. Retain
   continuous capture/reference and privacy responsiveness, but reject own
   playback before it can cancel an answer. Do not announce this mode as
   natural full duplex. Pending synthesis alone is not audible playback.
2. Replay the actual V1 adaptive gate against the retained capture and accepted
   render, with its warm-up and mapping differences reported. Preserve negative
   cases, real greeting controls and separately labelled synthetic overlap.
   Passing echo-only clips cannot establish interruption quality.
3. Port the useful candidate/confirmation separation. A reversible reduction
   in volume must preserve the answer and its owner; cancellation requires
   qualified evidence tied to retained input. Speaker gain changes must enter
   the actual accepted echo reference. Privacy/fault handling still stops output
   promptly and cannot wait for a cloud decision.
4. Compare the same fixture, build, device and level settings on hardware.
   First prove full replies with no extra inputs; then short follow-ups,
   early/mid/late interruptions, quiet speech and topic changes. Report lost
   opening words and false rejection as well as avoided interruptions. Repeated
   ducking caused by echo is a failure even if the answer eventually finishes.

Local baseline admission must add no network call or blocking I/O and should
cost at most one 10 ms audio block when playback protection is inactive. The
existing conversational target remains acoustic speech-end to first word p50
at most 2 seconds. Record the baseline's intentional playback suppression and
tail separately; it cannot pass the full-duplex acceptance scenarios by marking
them successful silence. No before/after acoustic latency result exists yet.

Main owns integration and device scheduling; Agent 1 supplies audio/Gemini and
offline comparisons, and Agent 2 maintains evaluation. Each checkpoint names
the V1 behavior reused, intentional differences, exact source and test evidence.
This plan adds no runtime behavior by itself.
