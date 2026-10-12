# The lamp's small body

`lamp-body` (crate `lamp-motor`, `src/body.rs`, `src/gesture.rs`,
`src/bin/lamp-body.rs`) moves the lamp's head a few degrees for what the lamp
is doing: a lift and a tilt while someone speaks to it, a look away while it
has not answered yet, small nods on its own voice while it speaks, and back to
where it was found between turns. It is not the qualified motion worker
described in [motor-worker.md](motor-worker.md) and replaces nothing there. It
exists so that a lamp folded in its rest pose can be expressive within limits
narrow enough to check by eye.

Status on 2026-10-12: built, run against simulated servos (below), and run
once on lamp-4ace for 1.3 s (see "First run on a lamp").

## What it may do

- It commands three joints: head pitch (servo 5), head roll (servo 4) and
  base yaw (servo 1). The shoulder and the elbow are read on every step and
  never written.
- It writes three registers: the goal position (42) of those three joints,
  in one sync write, and once at start their goal speed (46) and acceleration
  (41), so that the servo itself is slow whatever this process sends. It never
  writes torque.
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
| Load on a commanded joint, three steps in a row | pitch 170, roll 150, yaw 150 |
| A commanded joint away from its goal for ten steps | 30 counts |
| Any joint's status byte different from the one found at start | any change |
| The shoulder or the elbow away from where it was found | 6 counts |
| A commanded joint's temperature register, on every step for a second | 62 |
| A read that fails, three steps in a row | missing, late, malformed or unexpected bytes |

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
the lamp's own while speaking. Nothing comes back, and a body that is absent,
slow or stopped changes nothing in the voice service. A mood older than 1.5 s
is rest. Anything that is not exactly that object is ignored.

| Mood | Wanted pose, as offsets from where the head was found |
| --- | --- |
| resting | none |
| listening | pitch 18 up, roll 26 to one side; the side alternates between exchanges |
| thinking | pitch 30 up, yaw 36 to the side it was tilted from, roll 12 the other way |
| speaking | pitch 8 up, and a nod of up to 11 either way when its voice is louder or quieter than its last second |

The commanded pose follows the wanted one with no overshoot, inside the speed
and reach above.

## Running it

```text
lamp-body --device /dev/EXPLICIT_SERVO_ALIAS --unit-id PROVISIONED_UNIT --lock-directory /home/WORKER/private-motor-locks --mood-socket /run/lampos-body/mood.sock [--home PITCH,ROLL,YAW]
```

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

## Verification without a lamp

Host tests, `cargo test -p lamp-motor` (182 passed, 0 failed, on macOS):
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

Not shown by any of this: that the simulated servo resembles a real one (its
speed register is taken as counts a second, which is not established); that
roll and yaw are clear of the base and the arm in the folded pose, which no
Rust code has moved; that the load bounds suit the head's own weight (the head
at rest at 2,001 on lamp-4ace reads a load of 68, and read up to 116 while it
was moved there at about 47 counts a second); what the servos sound like to
the microphones.
