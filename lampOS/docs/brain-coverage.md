# The architecture page, row by row

Written 2026-10-13. The product owner asked that the whole of the "lampOS V2
brain architecture review" be read again and that nothing in it be missed.
This goes down version 0.6 of that page (`lampOS/docs/v2-brain-design.html`
on branch `commit-push`, commit `9e50ef5ea`) one row at a time: its fifteen
components, twenty topics, three kinds of service, nine walkthroughs, its
failure table, its fifty-eight capabilities taken from V1, its targets and
its tests, and the two things that are only in its drawings. For each it
says which process of the build carries it, at which step, and whether the
build does it as the page says. It is generated from the page's own tables
(`scratchpad` script, not in the repository), and the script stops if a row
of the page is left without an answer, so a row cannot be dropped by
oversight. Whether each answer is right is a matter of judgment, and those
are the voice owner's.

The page was written by its author over many hours and most of it stands as
written. Where the build differs it is because the product owner decided so
on 2026-10-13 ([brain-build.md](brain-build.md)), or because the lamp showed
something that day; each such place is marked, and they are gathered at the
end.

"Running" means on lamp-4ace today. Nothing else in the third column has
been built unless it says so.

## The build's steps

The order was changed on 2026-10-13 when the product owner asked why the bus was not first: "that to me is the core of all the sensors we have." It is now first.

| Step | What | In a line |
|---|---|---|
| 0 | The bus | Publish and subscribe with no process in the middle (`lamp-bus`). Being built. |
| 1 | The sensors on the bus | `mic1` from a driver of its own, a copy of `mic2`, the camera's newest frame, `servos`, `people`. |
| 2 | The lamp outlives the provider | No exit for a refused quota, a stalled link, a dead worker. First fix written. |
| 3 | The executive and the tools | `lamp-exec`; V1's 21 commands as the Live model's tools; one checked route for every action. |
| 4 | Memory, and who is there | `lamp-memory` (being built); faces recognised, one memory for each person. |
| 5 | One task, out and back | `lamp-tasks` through Harness on the lamp. |
| 6 | Questions and interruptions around a task | The agent's question asked at a pause; stopping speech never cancels work. |
| 7 | The situation, and attention with eyes and both microphones | `lamp-situation`; whether someone is facing the lamp when they speak; how near a voice is. |
| 8 | Decisions, goals and the owner's feedback | `lamp-decide` with Jev behind it; what the owner teaches, corrects and approves. |
| 9 | Controls | `lamp-controls`: touch pad, button, privacy switch. |
| 10 | Schedules | `lamp-schedule`: reminders and timers. |
| 11 | The second speech route | Deepgram, a text answer, speech synthesis. |
| 12 | The surround | Starting at power-up, setup, network, configuration, updates, reset, the operator's pages, V1's data. |

## The four proofs it asks of V2

The page opens by saying what V2 must show that V1 could not. Each is a test the build owes.

| The proof | In the build | When | Same, or how it differs |
|---|---|---|---|
| Camera-off or a stalled consumer does not disable unrelated sensing or erase fresh context. | The bus and the sensors on it: the camera switched off, or a subscriber stalled, and the others counted | Steps 0 and 1 | Same. |
| No duplicate task, stale movement, or false “done” claim in fault replays. | `lamp-exec` and `lamp-tasks`: faults played back, and no task sent twice, no stale movement, no "done" that was not | Steps 3, 5 and 6 | Same. |
| Exhaust Gemini Live and keep controls, light, task records and a working fallback available. | Voice with the provider exhausted: controls, light and task records kept | Step 2; a working second route at step 11 | Same, in two parts as the page itself says: staying up first, a useful fallback after. |
| The same Lamp workflow completes through two engines without changing the core. | The same task through two engines | Step 8 | Same. |

