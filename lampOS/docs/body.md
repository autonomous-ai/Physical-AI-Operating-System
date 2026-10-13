# The lamp's small body

`lamp-body` (crate `lamp-motor`, `src/body.rs`, `src/gesture.rs`,
`src/bin/lamp-body.rs`) moves the lamp's head a few degrees for what the lamp
is doing: a lift and a tilt while someone speaks to it, a look away while it
has not answered yet, small nods on its own voice while it speaks, and back to
where it was found between turns. It is not the qualified motion worker
described in [motor-worker.md](motor-worker.md) and replaces nothing there. It
exists so that a lamp folded in its rest pose can be expressive within limits
narrow enough to check by eye.

Given a pose to stand in, it also raises the lamp on its shoulder and elbow
when someone speaks to it and folds it down again when it has been left alone
(see "Standing up"). Without one it is exactly the body described first.
Given eyes, it turns the head to the face they see while the lamp listens or
speaks (see "Looking at a face").

Status on 2026-10-13: the head motion has run on lamp-4ace for a night of
conversation (see "Second and later runs on the lamp"). Standing up and
looking at a face are built and run against simulated servos; neither has yet
run on a lamp.

## What it may do

- It commands three joints: head pitch (servo 5), head roll (servo 4) and
  base yaw (servo 1). The shoulder and the elbow are read on every step and,
  unless it was given a pose to stand in, never written.
- It writes three registers: the goal position (42) of those three joints,
  in one sync write, and once at start their goal speed (46) and acceleration
  (41), so that the servo itself is slow whatever this process sends. Unless
  it was given a pose to stand in it never writes torque.
- It starts only if all five joints answer, the three it commands have torque
  on, a clear status byte, a temperature and a load under their bounds, and
  the head's pitch is found between 1,760 and 2,020. A head pressed on the
  base (found at 2,095 on lamp-4ace on 2026-10-12) is refused, with nothing
  written.
- Every goal is within a fixed reach of its home: pitch 45 counts up and 15
  down, roll 30 either way, yaw 40 either way (11.4 counts to a degree). Home
  is the pose given with `--home PITCH,ROLL,YAW`, in encoder counts, or else
  the pose found at start. A joint found more than 60 counts from a given
  home is a reason not to start. From anywhere nearer, the head first eases
  home at the same speed as every other move, and until it is inside the
  reach it is never moved away from it. The head's pitch goal is also never over 2,030: on lamp-4ace
  the head crossed 1,766 to 2,043 with no load and met the base near 2,045 to
  2,056.
- The commanded pose moves at 50 counts a second at most (about 4 degrees a
  second). That is the speed the head was moved at when it was backed off the
  base on lamp-4ace on 2026-10-12 (47 counts a second at most), which it
  followed within 13 counts on the servo profile written here (goal speed 150,
  acceleration 50). No goal moves more than two counts between two writes,
  20 ms apart.

## When it stops

After every step it reads all five joints. The run ends, the present positions
are written as the three goals so that nothing keeps pushing, and nothing is
written after that, when:

| What is seen | Bound |
| --- | --- |
| Load on a commanded joint, six steps in a row (120 ms), for the fourth time in a minute, or again within two seconds of the joint giving way | pitch 240, roll 150, yaw 220 |
| A commanded joint away from its goal for ten steps | 30 counts |
| Any joint's status byte different from the one found at start | any change |
| The shoulder or the elbow away from where it was found, in a body that does not stand | 6 counts |
| A commanded joint's temperature register, on every step for a second | 62 |
| A read, or a write, that fails on every step for half a second (25 steps) | missing, late, malformed or unexpected bytes |

A load over its bound for six steps does not by itself end the run. The
joint gives way: its goal becomes where it is, and nothing is asked of it for
two seconds while the other joints go on. Then it moves again from there. If
it still reads over the bound six steps after it stopped pushing, something
is pushing it and the run ends; so it does the fourth time any joint gives
way within a minute. Each time is counted in the report (`eased`) and written
to the log. Until 2026-10-13 the first such load ended the run, and with it
all motion until someone started the body again: on lamp-4ace that happened
twice in a quarter of an hour, at 217 and at 190 on the head joint, with
nothing in its way either time.

