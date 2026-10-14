# The lamp's small body

`lamp-body` (crate `lamp-body`, `src/body.rs`, `src/gesture.rs`,
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
  it was given a pose to stand in, or `--torque on`, it never writes torque. With
  `--force-limit` it writes one more, once at start: the force limit (48)
  of the joints that carry the lamp (below, "The force its servos are
  allowed").
- It starts only if all five joints answer, the three it commands have torque
  on, a clear status byte, a temperature and a load under their bounds, and
  the head's pitch is found between 1,760 and 2,020. A head pressed on the
  base (found at 2,095 on lamp-4ace on 2026-10-12) is refused, with nothing
  written, unless it was given `--wake on` (see "Waking").
- With `--torque on` (off unless given) it first takes hold of a joint it
  would otherwise refuse because its torque is off: the three it commands
  always, the shoulder and the elbow only for a lamp given `--stand`. A
  servo whose torque went off keeps an old goal (on lamp-52e6 on 2026-10-14,
  after the old runtime stopped, all five had torque off and goals 18 to 39
  counts from where they were), and torque on alone would send it there. So
  for each such joint it writes the goal (42) as the position just read,
  then torque on (40), each acknowledged. About 100 ms later it reads all
  five again; a joint it gave torque that moved more than 12 counts ends the
  run refused, its goal written where it went ("wrist_roll moved 30 counts
  when its torque came on"). Otherwise it starts as usual. The report counts
  the torque writes (`torque_writes`) and names the joints
  (`torque_given`).
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
`lamp-voice serve` sends one datagram to the path in `LAMP_BODY_MOOD_SOCKET`,
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
| thinking | pitch 30 up, yaw 36 to the side it was tilted from, roll 12 the other way. With a wider reach this is where it ends, not where it begins: see "A glance first" |
| speaking | pitch 8 up, and a nod of up to 7 either way when its voice is louder or quieter than its last second |

The commanded pose follows the wanted one with no overshoot, inside the speed
and reach above.

### A glance first

The look away while the lamp thinks begins as a glance and deepens only if
the thinking goes on. A person asked something easy flicks their eyes
aside and is back as they begin to answer; asked something hard, they
look away for as long as it takes.

- At once: the base turns 36 counts, a little over three degrees, and
  the lift and the tilt go the same share of their way.
- For the first 0.8 s it stays there.
- If the lamp is still thinking, it goes the rest of the way to the whole
  look away over the next 4 s, eased at both ends.

Within the folded reach (yaw 40) the glance is the whole look away, so a
lamp started without `--reach` moves exactly as it did. With lamp-4ace's
yaw reach of 250 the whole look away is 225 counts, near 20 degrees.

It was changed on what the joints showed. On lamp-4ace on 2026-10-14,
with `body/joint_states` recorded through three short played exchanges
and nobody in the office, the lamp had thought for 1.16 to 1.39 s when
its voice began. Its base was by then 94 to 133 counts round (8 to 12
degrees), and it was back and still 0.68 to 1.24 s after the first sound.
The three answers lasted 0.78 to 1.68 s: the lamp spent most of each
turning back to the person it was answering, and one had ended before it
was back.

The same three exchanges played again the same night with the glance,
measured the same way (the base's angle from where it was when the lamp
stopped listening, at moments counted from `voice/state` turning to
speaking):

| How far the base was from where it listened | Before | With the glance |
| --- | --- | --- |
| As its voice began | 8.3 to 11.7 degrees | 1.4 to 3.2 degrees |
| 0.6 s into the answer | 3.9 to 9.2 degrees | 0.4 to 2.2 degrees |
| The farthest at any moment of the answer | 8.3 to 11.9 degrees | 1.4 to 3.2 degrees |

Three answers each way, a played voice, joints sampled every 80 ms. What
it does not show: how the glance looks. Nobody has watched it, and a turn
of three degrees may be too small to read as looking away at all. The
base also does not come all the way back: it rests where it is once it is
within 20 counts of where it is wanted, as every joint does, so after an
exchange it can sit up to two degrees from where it began.

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

## Waking

No power cut has been tried. On lamp-52e6 on 2026-10-14, folded, with its
voice stopped, the body was started twice with `--wake on` and a home it was
not found at: once with the base's home 300 counts round (found at 2,024,
home 2,335), once back again (found at 2,318, home 2,035). It woke the base
311 counts in 24.3 s and 283 counts in 22.2 s, the head's roll the 8 counts
it was off with it, with no missed read, no joint giving way, and loads of
at most 127, 52 and 112 (pitch, roll, yaw). Each run then went on until it
was stopped. That the joints never moved away from home on the way is shown
by the tests below, not by these runs, whose reports give where each began
and ended. Every other sentence of this section is what the code does
against simulated servos.

On lamp-4ace on the same day, 15:16 to 15:20 (first builder, build
`dd336aef…` of `001bf889f`), folded, its voice running, the same test
with lamp-4ace's unit options less its standing ones: **the wake ended
itself both ways before reaching home.** Toward a base home 286 counts
round (found at 2,029, home 2,315), the base turned 72 counts and stuck at
2,101 while its goal went on to 2,135: off its goal by more than 30 for
ten steps, so the run held and ended (`"stop": {"kind": "tracking",
"joint": 1}`), at a load of 148. The other way (home 1,729, 300 counts),
the base turned 189 counts and the shoulder moved from 2,050 to 2,057 as
it went, one more than the 6 a bystander may drift (`"kind": "drift",
"joint": 2`). No read was missed and the peak loads were 130, 56 and 148.
In between, the ordinary start (a joint found within the 250 counts it
may start from) brought the base 177 counts home and the head's pitch 87
counts, at the body's own speed, with no check tripping. Not tried on
lamp-4ace: its own case, the head found pressed on the base at 2,095,
which needs the head's torque let go. The service runs with `--wake on`
since 15:21: a wake that ends early ends no worse than the refusal
before it, and each of systemd's three restarts begins from where the
last one held.

After a real unplug on lamp-52e6 on 2026-10-14, every servo read torque 0
and a goal register of 0, all five, when power came back. The head had
drooped in the cut, its pitch from 1,771 to 2,022 to 2,034 (toward the
base, some 250 counts), and the yaw and the roll had moved 10 to 80. Lifting
it from there, the head did not follow a goal a little ahead: a goal 40
counts up moved it 7 counts and it stuck at a load of 144; on a ramp of 260
counts in 3 s it lagged its goal by 40 to 80 counts at loads of 200 to 320
and stopped 37 short of the end (pitch gain P 32). The first wake, found at
yaw 1,958, shoulder 2,045, elbow 2,083, roll 2,100, pitch 2,034, ended after
a few seconds on the 30-count tracking bound (goal 2,000, head 2,032), and
its report said `"woke":null`, since only a finished wake was reported.
That is why a lifting head is led and given room, below. The fix has run
only against simulated servos, including that found pose with lamp-52e6's
options.

A lamp whose power comes back is found wherever it fell or was pushed: on
lamp-4ace the head rests pressed on the base (2,095 on 2026-10-12, above
the start window), and a lamp may also be found turned, or with its arm
somewhere a hand left it. Without `--wake on` the body refuses such a lamp
as before. With it, and with a `--home` given (without one, where the lamp
is found is its home and there is nothing to wake to):

- It reads every servo before moving any, takes hold of joints without
  torque if told to (`--torque on`), and checks torque, status, warmth and
  load exactly as before. Waking changes only the two checks of where the
  joints are.
- A head found outside its start window, or a commanded joint found farther
  from its home than it may start (60 counts folded, or the widest of
  `--reach`), but no more than 400 counts from it, is not refused. Before
  the body runs, the three joints are brought to their home the way
  something alive wakes, not the way a machine homes (the repository's
  rule 8): first a stir of up to 24 counts toward home (80 for a head that
  lifts: its goal must lead it by about that much before it moves at all),
  a second's pause,
  then the rest of the way, each eased at both ends and timed so that the
  joint with the farthest to go moves at 20 counts a second at most, two
  fifths of the body's own speed. Every goal starts where the joint is, so
  nothing jumps; each joint only ever moves from where it was found toward
  its home, so the head found on the base only ever rises away from it.
  The head pressed on the base at 2,095 on lamp-4ace's home of 1,950 takes
  about 12 s; lamp-52e6's head drooped to 2,034, about 21 s; a base turned
  300 counts, about 24 s.
- On the way it reads all five joints every step. A changed status byte, a
  shoulder or an elbow that moves more than 6 counts, a commanded joint over
  its load bound for six steps (400 for a head that lifts, which read up to
  320 doing it, otherwise the run's own), off its goal by more than 100 for
  ten steps while the goal is on its way, or a wire that fails for half a
  second ends the run, holding each joint where it is. Once every goal is
  home the wake waits for the joints: it is over when each is within the
  run's own 30 counts of its goal or has settled. A joint farther than 30
  that has not come 3 counts nearer in half a second has settled if it is
  within 50 counts of home, and otherwise ends the run. The run then takes
  over the goals the wake left, except that a joint settled more than 30
  short runs from where it is: its goal is written as its position, so that
  nothing keeps pushing, and its home moves there by as much, so that moods
  are laid on the pose it has and the gap is not tried again. The 50 is
  from lamp-52e6 (below): from below, these servos do not close the last
  30 to 40 counts. A step whose write or read failed
  does not advance the wake, so the next goal cannot jump. Asked to stop
  while waking, it holds where it is and ends.
- Farther than 400 counts from its home it still refuses, saying why
  ("base_yaw is 450 counts from its home, more than waking brings back
  (400)"). 400 is about 35 degrees: a head on the base was 145 from
  lamp-4ace's home, and lamp-52e6's home pitch of 1,771 would put its head
  on the base some 300 away. Farther than that, a joint is somewhere no lamp
  has been seen. The whole 400 at the wake's speed takes 31 s, inside the
  90 s systemd gives a service to say it is ready; the body says it is ready
  only once it has woken and its loop runs.
- An arm found off the way between folded and stood up, on a lamp given
  `--stand`, is not lowered. Which way down from there keeps the head clear
  of the base and the desk is not known, and these two joints carry the
  lamp, so the lamp runs folded and says why (`standing_refused`: "... off
  the way between folded and stood up; waking does not lower an arm found
  off its way"). The head still wakes.
- The report says what the wake did (`woke`: the joints it moved, where
  each was found, where their goals had got to, where they were when it
  ended (`settled`) and the seconds it took),
  whether it finished or not, and the journal has a line with the same when
  the body ends.

A lamp found where it may start writes exactly what it wrote without
`--wake on`.

The led wake was then run on lamp-52e6 from a real droop (found yaw 1,979,
roll 2,101, pitch 2,029): it woke for 20.8 s at peak loads of 232, 140 and
76, the head lifted 226 counts and stayed 32 short of home (the yaw 11, the
roll 17), and that build ended there on the stopped-short rule ("tracking",
goal 1,771, head 1,803). Settling within 50 of home, above, is the answer
to that; it has run only against simulated servos. Not shown: that the
head stays where it settled once its goal is its position, nor whether a head pressed on the base reads
under the start load bound (170), which waking does not relax; nor how the
wake looks to somebody beside the lamp. Folded with its torque switched off
by hand, lamp-52e6's head did not droop in 10 s; it drooped in the cut.

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
- A lamp with eyes (see "Looking at a face") also rises for somebody who
  comes to sit in front of it, without a word: a face in six looks running,
  about a second and a half at the eyes' four looks a second, counts as company, and company keeps it awake exactly as
  being spoken to does. So it does not fold away from a person who is still
  there, and folds once they have been gone for `--awake-seconds`. A face
  for less than that, somebody walking past, wakes nothing. Not yet seen on
  a lamp: whether a folded lamp-4ace's camera sees a person at the desk at
  all.
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
- Stood up, the head has room it does not have beside the base: it may look
  down as far as it may look up, and with `--stand-roll N` it may tilt N
  counts either way, both growing with how clear of the base it is. That
  is what gives a standing lamp the tilt it listens with (85% of that
  reach, to alternate sides), which a folded lamp-4ace cannot make. On
  lamp-4ace itself it is left off: tried on 2026-10-13 with 110 counts,
  the roll servo moved freely once the lamp stood, but its axis is the
  round head's own, so nothing is seen from outside and only the camera's
  picture rolls, by about nine degrees. The
  head's pitch is never asked for less than 1,670, just above that servo's
  own limit, whatever a mood, the eyes and a gesture add up to.
- The goals of all five joints go out in one sync write.
- An overload flag (`0x20`) latched on the shoulder or the elbow before the
  run leaves that servo holding with a fraction of its torque. Such a joint
  cannot carry the lamp, so at start the body writes torque on (register 40)
  to it once, and if the flag is still there, off and on. If it stays, the
  body does not stand and runs as a folded lamp; the report says why
  (`standing_refused`) and counts the writes (`torque_writes`). Besides
  `--torque on` (in "What it may do"), this is the only write of torque
  there is.
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
- After three such sights in a row with a face in them it takes the middle
  one, across and down separately (their mean until 2026-10-13, when one
  stray face at the corner of a picture moved lamp-4ace's head).
  A face within 0.15 of the middle across and 0.20 down is being looked at
  already. While the lamp speaks those are 0.35 and 0.40, since its head
  should be still then for its microphone.
- Otherwise it adds 70% of what would bring the face to the middle to an
  offset of base yaw and head pitch: `--gaze` says how many counts of each
  bring a face from the right edge, and from the bottom edge, to the middle,
  and their signs say which way each joint moves the picture. Under the whole
  amount on purpose: a number set too high still settles.
- The offset is added to what the mood wants and goes through the same
  window, speed and load rules as every gesture. It is bounded by
  `--gaze-reach` (250 and 120 counts without it), and sideways that may be
  wider than the reach of the poses: the head turns further to a person
  than it does to think. On lamp-4ace on 2026-10-13 people standing at the
  lamp were further round than the 250 counts (22 degrees) it could then
  turn, the face staying at 0.44 to 0.75 of the half-picture off centre.
  The old runtime's recorded moves turn the base 42 degrees either way, and
  the gaze may now be given up to 500 counts (44 degrees). Then it waits to
  be still again.
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
own processor in 50 ms.

The same day, with a photo of a face filling the screen of the computer the
lamp stands in front of: its eyes reported the face in 66 of 66 looks, an
eighth of the picture high. With the gaze first pushed 250 counts aside
the face was at -0.27 across; the next time the lamp listened the gaze went
to 113 (250 less 0.27 of 727 times 0.7) and the face to -0.11, inside what
counts as looked at. It has not yet been seen with a living person in front
of it.

## What Expression may add

On lamp-4ace since 2026-10-14, 10:05, switched on. What it did there is
at the end of this section.

The part that plans what the lamp shows ([expression.md](expression.md))
may ask the body to add a little to the pose it is in: a nod while the
person pauses is the first thing it is for. With `LAMP_EXPRESSION_BODY=on`
in the body's unit (and the bus on), the body listens to `expression/body`
(`src/expression_in.rs`). Off by default.

- **What is taken.** The head's pitch, the head's roll and the base's
  turn, in radians, turned into counts (652 to a radian). A positive pitch
  dips the head, as in the body's own poses. The shoulder and the elbow
  are the arm that carries the lamp and are not moved for expression. A
  gaze is not taken: the body already looks at the face its own eyes
  report. A request for stillness adds nothing.
- **How it is applied.** Added to the pose the mood wants, before
  anything else is done with that pose. So it goes through everything the
  mood's pose goes through: each joint's reach, the speed and
  acceleration the pose is followed at (120 counts a second on lamp-4ace),
  the rest of a joint that has arrived, and the hold after resistance. It
  is cut first to 60, 45 and 70 counts (pitch, roll, turn: about five,
  four and six degrees), whatever was asked. Nothing is added while a
  gesture plays, and a lamp at rest is not stirred by it.
- **Which request counts.** One that passes its own check, is of this
  start of the machine (the kernel's boot identity), has not run out, was
  not made ahead of the body's clock, lasts no longer than a request may
  (a fifth of a second), and is newer than the one in hand from the same
  start of the planner. Anything else is not taken, and the body says so
  in its journal for the first three and then ever more seldom.
- **When the planner stops.** Its last request runs out 0.2 s later and
  the pose is the mood's own again, followed back at the same speed.
- **Never waited for.** A thread of its own takes requests from the bus;
  the body's loop looks at what has come and goes on.

The body's report at the end of a run has `expression_steps`: the steps
in which something was added.

Against simulated servos, a lamp standing and listening, asked for a dip
of 40 counts for 0.6 s: the head's goal dipped 24 to 41 counts and was
back within 6 of where it listened from, no goal moving more than 3
counts in a step, and neither the tilt nor the turn moved. Asked for 500
counts of dip and of turn, it dipped no more than 60 and never under its
own reach, and turned 60 to 70. Asked while at rest, nothing was written.

**On lamp-4ace, 2026-10-14, 10:09, without a sound.** The lamp was made to
stand and listen through its mood socket (a socket of its own for the
trial, so that the voice's own word of its mood did not mix with it), and
its head was asked for five dips the way Expression asks: requests on
`expression/body` every 20 ms, each lasting a fifth of a second, easing
out to an offset and back (`cargo run -p lamp-body --example
expression_ask`). The joints were recorded from `body/joint_states`, one
reading every 80 ms.

| Asked | The goal dipped | The head itself dipped | Farthest, after the asking began | Back within 6 counts |
|---|---|---|---|---|
| 40 counts, out in 0.4 s and back in 0.4 s | 28 | 20 (1.8 degrees) | 0.72 s | 1.12 s |
| the same | 24 | 17 (1.5 degrees) | 0.65 s | 1.13 s |
| the same | 25 | 30 (2.7 degrees) | 0.73 s | it came to rest 11 counts from where it began |
| 55 counts, out in 0.5 s, held 0.15 s, back in 0.5 s | 37 | 26 (2.3 degrees) | 0.78 s | 1.42 s |
| the same | 49 | 38 (3.3 degrees) | 0.78 s | 1.89 s |

The base's turn did not move. The body took every request (it says when
it does not) and met no resistance.

What that shows: the way from a request on the bus to the head moving
works on the lamp, the head makes about half to two thirds of a dip
asked at this pace, it is at its farthest about 0.3 s after the request
is, and a dip and its way back take a little over a second. The head
does not start until 20 counts are asked of it, which is the first
150 ms of an eased move, and then it has to catch up. It can end up to
12 counts from where it began: a joint that is within 20 counts of
where it is wanted rests there.

What it means for a nod: the built-in one (23 counts, over 0.32 s) cannot
be made by this head. And a nod that takes a second cannot be made inside
a pause in someone's speech here, because 0.6 s of quiet ends their turn;
what this head can do is nod once as someone finishes, on its way to
looking aside to think. That needs the planner to carry a move begun
while listening on into thinking.

Not shown: what it looks like. Five photographs were taken during the
trial, one every five seconds; a move of a second and three degrees
needs film, or someone watching.

## Gestures

Not yet run on a lamp. This is what the code does against simulated servos.

A gesture is a short recorded move of the whole lamp: a nod, a shake of the
head, a greeting, a laugh, a shy dip, a stretch (`src/clips.rs`, with the
frames in `src/clips/data.rs`). Each is frames of offsets in encoder counts
for head pitch, head roll, base yaw, shoulder and elbow, a fixed time apart,
beginning and ending at nothing. They were recorded on the old runtime
(`hal/recordings/NAME.csv`) and are generated from those files by a script
kept outside this repository, which keeps 35 to 60% of each one's size and
slows it until no joint is asked to go faster than 900, 1,300, 1,000, 800
and 800 counts a second:

| Clip | Lasts | Mostly |
| --- | --- | --- |
| `nod` | 3.1 s | the arm bows 93 and 120 counts and comes back, the head with it |
| `headshake` | 5.1 s | head roll 135 either way, base yaw 85 |
| `greeting` | 5.0 s | shoulder up 221, elbow 153, head lifts 123 |
| `laugh` | 4.4 s | head back 134, shoulder back 111 |
| `shy` | 6.0 s | head dips 108, elbow folds 198 |
| `stretching` | 6.5 s | shoulder up 296, elbow 339 |
| `idle` | 10.0 s | shoulder and elbow sway about 40 either way, the head lifts 47 |

The voice service asks for one on the mood socket, `{"gesture":"nod"}`; a
name that is not a clip is ignored. It asks at most once an answer, from the
first words of what the lamp itself says (`crates/voice/src/body_link.rs`,
`cue_for`): a nod with "yes", "sure", "of course", "okay"; a shake with "no",
"nope", "I don't think"; a greeting with "hello", "hi", "good morning"; a
laugh with "haha"; the shy dip with "thank you". Most answers begin with
none of these and get no gesture. "No problem" and "no worries" get none.
The same in Vietnamese: a nod with "ừ", "dạ", "vâng", "đúng rồi", "có chứ";
a shake with "không", "chưa"; a greeting with "chào", "xin chào"; a laugh
with "hihi"; the shy dip with "cảm ơn"; none for "không sao" and "không có
gì", nor for a bare "có", which begins too many sentences that are not a
yes.

What the body does with the request:

- A gesture plays only on a lamp standing still at its full height. Asked
  of one still rising it waits up to three seconds for that; asked of a
  folded lamp, or one that may not stand, it is forgotten.
- Its offsets are laid over whatever the mood and the eyes have the lamp
  doing, outside the filter that follows the mood, and leave the lamp where
  it was. They change by at most 24, 34, 26, 21 and 21 counts a step and
  take no joint further than 220, 220, 170, 360 and 360 counts.
- While one plays, and for half a second after, a joint of the head may be
  150 counts from its goal and read a load of 700, and the shoulder and
  elbow may be 260 from theirs. A quick move lags and reverses hard. If a
  joint goes past even that for its usual number of steps, the gesture is
  let go, its offsets go back to nothing at the same bounded speed, and the
  lamp carries on; the run does not end, and the report counts it
  (`gestures`, `gestures_dropped`).
- The eyes correct nothing while a gesture plays, since the picture is
  moving.

With `--idle on` a lamp that has stood at rest for four seconds plays `idle`
by itself, the old runtime's idle loop at half its size, and again eight
seconds after each one ends: anything alive that stands with nothing to do
shifts a little now and then. It lets the shift go the moment a mood other
than rest arrives, and a gesture that is asked for replaces it. The report
counts them apart from gestures (`idles`).

A head that does not tilt when it stands (`--stand-roll` not given) does not
tilt in a gesture either: the roll in a clip is left out. On lamp-4ace that
joint turns a round head about its own axis.

The gestures need the servos' own speed bound out of the way:
`--servo-speed 1000`. The shoulder's and the elbow's are written as 800 or
more for a lamp that stands.

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
| `--arm-load S,E` | load that sends it back down | 950,950, or 40 under `--force-limit` when that is given | 50 to 1,000, and under `--force-limit` |
| `--force-limit N` | the most force the base's turn and, for a lamp that may stand, the shoulder and the elbow may push with, of 1,000 | not written: each servo keeps what it holds | 300 to 1,000 |
| `--give-way return\|stay` | what a joint does after it gave way, and a joint that pushes without moving told to stop (below, "Giving way to a hand") | as before the option: no stall rule, a joint that gave way always comes back | return or stay |
| `--torque on` | take hold of a joint found without torque: its goal written as where it is, then torque on, and refused if it then moves more than 12 counts | a joint without torque is refused, and torque is not written | on or off |
| `--wake on` | with `--home`, a joint found out of where it may start but within 400 counts of home is brought home slowly before anything else (see "Waking") | such a joint is refused | on or off |
| `--gaze-socket PATH` with `--gaze Y,P` | where the eyes report, and the signed counts of yaw and pitch that a half-picture is worth | never looks | 900 either way |
| `--gaze-reach Y,P` | the farthest the gaze takes yaw and pitch; the yaw may be wider than `--reach`'s | 250,120 | 500, 200 |
| `--idle on` | a standing lamp at rest shifts a little now and then | off | on or off |
| `--stand-roll N` | the head's tilt reach once the lamp has stood up, where that is more than the roll of `--reach` | no more than folded | 200 |

Every gesture is a share of the reach (listening lifts 40% of the way up and
tilts 85% of the roll; thinking lifts 70%, turns 90% of the yaw and rolls 40%
the other way; speaking sits 20% up and nods 15% of the way either
side), so a wider reach makes every gesture larger. The head's pitch window,
the load, tracking, drift, status and heat rules do not change with it.

It takes the same exclusive ownership of the servo port as
`lamp-body-inspect`, so neither can run while the other does. Stop every
other owner of the port first.

## On the bus

With `LAMP_BUS=on` in its unit the body publishes on the lamp's bus
([bus.md](bus.md)) under the name `body` (`src/bus_out.rs`). Without it,
or with a bus it cannot use (said once in its log), it runs exactly as
before.

| Topic | Type | When | What |
| --- | --- | --- | --- |
| `body/joint_states` | `lamp_msgs/JointState` | every fourth step: 12.5 a second | Where each joint is, how fast it is turning, how hard its servo is working, and where it was last asked to be |
| `diagnostics` | `lamp_msgs/Diagnostic` | once a second, and once more when it stops | How the body is, under the name `body` |

The joints are `base_yaw`, `shoulder`, `elbow`, `head_roll`, `head_pitch`,
in the order of their servos' numbers, 1 to 5.

- **Angles** are radians from the middle of each servo's turn (count
  2,048), growing the way the counts do: 1,024 counts is a quarter turn.
  They are not angles from the lamp's home pose, and their signs are the
  servos', not a convention of the lamp's.
- **`velocity`** is worked out from two positions 80 ms apart. The first
  message has none.
- **`effort`** is the share of its full strength each servo reports
  (its load register over 1,000), from 0 to 1 and without a direction. It
  is not a torque in newton metres, which is what ROS's field of that
  name holds.
- **`commanded`** is the goal the body last wrote; for a shoulder or an
  elbow it never wrote, where it found them.
- A message means nothing a second after it was sent, so a body that has
  stopped is not read as where the lamp is.

The report is `ok` with no message, or a warning:

| Level | Message | When |
| --- | --- | --- |
| `warn` | `shoulder is warm: it reads 55, and the body stops at 62` | Any of the five joints reads 55 or more. The body ends its run, or sends the arm down, at 62 |
| `warn` | `a joint met resistance and stopped pushing, 2 times in the last minute` | A commanded joint gave way within the last minute |
| `warn` | `stopped, as it was asked to` | Its last word after SIGINT or SIGTERM |
| `error` | `stopped: head_pitch was pushed against (load 260)`, and one such sentence for each other reason in "When it stops" | Its last word when it stopped itself or would not start |

Its facts are the mood, how far it has stood up, the warmest joint and
its reading, and how often a joint gave way in the last minute and since
it started.

The loop that talks to the servos never waits for the bus. It hands
every fourth step to a thread of its own, which has room for sixteen; a
step that finds no room is let go and counted, and the count is written
to the log when the body ends.

Not shown: this has run on a development Mac against a bus in a
temporary directory (six tests, one of which offers 40,000 steps to a
full thread and holds each to under a millisecond). The program that
owns the servos is built for Linux only and was not run there with the
bus. Which way each angle's sign runs on the lamp has not been looked at.

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

## The force its servos are allowed (lamp-4ace, 2026-10-14)

The old runtime capped the force of the three joints that carry the lamp
(the base's turn, the shoulder and the elbow) at 600 of 1,000 while it
ran, so that the arm gives way sooner against a hand, and measured what
that cost in lag ([v1-verdicts.md](v1-verdicts.md), HAL-4.5). This body
writes no such cap, and what the servos hold had never been read.

It was read on 2026-10-14 at 07:48, with the body stopped for the few
seconds it took and nothing written: one read instruction for each
register and servo, from a script on the lamp, each reply checked by its
checksum. The names are the servo's published register table's, not
checked against this firmware.

| Register | Base turn | Shoulder | Elbow | Head roll | Head pitch |
|---|---|---|---|---|---|
| 16, the force limit it starts with | 1,000 | 1,000 | 1,000 | 1,000 | 1,000 |
| 48, the force limit now | 1,000 | 1,000 | 1,000 | 1,000 | 1,000 |
| 40, torque on | 1 | 1 | 1 | 1 | 1 |
| 36, the load at which it protects itself, in hundredths | 80 | 80 | 80 | 80 | 80 |
| 35, for how long, in hundredths of a second | 200 | 200 | 200 | 200 | 200 |
| 34, the force it falls back to then, in hundredths | 20 | 20 | 20 | 20 | 20 |
| 28, the current at which it protects itself | 310 | 310 | 310 | 310 | 310 |

So every joint may push with all it has. What makes the arm give way
today is this program alone (a load over its bound for six steps, a
joint off its goal for half a second, warmth: "Standing up" above), and
behind it the servo's own protection, which lets go only after two
seconds over 80%.

What the lamp's own movements use: the run of the body that ended with
that reading had every stand-up and gesture of a night's played tests in
it (204,330 steps, no retreat). Its report gives the highest load each
joint read: 428 at the shoulder and 490 at the elbow, and 229, 60 and
156 at the three joints of the head and base. A cap of 600 is above all
of them, so on this unit it would cost ordinary movement nothing, and it
would take four tenths off what the arm can push with against a hand or
an edge.

The cap is `--force-limit N`, off unless given. At start, after the
servos' speed and before anything moves, the body writes it (register
48, two bytes) to the base's turn, and to the shoulder and the elbow if
the lamp may stand, and reads it back from each; a joint that reads back
anything else ends the run there, with nothing moved. The servo keeps it
until it loses power and then starts again with its own. The load bound
comes down with it: a joint capped at 600 never reads 950, so a bound
left there would be a guard switched off. Unless `--arm-load` says
otherwise the bound is 40 under the limit (560 for 600, over the 490 this
lamp's own movements reached), and an `--arm-load` at or over the limit
is refused. The report has `force_writes` and `force`.

Two tests against the simulated servos: the write byte for byte, to
which joints, before the first goal, and nothing written without the
option; and a servo that does not keep the limit, which ends the run
before anything moves. One for the option.

**On lamp-4ace since 08:01 on 2026-10-14, at 600.** One scene, played to
the body with no sound, once without the cap and once with it: the body
was told on its own socket what the voice would tell it (listening for
eight seconds, so that it stands; thinking; then speaking, with a nod, a
shake of the head, a stretch and a laugh asked for), while its joints
were recorded from the bus at 12.5 a second and the lamp was photographed
every five seconds.

| | Without the cap | With it at 600 |
|---|---|---|
| Highest load read: base turn, shoulder, elbow | 192, 368, 481 | 192, 316, 438 |
| Farthest from its goal: base turn, shoulder, elbow, in counts | 76, 95, 68 | 72, 81, 58 |
| The shoulder's way up, 609 counts | 9.2 s | 9.3 s |
| Gestures begun, and let go | | 3, 0 |
| Sent back down, or met resistance | | never |

The body's own report of the run with the cap, which reads every step
and not twelve times a second: 3 writes of the limit, 600 read back, the
elbow's highest load 508 and the shoulder's 340, nothing dropped. Read
afterwards with the body stopped and nothing written: the base's turn,
the shoulder and the elbow hold 600 and will start again with 1,000; the
head's two joints hold 1,000. In the photographs it stands upright and
moves as it did.

So on this unit the cap costs its own movements nothing that one scene
can show, and the elbow's 508 is 52 under the bound that sends the arm
back down.

**Not shown:** that the arm gives way against a hand, which is the point
of it and needs a hand; a cold lamp, a warm one, or another unit, where
the same movements may ask for more; and anything about leaning: this
body still has no check that a pose keeps the lamp on its base. One
oddity in the same report: the highest warmth read was 150 for
two of the three head and base joints, which no servo here has been; a
single bad reading counted as a peak is the likely cause, and the guard
that acts on warmth needs a whole second of it.

## Giving way to a hand

Not yet tried by a hand on any lamp. Everything in this section is what
the code does against simulated servos.

V1's lessons, kept by the fifth builder's person: a short bump should
give way and come back gently; a person who holds or adjusts the head for
longer than about a second wants it to stay where they put it; the joints
that carry the lamp's weight (the shoulder, the elbow) must never go limp,
since on V1 releasing them dropped the arm with nobody touching it; and a
joint held still against its goal must not cook.

`--give-way return|stay`, off unless given. Without it the body does
exactly what it did before, write for write.

**Both modes: a joint that pushes without moving is told to stop.** Any
of the five joints the body writes (the three it commands; the shoulder
and the elbow of a lamp given `--stand`) whose goal is more than 6 counts
from where it is, whose load is over 100 and which moves no more than 6
counts for 1.5 s, is given its own position as its goal. It is held there
until what is wanted of it moves 20 counts or more from what was wanted
when it stalled (a new mood, the arm rising or folding), and then follows
again. Torque is never written off for it. Why these numbers:

- 1.5 s is longer than the rules already there take to stop a joint
  pushing at rest (arrived for 0.3 s, at rest for 1 s), so it catches only
  what they leave pushing, and shorter than the servos' own protection
  (80% load for 2 s, then 20% of their force: on the shoulder or the elbow
  that is the arm dropping), so this acts first. Heat comes from minutes
  of pushing, not seconds: the elbow on lamp-4ace reached 69 °C one day
  with the head resting on the base.
- 100 of 1,000 is over what a joint at rest at its goal reads on
  lamp-4ace (the head 68, the base resting short 44 to 72) and under the
  100 to 150 that lamp-52e6's shoulder pushes with for as long as it
  stands, 22 to 34 counts short of its goal.

The report counts each joint's (`stalls`: head pitch, head roll, base yaw,
shoulder, elbow) and the log says each one with the joints' positions and
loads.

**`return`: a joint that gave way comes back,** as without the option
("When it stops").

**`stay`: moved by a hand, it stays where it was put.** For the three
commanded joints. While a joint that gave way is held, its load and
tracking are judged over a second instead of 120 ms and half a second,
since that is what a hand moving it looks like:

- Its load over its bound for a second in a row, and it has been moved
  more than 15 counts from where it gave way: a hand moved it. Where it
  is becomes where it is left, inside the same reach and window as every
  goal (a hand that takes it past the reach leaves it at the edge, where
  the servo takes it at its own speed), and what the lamp is doing then
  is laid from there: later moods are relative to it. It is held two
  seconds more, so a hand still on it can go on moving it. The report
  counts each (`kept_where_put`) and the log says it.
- The same second and it has not moved: something pushes it and it does
  not give; the run ends as it does without the option, but after a
  second rather than 120 ms.
- A bump that does not move it, or a hand that moves it for under a
  second, gives way and comes back as with `return`. A hand that lets go
  before the second sees the joint go back at the servo's own speed to
  where it gave way, and then to the pose its mood wants.

Asked to stop, the body still goes home, not to where a hand put it.

Five tests against the simulated servos and one for the option: a short
push gives way and comes back, and so does a head moved for half a
second; a head moved 30 counts by a hand for 1.5 s stays there while the
lamp rests and the next mood lifts it 40 counts from there, no goal
jumping; a head pushed 150 counts up with a reach of 100 is kept at 100;
a head stopping 27 counts short of its goal at a load of 130 is told to
stop 75 to 78 steps after it stopped moving and is not pushed at again
while the mood is the same, and without the option goes on pushing; a
standing shoulder 30 counts short at 150 is held where it is, and no
torque write is ever anything but on.

**Not shown:** that any of it is right against a hand. Whether a real
hand reads over the load bounds while it moves a joint (a servo whose
goal is where it gave way pushes back at the hand for that second, at
whatever force it is allowed), how hard that is to the hand, whether
1.5 s and 100 suit lamp-4ace's or lamp-52e6's servos, and what the
temperature of a joint held for minutes does with and without it. The
measurements task 24 asks for (by hand, standing and folded, through a
kitchen scale) have not been made.

## lamp-52e6

The fifth builder's lamp, from 2026-10-14. The old runtime was stopped and
disabled there the same morning; the units it runs are in
[`deploy/lamp-52e6/`](../deploy/lamp-52e6/README.md). How it differs from
lamp-4ace, as read on it with the old runtime stopped:

| | lamp-4ace | lamp-52e6 | Where it was read |
|---|---|---|---|
| Joints found at rest: yaw, shoulder, elbow, roll, pitch | 2023, 2052, 2047, 2053, 1950 | 2035, 2053, 2069, 2035, 1771 | `lamp-body-inspect`, torque off |
| Torque when the body starts | on, left so by the old runtime | off on all five once the old runtime stopped; the body refused ("torque is not on") | `lamp-body-inspect`; the body's own refusal |
| `--home` | 1950, 2043, 2015 | 1771, 2035, 2035 | found at rest. lamp-4ace's head rests leaning toward the base; this one's rests at the old runtime's rest pose (1770), which is why 1950 is refused here |
| `--folded-arm` | 2048, 2047 | 2053, 2069 | found at rest |
| `--stand` | 1772, 2043, 2015, 2657, 2602 | 1772, 2035, 2035, 2657, 2602 | lamp-4ace's pose with this lamp's roll and yaw |
| `--gaze` | 727, 553 | 782, 557 | measured here the same way, below |
| Servo limits in each servo's memory (yaw, shoulder, elbow, roll, pitch) | pitch's lower limit 1657; the rest not read | 809–3239, 2047–3799, 0–4095, 1065–2413, 1451–3067 | read without moving |
| Gains each servo holds, P, D, I (yaw, shoulder, elbow, roll, pitch) | not read | 16/32/0, 16/32/0, 32/32/10, 16/32/0, 32/32/10: the old runtime's, kept in the servos; this body writes none | read with the body stopped |
| Head roll, folded | meets something four counts from where it rests | moves 23 counts one way and 27 the other from 2,040 and stops 13 to 17 short of a goal 40 away, at a load of 64 to 80 | a goal stepped by hand with the body stopped |
| The shoulder standing | | rests 22 to 34 counts short of 2657, pushing at a load of 100 to 150 for as long as it stands | the body's log, two runs |
| The elbow folded | | at its goal, still reading a load of 120 to 140 | the body's log, two runs |

What follows from it:

- **Torque.** With nothing of the old runtime left to give its servos
  torque, the body takes hold itself: `--torque on` sets the goal of a joint
  found without torque to where it is, then writes torque on. Tried on
  2026-10-14 with the yaw and roll servos' torque switched off and their
  goals 12 and 16 counts from where they stood: without the option the body
  refused ("wrist_roll torque is not on"); with it, two torque writes, both
  joints read the same position after as before (2061 and 2049), and the
  run went on for 282 steps until it was stopped. A goal written to these
  servos while their torque is off turns their torque back on, so a stale
  goal cannot be made by hand that way; the 12 and 16 counts were the ones
  the servos held.
- **The start window.** The head must be found between 1,760 and 2,020; on
  this lamp it rests at 1,770 to 1,773, ten counts inside. The pitch floor
  of 1,670 is lamp-4ace's servo limit; this lamp's is 1,451, so the floor
  takes a few counts off the standing lamp's thinking lift (asked 1,667, held
  at 1,680) and nothing else.
- **The gaze.** Measured as on lamp-4ace, with its eyes stopped and the face
  told to the body on its own socket: a face at 0.5 across brought 254
  counts of yaw, and the picture (640 by 480, one frame before and one
  after, its shift found by phase correlation and checked by eye) moved 104
  of 320 pixels to the left; a face at 0.3 down brought 116 counts of pitch
  and the picture moved 50 of 240 pixels up. So a half-picture is 782 counts
  of yaw and 557 of pitch, both positive. Once each.
- **Driven by mood.** With its voice stopped and a script telling the body
  what the voice would (listening 6 s, thinking 4 s, speaking 5 s with a
  nod once, resting 4 s, three times; then left at rest until it folded,
  spoken to until it stood, and left again), twice, 13:46 to 13:51 on the
build it was installed with and 14:05 to 14:11 on this one with
`--torque on`: eleven exchanges, five nods asked for and played, folded
four times and stood up twice. No stop, no joint gave way, no arm was sent
back down. Thinking turned the base 170 to 190 counts and it was back
within 15 counts of where it listened about two seconds after the voice
began. The warmth guard read single steps of 65 to 109 on joints that read
46 to 50 either side of them, as on lamp-4ace, and never acted on them.

The raw readings are in
[`verification/body-lamp-52e6-20261014/`](verification/body-lamp-52e6-20261014/README.md).

**Not shown on this lamp:** that a power cut leaves the servos as stopping
the old runtime did (task 23); how the arm gives way against a hand (task
24); the head's roll in use: it moves when folded here, but stays at a reach
of one count as on lamp-4ace until a tilt is chosen and watched; and
anything about leaning.

## How the head follows (lamp-52e6, 2026-10-14)

Read from the servos themselves, about every 16 ms, with the body stopped
and the lamp standing, at the profile the body writes (goal speed 1000,
acceleration 60). Once each.

| Joint, goal moved | First movement | 90% of the way | Farthest | Comes to rest |
|---|---|---|---|---|
| Head pitch, 40 down | 85 ms | 181 ms | 108% | 3 counts past |
| Head pitch, 40 up | 95 ms | 727 ms | 98% | 1 count short |
| Head roll, 40 either way | 116 to 118 ms | never | 52 to 60% | 16 to 19 counts short |
| Base yaw, 60 | 114 ms | never | 83% | 10 counts short |

Roll and yaw hold a proportional gain of 16 (pitch 32); a servo with it
stops well short of a small goal and pushes there, which is why the body
gives a joint that has arrived its own position.

A nod, head pitch down and back, asked of the servo directly:

| Asked | Reached | When | Back within 3 counts |
|---|---|---|---|
| 25 counts (2.2 degrees), held 0.15 or 0.3 s | 12 to 14 counts, once 24 | 146 to 283 ms | 0.27 to 1.2 s |
| 45 counts (4 degrees), held 0.15 s | 34 to 35 counts (3 degrees) | 179 to 210 ms | 0.31 to 0.37 s |
| 45 counts, held 0.3 s | 38 to 41 counts | 158 to 212 ms | 0.62 s, or not within the second watched |

The acceleration written (60, 100 or 150) changed little; the highest load
was 320, under the 600 the joints that carry the lamp are allowed. So the
quickest nod this head shows clearly is about 3 degrees down and back in
about a third of a second, when the servo is asked 45 counts for 0.15 s. The
body itself is slower than that: its commanded pose moves at `--speed`
(120 counts a second on this lamp), so 45 counts take 0.375 s each way
before the servo is involved, and anything under 20 counts from where a
joint rests is not asked at all.

**Not shown:** whether such a nod looks soft and natural to a person (no one
has watched it), its cost in heat over many nods, and the folded lamp's
nods. The raw readings are in
[`verification/head-follow-lamp-52e6-20261014/`](verification/head-follow-lamp-52e6-20261014/README.md).

## Verification without a lamp

Host tests, `cargo test -p lamp-body` (256 passed, 0 failed, on macOS):
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
stops arriving returns it home. With `--torque on`: that three joints found
without torque and with stale goals each get the goal, then the torque, byte
for byte, and none moves; that a joint whose goal write is not kept moves 30
counts and ends the run refused before any other motion; and that the
shoulder and the elbow are taken hold of on a lamp given `--stand` and never
written on one that is not. With `--wake on`: that a head found pressed on
the base at 2,095 rises out of it, never toward the base and never more
than a count a write, in a little over eight seconds, and then runs inside
its window; that a base found 300 counts round comes home without a goal
moving away from home, the way itself never over 20 counts a second; that
one found 450 away is refused with nothing written, as is a head on the base
without a home given; that an arm off its way is never written and the lamp
runs folded, saying why; and that a lamp found where it may start writes
exactly what it writes without the option. And lamp-52e6 as found after its power cut (torque off and goal 0 on all five, head drooped to 2,034), with its unit's options and `--stand` and `--torque on`, in a simulation whose head lags 37 counts at a load of 320 while it lifts: the head is lifted home, never toward the base, and the run goes on; a head that will not lift ends the wake held where it is, the report saying how far its goal got. And a head that stops 32 short of home settles there: its goal and its home become where it is, and the run goes on.
`gesture::tests` bounds every wanted pose. `body_link::tests` in `lamp-voice`
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

Gestures (`clips::tests`, three in `body::tests`, and `body_link::tests` in
`lamp-voice`): every clip begins and ends at nothing and stays inside its
speed and size; a standing lamp asked to nod makes the clip's move, for the
clip's time, with no goal jumping, and stands afterwards exactly as it
stood; a gesture asked of a rising lamp begins when it stands, and one
asked too early or of a folded lamp is forgotten; a head joint that strains
ends the gesture and not the run; which first words get which gesture, that
one is asked once an answer, and the datagram's exact text.

Not shown by any of this: what a gesture looks like, sounds like or does to
the joints on a real lamp, at any of these speeds; that the head turns to a
real face, since no
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