## What it says to do with the code that exists

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Rust capture, AEC and speaker | Kept as it is | Running on lamp-4ace | Same: no broker, model call or acknowledgement is put in the audio path. |
| mic1 / mic2 | Each a topic with one publisher; the second microphone gets a driver | Step 1 | Same. |
| Voice coordination and Gemini | Kept; tools added; the second route later | Running on lamp-4ace; steps 3 and 11 | Same. |
| Body and eyes workers | Kept; they publish to the bus and are asked through `lamp-exec` | Running on lamp-4ace; steps 1 and 3 | Same. |
| Context and decision model | `lamp-situation` and `lamp-decide`, beside Voice | Steps 7 and 8 | Same. |
| V1 capabilities | The fifty-eight rows below |  | Same. |

## Its fifteen components

The page says of its own drawing: "Logical responsibilities, not a process count." Here each is given the process that carries it.

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Drivers & media workers | Voice's capture, speaker and ring workers; the camera in Eyes; the servo bus in Body; a new driver for the second microphone (`lamp-mic1`) | Running on lamp-4ace; onto the bus at step 1 | Same. New: nothing reads the second microphone today. |
| Perception producers | Voice's speech detector; Eyes for faces, and for who they are and whether they face the lamp; the look | Faces running on lamp-4ace; the rest at steps 1, 4 and 7 | Same. |
| Privacy & local control | The authority core inside Voice (permissions, the privacy generation every frame carries); the physical switch and stop in Controls | Running on lamp-4ace for the core; step 9 | Same. |
| Topic graph | The bus: a topic is a directory, a subscriber a socket in it | Step 0, being built | Differs in form: no process in the middle, so that nothing sits where one failure would reach everything. Media too large for a message goes by reference to a file. |
| Attention | Inside Voice (how loud, how long, the room), with the facing vote from Eyes and the second microphone's nearness | Partly running on lamp-4ace; step 7 | Differs: it cannot admit by meaning before the provider. Local evidence narrows what is sent and the Live model decides (decision 7). |
| Situation / context service | `lamp-situation`, which reads the topics and publishes `situation.revision`; it began as facts told to the voice | The room, told to the voice: running on lamp-4ace since 18:55; step 7 | Same, with one condition from the lamp: everything it hands the voice is tested on a first turn. |
| Executive | `lamp-exec` | Step 3 | Same. A process of its own, so that Voice does not become what everything depends on. |
| DecisionProvider | `lamp-decide` | Step 8 | Differs in order: not on the spoken path until it wins on recorded requests (decision 16). First for what no utterance starts. |
| Voice router & conversation adapter | Voice | Running on lamp-4ace for the Live route; the second route at step 11 | Same. |
| Text response / request shaping | With the second route; on the Live route the model's own tool call carries the request | Steps 3 and 11 | Same. |
| Behavior / operation runners | Body for motion; the light as a service behind the executive | Gestures and gaze running on lamp-4ace; steps 3 and 7 | Same. |
| Task controller & Harness adapter | `lamp-tasks` | Step 5 | Differs: Harness itself runs on the lamp, signed in to the owner's account, so nothing of V1's pairing is ported (decision 14). |
| Harness + agent engines | The Harness service on the lamp, and the engines on the owner's computers | Installed 2026-10-13; no task sent yet | Same in who owns it. The lamp is one of the account's machines. |
| Memory & scheduler | `lamp-memory` and `lamp-schedule`, two processes | Memory being built (step 4); step 10 | Differs: memory across days and for each person (decision 12). |
| Supervisor & operations | systemd units, a stated readiness for each capability, and the surround | Units running on lamp-4ace; steps 2 and 12 | Same. |

## Its three kinds of service

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Quick call | A request from `lamp-exec` to the component that owns the thing, over a local socket, with one answer | Step 3 | Same. |
| Device operation | The same, with an id, progress, an end and a request to stop: following someone, a fade, a gesture | Gestures running on lamp-4ace without ids; steps 3 and 7 | Same. |
| Agent task | `lamp-tasks` | Step 5 | Same, through Harness's own commands. |

## Its twenty topics