The bounds for the head and the base were 170 and 150 until 2026-10-13.
With everything above in place the head still gave way now and then when
it began to lift from rest, reading 173 to 203 for a little over 120 ms
before it moved, and the base once at 208, turning back in the middle of
its look away. Nothing was in the way of either. Since a joint that pushes
without moving is put to rest within half a second whatever it reads, the
bound no longer has to stop that; it is there for a joint that is pushed
hard, and 240 and 220 are a quarter of what these servos can exert. To
start at all a joint must still be found under the old bounds (170, 150,
150): at rest it should be carrying next to nothing.

While the wire is failing nothing new is asked of the joints, and the
write that failed is tried again on the next step. Until 2026-10-13 three
failed reads in a row, or one failed write, ended the run. On lamp-4ace
that took one stall of the whole machine: at 04:00:40, with the lamp idle,
the ring's write overran its 10 ms budget and in the same second the body
found 41 bytes of a late reply on the wire and stopped after its third
missed read. It had run for 36 minutes without a fault.

A status flag that was already latched at start on a joint it does not command
is not a reason to stop or to refuse: lamp-4ace's elbow has carried `0x20`
since the head test of 2026-10-12. A change of that byte is.

It also ends on SIGINT or SIGTERM, after easing back to its home (three
seconds at most), and prints one JSON report either way.

## What moves it

The voice service names a mood and a voice level; the body decides the rest.
`lamp-live serve` sends one datagram to the path in `LAMP_BODY_MOOD_SOCKET`,
when that is set, on every change of mood and otherwise every 40 ms:

```json
{"mood":"resting|listening|thinking|speaking","level":0}
```

`level` is the voice in play on the scale of the ring's face (see
[ring-choreography.md](ring-choreography.md)): the person's while listening,
the lamp's own while speaking. While the lamp has stopped speaking for someone
who spoke over it (a duplex yield in force) the mood sent is `listening` with
that person's level, so the head turns its attention to them at once and goes
back to `speaking` if the reply resumes. And `speaking` is sent from the
moment the answer's first audio arrives, a few hundred milliseconds before
it is played, so that the head is back on the person when the first word
comes and is not making its largest move while the lamp speaks.

The head also settles when the talk around the lamp is not for it. After
two turns in a row that the lamp did not answer, `listening` and `thinking`
are no longer sent and the head stays at rest: a lamp beside two people
talking to each other looks up at the first sentences and then leaves them
to it. The ring still shows that it hears. The first answer the lamp does
give brings the head back, from the moment that answer arrives, and so
does half a minute without a turn. Nothing comes back, and a body that is absent,
slow or stopped changes nothing in the voice service. A mood older than 1.5 s
is rest. Anything that is not exactly that object is ignored.

| Mood | Wanted pose, as offsets from where the head was found |
| --- | --- |
| resting | none |
| listening | pitch 18 up, roll 26 to one side; the side alternates between exchanges |
| thinking | pitch 30 up, yaw 36 to the side it was tilted from, roll 12 the other way |
| speaking | pitch 8 up, and a nod of up to 7 either way when its voice is louder or quieter than its last second |

The commanded pose follows the wanted one with no overshoot, inside the speed
and reach above.

A joint that has arrived rests, in any mood: once the commanded pose has
been within 6 counts of the wanted one and slower than 15 counts a second
for 0.3 s, the joint's goal becomes its own position, and it is asked
nothing more until the wanted pose is 20 counts or more from where it is. A
servo asked for less than it can move pushes and stays where it is. So a nod
smaller than 20 counts does not happen, and at the reach lamp-4ace runs
with that is all of them: the head comes back to the person as the answer
arrives and then keeps still while the lamp talks. That is also when its
microphone needs it still ([duplex-listening.md](duplex-listening.md),
"Seven blocks"). Nodding as it speaks is still in the gestures and comes
back with a head joint that starts for a small move, or a reach wide enough
to make every nod a large one. At the reach used without `--reach` the
lift for listening is 18 counts and does not happen either; the tilt, the
look away and the way back do.

