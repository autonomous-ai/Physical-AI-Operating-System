# Gemini Live tools, speech synthesis and prices

Web research on 12 October 2026 from Google's own documentation, read through a
summarizing fetcher. Field and enum names were checked against the API
reference; prose is paraphrased. Re-check a fact against the live page before
relying on it, and prefer the newest page.

## Models

The models page (https://ai.google.dev/gemini-api/docs/models, updated
9 October 2026) lists:

- `gemini-3.8-live` and `gemini-3.8-live-extended-thinking`: stable since
  15 September 2026 (https://ai.google.dev/gemini-api/docs/changelog). They take
  131,072 input tokens and return up to 65,536
  (https://ai.google.dev/gemini-api/docs/models/gemini-3.8-live).
- `gemini-3.1-flash-live-preview`: legacy.
- `gemini-2.5-flash-native-audio-preview-12-2025`: access limited to recent
  users since 18 September 2026.

## 1. Function calling in Live

**Declaration.** `setup.tools`:

```json
[{"functionDeclarations":[{"name":"…","description":"…","parameters":{…},"behavior":"BLOCKING"}]},
 {"googleSearch":{}}]
```

- **Fixed at setup:** tools cannot change during a session.
- **Schemas:** `parameters` and `parametersJsonSchema` are mutually exclusive.
- **Names:** `a-zA-Z0-9_:.-`, up to 128 characters
  (https://ai.google.dev/api/generate-content).
- **Count:** no hard limit is documented; the guidance is 10–20 active tools
  (https://ai.google.dev/gemini-api/docs/function-calling).
- **Search:** Google Search and function declarations can share the array
  (https://ai.google.dev/gemini-api/docs/live-api/tools).

**Messages** (https://ai.google.dev/api/live):

- **Server:** `{"toolCall":{"functionCalls":[{"id","name","args"}]}}`.
- **Client:** `{"toolResponse":{"functionResponses":[{"id","name","response":{…},"scheduling"?,"willContinue"?}]}}`,
  matched by `id`. An `"error"` key inside `response` reports failure.
- **Server cancellation:** `{"toolCallCancellation":{"ids":[…]}}`, sent "only in
  cases where the clients interrupt server turns". The client should undo the
  side effects.

**`behavior`:** `BLOCKING` or `NON_BLOCKING`. Pages disagree on the default:
the API reference says `BLOCKING`, while the 3.8 Live model page says
`NON_BLOCKING`. **Always set it explicitly.**

| Model | Function calling | `scheduling` |
| --- | --- | --- |
| `gemini-3.8-live` | asynchronous by default; `BLOCKING` available | supported |
| `gemini-3.8-live-extended-thinking` | `NON_BLOCKING` only; `BLOCKING` "returns a hard error" | not supported |
| `gemini-3.1-flash-live-preview` | synchronous only | not supported |
| 2.5 native audio | both | not stated |

Source: https://ai.google.dev/gemini-api/docs/live-api/capabilities and the
3.8 Live Extended Thinking model page. **The brain design therefore uses
`gemini-3.8-live`, not the extended-thinking variant.**

**`scheduling`** applies to `NON_BLOCKING` calls only; the default is
`WHEN_IDLE`.

- `SILENT` adds the result to context without generating a reply.
- `WHEN_IDLE` replies when nothing else is happening.
- `INTERRUPT` interrupts the current output (spelled `INTERRUPTED` on some
  model pages).
- On 3.8 Live the server downgrades `INTERRUPT` to `WHEN_IDLE` while the user
  speaks
  (https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/guides/gemini-3-8-live).
- Even with `SILENT`, the model may still talk about the tool.
- The model sometimes issues a duplicate call while one is pending; ignore it
  (https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/live-api/asynchronous-function-calling).

**Audio during a call:**

- **Blocking:** generation pauses until the response arrives. New user input
  meanwhile makes the server cancel the pending call.
- **Non-blocking:** the model keeps listening and speaking.
- **On an interruption,** generation is discarded and pending calls are
  dropped.
- **Resumption:** `sessionResumptionUpdate.resumable` is false during calls.

**Manual activity detection.**

- Set with `realtimeInputConfig.automaticActivityDetection.disabled: true`,
  then send `realtimeInput.activityStart` and `activityEnd`.
- `activityHandling` defaults to `START_OF_ACTIVITY_INTERRUPTS`; the
  alternative is `NO_INTERRUPTION`.
- **Inferred, not documented:** under the default, `activityStart` counts as
  an interruption, so pending calls would be cancelled.

## 2. Device-initiated speech

- **`clientContent`.**
  - `{"clientContent":{"turns":[{"role":"user","parts":[{"text":"…"}]}],"turnComplete":true}}`
    works on all current models, with role `user` or `model`.
  - `turnComplete:true` "unconditionally interrupts" ongoing generation.
- **`realtimeInput.text`.** `{"realtimeInput":{"text":"…"}}` also works, but it
  is "optimized for responsiveness at the expense of deterministic ordering".
- **Speaking first.** The best-practices page says "Live API expects user
  input before it responds"
  (https://ai.google.dev/gemini-api/docs/live-api/best-practices).
- **Proactive audio.** It is always on for the 3.8 models
  (`proactive_audio:false` errors), and the model "can proactively decide not
  to respond". Whether it may skip injected text is not documented.
- **Cost.** Injected text is billed as text input, and it stays in context.
- **Known bug.** On Vertex `gemini-3.8-live`, mid-session
  `send_client_content` with role `user` pollutes `inputTranscription`.
  Google confirmed it on 9 October 2026; one `role:"system"` message avoids
  it (https://github.com/googleapis/python-genai/issues/3078).

## 3. Session and context

**Limits** (https://ai.google.dev/gemini-api/docs/live-session):

- without compression, sessions are limited to 15 minutes audio-only and
  2 minutes with video;
- a connection lasts about 10 minutes;
- the context is 128k tokens.

**Compression.** `contextWindowCompression:{slidingWindow:{targetTokens},triggerTokens}`.

- `triggerTokens` defaults to 80% of the window; `targetTokens` to half the
  trigger.
- Vertex allows triggers from 5,000 to 128,000.
- The best-practices example uses a 25k trigger and an 8k window.

**Resumption.**

- Send `setup.sessionResumption:{handle}`; the server sends
  `sessionResumptionUpdate{newHandle, resumable}`.
- Google AI keeps handles 2 hours after a session ends. Vertex contradicts
  itself.
- **Through the Autonomous proxy, V1 found resumption is not forwarded**
  ([economy protocols](economy-protocols.md)).

**GoAway.** `goAway{timeLeft}` arrives before the close (60 s on Vertex). On
extended thinking, `turnComplete` does not mean idle; check
`interactionStatus` (`IN_PROGRESS` or `IDLE`).

**Changing the instruction mid-session.** Google AI says the configuration
cannot change while connected; change it on resumption. Vertex documents a
`clientContent` with `role:"system"` and `turnComplete:false`.

## 4. Gemini speech synthesis

**Models:**

- `gemini-3.8-flash-tts`: GA 22 September 2026, over 130 languages.
- `gemini-3.8-flash-lite-tts`: GA 22 September 2026, 101 languages,
  described as low latency for real-time voice agents.
- Both take 8,192 input tokens and return up to 16,384. They do not work in
  the Live API.

**Voices.**

- There are 30 prebuilt voices, including Kore, Puck, Charon, Zephyr, Aoede
  and Leda. An extended library of 150+ voices also exists
  (`GET /v1beta/voices`).
- Live native audio "supports any of the voices available for our TTS
  models", set with `speechConfig.voiceConfig.prebuiltVoiceConfig.voiceName`.
- **Not documented:** whether one name sounds the same in Live and in speech
  synthesis.

**Request and format**
(https://ai.google.dev/gemini-api/docs/speech-generation):

- The docs now use `POST /v1beta/interactions` with
  `response_format:{type:"audio", mime_type:"audio/l16", sample_rate:24000}`,
  `speech_config:[{voice:"Kore"}]` and `stream:true`.
- A stream returns headerless 16-bit PCM at 24 kHz.
- V1 uses `streamGenerateContent` through the relay
  ([economy protocols](economy-protocols.md)).
- Style goes in `speech_metadata.style`, plus inline tags such as `<sigh>`.

**First-audio latency:** not documented.

## 5. Prices

Paid tier, per million tokens (https://ai.google.dev/gemini-api/docs/pricing,
9 October 2026). Audio counts 25 tokens per second.

| Item | 3.8 Live and 3.1 Flash Live | 2.5 native audio |
| --- | --- | --- |
| Input text | $0.75 | $0.50 |
| Input audio | $3.00 (about $0.0045 per minute) | $3.00 |
| Output text | $4.50 | $2.00 |
| Output audio | $12.00 ($0.018 per minute) | $12.00 |

**Live re-bills context.** "The API charges you per turn for all tokens
present in the session context window." Past tokens, audio included, are
billed again every turn, up to the window. Transcriptions are billed at the
text output rate.

| Speech-synthesis model | Input text | Output audio | Per spoken minute |
| --- | --- | --- | --- |
| 3.8 Flash TTS, to 31 Dec 2026 | $0.50 | $9.00 | $0.0135 |
| 3.8 Flash TTS, from 1 Jan 2027 | $1.00 | $18.00 | $0.027 |
| 3.8 Flash-Lite TTS, to 31 Dec 2026 | $0.50 | $6.00 | $0.009 |
| 3.8 Flash-Lite TTS, from 1 Jan 2027 | $1.00 | $12.00 | $0.018 |

A text model for the economy route, `gemini-3.8-flash`, costs $0.75 in and
$3.75 out through 2026, then $1.50 and $7.50.

## Not documented

- whether a voice name sounds the same in Live and in speech synthesis;
- speech-synthesis first-audio latency;
- a hard limit on the number of tools;
- whether proactive audio may ignore injected text;
- a Google AI (not Vertex) mid-session instruction update;
- whether the per-minute Live price includes re-billed context.