Names as on the page. The publisher is the process that owns the thing published.

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| mic1 | `lamp-mic1`: frames and levels | Step 1 | Same. Its first consumers are sound and how near a voice is. |
| mic2 | Voice, as a copy made one step after capture | Step 1 | Same, with the page's own rule kept: the voice's path waits for nobody. What goes to a provider is admitted speech, not this topic (decision 13). |
| camera | Eyes: the newest frame by reference, with a counter | Step 1 | Same. |
| servos | Body: asked for and measured, faults | Step 1 | Same. |
| co2 | Nothing | When a lamp has the sensor | lamp-4ace has none that the build knows of. |
| gemini/transcript | Voice | Step 1 | Same. |
| deepgram/transcript | The second route | Step 11 | Same. |
| input.physical | `lamp-controls` | Step 9 | Same. |
| control.privacy_state | Voice's authority core, as a mirror | Step 1; the switch at step 9 | Same. |
| perception.speech_candidate | Voice's detector | Step 1 | Same. |
| people | Eyes: every face, its marks, whether it faces the lamp | Step 1 (one face is published today, by other means) | Same. |
| identities | Eyes | Step 4 | Same. |
| perception.sound | A consumer of `mic1` | Step 7 | Same. |
| perception.vision | The look, and what Eyes can say without the cloud | Step 7; the look is built and unheard | Same. |
| perception.environment | Nothing | When a lamp has such sensors | As `co2`. |
| conversation.transcript | Voice: each exchange it took part in | Step 1 | Same. Memory subscribes to it and is never called by Voice. |
| body.state | Body, and Voice for the ring and the speaker | Step 1 | Same. |
| situation.revision | `lamp-situation` | Step 7 | Same. |
| execution.outcome | `lamp-exec` and `lamp-tasks` | Steps 3 and 5 | Same. |

## Its nine walkthroughs

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Fast conversation | Voice | Running on lamp-4ace | Same. |
| A quick device call | The Live model's tool call, `lamp-exec`, the light | Step 3 | Differs: the command comes as the model's tool call. On the Live route a transcript is never a command (decision 4). |
| An ongoing operation | "Follow me": `people`, `identities` and `servos` into a behaviour in Body, started and stopped through `lamp-exec` | Step 7 | Same. |
| A long agent task | `lamp-tasks` | Step 5 | Same, with a spoken yes before it is sent (decision 7). |
| An agent asks a question | `lamp-tasks` to Voice, as a note to speak from | Step 6 | Same. |
| Interrupt speech | `lamp-controls` and Voice; the task goes on | Steps 6 and 9 | Same. |
| Gemini reaches its limit | Voice stays up and says it is down | Step 2; the second route at step 11 | Differs at launch: an honest "voice is down" first, the second route later (decision 9). |
| A computer disconnects | `lamp-tasks`: unknown is a real outcome | Steps 5 and 6 | Same. |
| Several consumers at once | The bus | Steps 0 and 1 | Same. |

## Its four classes of speech

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Casual answer or conversation | Voice | Running on lamp-4ace | Same. |
| Task acceptance, action confirmation or completion | A neutral "on it" in the native voice; "sent" and "done" from the receipt | Step 5 | The page left the policy open; decision 8 settles it this way. |
| “I need one detail” / pending question | `lamp-tasks` to Voice | Step 6 | Same. |
| Strict consistency for all speech | Not built | Not planned | Differs: holding every answer until its meaning is checked would slow all of them. |

## Reflex, behaviour, task

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| ReflexShort, immediate reaction. | Local code beside the hardware: Voice stops for a voice over it, Body gives way to a load | Running on lamp-4ace; the switch and the tap at step 9 | Same. |
| BehaviorBounded coordinated activity. | Body | Gestures, standing and turning to a face running on lamp-4ace; following at step 7 | Same. |
| Agentic taskOpen-ended goal. | `lamp-tasks` | Step 5 | Same. |

## What it brings from V1's command shortcut

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Deterministic command catalog and exact-command fast path | The 21 commands as the Live model's tools; V1's exact matcher for typed requests and the second route | Step 3 | Differs on the Live route only (decision 4). |
| Capability-filtered choices and finite parameter validation | `lamp-exec`: the model names a command and never a payload | Step 3 | Same. |
| Context and ambiguity safeguards | V1's regression cases as the test for the tools' descriptions | Step 3 | Same. |
| Failure-aware replies and handled-on-attempt behavior | `lamp-exec`: what is said is what was established, and a tried command is finished | Step 3 | Same. |
| Bounded semantic fallback | `lamp-decide` | Step 8 | Same. |