## Standing up

Not yet run on a lamp. Everything in this section is what the code does
against simulated servos.

With `--stand PITCH,ROLL,YAW,SHOULDER,ELBOW` (which needs `--home` and
`--folded-arm SHOULDER,ELBOW`, all in encoder counts) the body also commands
the shoulder (servo 2) and the elbow (servo 3):

- A mood other than rest raises the lamp from its folded pose to the one
  given. It stays up while such moods keep arriving and for `--awake-seconds`
  (90 without it) after the last, then folds down again. Asked to stop, it
  folds down first; that may take 3 s and twice the way.
- The way is one number from 0, folded, to 1, stood up. It moves at most the
  whole way in `--stand-seconds` (5 without it; 2 to 20), reaching that speed
  in half a second, and the two joints follow it eased at both ends, so
  neither starts or stops with a jerk. `--stand-share PERCENT` ends the way
  early: at 10 the shoulder moves about a degree and a half. It is there to
  bring a lamp up in steps the first time.
- The head keeps its folded pose until the arm has made a quarter of its way
  and is back at it before the arm comes down that far. Then its home moves
  to the standing pose's head pitch, roll and yaw, every gesture stays an
  offset from that home, and the head is carried along with it. The window
  that keeps the head off the base applies only while the head is beside the
  base, that is in that first quarter.
- The goals of all five joints go out in one sync write.
- An overload flag (`0x20`) latched on the shoulder or the elbow before the
  run leaves that servo holding with a fraction of its torque. Such a joint
  cannot carry the lamp, so at start the body writes torque on (register 40)
  to it once, and if the flag is still there, off and on. If it stays, the
  body does not stand and runs as a folded lamp; the report says why
  (`standing_refused`) and counts the writes (`torque_writes`). This is the
  only write of torque there is.
- It also does not stand, and says why, when it was given no home, when
  either joint has torque off, another flag, or is at 62 or warmer, or when
  the two are not found within 80 counts of one point on the way between
  folded and stood up. Found part of the way up, on that way, it takes the
  lamp as it is and stays up as if it had just been spoken to.

A shoulder or an elbow that reads over its load bound (`--arm-load`, 950
and 950 without it) for six steps, is more than 160 counts from its goal for
half a second, or reads 62 or warmer for a second sends the lamp back down
the way it came. It rests there for a minute whatever it is told, and after
the third time for the rest of the run; the report lists each time
(`retreats`). Only an arm still over its load or off its goal two seconds
later ends the run, held where it is. The load bound is high on purpose:
the old runtime's notes say these two joints read 830 to 1,000 in free
motion on another lamp, so following and warmth are the real guards.

The pose lamp-4ace is given is the old runtime's idle pose
(`hal/recordings/idle.csv`, first frame): head pitch 1,772, shoulder 2,657,
elbow 2,602, with this lamp's own roll and yaw. Its folded arm reads 2,048
and 2,047. On 2026-10-09 the old runtime saved a pose of this lamp with the
shoulder at 2,658 and the elbow at 2,650, so it has stood there; nothing
else about the pose has been checked on this unit, and the old runtime
reached it by moving all five joints together over five seconds.

## Looking at a face

Not yet run on a lamp. This is what the code does against simulated servos
and a simulated camera.

With `--gaze-socket PATH` and `--gaze YAW,PITCH` the body takes where a face
is from `lamp-eyes` ([eyes.md](eyes.md)) on that socket: one datagram a look,
`{"face":{"x":..,"y":..,"size":..,"turned":..}}` or `{"face":null}`. Anything
else is ignored, and only the newest waiting counts.

