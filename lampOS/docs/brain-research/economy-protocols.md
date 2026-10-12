# V1 wire protocols for the economy route

Code study of V1 `main` at `170a1b85e` (12 October 2026). The Python SDK
details come from google-genai 2.12.1 (`hal/uv.lock:940-941`). These are the
protocols a Rust economy route must match when it uses the same Autonomous
relay.

**Verified by hand for the design:**

- the STT URL builder (`hal/drivers/voice/stt/autonomous.py:96-100`);
- the speech-synthesis chunk start of 60 characters
  (`hal/drivers/voice/tts/service.py:1500-1503`);
- that V1 disables Gemini session resumption because the proxy does not
  forward it (`hal/config.py:754-757`).

## Summary

| Service | URL | Auth | Format | Limit signal |
| --- | --- | --- | --- | --- |
| STT relay | `wss://…/api/v1/ai/v1/ws/audio/transcriptions?…` | `Authorization: Token <key>` | PCM16 16 kHz in, Deepgram JSON out | none specific; logged only; 1011 = upstream failure |
| Gemini TTS | `https://…/api/v1/ai/v1/google-search/v1beta/models/{model}:streamGenerateContent?alt=sse` | `x-goog-api-key` | SSE, base64 PCM 24 kHz | HTTP 429 |
| Gemini Live | `{llm_base_url}/ws/gemini` plus the SDK's BidiGenerateContent path | `x-goog-api-key` | Live JSON over WebSocket | not documented in V1 (4029 appears only in comments) |
| GPT-Live (comparison) | `…/ws/openai/live/sessions` | `Authorization: Bearer` | Live WebSocket | 4001 / 4002 / 4029 |
| Hermes | `http://127.0.0.1:8642/v1/{runs,responses}` | `Bearer hermes-local-api-key` | JSON and SSE | the LLM's limit banner text in the reply |
| Hermes to LLM | `https://campaign-api.autonomous.ai/api/v1/ai` | `AUTONOMOUS_API_KEY` | Anthropic Messages | banner text |

## 1. STT relay (`hal/drivers/voice/stt/autonomous.py`)

- **Provider choice.**
  - Direct Deepgram through its SDK when `deepgram_api_key` is set.
  - Otherwise the relay, with `stt_api_key || llm_api_key` and
    `stt_base_url || llm_base_url` (`hal/runtime.py:537-544,579-592`).
- **URL.**
  - `_ensure_openai_v1(base)` appends `/v1` to a campaign-api base ending in
    `/ai` (`tts/openai.py:16-21`).
  - The scheme becomes ws or wss, and the path is
    `{base}/ws/audio/transcriptions?{query}` (`autonomous.py:96-100,292`).
  - Result: `wss://campaign-api.autonomous.ai/api/v1/ai/v1/ws/audio/transcriptions`.
- **Auth.** `Authorization: Token <key>`. Open timeout 10 s, close timeout 5 s
  (`:131-136`).
- **Query parameters for nova** (`:16-21,72-82,282`):
  - `model=nova-3-general` (default; os-server pushes it for every language,
    `system/device/config_update.go:785-790`);
  - `encoding=linear16`, `sample_rate=16000`, `channels=1`;
  - `language=en`, `smart_format=true`;
  - `interim_results=true`, `endpointing=1500`, `vad_events=true`.
- **Keywords.**
  - nova-3 gets a repeated `keyterm` with the `:int` boost stripped; other
    models get comma-joined `keywords` (`:32-40,83-95`).
  - The terms are the agent name, the device type, "autonomous" and the
    wake-phrase words (`hal/app_state.py:1501-1521`).
  - Flux sends only `model`, `encoding`, `sample_rate` and `keyterm`
    (`:43-60`).
- **Device to relay.**
  - Raw PCM in binary frames (`:221-223`).
  - `{"type":"KeepAlive"}` every 3 s, never for Flux (`:232-248`). It is sent
    only with `HAL_STT_KEEPALIVE=true`, which is off by default.
  - On close, `{"type":"CloseStream"}`, then up to 15 s waiting for the final
    transcript (`:250-273`).
- **Relay to device.**
  - `Results`: reads `channel.alternatives[0].transcript` and `is_final`
    (`:164-171`).
  - Flux `TurnInfo`: final when `event=="EndOfTurn"` (`:173-179`).
  - `Connected`, `Metadata`, `SpeechStarted` and `UtteranceEnd` are ignored
    (`:181`). V1 never reads `speech_final`.