## Its two ways of running Deepgram

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Warm subscriber alongside Gemini | Not at launch | Step 11 | Differs: a second cloud listening all the time is a disclosure the owner has not chosen (decision 13). |
| Start STT when fallback is needed | The second route | Step 11 | Same. |

## What it says of Harness

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| The OS client pairs with one computer. It can address agents/sessions exposed by that peer. | Harness on the lamp reaches every machine of the account | Installed | Differs (decision 14). |
| Negotiated operations include turn send/stop, ordinary question answers, status/recap and Store workflows. | Harness's own verbs: start, list, show, wait, close | Step 5 | Differs in form, same in caution: nothing is assumed that Harness does not offer. |
| Result records are durable; the current stream replay cursor is in memory. | `lamp-tasks` keeps what it sent and what it has said, on disk | Step 5 | Same. |
| Host permission dialogs are desktop-only notices. Current voice paths refuse submission/answers while such a dialog is active. | Said once, never answered by voice | Step 6 | Same. |
| V1 direct remote-Hermes support is a separate route from Harness. | Not used | Not planned | Same. |

## Its six kinds of state

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Current situation | `lamp-situation` | Step 7 | Same. |
| Conversation journal | `lamp-memory` | Step 4 | Same. |
| Persona & durable memory | The character file; `lamp-memory` for what it is told to keep | Character running on lamp-4ace; step 4 | Same. |
| Execution records | `lamp-exec` and `lamp-tasks` | Steps 3 and 5 | Same. |
| Configuration, trust & calibration | Files written whole by the surround; today by hand | Step 12 | Same. |
| Engine working state | Stays on the owner's computers | Nothing to build | Same. |

## Its failure table

Each row is also a test: the thing is made to fail on the lamp and what the others do is counted.

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Live quota, authentication, network or protocol failure | Voice | Step 2 | Same. |
| Deepgram, text model or Gemini TTS unavailable | The second route | Step 11 | Same. |
| Jev timeout, malformed reply or stale decision | `lamp-decide`: the rules alone decide | Step 8 | Same. |
| Slow camera or inference consumer | The bus: a slow subscriber loses its own messages and nobody else's | Steps 0 and 1 | Same. |
| Agent host disconnects after dispatch | `lamp-tasks` | Steps 5 and 6 | Same. |
| Speaker/motor/ring worker loses authority or stalls | Voice and Body: one writer for each, restarted alone | Partly running on lamp-4ace; step 2 | Same. |
| Disk full, telemetry backlog or corrupt config | Bounded records in each component | Records running on lamp-4ace; step 12 | Same. |
| Restart or OTA during active work | The hand-over rule: body at rest before anything is switched | By hand today; step 12 | Same. |

