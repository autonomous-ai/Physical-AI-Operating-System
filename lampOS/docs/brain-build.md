# Building the brain: who builds what, and what to change in the draft

Written 2026-10-13 by the owner of the voice runtime, in answer to the
"lampOS V2 brain architecture review", draft 0.5 of the same day (called
"the draft" below). It proposes who builds which part, four changes to the
draft, an answer to each of the draft's seven open decisions, and a build
order. Nothing here is approved. Everything said about the lamp is from
lamp-4ace on 2026-10-12 and 13; where a number is one run or a silent probe
it says so.

## Who builds what

Proposed, after the product owner's point that whoever builds the
conversation should control the whole experience, "starting with voice, body
language, head language, light, and then do the tasks and then get back to
users".

| Part | Builder | Why |
|---|---|---|
| Everything a person at the lamp experiences: voice, body, head and light; deciding what a request is; handing work to an agent through Harness; following the work; asking the agent's questions; delivering the result; the lamp staying up when the provider fails | The voice owner | It is one experience, and each failure heard in the office on 2026-10-13 came from a join between two mechanisms that were each right alone (below). It can only be proven on a lamp. |
| What nobody talks to: setup, network repair, configuration, updates, reset, the operator's pages, moving V1's data across | The draft's author | Large, separable, and none of it shapes a conversation. |
| The architecture document, and review of the experience work against it | The draft's author | Several faults of the voice owner's reached the office in one day. A reviewer who is not anchored on that code is worth having. |

The Harness computer side, the agent engines, the provider and Jev stay
where they are. The lamp's Harness client is built to the Harness team's
existing contract ([../../docs/harness.md](../../docs/harness.md)); nothing
here invents a pairing flow, a transport or a credential.

Between the two builders there is then one thin interface, to be written
before either starts: where configuration and credentials are read from,
what "ready" means for each capability, how an update stops the old
hardware writers before the new ones start, and where diagnostics go.

## Four changes to the draft

### 1. Whether speech is for the lamp is a measured problem with its own slice

The draft draws attention as a component that admits input before the
provider and lists "how does Lamp know it is being addressed?" as a product
decision. On the lamp nothing can admit by meaning before the provider
except a wake word or a tap, which the product does not want. What decides
is the Live model's own judgment of the audio, narrowed by local evidence
that costs nothing: how loud, how long, whose turn it is.

What that gives, in a working office, from the lamp's own record:

| Setting | Heard | Answered |
|---|---|---|
| Level floor -40 dBFS, character answers what seems meant for it (about 50 minutes) | not counted | about 30, to colleagues |
| Floor -28, character answers only when called or already talking (16:05 to 16:37) | 87 | 6, of which 4 to someone who came and talked to it |
| The same with the fault described in change 2 (16:37 to 16:47) | 70 | 12 |
| The same, what it remembered cleared by hand (16:47 to 16:55) | 71 | 2, each the first thing a new session heard |
| The same, repaired build (16:55 to 17:15, the office emptying) | 25 | 0 |

Two things follow for the architecture.

- A wrong answer today is an unwanted sentence. Once a request can become
  work on someone's computer it is an unwanted task. So the evidence asked
  for must rise with the consequence: chat on the model's judgment; a
  change to the lamp itself (light, a look) when called by name in that
  utterance or inside an exchange; anything that leaves the lamp also on a
  spoken yes, until a measured false-accept rate says otherwise.
- The local evidence is not what the draft assumes. The voice microphone's
  own processing evens loudness out: colleagues talking near the lamp (how
  near is not known) arrived at a median of -22 to -27 dBFS, and a person
  speaking to it at its desk at -19 to -24, so a level floor cannot tell
  them apart. The second, plain microphone rose only 2 to 8 dB over its
  resting -51.5 dBFS for those colleagues (ten minutes, levels only). It
  has not yet been compared with a person at the desk. If it separates them
  it is the proximity cue, and the reason to carry both microphones as
  topics.

Change: an addressing slice before the executive, with a recorded set of
office situations, the two rates to meet (uninvited answers per hour of
office talk; calls by name answered), and the rule above tying evidence to
consequence.

### 2. What a session is told is an interface, and every field of it is tested on a first turn

