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
  every 5 s. Only final segments are kept. At the end of the line, when
  the words are already settled (final segments cover all the audio sent,
  and no interim words or `SpeechStarted` came after them), they are the
  transcript at once (`Transcript::settled`). An earlier `speech_final`
  does not settle them: it marks a pause, and audio sent after it may
  still have words on their way ("set a timer" without "for twenty
  minutes", #878). Otherwise `{"type":"Finalize"}` asks for
  the final words at once, keeping the socket: final segments are taken
  until they cover all the audio sent, for up to 1 s (`FINALIZE`). An
  answer marked `from_finalize` that falls short of the audio sent does
  not end the line: `CloseStream` goes at once and its words are waited
  for. On lamp-52e6 (2026-10-16, #828) audio sent faster than speech, a
  held burst, got such an answer: "Hang" for "Hang on." (0.30 of 0.59 s)
  and "Lamp," for "Lamp, stop."; with `CloseStream` after it, 16 of 16
  rounds had the whole phrase, its segment starting where the answer
  ended, about 0.25 s later than the answer. If 1 s passes with no
  covering finals, `CloseStream` and up to 15 s in all for its final
  words, as before. `CloseStream` is sent in
  every case to close the socket, but waited on only in that fallback.
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
  own end of an utterance comes after 300 ms of silence and would answer a
  person still thinking. But when final segments covering all the audio
  came before the line ended, the words held are the
  transcript, and the chat starts as soon as the end of the line does.
  Otherwise `Finalize` asks for the final words and they are taken as
  soon as they come, up to 1 s; a proxy that does not answer it falls back
  to `CloseStream`'s answer, up to 15 s in all. Host tests on the paused
  clock (`net/stt.rs`), against a scripted 800 ms `CloseStream` round
  trip: 0 ms to the words with finals covering the audio before the end,
  0 ms with an at-once answer to `Finalize`, 50 ms + 800 ms when that
  answer is short of the audio sent (the rest from `CloseStream`), and
  1 s + 800 ms when `Finalize` is ignored. Two more hold the whole line in both arrival
  orders: an utterance ended before more audio was sent, and an older
  final arriving after `Finalize` and before its answer. On lamp-52e6 (2026-10-15 23:30, four played questions) the
  company proxy passed `Finalize`: every turn took `words from=finalize`,
  and the last words came 246 to 321 ms after the line's end, against 564
  to 662 ms with `CloseStream` at 23:00. Why a turn waited is
  journaled once, without words, when the words are in: "lamp-voice second: request N at the line's end:
  settled=<bool>, speech_final=<bool>, finals cover <ms|none> of <ms> ms
  sent, pending=<none|interim D ms after the last final|speech_started D
  ms after the last final>, last final <T ms before End|none>, words
  from=<settled|finalize|close_stream>", the last naming which path gave
  the words. D is audio time from the last final's end to the pending interim's end or
  `SpeechStarted`'s timestamp (negative: inside audio a final already
  covers); T is wall time since that final arrived.

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
  heard and how long after the line, the state of the words at the line's
  end (above), each chat round's first event (the first text or tool
  calls: a role-only chunk carries neither and is not an event), the
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
- **Back to Gemini:** silently, at the next idle moment with Gemini Ready,
  or at the start of the next turn if the room never goes quiet. Nothing
  is said about it.

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
Since then (#779) speech to text's last words are asked for with
`Finalize` at the line's end. On lamp-52e6 at 23:00 the journal showed that
no final segment had arrived by the end of any of four lines (`finals cover
none`), even with `endpointing` at 300 ms, so the words always waited for
`CloseStream`'s final; with `Finalize` at 23:30 they came 246 to 321 ms
after the end, about 0.34 s sooner on every turn. End of the line to the
first sound was then 3.4, 4.4, 6.0 (a tool round) and 2.8 s: the rest is the
chat through the relay, 1.0 to 2.7 s to its first words, whose own spread is
larger than anything left in this code. From the lamp, 20 requests each,
the relay's `gpt-4.1-mini` took 1.1 to 1.2 s (p50) to its first words, with
the 9.6 KB persona and tools no slower than one line.

The targets, as revised by the first builder on #779 for a voice used only
while Gemini is down:

- End of the line to the first sound: p50 ≤ 2.5 s, p95 ≤ 4 s.

Not yet met on lamp-52e6 (median about 3.9 s at 23:30). The relay's speed
and a direct provider key are the owner's and the relay team's.

A switch on a response timeout cannot meet the first (above).

## Timing the first words (#828)

`lamp-voice stt-check CONFIG WAV [ROUNDS] [--held-ms MS]` streams a recorded
line (16 kHz mono 16-bit) to this route's speech-to-text, as `CONFIG` (the
file `LAMP_VOICE_SECOND_CONFIG` names) sets it up, and prints one line a
round:
- `connect_ms`: opening the stream;
- `first_words_ms`: from the first audio queued to the stream to the first
  result with words parsed from it, interim or final
  (`SttStream::first_words`);