## Its fifty-eight capabilities from V1

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| V01 Audio capture, device routing, pre-roll | Voice | Running on lamp-4ace | Same. |
| V02 Addressing, wake and follow-up focus | Voice, Eyes and the second microphone | Partly running on lamp-4ace; step 7 | Differs as for Attention (decision 7). |
| V03 Deepgram and alternate STT | The second route | Step 11 | Same. |
| V04 Realtime conversation and handoff | Voice; tools and the executive | Running on lamp-4ace; step 3 | Same. |
| V05 Speech synthesis and voice continuity | Voice's native speech; synthesis on the second route | Running on lamp-4ace; step 11 | Same. |
| V06 Interruption, output ownership and echo | Voice | Running on lamp-4ace, and its weakest part: a voice over it stops it 5 to 8 times in 10 | Same. |
| S01 Camera lifecycle and frame access | Eyes | Running on lamp-4ace; `camera` topic at step 1 | Same. |
| S02 Presence, local gaze and attention movement | Eyes and Body | Running on lamp-4ace; the facing vote at step 7 | Same; V1's vote at the start of speech is the model for it. |
| S03 Face recognition and enrollment | Eyes | Step 4 | No longer deferred: the owner wants the camera used to the full. V1's recognition models are not in the repository and have to be obtained. |
| S04 Speaker recognition and enrollment | Not planned yet | After faces | Deferred, as the page has it. |
| S05 Shared identity semantics and attribution | `identities`, and one memory for each person | Step 4 | Same. |
| S06 Facial-emotion observations | Not built | Deferred | As the page has it. |
| S07 Speech-emotion observations | Not built | Deferred | As the page has it. |
| S08 Activity and posture | Not built | Deferred | As the page has it. |
| S09 Sound level/startle | A consumer of `mic1` | Step 7 | No longer deferred. |
| S10 Environment and ambient light | Nothing | When a lamp has the sensors | As the page has it. |
| S11 Active look, read text, object search/follow | The look; reading and searching through it | The look is built and unheard; step 7 | No longer deferred. |
| S12 Guard mode and visual hazard alerts | Not built | Deferred | As the page has it. |
| S13 Observation ingestion, backpressure and context updates | The bus and `lamp-situation` | Steps 0, 1 and 7 | Same. |
| B01 Motion, calibration and position control | Body | Running on lamp-4ace, calibrated by hand for this lamp; files at step 12 | No longer deferred: motion is at launch (decision 11). |
| B02 Motion holds, contact, overload and deterministic limits | Body | Running on lamp-4ace | Same. |
| B03 Lamp utility and user light state | The light as a service: the owner's light apart from the face the lamp shows while it talks | Step 3 | Same. |
| B04 Scenes and expressive actions | Body's gestures; the scenes as tools | Six gestures running on lamp-4ace; step 3 | Same. |
| B05 Physical buttons/touch and tap-to-talk | `lamp-controls` | Step 9 | Same. |
| B06 Physical privacy switch | `lamp-controls`, onto the authority core | Step 9 | Same. |
| B07 Sleep, wake, shutdown and reset gestures | `lamp-controls` and the surround | Steps 9 and 12 | Same. |
| B08 Ambient life and local reactions | Body and Voice; what to do unprompted through `lamp-decide` | Small ones running on lamp-4ace; step 8 | Same. Saying hello first is allowed once it has been heard to behave. |
| B09 Music, volume, mute and Bluetooth | Volume and mute as tools; music and Bluetooth not built | Step 3; the rest deferred | As the page has it. |
| A01 Swappable engine and model | Harness and the engines on the owner's computers | A second engine at step 8 | Same. |
| A02 Goals, progress, steering and cancellation | `lamp-tasks` | Steps 5 and 6 | Same. |
| A03 Task continuity and truthful uncertainty | `lamp-tasks` | Step 5 | Same. |
| A04 Persona, durable memory and engine migration | The character file and `lamp-memory`; V1's facts moved across | Steps 4 and 12 | Same. |
| A05 Conversation and external-history continuity | What Voice carries across a new session, and the journal | Carried for ten minutes: running on lamp-4ace; step 4 | Same. |
| A06 Per-person history, wellbeing, habits and suggestions | `lamp-memory`, one store for each person | Step 4; habits and nudges deferred | As the page has it. |
| A07 Schedules and delayed initiative | `lamp-schedule` | Step 10 | Same. |
| A08 Skills, tool schemas and action authority | `lamp-exec` and the tools | Step 3 | Same. |
| A09 Fast device intents and Jev command selection | The tools; `lamp-decide` | Steps 3 and 8 | Differs on the Live route (decision 4). |
| A10 Jev skill preloading | Not built | Deferred | As the page has it. |
| I01 Web/MQTT conversation, files and device commands | Not built | Deferred, with the operator's pages | As the page has it. |
| I02 Runtime-owned messaging sessions | Not built | Deferred | As the page has it. |
| I03 Harness tasks, results, voice mode and Jev chooser | `lamp-tasks` | Step 5 | Differs (decision 14): no pairing; the agent named, or else the one in focus. |
| I04 Buddy desktop tools/events and Jev suggestions | Not built | Deferred | As the page has it. |
| I05 Connectors, MCP and later FO authorization | On the owner's computers | Deferred | As the page has it. |
| I06 Remote perception service | Not built | Deferred | As the page has it. |
| O01 Boot/service lifecycle and truthful readiness | systemd units and a stated readiness | Step 2; starting at power-up at step 12 | Same. |
| O02 Hardware manifests, overlays and calibration | The surround | Step 12 | Same. |
| O03 First setup and reprovisioning | The surround | Step 12 | Same. |
| O04 Config storage, defaults and live changes | The surround | Step 12 | Same. |
| O05 Device/authentication credentials and local API trust | The surround | Step 12 | Same. |
| O06 Network recovery, local discovery and time | The surround; the time of day is told to the voice already | Step 12 | Same. |
| O07 OTA, version provenance, staged rollout and rollback | The surround | Step 12 | Same. |
| O08 Provider limits, watchdogs and degraded operation | Voice | Step 2 | Same. The page calls it the first release blocker and so does the build. |
| O09 Metrics, tracing and support logs | Each component's bounded record; the bus's recorder | Records running on lamp-4ace; steps 0 and 12 | Same. |
| O10 Retention, forgetting and whole-device reset | `lamp-memory` forgets on being told; reset in the surround | Steps 4 and 12 | Same. |
| D01 Safe local simulation and isolated state | Host tests and fixtures in each crate | Running on lamp-4ace on the builder's machine | Same. |
| D02 Contract tests, fault replay and latency gates | A gate at each step, and failure tests written by the page's author | Each step | Same. |
| D03 Operator UI, API docs and diagnosis | The surround | Step 12 | Same. |
| D04 Engine packaging, updates and local tool compatibility | Nothing on the lamp but Harness | Nothing to build | Same. |