- **Errors.** Any other message type, such as a proxy quota error, is only
  logged (`:181-187`). Close code 1011 means upstream STT failed
  (`:207-210`).

## 2. Gemini speech synthesis (`hal/drivers/voice/tts/gemini.py`, on `main`)

- **Selection and credentials.** `tts_provider: "gemini"`
  (`backend.py:12,68-70`). Key `tts_api_key || llm_api_key`; URL
  `tts_base_url || llm_base_url` (`runtime.py:541-542`).
- **URL.** `POST {base}/v1/google-search/v1beta/models/{model}:streamGenerateContent?alt=sse`
  through the campaign-api Gemini REST relay (`gemini.py:21,34,68`). The test
  pins `https://campaign-api.autonomous.ai/api/v1/ai/v1/google-search/v1beta/models/…`
  (`hal/test/test_tts_gemini.py:31-43`).
- **Auth.** `x-goog-api-key: <key>` (`:76`).
- **Body.** `contents[0].parts[0].text`, plus
  `generationConfig.responseModalities=["AUDIO"]` and
  `speechConfig.voiceConfig.prebuiltVoiceConfig.voiceName` (`:69-75`).
- **Model and voice.**
  - Default model `gemini-3.8-flash-tts`, overridable with
    `HAL_TTS_GEMINI_MODEL` (`:19,66`).
  - Default voice Kore, one of 30 prebuilt voices; unknown names fall back to
    Kore (`:20-30,67`).
- **Streaming.**
  - SSE `data:` lines, with audio base64 in
    `candidates[].content.parts[].inlineData.data` (`:90-98`).
  - Format: 24 kHz int16 mono PCM (`backend.py:14,40`).
  - The 3.8 and 3.1 models stream; 2.5 models return one event.
- **Speed and text.** There is no speed parameter: tempo is changed locally
  (`:100`). Audio tags are stripped (`:63`).
- **Errors.**
  - 429 raises `TTSRateLimitError` (`:87-88`).
  - httpx timeout 30 s, keepalive pool of 4 with 300 s expiry (`:38-41`).
  - Up to 3 retries per chunk (`service.py:211,1603-1648`).
- **Chunking.**
  - Sentences split on `[.!?;:]` into growing chunks: 60 characters,
    doubling, capped at 520, at most 12 chunks (`service.py:1500-1545`).
  - The head chunk plays while a tail producer synthesizes the rest
    (`:1761-1826`).
- **Same voice with Live.** With both Gemini TTS and Gemini Live selected,
  Live speaks natively in the TTS voice (`gemini.py:108-116`).
- **Latency.** No figure is recorded in V1.

## 3. Gemini Live through the proxy

- **Endpoint precedence.** `HAL_GEMINI_LIVE_BASE_URL` > `realtime.base_url` >
  `<llm_base_url>/ws/gemini`.
- **Key precedence.** `GEMINI_API_KEY` > `GOOGLE_API_KEY` >
  `realtime.api_key` > `llm_api_key` (`hal/config.py:732-742`).
- **The full URL.**
  - V1 passes the base to `genai.Client(api_key, http_options.base_url)`
    (`gemini_live.py:98-101`).
  - The SDK dials `wss://…/ws/gemini/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent`
    with `x-goog-api-key`.
  - This full URL is derived from the SDK, not written in V1. A bare
    `llm_base_url` fails with 404 at the handshake
    (`docs/realtime-voice.md:3498-3502`).
- **Models.**
  - HAL default: `gemini-3.8-live`, Kore, 16 kHz input
    (`hal/config.py:745-747`).
  - os-server pushes `gemini-3.8-live-extended-thinking` with thinking LOW
    (`system/server/config/realtime.go:73-75`).
- **What the proxy changes.**
  - It does not forward session resumption, so V1 turns resumption off
    (`hal/config.py:754-757`).
  - It never answers pings, so V1 sets `ping_timeout=None`
    (`gemini_live.py:102-108`).
- **At the limit: unknown.** V1 has no Gemini close-code handling; "4029"
  appears only in comments (`gemini_live.py:136,1962`). Any non-1000 close
  fails the turn over to the main agent.