A Live session lasts 3 to 39 minutes here and its successor knows only what
it is given at opening: the character, the time of day and a note of the
last exchanges ([conversation-thread.md](conversation-thread.md)). The draft
adds a shared situation, pending questions and task state to that. All of it
is read by the model as a cue to act.

What happened: the note said "this is the conversation you are in, carry
on". With a character that answers only when called or already talking,
five new sessions in a row answered the first or second thing they heard,
which was colleagues' talk. Each wrong answer became the newest exchange
told to the next session, and the fifth answered nine times in two minutes.
A safety net that renewed a session after eight unanswered requests made it
worse: in an office a lamp that rightly answers nobody is nothing but
unanswered requests, so it renewed every three minutes. Both are changed
(the note now tells an exchange older than 45 seconds as finished; the net
acts at most every fifteen minutes). Since then fourteen new sessions have
spoken to nobody, but only three of them heard two utterances before the
office went quiet, so this is not yet shown.

Two more facts of the same kind:

- Of six new sessions with nothing told to them at all, two answered the
  first sentence they heard, a colleague's to a colleague ("Está bien,
  gracias." got "You're welcome!"). Being new is itself a cue. A switch
  now lets each new session be told, in a turn that asks for no answer,
  that nobody is talking to the lamp at the moment
  (`crates/live/src/session_fact.rs`); it is off, and what it does to a
  first turn is unmeasured.
- One picture anywhere in a session made every later answer about 0.45 s
  slower (medians 1.32 s and 1.78 s over six balanced runs). What is put
  into a session can cost speed for the rest of it.

Change: list what may be given to a session (at opening, and while it runs,
below), who may add to the list, and require for each item a test of what
the lamp does with the first thing it hears afterwards and a speed
comparison with and without it.

### 3. For commitments, split the speech; do not hold the audio

The draft says strict commitments need the relevant speech "held or
generated from approved text before playback" and leaves the policy open.
Holding native audio until its meaning is known delays every answer; the
lamp answers 1.3 s after it decides a person has stopped (about 1.9 s after
their last word) and that is the experience to protect.

Three mechanisms already in the voice runtime do the job without a hold:

- **A turn the lamp begins from a note.** One complete written turn in
  place of speech (`InputSender::try_written`,
  [speaking-first.md](speaking-first.md)): the model says in its own voice
  what the note gives it to say. V1 delivers Harness results the same way
  ([../../docs/harness.md](../../docs/harness.md), "Spoken Harness
  updates"). Silent probe: a hello 0.91 s after the note.
- **The lamp's own opening words as a signal.** When its answer begins
  "Let me look", the runtime takes the look
  (`speak_up::asks_to_look`). Silent probes: 4 of 4 seeing questions opened
  that way. The same shape fits "on it": a fixed neutral phrase that
  promises nothing and tells the runtime a request was understood as work.
- **A fact given without asking for an answer.** One written turn left
  incomplete (`InputSender::try_context`, new on 2026-10-13). Silent probes,
  one run each: told a name this way the lamp said nothing back, then
  answered "do you know my name?" with it, 0.93 s after the question;
  without the fact it said it did not know.

Change: the policy is a neutral acknowledgement in the native voice, and
"sent", "it needs to know…" and "done" are spoken from the receipt through a
written turn. The test is a count: over N task requests, how often the
native voice claimed something before its receipt existed. None of the three
has yet run through the coordinator on a lamp with a person listening.

### 4. Topics are contracts first, and Jev starts where no utterance does

The draft's topic graph (19 families, a 12-field envelope, a queue per
subscriber) is right as vocabulary. The lamp today has several processes that
already pass latest values and ordered events to one another by the
simplest means that fits each: a datagram for the body's mood, a small file
for whether someone is there, a bounded channel for audio, a record on disk
for outcomes. Each is a topic in the draft's sense.

Change: write each topic's contract (name, freshness, what is lost and how
that is known) as the draft does, and give it a transport only when a
second real consumer exists. Nothing is added between the microphone and
the provider, which the draft also says.

For Jev: a spoken request already reaches the runtime as the model's choice
among the tools declared to it. Putting a second model behind that choice
adds a hop and a second opinion, and should be shown to help on recorded
requests before it is built in. Where no utterance starts the decision
there is no such choice, and that is where a decision model has work from
the first day: whether to speak up, which agent gets a task, when a result
may interrupt. The lamp's rule for saying hello is the deterministic
baseline to beat ([speaking-first.md](speaking-first.md)).