The camera is in the head, so turning toward a face moves the picture too.
The gaze (`src/gaze.rs`) therefore works in small corrections with stillness
between them:

- A sight counts only if no joint has been asked to move for 0.4 s, the lamp
  is listening or speaking, and it is not folded or folding. A picture taken
  while the head moved shows where the face was.
- After three such sights in a row with a face in them it takes their mean.
  A face within 0.15 of the middle across and 0.20 down is being looked at
  already. While the lamp speaks those are 0.35 and 0.40, since its head
  should be still then for its microphone.
- Otherwise it adds 70% of what would bring the face to the middle to an
  offset of base yaw and head pitch: `--gaze` says how many counts of each
  bring a face from the right edge, and from the bottom edge, to the middle,
  and their signs say which way each joint moves the picture. Under the whole
  amount on purpose: a number set too high still settles.
- The offset is added to what the mood wants and goes through the same reach,
  window, speed and load rules as every gesture; it is also bounded by
  `--gaze-reach` (250 and 120 counts without it). Then it waits to be still
  again.
- It never searches. With no face in the picture nothing is added, and when
  none has been seen for six seconds, or the lamp folds down, the offset goes
  back to nothing at 60 counts a second.

So a person who shifts in their chair is followed in a second or two, in one
or two moves; the look away to think, and back, happens as before, and the
head returns to where the face was.

The two numbers of `--gaze` have to be found on the lamp: nothing here knows
how wide its camera sees or which way its joints turn the picture. The old
runtime's own notes give its camera anything from 60 to 123 degrees across.