- `audio_ms`: how long queueing the line took;
- `final_after_audio_ms`: from the line's last audio queued to its final
  words;
- `words`: the final words.

What it times is the client's queue to a parsed result. That isn't the
moment audio leaves on the wire, and it isn't how long a stop takes on a
lamp, which also has the pause, the decision and the speaker.

The first `MS` of the line go at once as one message, standing in for a
paused reply's held audio (#828, step 2: a stop word heard during the
pause). The route sends 20 ms frames, and a pause that sends its held audio
the route's way would send those frames back to back. The rest of the line
goes at the pace it was said, in 20 ms frames. Three rounds and nothing
held, unless said; at most 20 rounds and 1,600 ms held. It reads the
credential file the config names, so on a lamp it runs as the voice's user
or root.

The sixth builder measured the relay with it on lamp-52e6 on 16 October,
with synthetic voices (#828, 08:22). No words come before a line ends: the
first came 0.22 to 0.30 s after its audio ended, and 0.24 to 0.38 s after a
600 ms burst. A burst lost the end of "Hang on." and "Lamp, stop.", which
came from `Finalize`, fixed in #956.

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
  still ends the service).
- **A new turn on the second route** goes to Gemini when Gemini's link is
  Ready, before anything of it is sent (`second::flush`): its queued
  commands are rebound to Gemini's session, recorded as
  `voice_route_switched` to `live`, `live_link_ready`, with the turn.
  Between turns is not the only way back: on lamp-4ace (2026-10-16
  09:03–09:06, #960) a busy room left no idle tick, and every turn went
  to the second route with Gemini up.
- **At startup**, before Gemini's first Ready of this voice process, the
  second route only stands in (that swap and the swap back are not
  recorded): a request admitted on it is held, nothing of it sent, for up
  to `STARTUP_HOLD_US` (lampOS#779). If Gemini's link is Ready in that
  time the turn is rebound to Gemini's session and its whole line goes
  there; if not, or once Gemini's link has ended, the second route is given
  the whole line, as a switch, even when Gemini's Ready comes in the same
  tick. The hold is derived from Gemini's audio age bound
  (`lamp_gemini::MAX_INPUT_AGE`, 2.5 s) less the line's lead-in (the
  detector's 30-block pre-roll, 300 ms) and a 300 ms margin: 1.9 s. A line
  held longer would replay a first block older than the bound, which
  Gemini's worker refuses as stale, and the worker ends (lampOS#892). It
  was 3 s when the four lamp-52e6 runs of PR #885 were measured: runs 1, 3
  and 4 (held 1.0 to 1.2 s) fall inside the new hold; run 2 (held 1897 ms)
  is at its edge and would now go to the second route if held 3 ms
  longer. One hold only: a request after that goes as
  at rest. A held turn cancelled and replaced before the next tick's
  upkeep gives up its hold there and then, so its replacement is held
  afresh rather than sent to the second route. After Gemini's first Ready
  nothing is held and an outage switches at once, as above: the first
  Ready is latched as it is observed (`CloudAvailability::was_ready`), so a
  Ready and its loss in one control drain still count, and a hold then in
  hand goes to the second route at once.
- **Mid-turn**, while Gemini is active, nothing of its answer was heard and
  the line was kept whole: a `TurnFailed` of that turn, or a link change
  during the line (the request is marked `movable`, so the gate leaves it
  for the switch). The turn is rebound to the second route's session and
  its line queued again: Start, every sent block from 1, the unsent ones
  (a held line's replay tail not yet sent to Gemini included), then what
  is queued, End among them. A reminder turn is not moved.
- **The line** (`second::Line`) is what the one send site sent Gemini for
  the turn in hand: in memory only, never on disk or the bus, at most 30 s
  of 16 kHz mono (beyond that it is dropped and the turn is not moved),
  cleared when the line is answered or abandoned and at once when an
  authority without its owner or with the microphone closed is published
  (mute, privacy, cancellation).
- **Pace.** A replay to the second route goes eight commands a tick,
  before what was queued since; the second worker takes eight inputs a
  tick, as the Gemini worker does, and holds 30 s of input while its
  speech-to-text connects. A held line given to Gemini goes two commands a
  tick (`LIVE_REPLAY_PER_TICK`, about ten times the pace of speech), with
  what is queued since joining it: Gemini's link takes input into a queue
  of 32 (`lamp_gemini::INPUT_QUEUE_CAPACITY`) and gives the connection up
  as `Backpressure` (reported `local`) when it is full. Sent at eight a
  tick, a line held 1.9 s overran it 37 ms after Gemini's first Ready on
  lamp-52e6 (PR #885 run 2, lampOS#892), and the second route answered.
  The pace alone does not hold through a write that stalls: a 90 ms stall,
  within the link's 200 ms write bound, still filled the queue at two a
  tick. So, with a second route configured, the coordinator gives Gemini's
  provider worker `--wait-for-room`, and the worker offers a command only
  when the queue has room (`InputSender::has_room`); one that finds it full waits, first in
  line, with nothing more read from the coordinator, for up to
  `ROOM_WAIT_US` (250 ms, past the write bound), and then goes, a queue
  still full being the link's failure as before. What the coordinator
  sends meanwhile waits in its own socket and queue, in order. The End a
  cancellation owes an open request waits for room the same way (the
  retirement itself stays immediate), and nothing of the next request is
  offered before it. This changes how Gemini's worker meets a full queue
  for all its input, not only a replay, but only on a lamp with a second
  route.
- **Without a configured route** `Rig::second` is `None`, the three
  intercepts (the send site, the output receipt, the gate's `movable`) are
  the old calls, Gemini's worker is not given `--wait-for-room` (a full
  queue ends its connection as it always has), and nothing else changes.
- Trace: `voice_route_switched` (`to`, `why`, `turn`, `replayed_commands`,
  and the failure's `stage` and `reason`); `voice_route_held` (`turn`,
  `held_ms`, `to`: `live` or `second`) when a startup hold ends; the standby's startup link
  record carries `"route":"second"`; an Ended of the standby, `"standby":true`;
  `voice_route_dropped` (`status`, `active`) when its worker exited.
- **Held up, the second worker goes on** (#960): authority that lapsed in
  its queue while it was held up (lamp-4ace's SD/MMC stalls, #784) is
  skipped, as the Live worker does, and the controller's heartbeat is
  judged once the queue is empty (or after 1.5 s of backlog). Before, a
  stall while finishing a turn ended it with "second route privacy or
  input lease expired", and the voice with it (lamp-4ace, 2026-10-16
  08:47).
- **A second worker that exits is dropped** (#960): checked once a tick
  before anything else of the tick touches it (`second::dropped_if_gone`).
  Standing by, the route goes and Gemini goes on. Active, Gemini's worker
  is put back first and the turn the route held ends
  (`second_route_failed`) as a turn the provider interrupted does: its
  unsent input and unwritten speech go, and the rest of a line still
  being said is drained rather than taken for a new request; the next
  turn is Gemini's. The 50 ms health check leaves the second worker to
  this. Recorded as
  `voice_route_dropped` (`status`, `active`). An authority that cannot
  reach it is not an error either. A worker that dies inside the very
  tick that touches it still ends the voice, as before.

## Not done

- **Gemini Terminal** (a wrong key or configuration) still ends the voice
  service as before. The second route covers Unavailable and failed turns
  only.
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
- **Short sounds, by words alone:** with `LAMP_SHORT_SOUNDS=quiet-room-checked`
  and `LAMP_ATTENTION` on, a turn whose end is marked as a short sound let
  through is answered only when what was heard is, whole, a short word said
  to the lamp; otherwise it ends after its words, unanswered (#800,
  [not-speech.md](not-speech.md#a-room-sound-let-through-and-answered-800)).
  Being close to the lamp or facing it cannot be seen here, so it does not
  count as it does on the Live route.
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
the production `conversation()` with peer processes on real local IPC,
stepped on the scenario clock so a busy host cannot age a lease
(lampOS#890): without a route nothing of it is seen and a route standing by leaves
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
