# Faster answers: what exists

Task 9 of the running list ([work-split.md](work-split.md)), by the second
builder, 2026-10-14. A study. Nothing is built, nothing was run on a lamp,
and no paid service was called. Everything here comes from the lamp's own
records already in this repository and from published pages, read on
2026-10-14 (the lamp's date; 13 October in the United States). A figure
from outside the repository is marked **published, not measured here**,
with who published it, when, and how it was measured where they say.

The boundary is the lamp's own ([conversation-latency.md](conversation-latency.md)):
from the person stopping to the first sound of the answer written to the
speaker. Half of the lamp's answers begin within 1.98 s; the target is 0.3
to 0.5 s. Published figures use other boundaries, and each one's is said.

## What exists and what we take

**In short.** The lamp's speech service, `gemini-3.8-live`, is published
at 1.18 s from a question to its first sound. The lamp sees 1.15 s. So the
road from Ho Chi Minh City and the relay are probably a small part of the
wait, and no change of road or lamp setting brings this service near 0.3
to 0.5 s. What the lamp can take off is its own: the 0.6 s it waits to be
sure the person has finished, and part of the 0.35 s it holds the first
sound. That gives about 1.5 s. Near 1 s needs a faster service as well,
which today means a different voice or a voice made separately, against
the owner's decision. No shipped voice agent measured
from the user's side was found answering within 1 s.

| Way | What it would save here | Evidence | Cost | What it does to the lamp | Take it? |
|---|---|---|---|---|---|
| 1. End the turn sooner when the sentence is plainly finished (task 1) | 0.3 s on turns judged finished, three in four to nine in ten finished turns in recorded clips by the bar used; nothing on the rest | The fast model's times on a VM, its agreement on 1,167 clips; an estimate for the lamp | Built. A 25 MB library. No money, no keys | More turns ended too soon, each a request given up and sent again, rarely heard. The model is weakest in Vietnamese (published) | **Yes, first** |
| 2. Send the end sooner, play only when sure | Up to 0.4 s on every answer (the end sent at 0.2 s of quiet in place of 0.6 s) | The lamp's records: no answer has arrived sooner than about 0.85 s after the end was sent, so holding it costs nothing. Two of three continuation probes answered rightly | Code in the voice, the first builder's. About one turn's input charged again for each request given up | Many more fragments sent: the name said alone must not be thrown away as a sound; more given-up turns in the model's memory of the talk | **Yes, after 1**, behind a switch |
| 3. A shorter hold before the first sound | About 0.1 s at 200 ms; up to 0.25 s with none | The lamp's records: the hold is most of its setting | A setting | More gaps in an answer's first second; the check that drops a wordless "..." answer needs its text before it plays | **Measure, then try 200 ms** |
| 4. The same service, reached directly | Not known; at most the relay's detour; probably small | No round trip from the lamp is recorded. Google says direct is "generally" better. The published time is about the lamp's whole wait | A Google key on each lamp, or short-lived keys from the company's server. The relay's limits and metering move | None on what it says. Sessions could resume, which the relay does not pass on, so the lamp would keep its thread | **Measure the road first** |
| 5. The same service, set differently | Up to about 0.5 s by published times of older models; nothing from the rest | Published, boundary not stated; other benchmarks order the models differently | Configuration; the same voice. The faster models are older, preview or retiring | The character file was tuned on 3.8 Live. Proactive audio, always on in 3.8 Live, is not supported in 3.1 Flash Live | **Not now** |
| 6. Another service that takes speech in and gives speech out | 0.2 to 0.7 s by published times (0.44 to 0.97 s against 1.18 s) | Published, boundaries mostly not stated. Measured from the user's side, shipped agents answer in 1.0 to 1.7 s | A new voice. An account and a key. A new provider part. OpenAI's prices are about ten times Gemini's per token | Character, staying out of talk and being interrupted all to be earned again | **No**, unless the owner reopens the voice |
| 7. Speech to text, a text model and a voice, in a row | Not shown to be faster from here: about 1 to 1.6 s after the end by published stage times, with three trips to the United States | Published stage times, all measured in the United States. One Vietnamese measurement on a lamp (V1): 524 ms to the final words | Three services and their keys; a whole new part. Gemini's speech models offer the same voice names (Kore); whether they sound the same is not published | The model hears words, not a voice: tone and hesitation are lost. In return the lamp reads the words before any answer is made, and can start one on a likely end and drop it | **Not now**; later, as the second route already planned |
| 8. What shipped voice agents publish | A yardstick, nothing to take | Third-party recordings put shipped agents at 1.0 to 1.7 s median; claims under 0.5 s state no method | | | Keep the lamp's boundary: it is the strict one |

The order of trials is at the end ("Recommended order of trials"). Its
first step changes nothing on the lamp: it reads what the lamp has
already recorded.

## Where the time goes today

**How the lamp talks to the service.** One secure WebSocket to Gemini
Live, kept open, through the company's relay (`campaign-api`; below).
Model `gemini-3.8-live`, voice Kore, thinking budget zero. Sound goes up
as 16 kHz 16-bit PCM in JSON, as the microphone delivers it in 10 ms
blocks (how many blocks go in one message was not checked); Google advises
chunks of 20 to 40 ms and no buffering
([best practices](https://ai.google.dev/gemini-api/docs/live-api/best-practices)),
and nothing found suggests the lamp holds sound back. The answer comes
down as 24 kHz PCM. The service's own detection of speech is off
(`automaticActivityDetection: {disabled: true}`): the lamp marks the start
and end of each utterance itself (`activityStart`, `activityEnd`), and a
new start interrupts an answer (`START_OF_ACTIVITY_INTERRUPTS`).
Transcripts of both sides are on. All of it is in `encode_setup`,
`crates/gemini/src/wire.rs`.

From the lamp's records on lamp-4ace (`--end-silence-ms 600`,
`--playout-start-ms 350`):

| Part | Whose | How long | Evidence |
|---|---|---|---|
| Waiting to be sure the person has stopped | The lamp's | 600 ms, fixed | The setting; 900 of the 1,034 answers ran with it |
| From the end sent to the service's first sound | The service's, and the road to it | 1.15 s at the median of 1,064 answers | [conversation-latency.md](conversation-latency.md) |
| From that sound arriving to the first sound written to the speaker | The lamp's | About 0.2 s, by difference | The first sound is held, at most 350 ms |
| All of it | | Half within 1.98 s, nine in ten within 2.81 s, the quickest 1.33 s | 1,034 answers, 2026-10-12 to 14 |

The 0.2 s is the difference between two medians over nearly the same
answers (1.98 s less the 0.6 s wait, against 1.15 s); the records hold it
answer by answer. Smaller samples agree. On 2026-10-11, two runs of eight
cached questions on lamp-4ace, with the hold set to 200 ms, gave medians
of 929 and 1,037 ms from the end decided to the service's first sound
(843 ms the least of the sixteen), and 1,123 and 1,214 ms to the first
sound written. The first run measured the hold itself: 180 ms at the
median, 94 to 396 ms
([installed-eight-turn-20261011](verification/installed-eight-turn-20261011/README.md),
[provider-control-20261011](verification/provider-control-20261011/README.md)).
The same day two probes from the lamp timed 955 and 960 ms from a
question's end being queued to its first sound received
([continuation-service-20261011](verification/continuation-service-20261011/README.md)).

What happens inside the 1.15 s cannot be seen from the lamp. It holds the
end of the question going out through the relay to Google, the model
reading the question and the conversation so far, making the first sound,
and that sound coming back. Five things are known:

- **The service adds no wait of its own.** The lamp ends each question
  itself (`activityEnd`; the service's own detection is off). Google:
  "The server acts immediately on your `activityEnd` signal with no
  additional wait" ([capabilities guide](https://ai.google.dev/gemini-api/docs/live-api/capabilities),
  updated 2026-09-18). With its own detection the service waits about
  800 ms of quiet by default, longer than the lamp's 600 ms.
- **Thinking is not it.** With the thinking budget at zero, the first
  sound was written 1.21 to 1.43 s after the end (five of five); without
  it, 1.19 to 1.51 s (four of five) ([live-runtime.md](live-runtime.md)).
- **Search is not it.** Ordinary questions began after medians of 1.70
  and 1.83 s with the service's search on, 1.77 and 1.73 s without.
- **The words heard come first.** The service's text of what it heard
  arrived before the first sound of each of 24 answers, by 0.32 s at the
  least and 0.44 s in the middle ([attention.md](attention.md)).
- **The conversation grows.** The prompt was 1,327 tokens on a session's
  first turn and 4,234 on its fourteenth. Whether the time grows with it
  has not been looked at. Google warns that shortening it ("compression")
  "will cause a temporary latency increase"
  ([API reference](https://ai.google.dev/api/live), updated 2026-10-09).

**The road.** The lamp is in Ho Chi Minh City
([brain-build.md](brain-build.md)). It reaches the service through the
company's relay, `campaign-api.autonomous.ai`. Looked up from the second
builder's laptop on 2026-10-14, that name answers from Cloudflare
addresses (104.20.29.45 and 172.66.158.217), so the lamp's first hop is a
Cloudflare edge. Where the relay's own server is, the repository does not
say. **No round trip from the lamp to the relay or to Google is recorded
anywhere in the repository.** Google does not say where it serves the Live
models for an AI Studio key (its [regions page](https://ai.google.dev/gemini-api/docs/available-regions),
updated 2026-04-28, lists Vietnam as a country where the service may be
used and says nothing of where it runs). Published, not measured here:
average pings between hosted servers, from Ho Chi Minh City, are 44 ms to
Singapore, 205 ms to San Jose and 198 ms to Des Moines (WonderNetwork,
[Ho Chi Minh City](https://wondernetwork.com/pings/Ho%20Chi%20Minh%20City),
read 2026-10-14; no method or count stated). So if the model runs in the
United States, about 0.2 s of the 1.15 s is the road, whichever way the
lamp goes there.

**The service itself.** Published, not measured here: Artificial
Analysis lists Gemini 3.8 Live at **1.18 s** "Time to First Audio",
defined as the "average number of seconds required to generate the first
token of audio output, measured across the Big Bench Audio question set"
([leaderboard](https://artificialanalysis.ai/speech-to-speech), read
2026-10-14; [method](https://artificialanalysis.ai/methodology/speech-to-speech-benchmarking)).
The method does not say where the clock starts, where the client is, or
how the question is sent. Its text models are measured from a Google
Cloud machine in `us-central1`. If the speech figures are taken the same
way, the model alone takes about what the lamp sees, and the relay and
the road from Vietnam add little. That is an inference from a figure
whose boundary is not stated, and trial 1 below is how to check it.

## 1. Ending the turn sooner when the sentence is plainly finished

This is task 1 ([endpoint-pilot.md](endpoint-pilot.md), "The fast
model"). The lamp waits 600 ms of quiet before it ends a turn. A shorter
fixed wait was tried at 400 ms on 2026-10-14 and withdrawn: the pause
after "Lamp," cut three of five called sentences in two after the name
([conversation-latency.md](conversation-latency.md)). The end-of-turn
model (Pipecat's Smart Turn v3.2) can end the wait early only when the
sentence sounds finished.

**What it would save.** 0.3 s on a turn the model calls finished, if it
ends at 300 ms in place of 600 ms; nothing on the others. How many turns
that is depends on the bar. On 1,167 held-out recorded clips in English
and Spanish: asked 100 ms into a pause, today's model put 91.5% of
finished turns on the finished side of 0.5, and 3.3% of unfinished ones
(endpoint-pilot.md); at the voice's own bar of 0.9, asked at 200 ms, 77%
of finished turns ended early, and 19 of 569 unfinished ones
([endpoint-detection.md](endpoint-detection.md), the parity profile). The
fast form takes 30 ms a verdict on the Linux ARM64 VM; about 130 ms is
estimated for the lamp, by the ratio the exact model showed there. Asked
at 100 ms, a verdict would be in hand at about 230 ms, inside the 300 ms.

**On what evidence.** Times on a VM, an estimate for the lamp, and
recordings replayed digitally. Nothing on the lamp's microphone.

**What it costs.** Built, off by default. ONNX Runtime 1.30.0 (25 MB, MIT
licence) beside the worker. No money and no keys.

**What it does to the lamp.**

- **More turns ended too soon.** A turn ended too soon is not usually
  heard as a cut-off on this lamp: the answer cannot arrive for about a
  second, and speech in between drops it (`user_interrupted`) and sends
  the rest. It costs a request given up and sent again, and the model
  then hears the thought in two pieces. At 600 ms the lamp already
  answered such pieces in three of four cases (pauses.sh, 2026-10-14).
  How many more: in the end-of-turn study, ending a "finished" turn at
  400 ms added such a split in 271 of 1,167 held-out clips (23%), most of
  them long English recordings; at pauses of 0.8 s or more after which
  the person went on, the model called 43% finished 200 ms in
  ([endpoint-detection.md](endpoint-detection.md)).
- **Published warnings, not measured here.** LiveKit's own end-of-turn
  benchmark ([eot-bench](https://github.com/livekit/eot-bench), English,
  run by LiveKit, no date) gives Smart Turn v3.2 false cut-offs of 35.2%
  when it must decide within 300 ms and 14.8% within 600 ms, against 9.9%
  and 4.5% for LiveKit's own audio model. Pipecat's v3 announcement
  ([2025-09-11](https://www.daily.co/blog/announcing-smart-turn-v3-with-cpu-inference-in-just-12ms/))
  gives Vietnamese the lowest accuracy of 23 languages: 81.27%, with
  14.84% of unfinished turns called finished (English 94.31% and 2.64%,
  on 1,004 and 2,846 clips). Newer versions publish no per-language table.
  The people at the lamp on 2026-10-13 spoke mostly Vietnamese
  ([character.md](character.md)).
- **Other people's talk.** In a group, 21 of 47 turns were replaced by
  more talk before any answer had arrived ([group-talk.md](group-talk.md)).
  A sooner end lets the lamp get a word in more often, and sends more
  scraps of others' talk to the model. The faint floor, the rule for a
  busy room and the model's own silence still apply.
- **Being interrupted and its character:** unchanged.

**Other end-of-turn models, published, not measured here.** LiveKit's
audio model leads its own benchmark, in 14 languages, Vietnamese not
among them ([docs](https://docs.livekit.io/agents/logic/turns/turn-detector));
its size, licence and speed on this board were not looked at. Deepgram
Flux decides the end inside its speech-to-text and has no Vietnamese
([languages](https://developers.deepgram.com/docs/models-languages-overview)).
AssemblyAI's turn detection also lives inside its speech-to-text
([turn detection](https://www.assemblyai.com/docs/streaming/universal-streaming/turn-detection)).
Each of these needs the lamp's sound sent to a second service, or a new
model on the board; neither is a small change.

## 2. Starting the answer before the person has surely finished

**How shipped agents do it, published, not measured here.** They start
the text model on the words heard so far at a likely end, hold its
output, and use it if the turn is confirmed or throw it away if the
person goes on.

- LiveKit's `preemptive_generation`, on by default: only the text model
  starts early, and the voice waits until the turn is confirmed unless
  `preemptive_tts` is set; it "increases LLM token usage"
  ([docs](https://docs.livekit.io/agents/build/speech/), no date). No
  saving is published.
- Pipecat's eager end of turn (v1.9.0, 2026-09-10), with Deepgram Flux or
  Cartesia, off unless `enable_eager_end_of_turn=True`: "a response is
  generated from a service's predicted end of turn and held until the
  turn is confirmed", and "discarded if the user resumes speaking or if
  the committed transcript differs from the predicted one"
  ([CHANGELOG](https://github.com/pipecat-ai/pipecat/blob/main/CHANGELOG.md)).
- Deepgram: with an eager threshold of 0.3 to 0.5, its early event fires
  "150–250ms earlier than EndOfTurn", "at the cost of 50–70% more LLM
  calls" ([Flux](https://deepgram.com/flux), 2025-10-01, updated
  2026-10-13); its guide says this trims "that last 100-200ms of
  end-to-end latency"
  ([guide](https://developers.deepgram.com/docs/flux/voice-agent-eager-eot)).

No one publishes how often a started answer is thrown away. The 50 to 70%
more calls is the only published cost.

**On this lamp the two ways meet.** The lamp cannot ask Gemini Live for
an answer without ending the person's turn: `activityEnd` puts the turn
into the service's memory of the conversation and the answer follows. So
"starting before the person has surely finished" here means sending
`activityEnd` at a likely end, and playing what comes back only if the
person has not gone on by then. That is close to what the lamp already
does. It sends the end at 600 ms of quiet, the answer comes about a
second later, and speech in between drops it. What would change: the end
is sent at, say, 200 ms of quiet; the lamp's own decision that the turn
is over (600 ms, or the model's "finished") comes later but always before
the answer can arrive; and nothing is played before that decision.

**What it would save.** Up to 0.4 s on every answer, finished-sounding
or not: 200 ms in place of 600 ms. Holding the answer until the lamp is
sure costs nothing so long as the answer takes longer than the gap
between sending and being sure (0.4 s here). In every run cited above it
took more than twice that (to the service's first sound, 843 ms the
least of sixteen on 2026-10-11; to the first sound written, 0.98 s the
least on 2026-10-14). With way 1 the two do not add up: the
saving is from 600 ms to whichever is earlier.

**On what evidence.** The lamp's timings above. The service's handling of
a thought sent in two pieces: on 2026-10-11 from lamp-4ace, a correction
sent 300 ms after the first question's end timed out when its start was
written at once, and was answered rightly twice when its start was held
until the first answer had begun (`kept-start`;
[continuation-service-20261011](verification/continuation-service-20261011/README.md)).
At 600 ms, thoughts with 650 and 900 ms pauses were answered in three of
four. That is all; it is not a rate.

**What it costs.**

- **Code**, in `crates/voice`, which is the first builder's. Two things
  must change, or the 400 ms trial's failures come back. The rule that
  throws away a short utterance as a sound (`not_speech`, under 30 blocks
  of speech; [not-speech.md](not-speech.md)) may judge only once the
  turn is sure, so that "Lamp," sent early is not thrown away. The rest
  of a thought must follow its first piece as `kept-start` does, so that
  the model hears the name and the question together.
- **Money.** Each request given up costs about one more turn's input,
  because the service bills the whole conversation again on every turn.
  At Google's list prices ($3.00 per million audio tokens in, $0.75 per
  million text tokens in, $12.00 per million audio tokens out;
  [pricing](https://ai.google.dev/gemini-api/docs/pricing), updated
  2026-10-13) and the lamp's prompts of 1,327 to 4,234 tokens, that is
  between a tenth of a US cent and a little over one cent. How often:
  even on the model's "finished", ending at 400 ms added a split in 271
  of 1,167 held-out clips (23%) in the end-of-turn study
  ([endpoint-detection.md](endpoint-detection.md)); sending at 200 ms of
  quiet, whatever the model says, will give more. These are list
  prices, not what the company pays through its relay, which is not in
  the repository. A night of played tests already used up the Gemini
  Live quota once ([RUNBOOK](../deploy/lamp-4ace/RUNBOOK.md)).

**What it does to the lamp.**

- **Its memory of the talk.** The model keeps every piece and every
  answer it began. Whether a long conversation of pieces changes what it
  says has not been looked at.
- **Other people's talk.** More scraps of others' talk reach the model,
  one at each short pause. The busy-room rule holds back answers to a
  new session's first two requests only; after that the model's own
  judgement decides.
- **Being interrupted:** unchanged; the lamp stops its speaker by
  itself, whatever the service does.

## 3. A shorter hold before the first sound

The lamp holds the first sound of an answer for at most 350 ms, less when
the service sends sound faster than it plays: it starts once the time
since the first sound and the sound in hand add up to 350 ms
(`--playout-start-ms 350`; [playout-start.md](playout-start.md)). That
stops an answer starting and then running dry, and it gives the answer's
own text time to arrive, so that a wordless "..." is caught before it
plays ([not-speech.md](not-speech.md): the "..." came about 180 ms after
the first sound).

**What it would save.** Less than it looks. At a 200 ms setting the hold
was still 180 ms at the median, and every one of those eight answers
started because the sound had covered the setting, at a median of 174 ms
after the first sound ([installed-eight-turn-20261011](verification/installed-eight-turn-20261011/README.md)).
So the service sends its first sound little faster than it plays, and the
hold is most of whatever it is set to. From 350 ms to 200 ms would save
about 0.1 s; a hold near zero, up to about 0.25 s, at the price of the
two risks below. The records hold the exact hold for each answer.

**What it costs.** A setting. The risks are the two things it is there
for: gaps in the first second of an answer, and a "..." played before its
text arrives. Neither has been counted against the setting.

## 4. The same service, reached directly

The lamp's configuration already allows it: the `endpoint` field takes
Google's own address (`GOOGLE_ENDPOINT` in `crates/gemini/src/config.rs`),
and `authentication` takes `api_key`, `bearer` or `ephemeral`
([live-runtime.md](live-runtime.md)).

**What it would save.** Not known. At most what the relay adds: its
Cloudflare edge, its own server wherever that is, and its hop on to
Google. If the relay's server sits on the way from Vietnam to where
Google runs the model, that is little. Google says only that a direct
connection "generally offers better performance for streaming audio and
video, since it bypasses the need to send the stream to your backend
first" ([Live API](https://ai.google.dev/gemini-api/docs/live), updated
2026-10-06). The published 1.18 s for the model alone against the
lamp's 1.15 s suggests the relay is not a large part (above).

**Nearer than the United States, published, not measured here.**
Google's pages disagree. The Vertex AI model page for Gemini 3.8 Live
lists `us`, `eu` and `us-central1` only
([3.8 Live](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-8-live),
GA 2026-09-24). Its locations page lists Gemini 3.8 Live among the Asia
Pacific models, which include Singapore, and says endpoints "don't
guarantee data residency or in-region ML processing"
([locations](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/learn/locations),
updated 2026-10-13). There is no Google Cloud region in Vietnam. Vertex
needs a Google Cloud project and a change in `crates/gemini`, which
writes the model as `models/<name>` where Vertex wants a full resource
name.

**What it costs.**

- **A key.** Either a Gemini API key on each lamp, or short-lived keys
  made by the company's server for each session. Google's ephemeral
  tokens are in Preview, for the Live API only; by default one starts a
  single session within a minute and lasts 30 minutes, and it can be
  locked to one model and configuration
  ([ephemeral tokens](https://ai.google.dev/gemini-api/docs/ephemeral-tokens),
  updated 2026-09-15). The lamp already sends them (`Token` header).
- **The relay's work moves.** It meters each lamp and stops one over its
  limit (close codes 4001, 4002 and 4029 in `crates/gemini/src/lib.rs`).
  That would have to live wherever the keys are made.
- **The same prices**, billed to Google directly.

**What it does to the lamp.** Nothing to what it says. One gain: the
relay does not pass on session resumption
([conversation-thread.md](conversation-thread.md)), so today each
renewal begins a new conversation. Direct, the lamp could resume.

## 5. The same service, set differently

**Another Gemini Live model.** Published, not measured here (Artificial
Analysis, as above): Gemini 3.8 Live 1.18 s; 3.8 Live Extended Thinking
(High) 1.35 s; 3.1 Flash Live at minimal thinking 0.96 s; 2.5 Flash
Native Audio Dialog 0.63 s. Other benchmarks order them differently.
Pipecat's evaluation, from recordings with the service's own turn
detection, gives medians of 1,632 ms for 3.1 Flash Live at minimal
thinking and 2,624 ms for a row called "gemini-live", model not named
([aiewf-eval](https://github.com/kwindla/aiewf-eval), undated). Against
that:

- 3.1 Flash Live is listed as a legacy preview
  ([models](https://ai.google.dev/gemini-api/docs/models), updated
  2026-10-13), and the 2.5 native-audio Live model's support on Vertex
  retires on 2026-12-13
  ([2.5 Flash Live](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/2-5-flash-live-api),
  updated 2026-10-13). The lamp ships late this year.
- The voice stays Kore. But everything the character file does was found
  on 3.8 Live, by changing the file and replaying ([character.md](character.md)).
- Gemini 3.8 Live has proactive audio "permanently enabled"
  ([3.8 Live](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-live),
  updated 2026-09-15): "Gemini can proactively decide not to respond if
  the content is not relevant", and the guide marks it unsupported in 3.1
  Flash Live ([capabilities](https://ai.google.dev/gemini-api/docs/live-api/capabilities)).
  That is part of how the lamp stays out of other people's talk.
- The repository's last model comparison found no speedup established
  ([gemini-comparison-config.md](gemini-comparison-config.md)).

**Thinking.** Google says `thinking_level` is not supported for
`gemini-3.8-live`; the lamp sends a thinking budget of zero, which the
service accepted, and it made no difference that could be seen (above).

**A shorter memory of the talk.** `context_trigger_tokens` and
`context_target_tokens` bound it. It lowers the cost of every turn.
Whether it shortens the wait is not known, and Google says each
shortening briefly adds to it.

## 6. Another service that takes speech in and gives speech out

Published, not measured here.

| Service | Time to first sound, Artificial Analysis | Where it runs | Price | Vietnamese |
|---|---|---|---|---|
| Gemini 3.8 Live (today) | 1.18 s | Not said for AI Studio | $3 in, $12 out per million audio tokens | Yes |
| OpenAI GPT-Realtime-1.5 / 2.1 minimal / 2.1 high | 0.81 / 0.97 / 1.21 s | United States and Europe only | $32 in, $64 out per million audio tokens (mini: $10, $20) | Not listed by OpenAI; Microsoft lists it for gpt-realtime |
| OpenAI GPT-Live-1 | 1.24 to 1.34 s | United States and Europe only | $0.05 a minute, billed by the second | Not confirmed |
| xAI Grok Voice Think Fast 2.0 (High) | 0.70 s | One endpoint, no Asian region | $0.08 a minute | Yes |
| Amazon Nova 2 Sonic | 1.14 s | Includes Tokyo | Not shown on AWS's page | No |
| Qwen3.5 Omni Flash Realtime | 0.79 s | Singapore | $16.5 in, $62 out per million tokens | Not confirmed |
| Deepslate Opal | 0.44 s | Europe | Not found | Not found |

Sources, all read 2026-10-14: [Artificial Analysis](https://artificialanalysis.ai/speech-to-speech);
OpenAI [pricing](https://developers.openai.com/api/docs/pricing),
[data and regions](https://developers.openai.com/api/docs/guides/your-data),
[GPT-Live](https://developers.openai.com/api/docs/models/gpt-live-1);
Microsoft [Voice Live languages](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/voice-live-language-support);
xAI [pricing](https://docs.x.ai/developers/pricing) and [voice guide](https://docs.x.ai/docs/guides/voice/agent);
AWS [Nova 2 release notes](https://docs.aws.amazon.com/nova/latest/nova2-userguide/release-notes.html)
and [Nova 2 Sonic](https://aws.amazon.com/blogs/aws/introducing-amazon-nova-2-sonic-next-generation-speech-to-speech-model-for-conversational-ai);
Alibaba [Qwen3.5-Omni Realtime](https://www.alibabacloud.com/help/en/model-studio/qwen3-5-omni-plus-realtime);
Deepslate [Opal](https://docs.deepslate.eu/opal.md). openai.com's own
pages refused to be read (403); OpenAI's developer pages were read.

Hume's speech-to-speech API ends on 2026-11-13, by its own
[docs](https://dev.hume.ai/docs/speech-to-speech-evi/overview). Kyutai's
Moshi, "theoretical latency of 160ms, 200ms in practice"
([paper](https://arxiv.org/abs/2410.00037), 2024), is a research model to
run oneself, with no hosted service or Vietnamese found.

**What it would save.** 0.2 to 0.7 s on the published times, if they mean
what the lamp measures. Most of these run only in the United States or
Europe, so from Vietnam about 0.2 s of road is in them, as it probably is
in Gemini's.

**What it costs.** The voice. The owner decided on Gemini's own voice
everywhere, for cost and for one consistent voice. Each service also
needs an account and a key, and a new provider part beside
`crates/gemini`: the lamp's handling of sessions, interruptions,
renewals and held answers was built and tested against Gemini's protocol
([gemini-session-reliability.md](gemini-session-reliability.md)). OpenAI's
realtime models cost about ten times Gemini's per audio token in and five
times out. A price by the minute, as GPT-Live's and Grok's are, would be
paid all day if it counts the time a session is open, as the lamp's
always is; what exactly it counts was not checked.

**What it does to the lamp.** Everything that rests on the model is to be
found again: the character file's rules, its staying out of other
people's talk, what it says after being interrupted. GPT-Live decides
turns itself ("GPT-Live is full duplex"; [guide](https://developers.openai.com/api/docs/guides/live-conversations)),
so the lamp's own rules for when a turn ends would not apply to it.

## 7. Speech to text, a text model and a voice, in a row

Three streaming services, one after another: the person's speech becomes
words, a text model answers, a voice says it.

**The stages, published, not measured here.** All measured in the United
States; none from Vietnam.

| Stage | Figure | Boundary and source |
|---|---|---|
| Speech to final words | Deepgram Nova-3 247 ms median, 298 ms p95; AssemblyAI 256 to 307 ms; Google `gemini-3.5-transcribe-live` 458 ms | End of speech to the final words, 1,000 English clips played in real time ([Pipecat stt-benchmark](https://github.com/pipecat-ai/stt-benchmark), undated, scored 2026-09) |
| Speech to final words, on a lamp | Deepgram Nova-3 524 ms median, 611 ms worst, 11 Vietnamese recordings | V1 on a lamp, through the company's relay, 2026-10-09 (`docs/realtime-voice.md` at the repository root). **Measured, but on V1** |
| Text model, first token | Gemini 3.5 Flash at minimal thinking 0.80 s; gpt-oss-120b on Cerebras 0.44 s to the first chunk, 1.58 s to the first answer token | From a `us-central1` machine, 10,000-token input, median over 72 hours ([Artificial Analysis method](https://artificialanalysis.ai/methodology/performance-benchmarking)) |
| Voice, first sound | Not published for Google's voices. Others: 37 to 120 ms, most "excluding network" | Vendors' own figures |
| A whole round trip | 1,293 ms, microphone to speaker, of which 300 ms words and end of turn, 650 ms text model, 120 ms voice | [Kwindla Kramer's guide](https://voiceaiandvoiceagents.com/), June 2026, which now calls 1,500 ms "an important target"; measured by recording the conversation and reading the waveform |
| A target | 1,115 ms median "mouth-to-ear", network included | [Twilio](https://www.twilio.com/en-us/blog/developers/best-practices/guide-core-latency-ai-voice-agents), 2025-11-17 |

**What it would save here.** Not shown. Run from the lamp, each stage is
a trip to the United States (Google's streaming speech-to-text Chirp 3
runs in `us` and `eu` only; [Chirp 3](https://cloud.google.com/speech-to-text/docs/models/chirp-3),
updated 2026-10-09). Words in about 0.25 to 0.5 s after the end, a first
token in 0.4 to 0.8 s, a first sound in 0.1 to 0.3 s, and the road for
each: about 1 to 1.6 s after the end, against Gemini Live's 1.15 s. What
it buys is not speed by itself but the part of way 2 that Gemini Live
cannot do cleanly: the text model can start on the words heard so far
and be thrown away without touching any memory of the conversation.

**The voice.** Google says its native-audio Live models "support any of
the voices available for our Text-to-Speech (TTS) models"
([capabilities](https://ai.google.dev/gemini-api/docs/live-api/capabilities)).
Its speech models `gemini-3.8-flash-tts` and `gemini-3.8-flash-lite-tts`
offer 30 voices including Kore, stream 24 kHz PCM and speak Vietnamese
([speech generation](https://ai.google.dev/gemini-api/docs/speech-generation),
updated 2026-10-12); Flash-Lite costs $6 per million audio tokens out,
about $0.0015 for ten seconds. Cloud Text-to-Speech's Chirp 3 HD voices
carry the same names (`en-US-Chirp3-HD-Kore`), stream, speak Vietnamese,
run in `asia-southeast1` among other places, and cost $30 per million
characters ([Chirp 3 HD](https://cloud.google.com/text-to-speech/docs/chirp3-hd),
updated 2026-10-09). Google calls Gemini's voices "similar to" Chirp 3 HD;
nowhere does it say a voice of the same name sounds the same as the Live
model's. That has to be listened to. It matters beyond speed: it is the
matching Google voice the owner asked for in an economy or fallback path.

**What it costs.** Three services, each with a key (Google's three would
need one Google account). A whole new part on the lamp, streaming to
three services and stopping all three when the person speaks. The
brain decisions already plan such a route for when the speech service is
down, as later work that does not block launch (decision 9 in
[brain-build.md](brain-build.md)).

**What it does to the lamp.** The text model hears words, not the voice:
tone, hesitation, laughter and speech it cannot make out are gone, and it
can no longer tell "¿Qué?" from a cough by sound. In return the lamp has
the words before any answer is made, so the rules for whom it answers can
act before anything is generated, not on an answer already arriving
([attention.md](attention.md)). Its character would need a new file and
new checks.

## 8. What shipped voice agents publish about their own timings

Published, not measured here. Every figure under about 0.5 s found is
either a claim with no stated method, one stage only, or a server's time
to its first sound. Measured from the user's side, on a recording, shipped
agents answer in about 1.0 to 1.7 s at the median.

| Who | Figure | How, in their words |
|---|---|---|
| Openbenchmarks, phone agents ([leaderboard](https://github.com/openbenchmarks-labs/voice-agent-latency), generated 2026-08-01) | Telnyx 1,296 ms p50, 1,856 ms p95; ElevenLabs 1,424 / 1,768; Bland 1,520 / 2,248; Vapi 1,558 / 2,008; Retell 1,740 / 2,259 | "the gap between the moment the caller stops speaking and the moment the agent's audio starts", from 2,078 turns of 550 recorded calls; each platform's own figure runs "roughly 490 ms below" |
| Hume, from New York ([EVI 3](https://www.hume.ai/blog/introducing-evi-3), 2025-05-29) | Gemini Live API about 1.5 s (1.2 to 3.6); GPT-4o Realtime about 2.6 s; Sesame's web app about 1 s; EVI 3 about 1.2 s | "interval between end of user speech and beginning of assistant speech"; count not given |
| Agora, ChatGPT on an iPhone ([post](https://prod.agora.io/en/blog/openai-didnt-publish-gpt-lives-latency-so-we-measured-it), 2026-07-10) | GPT-Live "roughly 1.1 s to acknowledge" | "last frame of user speech to first audible frame of output", 30 a condition, read off the waveform; the first sound may be an acknowledgement |
| Full-Duplex-Bench v1 ([paper](https://arxiv.org/html/2503.04721v3)) | Gemini Live (2.0 Flash) 1.301 s; Moshi 0.265 s | End of the user's speech to the start of the model's, through the APIs |
| OpenAI, GPT-Live-1 ([forum](https://community.openai.com/t/introducing-gpt-live-1-in-the-api/1396471), 2026-09-10) | "0.798-second turn-taking latency", against 1.41 s for GPT-Realtime-2.1 | Not stated in the post |
| OpenAI, GPT-4o (2024-05-13, via its [system card](https://arxiv.org/abs/2410.21276)) | "as little as 232 milliseconds, with an average of 320 milliseconds" | Not stated |
| Amazon Nova Sonic ([2025-04-08](https://press.aboutamazon.com/2025/4/introducing-amazon-nova-sonic-a-new-gen-ai-model-for-building-voice-applications-and-agents)) | 1.09 s average | "from the time the customer is done talking to the time the system generates the first speech response", by Artificial Analysis |
| Retell ([docs](https://docs.retellai.com/reliability/check-estimated-latency)) | "as low as 600ms" | User stops to agent responds; its own report "does not account for network trip time" to the user |
| Bland, Synthflow, PolyAI | 200 ms; about 250 ms; 280 to 500 ms | No method |
| LiveKit, Vapi | Definitions, no results | LiveKit's `e2e_latency` is "from when the user stopped speaking to when the agent began responding"; Vapi's total "excludes endpointing and transport time" |

Google publishes no figure for Gemini Live, only "ultra-low latency" and
"sub-second latency". The human reference is about 200 ms
([conversation-latency.md](conversation-latency.md)).

**What it tells the lamp.** The lamp's boundary (the person stopping, to
the first sound written) is stricter than most of these: it includes its
own wait for the end. Only a recording read by its waveform, as
Openbenchmarks, Hume, Agora and the Pipecat guide do, is like it; the
room recordings the lamp already keeps can give that (the acoustic
boundary in [conversation-latency.md](conversation-latency.md)).

## What the ways add up to

Estimates by adding medians. Only the first row is measured. The hold is
taken as about 0.23 s at today's 350 ms (the difference of medians) and
0.18 s at 200 ms (measured once, eight answers).

| What is done | Wait | Service | Hold | Typical total |
|---|---|---|---|---|
| Today (measured) | 0.6 s | 1.15 s | about 0.23 s | 1.98 s |
| 1: a finished sentence ends at 0.3 s | 0.3 s for most finished turns, 0.6 s for the rest | 1.15 s | 0.23 s | about 1.7 s for those |
| 1 and 3 (hold at 200 ms) | 0.3 s | 1.15 s | 0.18 s | about 1.6 s |
| 2 and 3: the end sent at 0.2 s | 0.2 s | 1.15 s | 0.18 s | about 1.5 s |
| 2 and 3, and the model nearer (if Google serves it in Asia) | 0.2 s | about 1.0 s | 0.18 s | about 1.4 s |
| 2 and 3, and a service at 0.6 s (published, not measured here) | 0.2 s | about 0.6 s | 0.18 s | about 1.0 s |
| The target | | | | 0.3 to 0.5 s |

So: with this service the lamp can come to about 1.5 s. Near 1 s it
needs a faster service as well, and below it a shorter hold too. 0.3 to
0.5 s is not reached by any way found. It would take a model that, like
a person, prepares its answer while the other is still speaking. The one
research model that does (Moshi, 0.2 s in practice) is not offered as a
service, and the full-duplex service found (GPT-Live) is published at
0.8 to 1.3 s. A stock sound or word in place of the answer is ruled out
([AGENTS.md](../AGENTS.md)). This study does not change the target in
[conversation-latency.md](conversation-latency.md).

## Recommended order of trials

Cheapest and safest first. Played trials at night: played tests in the
working day are a nuisance to the office. The played tests named below
(`pauses.sh`, `plain.sh`, `busy.sh`, `hold.sh`, `stories.sh`,
`checks.sh`) are the office iMac's, in `~/lamp-voice-data/tools/`
([RUNBOOK](../deploy/lamp-4ace/RUNBOOK.md)).

**How to measure each one with the records the lamp keeps.** Each
session's `events.jsonl` (under `/var/lib/lampOS/voice/sessions/`) has,
for each answer:

- `local_endpoint`: `at_us`, when the lamp decided the person had
  finished; `includes_silence_wait_ms`, the wait it used; `endpoint`, the
  model's verdict and the basis of the decision when the turn model is on;
- `provider_first_audio`: `at_us`, when the service's first sound reached
  the lamp;
- `speaker_first_write`: `at_us`, when the first sound was written to the
  speaker ("ALSA accepted; acoustic onset unmeasured");
- the outcome when there was no answer: `user_interrupted`, `not_speech`,
  `nothing_said`, `no_audio_answer`.

The journal has, for each answer, the provider's line `lamp-voice
provider: usage total=… prompt=… prompt_audio=… response_audio=…`.

From these, per answer: **wait** = `includes_silence_wait_ms`; **service**
= `provider_first_audio` less `local_endpoint`; **hold** =
`speaker_first_write` less `provider_first_audio`; **total** = the three
added, which is the table in [conversation-latency.md](conversation-latency.md).
Report half within and nine in ten within, with the count, beside
the same runs at today's settings, and keep every failure. Score a few
answers by ear from the room recording too: a first write is not a first
audible word.

1. **Read what is already recorded.** No change, no sound, no cost.
   - Split each of the 1,034 answers into wait, service and hold. Does
     the service part grow with the prompt (the usage lines), with the
     session's age, with the hour? Is the hold near 0 or near 350 ms?
   - Ask whoever runs `campaign-api` where its server is, and whether its
     Gemini route buffers anything. One question.
   - From the lamp, with no key: `curl -so /dev/null -w
     '%{time_connect} %{time_appconnect}\n'` to
     `https://campaign-api.autonomous.ai/` and to
     `https://generativelanguage.googleapis.com/`, ten times each. That
     times the first hop only (Cloudflare's edge; Google's front door).
     Then ten timed `lamp-voice provider-check CONFIG`: each sets up a
     session through the relay and sends no sound.
   - What it should show: the service part near 1.15 s and the hold
     between 0.15 and 0.35 s. If the service part grows with the prompt,
     a shorter memory of the talk (way 5) moves up the list.
   - What it decides: whether the road is worth a key (trial 5), and
     whether the hold is worth trying (trial 3).
2. **The fast end-of-turn model** (way 1). First its time on the lamp, as
   [endpoint-pilot.md](endpoint-pilot.md) says ("On the lamp": `bench
   --fast`, under 150 ms wanted). Then the voice started with
   `WorkerCommand::smart_turn_fast`, asked at 100 ms, a finished turn
   ending at 300 ms (the voice's rule, the first builder's to change).
   `pauses.sh` twice, `plain.sh`, `busy.sh` and `hold.sh`, against the
   same at 600 ms. Count beside the times: the `endpoint` bases; turns
   given up before an answer; `not_speech`; whether every "Lamp," stays
   with its question. Then a working day, for Vietnamese.
   - What it should show: the waits of `plain.sh`'s questions at 300 ms
     in most turns, and the median total about 0.2 to 0.3 s lower.
   - Stop if a called sentence loses its name, or the lamp answers more
     scraps of others' talk.
   - Most likely to be wrong: Vietnamese, where the model is weakest, and
     "Lamp," with a pause after it, which may sound finished.
3. **A shorter hold** (way 3). From trial 1's split first. Then
   `--playout-start-ms 200`, with `plain.sh` and `stories.sh`: count gaps
   in the first second from the playback records, and whether each
   "..." is still caught before it plays.
   - What it should show: the hold about 0.1 s shorter, as on
     2026-10-11; no more gaps in the first second.
4. **The end sent sooner, the answer held until sure** (way 2). Built by
   the first builder behind a switch that is off unless set, with the two
   changes above (the sound rule judged only when the turn is sure;
   `kept-start`). The wait lowered in steps: 400, 300, 200 ms, with
   `pauses.sh` and `plain.sh` at each. Count beside the times: requests
   per answer (given up and sent), prompt tokens per answer from the usage
   lines, and thoughts answered whole. Watch the quota.
   - What it should show: the median total lower by the step taken from
     600 ms, on every answer; requests per answer above one, more so at
     each step.
   - Most likely to be wrong: the model's answers to a thought sent in
     pieces, which have been seen only a few times.
5. **The same service, directly** (way 4), only if trial 1 shows the road
   is worth it, and only with a key the owner provides. In the provider
   file: `endpoint` set to Google's address and `authentication`. One
   night of `plain.sh`, ten questions through the relay and ten direct,
   in turn, so the hour is the same. Then `resumption: retain` on the
   direct path.
   - What it should show: the service part lower by what the relay
     adds, which is not known; small if the relay's server is on the way
     to Google's.
6. **Another Gemini Live model** (way 5), only when Google lists one as
   stable that is published as faster. `model` in the provider file;
   `checks.sh` as well as the times, because the character rests on the
   model.
7. **The second route as a pipeline** (way 7): a timing bench off the
   lamp, on the office network, before anything is built. Recorded
   questions through streaming speech-to-text (`gemini-3.5-transcribe-live`
   or Deepgram Nova-3, both Vietnamese), a text model, and
   `gemini-3.8-flash-lite-tts` with Kore, each stage timed; and a
   listening test of that Kore against the Live model's. Needs keys: the
   owner's decision.
8. **Another speech service** (way 6), only on the owner's word about the
   voice.

## What was not shown

- **Nothing was timed.** No service was called, nothing ran on a lamp,
  and no published figure was measured from Vietnam or on the lamp. The
  one thing looked up here is the relay's public address.
- **Where the time inside the 1.15 s goes**: the road, the relay and the
  model have not been told apart. No round trip from the lamp is
  recorded; the relay's server and Google's serving place for this key
  are not known.
- **What the published figures measure.** Artificial Analysis does not
  say where its clock starts or where its client is. Most vendors' figures
  state no method. Different benchmarks put the same models in different
  orders.
- **Google's pages disagree** on whether Gemini 3.8 Live is served in
  Asia.
- **Whether a Kore from Gemini's speech models or from Chirp 3 HD sounds
  like the Live model's Kore.** Not published; not listened to.
- **What the company pays.** All prices are list prices; what the relay
  charges or passes on is not in the repository.
- **How the model answers a thought sent in pieces**, beyond two probes
  and one played test, and how often a piece is given up at 200 to 400 ms
  on the lamp's own microphone. The 23% is from recorded clips replayed
  digitally, mostly long English recordings.
- **The fast end-of-turn model in Vietnamese.** Only Smart Turn v3's
  published per-language figures exist; v3.2's are not published.
- **The hold at today's 350 ms** is a difference of two medians; at
  200 ms it was measured once, on eight answers.
- **Some primary pages could not be read**: openai.com refused (403), and
  the GPT-4o figures come from OpenAI's system card. Long pages were read
  in part.

## Sources

All read on 2026-10-14 (the lamp's date). The page's own date is given
where it shows one.

In the repository: [conversation-latency.md](conversation-latency.md),
[endpoint-pilot.md](endpoint-pilot.md) ("The fast model"),
[endpoint-detection.md](endpoint-detection.md),
[live-runtime.md](live-runtime.md), [attention.md](attention.md),
[group-talk.md](group-talk.md), [not-speech.md](not-speech.md),
[character.md](character.md), [conversation-thread.md](conversation-thread.md),
[playout-start.md](playout-start.md), [brain-build.md](brain-build.md),
[gemini-comparison-config.md](gemini-comparison-config.md),
[gemini-session-reliability.md](gemini-session-reliability.md),
[installed-eight-turn-20261011](verification/installed-eight-turn-20261011/README.md),
[provider-control-20261011](verification/provider-control-20261011/README.md),
[continuation-service-20261011](verification/continuation-service-20261011/README.md),
[strict-continuation-20261011](verification/strict-continuation-20261011/README.md),
`deploy/lamp-4ace/lamp-voice.service` and `RUNBOOK.md`,
`crates/gemini/src/{config,wire,lib}.rs`, and the V1 measurement in
`docs/realtime-voice.md` at the repository root.

Google:

- Live API overview, https://ai.google.dev/gemini-api/docs/live (2026-10-06)
- Live capabilities, https://ai.google.dev/gemini-api/docs/live-api/capabilities (2026-09-18)
- Live best practices, https://ai.google.dev/gemini-api/docs/live-api/best-practices (2026-09-15)
- Live API reference, https://ai.google.dev/api/live (2026-10-09)
- Gemini 3.8 Live, https://ai.google.dev/gemini-api/docs/models/gemini-3.8-live (2026-09-15)
- Models, https://ai.google.dev/gemini-api/docs/models (2026-10-13)
- Ephemeral tokens, https://ai.google.dev/gemini-api/docs/ephemeral-tokens (2026-09-15)
- Pricing, https://ai.google.dev/gemini-api/docs/pricing (2026-10-13)
- Available regions, https://ai.google.dev/gemini-api/docs/available-regions (2026-04-28)
- Speech generation, https://ai.google.dev/gemini-api/docs/speech-generation (2026-10-12)
- Live transcription, https://ai.google.dev/gemini-api/docs/live-api/live-transcribe (2026-08-26)
- Vertex Gemini 3.8 Live, https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/gemini/3-8-live
- Vertex locations, https://docs.cloud.google.com/vertex-ai/generative-ai/docs/learn/locations (2026-10-13)
- Chirp 3 HD, https://cloud.google.com/text-to-speech/docs/chirp3-hd (2026-10-09)
- Text-to-Speech pricing, https://cloud.google.com/text-to-speech/pricing
- Chirp 3 speech-to-text, https://cloud.google.com/speech-to-text/docs/models/chirp-3 (2026-10-09)

Measurements and benchmarks:

- Artificial Analysis, speech to speech, https://artificialanalysis.ai/speech-to-speech, and its method, https://artificialanalysis.ai/methodology/speech-to-speech-benchmarking
- Artificial Analysis, text model method, https://artificialanalysis.ai/methodology/performance-benchmarking (v2.2.0, 2026-03-02)
- Pipecat aiewf-eval, https://github.com/kwindla/aiewf-eval
- Pipecat stt-benchmark, https://github.com/pipecat-ai/stt-benchmark
- LiveKit eot-bench, https://github.com/livekit/eot-bench
- Smart Turn v3, https://www.daily.co/blog/announcing-smart-turn-v3-with-cpu-inference-in-just-12ms/ (2025-09-11)
- Openbenchmarks, https://github.com/openbenchmarks-labs/voice-agent-latency (2026-08-01)
- Hume EVI 3, https://www.hume.ai/blog/introducing-evi-3 (2025-05-29)
- Agora, https://prod.agora.io/en/blog/openai-didnt-publish-gpt-lives-latency-so-we-measured-it (2026-07-10)
- Full-Duplex-Bench v1, https://arxiv.org/html/2503.04721v3
- Moshi, https://arxiv.org/abs/2410.00037 (2024)
- Kwindla Kramer, https://voiceaiandvoiceagents.com/ (June 2026)
- Twilio, https://www.twilio.com/en-us/blog/developers/best-practices/guide-core-latency-ai-voice-agents (2025-11-17)
- WonderNetwork pings, https://wondernetwork.com/pings/Ho%20Chi%20Minh%20City

Starting before the end:

- LiveKit, https://docs.livekit.io/agents/build/speech/ and https://docs.livekit.io/agents/logic/turns/turn-detector
- Pipecat, https://github.com/pipecat-ai/pipecat (CHANGELOG, v1.9.0, 2026-09-10)
- Deepgram Flux, https://deepgram.com/flux (2025-10-01) and https://developers.deepgram.com/docs/flux/voice-agent-eager-eot
- Deepgram languages, https://developers.deepgram.com/docs/models-languages-overview
- AssemblyAI turn detection, https://www.assemblyai.com/docs/streaming/universal-streaming/turn-detection

Other services and vendors' own figures: as linked in sections 6 and 8.