## Its seven slices, against the build's steps

| The page's slice | The build's steps |  | Same, or how it differs |
|---|---|---|---|
| 1 · Contracts & scope | Steps 0 and 1 |  | Same in content. The contracts are written as each topic and service is built, not all first. |
| 2 · Continuous local OS | Step 2 |  | Same. |
| 3 · Voice + shared state | Steps 3 and 11 |  | The second route is moved late (decision 9). |
| 4 · Executive + delegation | Steps 3, 5, 6 and 8 |  | Same. |
| 5 · Selected senses + body | Steps 1, 4 and 7; the body is running |  | Moved early: the owner wants the senses used to the full. |
| 6 · Product operations | Step 12 |  | Moved last, on the owner's word. |
| 7 · Matched qualification & cutover | At each step, on the lamp |  | Same in intent. |

## Its six targets

Taken as targets. The last column is what the lamp does today.

| The page's target | In the build | When | On the lamp today |
|---|---|---|---|
| Recognized touch → local feedback | `lamp-controls` | Step 9 | Not measured: nothing reads the touch pad yet. |
| Speech-stop request → local output revocation | Voice | Running on lamp-4ace | Not measured to the writer. A played "Lamp, stop" was heard within 0.1 s with the head still. |
| End of utterance → first useful Live audio | Voice | Running on lamp-4ace | First sound 1.3 s after the lamp decides a person has stopped, about 1.9 s after their last word. A matched V1 number has not been taken. |
| Decision overhead | Voice | Running on lamp-4ace | Holds: no decision model on ordinary chat. |
| Forced Live terminal failure → degraded/fallback state | Voice | Step 2 | Not met today: the service ends and systemd restarts it in about four seconds. |
| Task/operation correctness | `lamp-exec` and `lamp-tasks` | Steps 3 and 5 | Nothing to measure yet. |

## Its eleven tests