- **GPT-Live, for comparison** (`docs/realtime-voice.md:2091-2109`,
  `gpt_live.py:79-84`):
  - HTTP 401 means no key. Close 4001 = no key, 4002 = not configured,
    4029 = over the limit ("GPT-Live usage limit reached", possibly
    mid-session).
  - The relay swaps in its own credential, meters usage, and checks the
    device limit at connect and after every usage row.

## 4. Hermes API (no longer planned for V2; kept for reference)

- **Server and auth.** `http://127.0.0.1:8642`,
  `Authorization: Bearer hermes-local-api-key`, model `hermes-agent`
  (`runtimes/hermes/constants.go:5-8`).
- **Capabilities.** `GET /v1/capabilities` must report `run_submission`,
  `run_events_sse`, `run_status`, `run_stop` and `run_steer`, plus
  `runs_idempotency.autonomous_run_events_v1`, which comes from an OS patch
  (`runs_transport.go:57-82`).
- **Runs.**
  - `POST /v1/runs` with `{input, model, session_id?, instructions?}`, which
    returns `run_id`.
  - Steer: `POST /v1/runs/{id}/steer` with `{"input":…}`. Stop:
    `POST /v1/runs/{id}/stop`.
  - Events: `GET /v1/runs/{id}/events` (SSE): `message.delta`,
    `tool.call.started` / `completed`, `run.completed`, `run.failed`,
    `run.cancelled`, `run.interrupted` (`runs_transport.go:120-296`).
- **Fallback.** `POST /v1/responses` with
  `{model, conversation, stream:true, input}` (`client.go:21-28,55-88`).
- **Model config** (`presync.sh:40-47,84-115`):
  - provider `custom:autonomous`, model `Auto-AI`, `api_mode:
    anthropic_messages`;
  - base `https://campaign-api.autonomous.ai/api/v1/ai`, key
    `AUTONOMOUS_API_KEY` from `llm_api_key`;
  - 1 h prompt cache, `context_length` 256000, vision `qwen/qwen3.6-plus`.
- **Sessions.** `device-main-<bootUnix>`, rotated at 40 turns or 250k tokens
  (`rotation.go:11-49`).

## 5. How each service signals a limit, and what V1 says

- **LLM.**
  - No status code: os-server matches banner text in the reply ("reached the
    usage limit", "upgrade for higher limits", "your access will reset",
    "upgrade your plan"; `system/server/agent/delivery/http/handler_tts.go:20-36`).
  - It plays the cached "[sigh] I've hit my usage limit." with a 30 s
    debounce (`:219-236`; `system/lib/i18n/phrases.go:18-19,148-153`).
- **Speech synthesis.**
  - HTTP 429 raises `TTSRateLimitError` (`backend.py:18-25`).
  - V1 plays "I'm out of voice quota — check my plan." at most once per
    300 s, if cached (`hal/i18n.py:43,152-158`).
  - The cache is prerendered at boot (`service.py:38-40,1960-1979`;
    `runtime.py:661-668`).
- **STT.** Nothing is spoken; errors are only logged.
- **Perception.** A 429 pauses detection (`fire_hazard.py:28,146`).

## 6. lampOS today

- **Configuration.**
  - A private JSON with `endpoint`, `model`, `voice`, `thinking_level?`,
    `language_code?`, `credential_file` and `authentication`
    (`api_key` / `bearer` / `ephemeral`) (`crates/live/src/config.rs:12-33`).
  - Headers: `x-goog-api-key`, `Authorization: Bearer` or
    `Authorization: Token` (`crates/gemini/src/config.rs:18-26`).
- **Endpoint.** A full WSS URL. The default `GOOGLE_ENDPOINT` is direct
  Google. Pilots go through campaign-api, with the URL on the device only.
- **Limit handling.**
  - Close codes 4001, 4002 and 4029 map to `Unavailable`
    (`crates/gemini/src/lib.rs:424-430`).
  - Close reasons about quota, resource exhaustion, usage or rate limits map
    to `QuotaExceeded` (`session.rs:1946-1966`).
  - HTTP 401, 403 and 429 at the handshake map to authentication and rate
    limit (`session.rs:1985-1993`).

## Gaps

- V1 records no Gemini close code or reason at the limit.
- The full campaign-api Live URL is inferred from the SDK.
- The relay's accepted Live models are not in the repo.
- Whether STT, speech synthesis and the text model share the Live budget is
  unknown.
