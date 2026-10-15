# The second voice route

`crates/voice/src/second_config.rs`, `crates/voice/src/second_provider.rs`
and `second_provider/{net,services,worker}.rs`. Issue
[#754](https://github.com/autonomous-ai/lampOS/issues/754). Status on
2026-10-15: host tests only. The coordinator switch is being written
(below). Nothing here has run on a lamp.

## Why

Step 11 of [brain-build.md](brain-build.md) is the second speech route:
speech to text, a text answer and speech synthesis, for when Gemini Live
cannot answer. Without it a refused quota or a Google outage leaves the
lamp with the "voice is down" state and nothing to say.

The route is a provider worker of its own, `lamp-voice worker second` (role
`second`). It speaks the same `ProviderInput` / `ProviderOutput` wire as the
Gemini worker and the fixture, so the coordinator treats it as one more
provider. Off unless configured: with no second-route file, nothing about
the voice service changes.

## The configuration

A private JSON file, named by `LAMP_VOICE_SECOND_CONFIG` in the voice
service's environment (`SECOND_CONFIG_ENV`). Unset or empty: the route is
off. The coordinator passes the path to the worker as
`worker second DIR PARENT WORKER PROVIDER_CONFIG SECOND_CONFIG [flags]`,
where the flags are the Gemini provider worker's, less `--keep-link`.

The field names are V1's (`system/domain/device.go`, `hal/models.py`), so
whatever carries V1's settings over can copy them. The one change: each
`*_api_key` becomes a `*_credential_file`. A key written in the JSON is
refused (unknown fields are refused).

| Field | Required | What |
|---|---|---|
| `llm_base_url` | yes | OpenAI-compatible base URL of the text model |
| `llm_model` | yes | model name, not empty |
| `llm_credential_file` | yes | file holding the text model's key |
| `stt_base_url` | no | the company's speech-to-text proxy; empty or absent: `llm_base_url` |
| `stt_credential_file` | no | its key; empty or absent: `llm_credential_file` |
| `stt_language` | no | sent as `language`; absent: `en` |
| `deepgram_credential_file` | no | when set, speech to text goes straight to Deepgram with this key, and the two `stt_*` above are not used |
| `tts_base_url` | no | empty or absent: `llm_base_url` |
| `tts_credential_file` | no | empty or absent: `llm_credential_file` |
| `tts_provider` | no | `openai` (the default) or `elevenlabs`; anything else is refused |
| `tts_voice` | no | the voice; absent: OpenAI `nova`, ElevenLabs `Rachel` |

The fallback is V1's: an empty or absent STT or TTS URL or key is the LLM's.

Rules the loader keeps (`load_from_path`):

- The JSON file and every credential file are bounded regular files owned
  by the service's user, mode 0600: a symlink or group or world access is
  refused, as for `provider.json` ([live-runtime.md](live-runtime.md),
  "Private configuration and commands"). The JSON is at most 16,384 bytes.
- A credential path must be absolute and the file not empty. Its trailing
  line ending is dropped. A key is never shown by `Debug`, and no error
  carries a key, a URL or a server's words.
- Every URL must be `https://` with a host, at most 2,048 bytes, with no
  `?`, `#` or `@`. HTTP redirects are not followed.

An example, with the company's endpoints:

```json
{
  "llm_base_url": "https://campaign-api.autonomous.ai/api/v1/ai",
  "llm_model": "<model>",
  "llm_credential_file": "/etc/lampOS/voice/second.key"
}
```

The first builder writes lamp-4ace's file from the company's endpoints.
Who writes it at onboarding is not built (below).

## What each client speaks

As V1's HAL speaks them (`hal/drivers/voice/stt/*`, `hal/drivers/voice/tts/*`).
Every connect, TLS included, is bounded at 10 s; a streamed response that
says nothing more for 30 s is a timeout. TLS is rustls with the `ring`
provider and the webpki roots, as `lamp-gemini` has it.

A base URL on `campaign-api.autonomous.ai` ending in `/ai` is given `/v1`
(V1's `_ensure_openai_v1`); any other is used as it is, less a trailing `/`.

- **Speech to text** (`net/stt.rs`): Deepgram's websocket protocol, with
  `Authorization: Token <key>`. Straight to `wss://api.deepgram.com/v1/listen`
  when `deepgram_credential_file` is set, otherwise to the proxy at
  `<base>/ws/audio/transcriptions` (the base made `wss://`). Query, V1's for
  nova models: `model=nova-3-general`, `encoding=linear16`,
  `sample_rate=16000`, `channels=1`, `language`, `smart_format=true`,
  `interim_results=true`, `endpointing=1500`, `vad_events=true`. A keep-alive
  every 5 s; `CloseStream` at the end of the line, and up to 15 s for the
  final words. Only final segments are kept.
- **Text model** (`net/chat.rs`): `POST <base>/chat/completions`,
  `Authorization: Bearer <key>`, `"stream": true`, with `tools` when any are
  declared. The answer is read as server-sent events and cut into
  sentences as it arrives (`net/sentences.rs`).
- **Speech** (`net/tts.rs`), 24 kHz mono 16-bit PCM:
  - `openai`: `POST <base>/audio/speech`, `Authorization: Bearer <key>`,
    model `tts-1`, `response_format: pcm`.
  - `elevenlabs`: `POST <base>/text-to-speech/<voice id>/stream?output_format=pcm_24000`,
    `xi-api-key: <key>`, model `eleven_v4_turbo`, speed 1.0. A base that is
    not an ElevenLabs host (`api.elevenlabs.io`, `api.us.…`, `api.eu.…`)
    gets `/elevenlabs` first, as V1's proxy path. A V1 voice name
    (`VOICE_IDS_BY_LANG`) is turned into its id; any other value is taken as
    an id.

  Text in `[...]` is not spoken. A sentence with nothing left to say sends
  no request.

A service's answer becomes the turn's failure reason (`fault`): 401 and 403
Authentication, 402 QuotaExceeded, 429 RateLimited, 5xx ServerUnavailable,
other statuses ServerRejected; a connect timeout ConnectTimeout, any other
timeout ResponseTimeout.

## A turn on the second route

`Started` at once, and speech to text opened while the person is still
talking. Up to 15 s of microphone audio waits while it connects. At the
end of the line: the person's words, then the chat answer, each sentence
said as it is complete while the next is still being written. Sentence
order is kept. A written turn is the same from the chat on.

- Up to four chat requests in a turn, tool rounds included (`MAX_ROUNDS`).
- A call to `not_for_me` ends the turn silently, before the unfinished last
  words are said.
- A due reminder is deferred (`ReminderDeferral::Disabled`): this route
  does not speak the lamp's own requests.
- A failure is `TurnFailed` at the stage the coordinator accepts: InputOpen
  while the line is coming, AwaitingResponse before any answer audio,
  Responding after.

## What a person hears

When the coordinator switches (being written, below):

- **Switched at once:** a turn that fails with a quota, a refusal, a rate
  limit or a server rejection, or a link that drops during the line.
- **Switched on a response timeout:** not sooner than Gemini's
  `response_timeout_ms`, which cannot go below 2 s (`config.rs`). A turn
  switched this way cannot meet the 1.2 s target below. With
  `no_reply_after_ms` set, slowness ends as Unanswered, which does not
  switch.
- **Nobody repeats themselves:** the line is replayed to the second route
  from its start.
- **The lamp never answers twice:** Gemini's late answer to that turn is
  dropped.
- **One sentence:** "My voice sounds a little different just now." is said
  before the first answer when the route has not answered in the last 120 s
  (`ANNOUNCEMENT`, `ANNOUNCE_AFTER`), because the voice cannot be Gemini's.
  It is not kept in the thread.
- **Back to Gemini:** silently, at the next idle moment with Gemini Ready.
  Nothing is said about it.

## Same character, memory, thread and tools

`worker::serve`:

- **Character:** the instruction of the Live route's `provider.json`
  (`ProviderConfig::instruction`, the version without a camera), then the
  time and place as a Live session is told them, then the memory note (the
  Days note frozen for the process, or the memory writer's, read for each
  turn), then the conversation thread in the room the rest leave of
  16,384 bytes. Built when each turn's chat begins.
- **Thread:** the same file as the Gemini worker (`--thread-file`).
  Before each turn the route takes the file if Gemini wrote it since, and
  after each turn it writes the file whole. Gemini does the same at each
  connect. The rule is in [conversation-thread.md](conversation-thread.md),
  "Two voice routes, one file". The memory-forget path is honoured first.
- **Journal:** each completed turn goes to the memory's journal as the
  Live route's do (`remember::finished`).
- **Tools:** the same declarations (`tools::declared`), mapped to the
  OpenAI `tools` schema. Each call runs through `tools::answer` and
  `tools::run_call`, the one function the Gemini worker also uses
  (`ToolDesk::called`), so the light, schedule, memory, tasks and eyes
  behave the same. The Gemini worker's log line ("request N asked for X;
  the tasks part answered in N ms; ...") is unchanged; this route logs
  "lamp-voice: request N asked for X; the tasks part answered" (or "did not
  answer").
- `LAMP_MEMORY_DAYS=on` reaches this worker as it reaches the Gemini one
  (`process.rs`).

## What the rest of the lamp sees

The route's audio goes through the same speaker worker, so the echo
canceller and "spoken over: it stops" behave as now. The ring, the body and
Expression follow the same provider states. Its turns are journalled like
any other. The first builder asked for this to be shown with the fixture
provider's tests; that showing comes with the coordinator switch.

## The trade-off

With the company's defaults the route goes through the same relay as
Gemini. It then covers a Google-side outage, quota or refusal, not an
outage of the relay. A lamp with its own keys set (its own endpoint, or a
`deepgram_credential_file`) is covered for both. This is the intended
default.

## Latency targets

Agreed on the issue, from the end of the line to the lamp's first sound.
**Not yet measured on a lamp.**

- First turn after a switch: the sentence starts within 1.2 s, and the
  answer follows with no gap longer than 0.5 s.
- Steady turns on the second route: p50 ≤ 1.5 s, p95 ≤ 2.5 s.

A switch on a response timeout cannot meet the first (above).

## Coordinator switch

**(being written; see the pull request)**

The design agreed on the issue:

- A standby worker, `second`, beside the Gemini one, with its own link
  register and output flow. When Gemini is not Ready, the second route is
  Ready and no turn is in flight, the two are swapped, so every existing
  provider path runs unchanged. The standby's output is drained and
  dropped, which is how a late Gemini answer is lost.
- A buffer of the current line, at most 30 s of 16 kHz mono, in memory
  only: never on disk or the bus. Cleared when the line is answered or
  abandoned, and at once on mute or privacy.
- The intercepts that replay the line act only when a second route is
  configured. Without one, the coordinator's packets and records are
  byte for byte as before.

## Not done

- **Gemini Terminal** (a wrong key or configuration) still ends the voice
  service as before. The second route covers Unavailable and failed turns
  only.
- **Onboarding writing `second.json`:** waits on the owner's choice of how a
  lamp is set up. Not built.
- **`gemini` and `piper` speech:** `gemini` fails with Gemini; `piper` on
  the device is a later piece.
- The services are not probed at startup: the link says Ready and a service
  that cannot be reached fails that turn (`ponytail:` in `worker.rs`).

## Open limits

Reported by the builders:

- **Facts not told:** the worker accepts the Gemini worker's flags, but
  sight, `look`, the camera, attention, the session fact, room facts and air
  facts are not told to the text model. A written turn's `look` is ignored.
- **Tools are declared once,** when the worker starts.
- **Deepgram direct uses nova-3 on `/v1/listen`.** V1's default
  (`hal/drivers/voice/stt/deepgram.py`) is `flux-general-en` on Deepgram's v2
  protocol.
- **Simplifications marked `ponytail:`:**
  - sentences: a fixed English abbreviation list and "stop then
    whitespace"; quotes after the stop, ellipses and non-Latin stops are not
    handled;
  - the chat stream: an end without `[DONE]` counts as one, so a cut stream
    looks finished;
  - speech: every `[...]` is dropped, where V1 drops only its ElevenLabs
    emotion tags;
  - a model still calling tools after four rounds ends the turn with what
    it has said.

## Verification

Host tests: `second_config::tests` (the fallback, Deepgram chosen by its
file, an inline key refused, an unknown `tts_provider` refused, a readable
credential refused, plain `http` refused); the clients' tests on recorded
bytes and a local server (`net/*`); the engine's (`second_provider/tests.rs`);
the worker's authority and a turn kept in the thread, its file and the
journal (`worker.rs`); the thread reload (`thread::tests`).

Not shown: anything on a lamp, the switch, and the latency targets.