The page's author writes these as failure tests without reading the code under test (decision 1). This is where each will bite.

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Two subscribers, one stalled; camera disabled while sound and voice continue | The bus's own tests; then on the lamp | Steps 0 and 1 | Same. |
| Privacy changed during capture, inference, upload, queued audio and body motion | Voice's authority tests (host); on the lamp with the switch | Running on the host; step 9 | Same. |
| Live audio with late, missing, duplicated and reordered transcripts and tool calls | `lamp-exec`: one request, carried out once | Step 3 | Same. |
| Quota failure mid-answer, no terminal marker, STT outage, shared quota exhaustion | Voice | Step 2; the second route at step 11 | Same. |
| Remote admission with lost acknowledgement; reconnect and restart | `lamp-tasks` | Step 5 | Same. |
| Task question answered while another conversation is active; permission notice | `lamp-tasks` and Voice | Step 6 | Same. |
| Touch or stop during speech while a remote task continues | `lamp-controls`, Voice, `lamp-tasks` | Steps 6 and 9 | Same. |
| Stale person track, conflicting identity, ownership change during a gesture, lost motor feedback | Eyes, Body | Steps 4 and 7 | Same. |
| Control frozen while a heartbeat lives; supervisor killed; a stalled teardown | Each process killed and frozen on the lamp, the others counted | Each step | Same. |
| Cold boot, missing calibration, disk pressure, interrupted config write, update, reset, cutover | The surround | Step 12 | Same. |
| Naturalness: false interruptions, unwanted speech, repeated glances, light restored, respectful updates | Read from the lamp's own record of real use, and by the owner's ear and eye | Running, every day | Same. |

## Only in its drawings

The first reading of the page took its text and not the words inside its ten drawings. Two things are only there.

| On the page | In the build | When | Same, or how it differs |
|---|---|---|---|
| Owner feedback: teach ("when I do X, do Y"), correct ("I meant this instead"), approve (thumbs up or down) | What is taught and corrected is kept by `lamp-memory` for that person and applied by `lamp-exec` and `lamp-decide`; a thumbs up or down comes by voice or by the touch pad | Steps 4, 8 and 9 | Was missing from the build's plan until this reading. The page keeps training a model apart from this, and so does the build. |
| Goals: what to achieve, priorities, constraints | Kept by `lamp-memory` as the owner's standing wishes ("do not interrupt me before ten"), read by `lamp-decide` and checked by `lamp-exec` | Step 8 | Was missing from the build's plan until this reading. |
| The servos topic carries state; movement is asked for through a service | Body publishes `servos`; `lamp-exec` asks Body to move | Steps 1 and 3 | Same. |
| Final writers behind one local authority: privacy, stop, ownership, limits, expiry | One writer for each piece of hardware, inside Voice and Body | Running on lamp-4ace | Same. |

## What is taken from the page as it stands

Its way of seeing the lamp as an agent in an environment, with what it
observes kept apart from what it believes; its vocabulary (topic, service,
observation, policy, action, reflex, behaviour, executive, agent, engine,
session, admission, completion, delivery, local privacy); its four
recommendations on learned policies (the decision model stays replaceable,
behaviours are installed code, the Live model and the agents' engines each
do their own job, and nothing is trained on a deployed lamp without a
bounded task, a reward and a test set first); its division of work among
local code, a decision model, a language model and an agent; and its
rule that being accepted, being done and being told are three different
things. None of these is changed.


## Where the build differs from the page, in one place

Each was decided by the product owner on 2026-10-13 or shown by the lamp
that day; the page's author is asked to fold them in.

1. **Whether speech is for the lamp** is decided by the Live model with
   local evidence narrowing what it hears; nothing admits by meaning
   before it. It follows the room: called by name where people are
   talking, anything said near it where it is quiet.
2. **A command comes as the model's tool call,** never from its
   transcript, on the Live route.
3. **Promises are split, not held:** "on it" at once, "sent" and "done"
   from the receipt.
4. **With the provider down the lamp says so;** the second speech route is
   later.
5. **No second cloud listens all the time.**
6. **Harness runs on the lamp.** Nothing of V1's pairing is ported.
7. **The decision model is not on the spoken path** until it wins there.
8. **The bus has no process in the middle,** and is built first.
9. **The senses are used to the full,** the camera above all: who is
   there, whether they face the lamp, what they show it. The page deferred
   most of this.
10. **Memory across days, for each person.**
11. **One builder,** with the page's author reviewing and writing the
    failure tests; setup, updates and the operator's pages last.

And three things the page does not have that the lamp taught:

- **What a new session is told is acted on at its first turn,** so each
  item told is tested there ([brain-build.md](brain-build.md), change 2).
- **Nothing restarts the voice while someone is speaking.**
- **A picture in a session slows every later answer,** so what can be seen
  on the lamp is seen on the lamp.

