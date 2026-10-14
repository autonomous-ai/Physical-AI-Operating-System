# The keeper: one lamp of many parts

The lamp is a dozen processes, each its own systemd unit
([brain-build.md](brain-build.md), "Components, processes and owners").
This page says how they are started, watched, paused and stopped as one
lamp: the units and their order, the slices, the restart limits, the
watchdog each part keeps, and the states each part says. The units are in
[`deploy/units/`](../deploy/units/README.md). The program that acts on
them, `lamp-keeper` (`crates/keeper`), is described with its crate.

## What exists and what we take

**systemd** is the process manager, and nothing here replaces it. A
*target* groups units; `Wants=` pulls a unit in, `After=` orders it, and
`PartOf=` carries a stop or restart of the target to the unit (not a
start). `Type=notify` lets a process say `READY=1` when it is ready rather
than when it has forked, and `STOPPING=1` when it begins to stop.
`WatchdogSec=` expects `WATCHDOG=1` at least that often; a process that
misses it is ended (SIGABRT) and, with `Restart=on-failure`, started again.
`StartLimitBurst=` starts in `StartLimitIntervalSec=` and the unit stays
failed (`start-limit-hit`) until someone resets it. *Slices* are the cgroup
tree: `CPUWeight=` shares the processor between slices, `Nice=` orders
processes within one. Taken: all of these, as they are. Not taken:
`StartLimitAction=reboot` (below), and `systemctl freeze`, which stops a
process where it is: a frozen body holds its servos at their last goal with
nothing watching their load or heat, so it is not a pause.

**ROS 2 managed (lifecycle) nodes** have named states (unconfigured,
inactive, active, finalized) and transitions driven by a manager; Nav2's
lifecycle manager also keeps a heartbeat (a "bond") with each node and
brings the set down when one is lost. Its **diagnostics aggregator** groups
every node's `/diagnostics` into one view with OK, WARN, ERROR and STALE.
Taken: a few named states each part says, and a pause ("inactive") that is
not a stop; the heartbeat, as systemd's watchdog. The aggregator was
already taken, as `lamp-health` ([health.md](health.md)). Not taken: the
configure and cleanup transitions and a manager that drives each node
through them. systemd starts a process; a part configures itself as it
starts, and a part that cannot is not ready.

**Android's init** reads `.rc` files of services, each in a *class*
(`core`, `main`, `late_start`) started and stopped together with
`class_start` and `class_stop`. A service is restarted when it exits, after
`restart_period` (5 s unless set). A `critical` service that exits more
than four times in four minutes, or before boot completes, reboots the
device into the bootloader; repeated crashes of the system server lead
Rescue Party to offer recovery. The system server's **watchdog** thread
checks that its main threads still turn over and kills the process when
one has been held for a minute, after dumping its stacks at half that.
Taken: the class, as `lampos.target`; restarts with a bound. Not taken:
rebooting on repeated failure. A lamp that reboots does not talk any more
than one with a part stopped, and what the servos do at power-up has not
been checked on any lamp (lamp-4ace runbook, "Do not"). A part that will
not stay up is stopped and reported.

