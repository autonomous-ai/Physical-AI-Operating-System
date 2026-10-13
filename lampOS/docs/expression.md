# Expression

Written 2026-10-14. **A design decision record: no part called Expression
exists yet.** What it describes runs today in a rougher form, split between
two programs, and the last column of each table says what is built.

Expression is the one part that decides what the lamp shows: its posture,
where its head looks, small gestures, signs of life, and the light's face,
all in time with its voice. The owner's words for it: a component "that
coordinate all things like body language (servos), head movements (nod,
shake, etc), facial expression (light), voice, eye contact, etc to make sure
all movements/actions are in sync and very much human like because that's
how human coordinates all parts of our bodies". The project's rule for all of
it: robot behaviour must read as a living thing, not a machine, and what
nobody asked for stays small.

## Expression and Actions

Two ways the lamp moves, kept as two parts because they behave differently,
as they do in a person: a deliberate act is decided, selected and commanded;
expression runs on older, faster pathways and is planned together with
speech. Both end on the same muscles.

| | Actions ([actions.md](actions.md)) | Expression |
|---|---|---|
| What starts it | A decision: "turn the volume down" | Nothing decides it; it runs all the time |
| Its shape | One thing: started, finished, checked, reported | Continuous and never finished |
| Needs a model | Yes, something chooses it | No: it has to work in milliseconds |
| If it fails | The lamp says so | The lamp looks stiffer; nothing else breaks |

The body program and the light's owner take requests from both. A
deliberate act wins for the part it needs, and expression carries on around
it. A lasting setting asked of Actions ("make your light blue") is a setting
Expression draws with, not a second command to the light.

## What runs today

| What | Where | How it is decided |
|---|---|---|
| Resting, listening, thinking, speaking, and how loud the lamp's own voice is now | The voice's coordinator, from real capture and playback events | It is the state everything below follows |
| The light's face for each state, pulsing with the voice | The voice (`choreography.rs`, `ring_face.rs`, the ring worker) | From that state, in the same process |
| Stand when spoken to, fold when left; tilt while listening; look away to think; return and nod lightly on its own voice | The body (`gesture.rs`), told the state every 40 ms over a private socket (`body_link.rs`) | A pose for each state |
| Turn toward a face, in small corrections | The body (`gaze.rs`), told where the face is by the eyes | Follows the largest near face |
| One of six small gestures | The body (`clips.rs`), asked by the voice from the first words of an answer | A nod with "yes", a shake with "no", a greeting |

It looks in step because every piece follows one state. It cannot do more
than that: there is no shared clock, so nothing can land on a word; no
arbitration, so two things that want the head are settled by whichever
code runs last; and anything richer has to be written in two programs.

## What is taken from the best expressive robots

From a study made on 2026-10-13 and kept outside the repository, each
claim with its source: Apple's ELEGNT lamp, Disney's BDX character, Anki
Vector, Jibo, Reachy Mini, Spot's choreography tooling, the SAIBA and BML
work on virtual agents, and the experiments on gaze and nodding in
conversation. The findings are the study's and were not re-measured here.

| What they do, and the evidence | What the lamp does about it | Built |
|---|---|---|
| One author for everything expressive, one owner for each motor and light (every shipped system surveyed) | Expression is the only part that asks the body and the light to show something; the body and the light's owner keep the hardware and its limits | No: the voice and the body each decide part |
| Layers that are summed: a base posture, where the head looks, small gestures added on top, signs of life, the light | The same five, low to high, summed and clamped before the body's own smoothing | Poses per state only |
| One clock, and for speech it is the sound actually played (Vector uses the audio stream; Jibo orders by word timing) | Each request carries the time to apply it, on the clock the voice schedules playback with; the body's measured lag is taken off the front | No clock is shared |
| A behaviour has a preparation, a stroke and a relaxation, and the stroke is what is timed (BML's sync points) | Gestures are clips with those three; with too little time before the stroke, the gesture shrinks, it does not land late | Clips play when asked |
| One head, several wants: a priority among gaze targets and small additions on top, blended so nothing has to finish first | Explicit request, then reaction to an interruption, then look away to think and back to hand over, then follow a face, then an idle glance; a nod or a tilt is added within a few degrees | The body follows a face or the state's pose |
| React to what happened or do nothing (nods at random moments made speakers stumble about twice as often) | A nod when the person pauses, at most one in several seconds; nothing on a timer | No |
| Look away to think, look back to hand over (answers judged more thoughtful; people waited about 0.6 s, not 0.3 s, before speaking again) | Kept, and made exact: away when the person stops, back as the answer's first sound is scheduled | Roughly, by state |
| The face changes first and the body follows (0.1 s and about 0.35 s in Disney's blends) | On being interrupted: the light is listening within 100 ms, the body eases to its listening pose in about 350 ms from wherever it is | Not measured |
| Expressive movement for social moments, plain movement for work; stillness when a person is concentrating (ELEGNT's users complained of motion during tasks) | Signs of life are scaled down and switched off while the person works | No |
| Vary inside a theme and mostly do nothing (alternative clips; most idle draws are empty) | Several versions of each small behaviour; an idle draw that usually does nothing | One clip each |
| Report what is being shown (BML's feedback) | A plain statement on the bus of what it is doing, for the record and for the model | No |

Not taken: emotion models with many dimensions (no evidence found that
they improve what people experience on a three-joint lamp), gesture
generated by a model from the audio (the published systems that ship use
authored clips and simple rules on loudness), and faces drawn with colour
alone (not readable to everyone).

## The design

One program, `lamp-expression`, a subscriber and a publisher on
[the bus](bus.md):

- **It listens to** `voice/state` and its edges; a voice-level topic to be
  added, each value stamped with when that sound will play;
  `humans/faces/tracked`; the room's state; the body's joints; and a
  lasting light setting from Actions.
- **It publishes** `body/command` and `light/command`, each with the time
  to apply it, and `expression/state`, what it is showing, in words.
- **The body and the light's owner** carry the requests out within their
  limits and publish what really happened. A request that is not renewed
  lapses, as ROS's command multiplexers have it.
- **If it stops,** the body and the light go back to following
  `voice/state` by themselves, which is what they do today. The lamp looks
  plainer and nothing else changes.

The first five behaviours, in the order they will be built: look away to
think and come back as the answer starts; a nod when the person pauses;
sway and light pulse on the sound actually played; on being interrupted,
light first and then body; stillness while the person is concentrating.
Three exist today in the rougher form above.

## How it will be judged

By watching it. A recording of the lamp through each of the catalogue's
situations, looked at by the owner and scored against the rules above
(does it react to something that happened; does the face lead; does it
ever move for no reason; does it ever look dead while someone is talking
to it). The questionnaires the field uses (Godspeed, RoSAS) need people
this project does not have yet.

## Not shown

Nothing here has been built or run as Expression. The numbers (degrees,
milliseconds) are the study's starting values and none has been tried on
this lamp. The body's lag from request to movement has not been measured,
and the timing design depends on it. The study could not verify several
sources (it lists them), and found no study of how a desk robot's motion
wears on someone over weeks.
