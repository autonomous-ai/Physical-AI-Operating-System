# The second voice route

`crates/voice/src/second_config.rs`, `crates/voice/src/second_provider.rs`
and `second_provider/{net,services,worker}.rs`; the switch in
`crates/voice/src/coordinator/second.rs`. Issue
[#754](https://github.com/autonomous-ai/lampOS/issues/754). Status on
2026-10-15: host tests, the switch included, and four played runs on
lamp-52e6 with Gemini made unreachable (the last kept in
[verification](verification/754-lamp-52e6-20261015/README.md)): the
second route answered eight of eight questions there. A switch mid-line and
Gemini's return have run on the host only.

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

Two of V1's values do not carry over as they are. Read from lamp-52e6's V1
`config.json` on 2026-10-15 (endpoints only):

- `llm_model` is `Auto-AI`, which the company relay serves only on its
  Anthropic-messages route; its `/chat/completions` answers 404 for it. Set
  an OpenAI chat model there instead: `gpt-4.1-mini` answered through the
  relay (first byte 0.32 s from the lamp, one probe).
- `tts_provider` is `gemini`, which this route does not take (it would fail
  with Gemini). Set `elevenlabs`. The relay's OpenAI `/audio/speech` answers
  only once the whole sentence is made: "Paris!" took 2.4 to 5.5 s to its
  first audio, a 4 s sentence 5.0 s (`tts-1`; `gpt-4o-mini-tts` 2.2 s). Its
  ElevenLabs path streams: 0.60 to 0.68 s to the first audio for the same
  sentences (`eleven_flash_v2_5`; probes from lamp-52e6 on 2026-10-15).

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
  `interim_results=true`, `endpointing=300` (V1's was 1500; see Latency
  targets), `vad_events=true`, and
  `keyterm=Lamp`, as V1 boosts the lamp's name: without it, on lamp-52e6,
  "Hey Lamp" came back as "Palin", "Palan" and "Len", and the model took
  the question for other people's talk. A keep-alive
  every 5 s; `CloseStream` at the end of the line. Only final segments are
  kept. When the words are already settled at the end of the line (the
  last final segment had `speech_final`, or final segments cover all the
  audio sent, and no interim words or `SpeechStarted` came after it), they
  are the transcript at once; otherwise up to 15 s for `CloseStream`'s
  final words, as before (`Transcript::settled`).
- **Text model** (`net/chat.rs`): `POST <base>/chat/completions`,
  `Authorization: Bearer <key>`, `"stream": true`, with `tools` when any are
  declared. The answer is read as server-sent events and cut into
  sentences as it arrives (`net/sentences.rs`).
- **Speech** (`net/tts.rs`), 24 kHz mono 16-bit PCM:
  - `openai`: `POST <base>/audio/speech`, `Authorization: Bearer <key>`,
    model `tts-1`, `response_format: pcm`.
  - `elevenlabs`: `POST <base>/text-to-speech/<voice id>/stream?output_format=pcm_24000`,
    `xi-api-key: <key>`, model `eleven_flash_v2_5` (V1 used
    `eleven_v4_turbo`, 0.81 to 1.30 s to its first audio through the
    relay), speed 1.0. A base that is
    not an ElevenLabs host (`api.elevenlabs.io`, `api.us.…`, `api.eu.…`)
    gets `/elevenlabs` first, as V1's proxy path. A V1 voice name
    (`VOICE_IDS_BY_LANG`) is turned into its id; any other value is taken as
    an id.

  Text in `[...]` is not spoken. A sentence with nothing left to say sends
  no request. Every sample is doubled (+6 dB) and clipped at full scale:
  through the relay `tts-1` `nova` measured peak -7.3 dBFS and RMS -28.8
  dBFS, ElevenLabs Rachel peak -6.4 and RMS -23.5, and the second voice was
  heard much quieter than Gemini's.

A service's answer becomes the turn's failure reason (`fault`): 401 and 403
Authentication, 402 QuotaExceeded, 429 RateLimited, 5xx ServerUnavailable,
other statuses ServerRejected; a connect timeout ConnectTimeout, any other
timeout ResponseTimeout.

## A turn on the second route

`Started` at once, and speech to text opened while the person is still
talking. Up to 30 s of microphone audio waits while it connects, the
length of the coordinator's line buffer, so a replayed line fits. At the
end of the line: the person's words, then the chat answer, each sentence
said as it is complete while the next is still being written. Sentence
order is kept. A written turn is the same from the chat on.

- **The chat starts without `CloseStream`'s round trip when it can.** It
  never starts before the coordinator's end of the line: speech to text's
  own end of an utterance comes after 1.5 s of silence and would answer a
  person still thinking. But when that end, or final segments covering all
  the audio, came before the line ended, the words held are the
  transcript, and the chat starts as soon as the end of the line does.
  Otherwise the words wait for `CloseStream`'s answer, up to 15 s. Host
  tests on the paused clock (`net/stt.rs`): 0 ms to the words with a
  `speech_final` before the end, against the scripted 800 ms round trip
  without one. Not yet measured on a lamp.

- **The first words go early.** Until some of the answer is on its way, a
  clause of at least four words ending in a comma, semicolon or colon is
  said on its own (`Sentences::clause`).
- **The chat is given the person's last exchanges.** Up to six, each said
  within five minutes, as user and assistant messages after the system
  message: a Live session keeps its own, and "and how many people live
  there?" needs the one before (on lamp-52e6, without it, "there" became
  the lamp's own city). They are read from the thread (`Thread::recent`),
  under the same lock as its recall and after its forgetting and reload
  (`thread::current`), so they are only the person's at the lamp and never
  something forgotten. The route keeps no copy of its own.
- **A tool call knows whether it was heard.** The engine passes `heard`
  to the tool runner: true in a spoken turn, false in a written one, as the
  Live route does, so a yes to prepared work cannot come in a turn the lamp
  wrote itself (`agent_tasks::may_confirm`).
- **A written request stays with its answer as a note.** When the lamp
  begins a turn itself, the request's text is kept on that exchange in the
  thread (`Thread::written`, a `written` field in the file) and given to the
  next chat as a system note ("A note you were given in writing, not words
  anyone said: ..."), never as a user message. A person who said nothing
  sends no empty user message. A Live session's recall is unchanged: it is
  not read the note. While it waits for its answer the note is bound to its
  request and to the person the turn is answered for: only that answer
  takes it, an answer to a later request (the written one was retired or
  failed) drops it, and a forgetting drops it too.
- **Each turn is one person's**, as a Live session is since #775: before
  the chat, the thread is opened for whoever the eyes know at the lamp
  (`Sessions::open`), and that person's kept things are asked for. With
  Days memory, the note is asked once per run of turns for one person.
- **Each turn is logged** as "lamp-voice second: request N ...": the words
  heard and how long after the line, each chat round's first event, the
  tools called, the first answer audio, and any failure.

- Up to four chat requests in a turn, tool rounds included (`MAX_ROUNDS`).
- A call to `not_for_me` ends the turn silently, before the unfinished last
  words are said.
- A due reminder is deferred (`ReminderDeferral::Disabled`): this route
  does not speak the lamp's own requests.
- A failure is `TurnFailed` at the stage the coordinator accepts: InputOpen
  while the line is coming, AwaitingResponse before any answer audio,
  Responding after.

## What a person hears

When the coordinator switches (below):

- **Switched at once:** a turn Gemini fails before any of its answer was
  heard, whatever the fault (a quota, a refusal, a rate limit, a server
  rejection, a dropped connection), or a link that changes during the line.
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
  the tasks part answered in N ms; ...") is unchanged; this route logs the
  same words after "lamp-voice second:", ending "the answer was sent".
- `LAMP_MEMORY_DAYS=on` reaches this worker as it reaches the Gemini one
  (`process.rs`).

## What the rest of the lamp sees

The route's audio goes through the same speaker worker, so the echo
canceller and "spoken over: it stops" behave as now. The ring, the body and
Expression follow the same provider states. Its turns are journalled like
any other. Shown with peer processes in `coordinator/tests/second_route.rs`
(below): a turn the second route answers leaves the speaker the same
authorities, the ring the same phases and the trace the same finished-turn
record as one Gemini answers, and spoken over it stops the same way.

## The trade-off

With the company's defaults the route goes through the same relay as
Gemini. It then covers a Google-side outage, quota or refusal, not an
outage of the relay. A lamp with its own keys set (its own endpoint, or a
`deepgram_credential_file`) is covered for both. This is the intended
default.

## Latency targets

Agreed on the issue, from the end of the line to the lamp's first sound.
**Not met on lamp-52e6** (2026-10-15, eight played questions,
[verification](verification/754-lamp-52e6-20261015/README.md)): median 4.3 s
to the first write to the speaker, 3.0 to 3.6 s for a plain question with no
slow chat. The chat through the relay takes 1.2 s at best to its first
event, with spikes to 7.7 s; speech to text gives its last words 0.6 to 0.9
s after the line; speech starts 0.4 to 0.6 s after the chat's first event.
Since then (#779, step 1) the chat no longer waits for that round trip when
the words had settled before the line ended (see "A turn on the second
route"); how often that is so on a lamp, and the time saved, is not yet
measured. With V1's `endpointing=1500` an utterance's end needed 1.5 s of
silence, and the coordinator ends a line after 0.4 to 1.5 s of it
(`coordinator/endpoint.rs`), so a `speech_final` would seldom come first;
it is now 300 ms, so that it usually does. Whether that ever splits one
spoken line into two final segments in a way the chat reads badly is not
yet seen on a lamp.

- First turn after a switch: the sentence starts within 1.2 s, and the
  answer follows with no gap longer than 0.5 s.
- Steady turns on the second route: p50 ≤ 1.5 s, p95 ≤ 2.5 s.

A switch on a response timeout cannot meet the first (above).

## Coordinator switch

`coordinator/second.rs`. With `LAMP_VOICE_SECOND_CONFIG` set in the
service's (`serve`) environment and the Gemini source, `spawn_rig` starts
the standby `second` worker just before the privacy worker, with
`second::arguments`: the Gemini worker's arguments, the second
configuration after the first, less `--keep-link`. Its readiness is taken
and its authority kept fresh during the rest of startup.

- **One active route.** `Rig::second` holds the standby worker, its own
  link register and output credit. A switch swaps all three with
  `rig.provider`, `cloud` and the output flow, so every provider path runs
  unchanged against the other route. The standby gets every authority;
  its link reports go to its register; its output is drained, credited and
  dropped (how a late Gemini answer is lost).
- **At rest** (no turn in flight, nothing queued): to the second route
  while Gemini is not Ready and has not ended; back to Gemini, silently, on
  the first idle tick its link is Ready again or has ended (so Gemini's end
  still ends the service). At startup the second route is taken until
  Gemini's first Ready.
- **Mid-turn**, while Gemini is active, nothing of its answer was heard and
  the line was kept whole: a `TurnFailed` of that turn, or a link change
  during the line (the request is marked `movable`, so the gate leaves it
  for the switch). The turn is rebound to the second route's session and
  its line queued again: Start, every sent block from 1, the unsent ones,
  End if it was sent. A reminder turn is not moved.
- **The line** (`second::Line`) is what the one send site sent Gemini for
  the turn in hand: in memory only, never on disk or the bus, at most 30 s
  of 16 kHz mono (beyond that it is dropped and the turn is not moved),
  cleared when the line is answered or abandoned and at once when an
  authority without its owner or with the microphone closed is published
  (mute, privacy, cancellation).
- **Pace.** A replay goes eight commands a tick, before what was queued
  since; the second worker takes eight inputs a tick, as the Gemini worker
  does, and holds 30 s of input while its speech-to-text connects.
- **Without a configured route** `Rig::second` is `None`, the three
  intercepts (the send site, the output receipt, the gate's `movable`) are
  the old calls, and nothing else changes.
- Trace: `voice_route_switched` (`to`, `why`, `turn`, `replayed_commands`,
  and the failure's `stage` and `reason`); the standby's startup link
  record carries `"route":"second"`; an Ended of the standby, `"standby":true`.

## Not done

- **Gemini Terminal** (a wrong key or configuration) still ends the voice
  service as before. The second route covers Unavailable and failed turns
  only.
- **The standby is a voice worker:** if it exits or its channel fails, the
  runtime cycle ends as for the Gemini worker, not as an optional part.
- A link change in the very first tick of a line, before the line is
  marked `movable`, abandons the line as before.
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

The switch: `coordinator::second::tests` (the line's 30 s bound, its
request, its clearing on mute or a closed microphone and once answered or
abandoned, the worker arguments) and `coordinator::tests::second_route`,
the production `conversation()` with peer processes on real local IPC:
without a route nothing of it is seen and a route standing by leaves
Gemini's packets and the kinds and fields of every record unchanged; a
spent quota moves the whole line and the rest of the lamp sees the same
turn; a link dropped mid-line loses nothing; Gemini's audio after its
failure is never played; the next turn goes back to Gemini once Ready;
spoken over, the second route's answer stops as Gemini's does. The
comparison across runs is of kinds, fields and sequences, not bytes:
times and boots differ from run to run.

Not shown on the host: the real second worker behind the switch (the
tests' second route is a peer). On a lamp: a switch mid-line and Gemini's
return, and the latency targets, which lamp-52e6 did not meet (above).