**launchd** keeps a job running with `KeepAlive` (always, or on conditions:
it crashed, a path exists, the network is up) and will not start a job
again within `ThrottleInterval` (10 s) of the last start. It has no order
between jobs: launchd creates each job's socket itself and a client waits
on the socket, not on the job. Taken: the throttle, as `RestartSec=`, and
the lesson that order is a convenience, not a dependency. Every lampOS part
already does without a neighbour that is not there (brain-build.md, "No
component waits on another"); the order below only keeps early messages
from being lost. Not taken: conditional keep-alive.

## The parts and their order

`lampos.target` is the lamp. Every part is `PartOf=lampos.target` and
`WantedBy=lampos.target`, so one command starts or stops all of them:

    systemctl start lampos.target
    systemctl stop lampos.target

and `lamp-keeper start lamp` / `lamp-keeper stop lamp` do the same with a
pause first (below). The target is active when every part it wants has
started; for a `Type=notify` part that means it has said it is ready, so
the target's start time is the lamp's "power-up to every part ready"
once every part notifies.

Start, in this order (each line after the one above it; parts on one line
together):

| Step | Parts | Why |
|---|---|---|
| 1 | memory, actions, health | The voice reads what is kept before its first session and calls the actions for its functions. Health is there to see the others come up |
| 2 | voice | The lamp's reason to be: listening as early as it can. The ring is the voice's own, so "the light" comes with it |
| 3 | body, name, controls, policy | The body's mood comes from the voice; name, controls and policy read and write the voice's topics |
| 4 | eyes, Expression | The eyes send each look to the body's gaze socket; Expression's requests go to the body and the ring's owner |

Stop is the reverse: eyes and Expression first, so nothing is sent to a
body on its way down; then the body, which folds before it ends (its
`TimeoutStopSec=25s`), with name, controls and policy; then the voice; then
memory, actions and health, so health reports the others going.

What is known and what is not:

- **Body before eyes**: checked in the code. `lamp-eyes` sends each look as
  a datagram to `--body-socket` and ignores a failure to send
  (`crates/eyes/src/main.rs`), so eyes without a body work and their looks
  are dropped. With the body `Type=notify`, the eyes start once the body is
  ready, and its gaze socket is there for the first look.
- **Voice before body**: as on lamp-4ace since 2026-10-13. Nothing needs it:
  the voice sends moods without waiting. It is chosen so that the lamp can
  talk before it can move, and a slow body start does not delay listening.
- **Memory before voice**: from memory.md (what is kept is read before the
  first session). While memory is `Type=simple` this orders only the
  forks; once memory notifies, the voice waits for it, and how long that
  delays the first listening is something only a lamp shows.
- Only `Type=notify` parts make an `After=` wait for anything. Today that is
  the body, so the only real wait in the chain is the eyes and Expression
  on the body's start checks. How long those take at power-up, and whether
  the eyes should wait for them, only a lamp shows.
- Surround is not part of the target: an update stops and starts the lamp,
  and what asked for the update is not stopped with it. The keeper's own
  unit is outside the target for the same reason.

## Slices and priorities

| Slice | Parts | Weight | Why |
|---|---|---|---|
| `lampos-voice.slice` | voice | `CPUWeight=1000`, `IOWeight=1000` | A late word is worse than anything else late. The voice's microphone and speaker threads are also round-robin real-time at 20 (lamp-4ace, 2026-10-13: with every core busy the voice ended four times at ordinary priority) |
| `lampos-parts.slice` | every other part | `CPUWeight=100` | Together no more than one ordinary service. Within it: eyes `Nice=10` (looking is never worth a late word); name, Expression, controls, policy, health `Nice=5`; body and memory ordinary |

The name also keeps to the board's fast processors (`CPUAffinity=`), as on
lamp-4ace.

## Restart limits

| Part | Restarted after | Limit | Then |
|---|---|---|---|
| voice | 4 s | 10 starts in 10 min | stays failed |
| body | 15 s | 3 starts in 10 min (as on lamp-4ace) | stays failed until someone has looked: a body that stopped itself three times has found something wrong with the lamp |
| every other part | 5 s | 5 starts in 10 min | stays failed |

A part over its limit is `failed` with `start-limit-hit`. `lamp-keeper
status` shows it, and it is the keeper's to report on the bus. `systemctl
reset-failed UNIT` and a start try it again. lamp-4ace's voice unit has no
limit (`StartLimitIntervalSec=0`); the template's ten in ten minutes is a
choice not yet tried on a lamp.

## The watchdog each part keeps

Through the keeper's helper (`crates/keeper`), each part:

1. says `READY=1` when it really is ready, not when it has started: the
   body once all five joints answer, its start checks have passed and its
   loop runs; the voice, when it can hear; the eyes, when the camera can be
   opened on a permit.
2. says `WATCHDOG=1` from its main loop, once per turn of it, and never from
   a timer thread of its own: a timer goes on while the loop it should
   vouch for is stuck. At least twice per `WatchdogSec`.
3. says `STOPPING=1` when it begins to stop (the body, as it begins to fold).

Without `NOTIFY_SOCKET` (a part run by hand) the helper does nothing.
A part's unit becomes `Type=notify` with a `WatchdogSec=` only once the
part calls the helper; a `Type=notify` unit whose process never says ready
fails its start after `TimeoutStartSec` (90 s). Today only the body does:
`WatchdogSec=5`. A body whose loop is silent for five seconds is ended and
started again after 15 s, within its limit. Its servos hold their last goal
meanwhile, with torque on; it does not fold.

## Named states and the pause

Each part says one of four states on the bus:

| State | systemd | Means |
|---|---|---|
| starting | activating | Begun; not yet able to do its work |
| ready | `READY=1` said | Doing its work |
| paused | (none) | Running, ready, and doing no new work |
| stopping | `STOPPING=1` said | Ending; what it holds is being put down |

systemd has no pause. Each part honours one itself, when the keeper asks:

| Part | Paused |
|---|---|
| voice | Takes no new turn. A turn under way is finished, the person is not cut off. The microphone stays as the privacy switch has it |
| body | Goes to rest (folds, if it stands), sends no new goal, and holds there |
| Expression | Holds still: no new requests; those already sent run out |
| eyes, name, memory, actions, policy, controls, health | Nothing: none of them moves or speaks by itself |

Resume undoes it. A stop by the keeper is: pause, wait until the voice has
been quiet (task 12's 45 s, [surround.md](surround.md), "the owner
protocol"), then `systemctl stop lampos.target`. `systemctl stop` alone
does not pause: the body still folds on its way down, but the voice may be
ended mid-word.

## Switches

Every action the keeper takes is behind a switch, off unless set: without
it the keeper reports and does nothing. Copying the units in starts
nothing; each part is in the lamp once enabled, and the lamp starts at
power-up once `lampos.target` is enabled. Nothing restarts the voice while
someone speaks to the lamp: the keeper's restarts of the voice, and every
update and reset, wait for the quiet above. systemd's own restart of the
voice comes only after its process has ended, when nobody is being heard.
Once the voice keeps a watchdog, a voice whose loop has hung is ended
whether or not someone is speaking; a hung voice hears nobody either.

## What was not shown

- No unit here has run on a lamp. Each was checked with `systemd-analyze
  verify` (systemd 252, Debian bookworm, in a container) with every
  placeholder filled: no complaint but the missing binaries.
- The order at power-up and how long each step takes; whether the eyes
  should wait for the body's start checks; whether the voice should wait
  for the memory once it can.
- Whether the lamp's kernel has `CONFIG_RT_GROUP_SCHED`. With it, the
  voice's real-time threads cannot be made in `lampos-voice.slice` and the
  voice will not start; then it goes back to `system.slice`, as on lamp-4ace.
- The voice's limit of ten starts in ten minutes.
- The pause, in any part: none honours one yet.
- lamp-4ace's units are unchanged here: they have no target, no slices and
  no watchdog.