## The seven open decisions

| Decision | Proposed answer | State on the lamp |
|---|---|---|
| How does it know it is addressed? | No wake word, no tap. In a room that is talking: when called ("Lamp", "Hey Lamp") or inside an exchange. In a quiet room with someone at its desk: whatever is said near it. Audio reaches the provider before the decision, because the provider makes it; say so plainly to users. Targets to agree: at most one uninvited sentence an hour in a talking office, and 19 of 20 calls by name answered. | Called-only is running. Quiet-room listening is not built; as silent probes with a written line in place of speech, one run each, the fact "the office has emptied" made it answer an uncalled question it otherwise left, and "the office is full" got "..." with no sound. |
| Which senses at launch? | The voice microphone for speech; the plain one for how near a voice is, if the comparison holds; the camera on the lamp only, for whether a face is there and where, plus one picture sent when the lamp has said it will look. No recognition of who, no reading of mood. | Both running except the second microphone, which nothing reads yet. |
| Is motion at launch? | Yes, what runs now: it stands when spoken to and folds when left, turns up to 44° to a face, looks away to think, and makes six small gestures chosen from the first words of its answer. | Running. Not yet: started at power-up, behaviour at power-up checked by hand, a long hold measured (the shoulder warmed 42 to 46 °C in eight minutes). |
| Agents off the lamp? | Yes. No general agent on the lamp. | Nothing to change. |
| Voice commitment policy? | Change 3. | Pieces exist; unproven aloud. |
| First decision model and engines? | No decision model on the spoken path until it wins on recorded requests; Jev first for decisions no utterance starts. Engines as the draft says: the V1 one first, a second through the same contract. | Not started. |
| First Harness scope? | One paired computer, the existing contract, permission prompts stay on the computer. | Not started in Rust. |

## Build order for the experience

Each step ends with something a person can try at the lamp.

1. **The lamp outlives the provider.** No exit for a refused quota, a
   stalled link, a full input queue or a slow microphone read: the voice
   goes unavailable, the light says so, the lamp keeps its controls and
   comes back by itself. Today each of those ends the service, which
   systemd restarts in four seconds; the stalled link did it at 16:50 on
   2026-10-13. Try: pull the network for five minutes.
2. **The model can ask for something.** Tool calls parsed, a typed request
   out, a result back; first for the lamp itself (light level, a look).
   Measure what declaring tools costs ordinary answers, as was done for
   search (none, two runs each way). Try: "Lamp, dim a little."
3. **One task, out and back.** The Rust Harness client to one computer, a
   record of the task on disk, the neutral acknowledgement, "sent" from the
   receipt, the result spoken at a pause, and what the body and light do
   while work is running. Try: "Lamp, ask my coding agent to…".
4. **Questions and interruptions around a task.** The agent's question
   asked at a pause and the answer tied to that question; stopping the
   lamp's speech never cancels the task.
5. **The room, told to the voice.** Quiet or busy, someone arriving, a task
   finishing, each as a fact with its first-turn test (change 2).
6. **A decision model for what no utterance starts, and a second engine.**

## What the draft's review of the voice code predates

The draft read the voice branch about a day before this was written. Since
then, on that branch: the conversation is carried across a new session and
across a restart of the service; web search is on; the lamp can begin a
turn itself and take a look when it says it will; a sound too short to be
speech never reaches the model; the voice has real-time priority (a load
that ended it four times in forty seconds ends it not at all). Still as the
draft says: tool calls are rejected, what a person said is tied to an
answer only by timing, and a refused quota ends the service after a
two-minute wait.

## Not known

- Whether the changed note keeps a new session quiet in a talking office.
- Whether the second microphone separates a person at the desk from one
  two desks away.
- What tool declarations do to speed and to how readily the model acts on
  what it overhears.
- Whether a fact given mid-session changes how the lamp treats speech it
  hears, as it changed how it treated a written line.
- How long this takes. It is weeks; the voice itself is not finished
  (interrupting it, a group at the lamp, speaking first are open).