Found on lamp-4ace on 2026-10-13, standing, by telling the body a face was to
one side (a small script sending the eyes' datagram, no camera involved) and
comparing one frame taken by hand before and after: 250 counts more of base
yaw moved the scene 110 of the picture's 320 pixels of half-width to the
left, and 120 counts more of head pitch moved it 52 of 240 pixels of
half-height up. So a half-picture is worth 727 counts of yaw and 553 of
pitch, both positive, near the middle of a lens that sees about 128 degrees
across: `--gaze 727,553`. The body made the two corrections asked of it
(160 and then to its reach of 250) and held. No face was in front of the
lamp for any of this: its eyes have looked at an empty office for several
minutes without reporting one, and found one in a photo file on the lamp's
own processor in 50 ms, but the lamp has not yet turned to a person.

## Running it

```text
lamp-body --device /dev/EXPLICIT_SERVO_ALIAS --unit-id PROVISIONED_UNIT --lock-directory /home/WORKER/private-motor-locks --mood-socket /run/lampos-body/mood.sock [--home PITCH,ROLL,YAW] [--stand PITCH,ROLL,YAW,SHOULDER,ELBOW --folded-arm SHOULDER,ELBOW] [--gaze-socket /run/lampos-body/gaze.sock --gaze YAW,PITCH]
```

Reach and speed can be chosen at start, each inside a cap fixed in the code:

| Option | Meaning | Without it | Cap |
| --- | --- | --- | --- |
| `--reach UP,DOWN,ROLL,YAW` | farthest a goal may be from home, in counts | 45,15,30,40 | 180,25,100,250 |
| `--speed N` | fastest the commanded pose moves, counts a second | 50 | 300 |
| `--servo-speed N` | goal speed written to the three servos | 150 | 1,000 |
| `--servo-acceleration N` | acceleration written to them | 50 | 150 |
| `--stand P,R,Y,S,E` with `--folded-arm S,E` | the pose it stands in and the arm's folded one, in counts | never stands | shoulder and elbow each at most 1,400 from folded |
| `--stand-share N` | how much of the way up it goes, percent | 100 | 1 to 100 |
| `--stand-seconds N` | fastest the whole way is travelled | 5 | 2 to 20 |
| `--awake-seconds N` | how long it stays up after the last mood that was not rest | 90 | 5 to 3,600 |
| `--arm-load S,E` | load that sends it back down | 950,950 | 50 to 1,000 |
| `--gaze-socket PATH` with `--gaze Y,P` | where the eyes report, and the signed counts of yaw and pitch that a half-picture is worth | never looks | 900 either way |
| `--gaze-reach Y,P` | the farthest the gaze takes yaw and pitch | 250,120 | 400, 200 |

Every gesture is a share of the reach (listening lifts 40% of the way up and
tilts 85% of the roll; thinking lifts 70%, turns 90% of the yaw and rolls 40%
the other way; speaking sits 20% up and nods 15% of the way either
side), so a wider reach makes every gesture larger. The head's pitch window,
the load, tracking, drift, status and heat rules do not change with it.

It takes the same exclusive ownership of the servo port as
`lamp-motor-inspect`, so neither can run while the other does. Stop every
other owner of the port first.

## First run on a lamp

lamp-4ace, 2026-10-12 15:18 UTC, the build of `6dfc18bb8`, started by hand
while a played conversation ran. It found pitch 2,000, roll 2,043, yaw 2,015,
wrote the four profile registers the roll and yaw servos lacked, and moved for
66 steps: 59 goal writes, pitch to 1,973, yaw to 2,033, no missed read, peak
loads 104, 96 and 88. Then it stopped itself on `temperature`, joint 1: the
yaw servo's temperature register read 72 on one step. It had read 45 at start
and read 44, 44 and 45 on three reads at rest within the next two minutes, so
that was one reading taken while the motor drew current, not heat. The goals
were held at 1,973, 2,046 and 2,033, where the joints were.

Two changes followed. A temperature now ends the run only when it is read on
every step for a second, and the highest reading is reported either way. And
the home can be given: a stop that holds the head where it is no longer moves
the pose the next start works around. The load bounds were 160, 110 and 110
with two steps; they are 170, 150 and 150 with three, above what this run
read.

## Second and later runs on the lamp

Same evening, with `--home 1950,2043,2015`.

- With `--reach 100,15,50,120 --speed 120 --servo-speed 300
  --servo-acceleration 60` it stopped itself 14 s after start on `load`,
  joint 4: asked to tilt, the roll servo stayed at 2,047 (four counts from
  where it was found) while its load rose to 156. In its first run it had also
  stayed within three counts, at a load of 96. In this folded pose the head
  does not roll that way. Roll is switched off there since (`--reach
  100,15,1,120`); which way it is blocked, and by what, has not been looked
  at.
- With roll off, across four played exchanges (two lines it ignored, two it
  answered) the body's own log shows: listening, pitch 1,950 to 1,911 in
  about a second; thinking, pitch to 1,878 and yaw from 2,023 to 2,108 one
  time and to 1,920 the next; speaking, yaw back and pitch moving between
  1,919 and 1,947 with the voice; then home. No stop, no missed read, loads at
  or under 84, and a commanded joint never more than 18 counts behind its goal
  while moving. The yaw servo comes to rest 8 to 15 counts short of its goal
  and holds there at a load of 44 to 72; roll rests 4 counts off at 28.
- Photographs from the office iMac, about a metre away, show the head turned
  during thinking and facing the camera during listening and speaking.

Since then a joint that has rested for a second within 24 counts of its goal
is given its own position as the goal, so that nothing pushes at rest.

It then ran for about 1 h 40 min of played exchanges without a stop, until
it was stopped by hand, and for 11 minutes after it was started again. Then
it stopped itself on `load`, joint 5: the head, coming down from its thinking pose as an answer
began and nodding on the first syllable, read 217 on the third of three
steps over 170. It was within 6 counts of where it had been and 19 from its
goal, the other joints read 28 and 56, and nothing was in its way. Peak
load on the yaw joint in that run was 104. From this: the nod is 15% of the
reach where it was 25% and follows the voice a third more slowly, the
commanded pose changes speed at six times the top speed a second where it
was eight, and the load rule wants six steps in a row, not three.

Three minutes into the next run it stopped the same way, at 190, the head
9 counts short of its thinking pose with the base turned to one side; it
had reached that pose at a load of 74 twenty minutes before. A photograph
shows nothing against the head. What resists it there is not known. Since
then a joint that meets resistance gives way and goes on (see "When it
stops").

That showed what the resistance was. In the next run, 18 minutes in, the
head gave way three times in 35 s and the run ended on the fourth: each
time it sat at 1,936 or 1,937 with its goal 3 to 8 counts above, the goal
of a small nod, and did not move while its load climbed from 48 past 170
in a second and a half. The nods had been made smaller the same night and
had gone under what the joint can move. Since then a joint that has
arrived rests until it is asked for a move large enough to make. Twelve
counts was not enough: over the next ten minutes the head gave way four
more times, each time at rest at 1,937 and asked to lift 11 or 12 counts,
which it did not begin to do while its load passed 170. It lifts 40 from
rest without reading over 90. The bound is 20 counts now.

## Verification without a lamp

Host tests, `cargo test -p lamp-motor` (207 passed, 0 failed, on macOS):
`body::tests` runs the engine against five simulated servos on a simulated
clock. They show that it writes only the three packets above, byte for byte;
that over a scripted exchange every goal names servos 5, 4 and 1, stays within
reach and moves at most two counts per write, and that the goals end exactly
where they were found; that it refuses each start condition with nothing
written; that an obstacle ten counts into a tilt ends the run within a few
steps with the goal held at the obstacle; that a new flag on the shoulder, a
changed flag or a nine-count move on the elbow, a head servo that stays hot,
and a silent wire each end it, while one hot reading does not; that a head
found 43 counts from a given home is brought there in small steps and never
away from it, and one found 100 counts away is refused; and that a mood that
stops arriving returns it home.
`gesture::tests` bounds every wanted pose. `body_link::tests` in `lamp-live`
checks the datagram.

Standing up, in the same simulation with a shoulder and an elbow that follow
their goals: the five-joint write and the torque write byte for byte; that a
lamp spoken to rises to the pose and, left alone, folds down to exactly
where it was found, with every write naming servos 5, 4, 1, 2 and 3, the arm
never outside the way between folded and stood up and no arm goal moving
more than six counts in a write; that the elbow's old flag costs one torque
write to the elbow alone; that a head whose standing pitch is toward the
base stays inside its folded reach and window until the arm has made a
quarter of its way; that a shoulder strained for a second on the way up
sends the lamp down, where it stays though it is still spoken to; that an
elbow strained for longer ends the run; that a lamp found standing is taken
as it is and folds down when asked to stop, and one found off the way is
not stood; and that a flag that will not clear, or no home, leaves it a
folded lamp that runs as before.

Looking at a face (`gaze::tests`, and two in `body::tests` with a simulated
camera in the simulated head): that it turns to a face in two to six
corrections and then keeps still; that a `--gaze` set half again too high
still settles, and the reach is never passed; that nothing is corrected from
a moving picture, while it thinks or rests, or for a face a little off the
middle while it speaks; that with nobody there it does nothing, holds for a
few seconds when someone leaves and then lets go, and lets go when it folds;
that the head of a folded lamp does turn to a face while it listens, inside
its reach, and comes home; and that without a face, or without eyes, the
goals written are exactly those of a body that has none.

Not shown by any of this: that the head turns to a real face, since no
camera was involved and the two numbers of `--gaze` are not known for any
lamp; anything about standing on a real lamp, where
the weight of the head, the slack in the joints and the loads these two
servos read have not been seen by this code; that the simulated servo resembles a real one (its
speed register is taken as counts a second, which is not established); that
roll and yaw are clear of the base and the arm in the folded pose, which no
Rust code has moved; that the load bounds suit the head's own weight (the head
at rest at 2,001 on lamp-4ace reads a load of 68, and read up to 116 while it
was moved there at about 47 counts a second); what the servos sound like to
the microphones.
