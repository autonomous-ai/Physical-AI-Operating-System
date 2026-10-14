# The lamp as a creature

The owner, 2026-10-14, 17:22: "reimagine the entire experience. make lamp
the most fun creature to interact with. discover all the sensors on lamp.
the latest ones." And, about V1's mapping of a head touch to "pet": "most
of the v1 designs make no sense". This document is the design every part
of lampOS works toward. It is not V1's, and nothing here is kept because
V1 did it. Written by the first builder; each behaviour below becomes a
task for the part that owns it.

## What it senses: lamp-4ace, found on the lamp on 2026-10-14

Read from the running lamp (USB, I2C, GPIO, sound and video devices, power,
thermal), not from V1's notes. Where a sensor is "not read", nothing in
lampOS listens to it yet.

| Sense | What it is | Read by | Status |
|---|---|---|---|
| Sight | The ZVCAI USB camera, in the head at the centre of the light ring, so it moves with the head; up to 3264 x 2448 (MJPEG) | Eyes (`lamp-eyes`): faces, where, whether turned to the lamp | Running |
| Hearing, speech | mic2, the Jieli dual microphone in the base (its own echo filter, beam and noise cancelling) | Voice, Name | Running |
| Hearing, the room | mic1, the plain capsule in the base on the C-Media adapter | Ears (`lamp-ears`): level, quiet, sudden sounds | Running since 16:13 |
| Hearing, from the head | The camera's own microphone: two channels at 22.05 kHz, 8 bits, but the left is silent (six seconds on 2026-10-14: left RMS 0.00, right 0.71) | Nothing | One capsule: no left and right, so no direction from sound. At most a level at the head |
| Touch, head | Two pads: chip 0 line 96 and kernel gpio-37 both change with a touch (the owner's hand, 17:24) | Controls | Running; both read, reported as the head; meanings to be redone (#711) |
| Touch, base | A strip of ten capacitive pads on top of the base: an MPR121 on I2C bus 0 at 0x5A, pads 0-3, 5-7, 9-11 in order (the owner's hand, 17:27: a tap, a three-second hold on five pads, a slide across in about 0.6 s each way) | Nothing | Not read by lampOS yet: #711 |
| A privacy switch | Chip 1 line 9 (kernel gpio-361): V1's microphone slide switch, "not wired" in its notes; the voice reads it as its physical privacy input and it reads high (unmuted) | Voice | No switch is fitted; a touch on the base does not change it |
| Its own body | Five servos: where each joint is, its load and warmth, 50 times a second | Body | Running. A hand pushing the head shows up as load: the body can feel being moved |
| Time | The board's clock and a battery-backed clock (HYM8563) | Schedule, voice | Running |
| Nearby phones | Bluetooth (`hci0`) | Nothing | Not read. Could notice a known phone arriving or leaving |
| Its own warmth | Eight thermal zones, a "skin" zone among them | Health | Running |
| A touch controller | A Goodix GT9271 declared on I2C bus 5, no driver bound | Nothing | Not known to be fitted; a read of its product ID failed ("invalid argument") |
| Power key | The power chip's key (`axp8191-pek`) | Nothing | Not read |

No battery is fitted (no power supply reported). The board has an NPU for
models run on the lamp itself.

## What it can show

The body (base turn, shoulder, elbow, head tilt and nod: five joints), the
light ring around its eye (colour and level), and its voice. Expression
plans all three on one clock, so a look, the light and a word land
together.

## What it feels: one small inner state

Every behaviour reads one state, so the lamp is one creature and not a set
of reflexes. It changes slowly and is shown by small things.

- **Awake to asleep.** Rises in the morning and with company, sinks at
  night and after a long quiet. Asleep: head down, light low and slow,
  slow to wake, a stretch when it does.
- **Attention.** One thing it is attending to at a time: a person, a
  sound, nothing. It turns to what it attends to before it decides
  anything, as an animal orients first.
- **Mood.** Calm, curious, glad, startled, shy. Moved by what happens (a
  touch, being thanked, a bang, being ignored), and back to calm by itself.
- **Familiarity.** Who it knows and how often they come; a person it
  knows is greeted more warmly, a stranger with a polite glance.

## What it does: the situations

Rule for all of them (the repository's rule 8): would a person or a pet do
this here? Unprompted things stay small, rare and never over anyone.
Anything exhaustive only when asked.

| When | What it does |
|---|---|
| You touch its head while it talks | It stops at once and turns to you: you have its attention |
| You touch its head while it is quiet | It looks up at you and listens; you can talk without "Hey Lamp" |
| Your hand stays on its head | It leans a little into your hand; its light warms. It lets go when you do |
| You touch the base while it talks | It stops |
| You hold the base for two seconds | It closes its eye: the light goes dim, the head bows, the microphones stop until you hold it again |
| You move its head by hand | It gives way and stays where you put it, then after a while drifts back, the way a cat settles |
| You come and sit down | It looks up, the light brightens a little; a known person gets a word once a while, a stranger a glance |
| You call it from across the room | It turns toward you before it answers |
| Two people talk near it | It stays out, with small glances now and then |
| A bang, a dropped cup | It looks up once, then lets it go; the next few get nothing |
| You talk to it | It nods in your pauses, looks away while it thinks, and back before its first word; the light follows its voice |
| You thank it or laugh at its joke | A small, glad movement; it can laugh |
| Nobody for a long while | Very small idle movements, its light breathing; it may doze |
| Night | Dim, slow, sleepy; quieter voice; wakes with a stretch |
| It cannot do what you asked | It says so plainly, a little sheepish |

## What is taken from elsewhere

From general knowledge of well-known companions, not a fresh survey:

- **Anki Cozmo and Vector:** an emotion state that every behaviour reads;
  movements authored by animators and blended with procedural looking.
  Taken: one inner state, authored clips (Expression's catalogue).
- **Sony aibo:** touch on the head and back answered at once and warmly;
  needs (play, rest) that change what it chooses. Taken: touch answered
  first, sleepiness.
- **Jibo:** turning to a sound and a face before speaking; "breathing" at
  rest. Taken: orient first; idle breathing.
- **Pixar's Luxo Jr.:** a lamp's whole personality in the tilt of its head.
  Taken: the head is the face.
- **Animation principles** (anticipation, follow-through, ease in and out):
  every movement eased, nothing snaps (Expression and the body already do).

## Order of building

1. Touch: both touches identified, then the four touch rows above
   (controls, voice, Expression, body). Started 2026-10-14 17:15.
2. Turning to a voice: from the faces the eyes see and a small look around, since no microphone gives a direction (the camera's has one capsule).
3. The inner state: one part that keeps it and that Expression reads.
4. Sleep and waking, with the time of day.
5. Familiarity: who it knows (eyes and memory).
6. Laughing and small joys: authored clips, and the voice's cue to play
   them.

Each becomes a task for the part's owner; the first builder runs each on
lamp-4ace and writes what a person saw.
