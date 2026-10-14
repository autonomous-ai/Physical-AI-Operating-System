# Controls: a hand can ask the lamp to stop

## What exists and what we take

V1 has three different inputs. Its mechanical button stops speech on a short
release. Its MPR121 base pad also stops speech. Its TTP223 head pads are for
petting and never stop speech. They are not interchangeable touch sensors.
The actual drivers, tests and `robots/lamp/docs/physical-controls.md` were read
before this implementation; the source-by-source decision is in
[reuse decisions](reuse-decisions.md#controls-keep-the-recognition-cases-change-the-actions-2026-10-14).

Keep V1's independent 200 ms press/release debounce, immediate first short
release, matched-edge requirement, startup-held suppression, release-only hold
actions, and touch-noise cases. Reuse the MPR121 fixture cases and its 5 ms
pad filter, 10 ms contact/chord qualification and three-pad minimum. Retain
TTP223's 500 ms settling, 200 ms contact grouping, 1.2 s decision window and
extending 1.5 s cooldown. These are source settings and historical fixtures,
not new measurements of this lamp.

For First's adapter, the repository already uses Rust
[`gpiocdev`](https://docs.rs/gpiocdev/latest/gpiocdev/) for physical privacy.
It supplies Linux line requests, timestamps and event sequence information.
The [official libgpiod Rust bindings](https://libgpiod.readthedocs.io/en/master/rust_api.html)
are another option, with a C library dependency. A plain
[`embedded-hal::InputPin`](https://docs.rs/embedded-hal/latest/embedded_hal/digital/index.html)
does not carry acquisition times or missed-edge evidence, so Controls uses
the slightly richer local `PinReader` trait. No GPIO library is added here.

Linux's [kernel debounce](https://www.kernel.org/doc/html/latest/userspace-api/gpio/gpio-v2-get-line-ioctl.html)
waits for a stable level and timestamps the end of that interval. Setting it
to 200 ms would discard quick taps and would not reproduce V1's per-polarity
suppression. Keep that rule in this engine. The
[edge-event sequence](https://www.kernel.org/doc/html/latest/userspace-api/gpio/gpio-v2-line-event-read.html)
must also be checked for overflow by the adapter. Active-low configuration
changes logical polarity; never guess it from a board name.

## Built here

`lamp-controls` is an optional process, crate `crates/controls`. It reads
complete observations through `PinReader`, recognizes gestures on supplied
monotonic clocks, publishes typed requests and feedback, and reports health.
There is no GPIO, I²C, capture, speaker, camera, ring or motor owner in it.
The shipped `serve` command uses `MissingPins` and reports that no adapter is
installed. First replaces that one construction with its verified reader.
No service unit or deployment change is included.

The process and all receivers require explicit enablement. CLI configuration
cannot turn on a process without `--enable`; the flag overrides the file's
`enabled` value. Shadow mode publishes recognizable evidence with
`enabled:false`, which `Receiver` refuses as authority. A publishing success
means neither a stopped speaker nor a closed microphone.

| Input | Recognized gesture | Request |
|---|---|---|
| Mechanical button | Short release, under 2 s | Stop speech/playback/following through their owners |
| Mechanical button | Double or further short releases | Another Stop; no reboot and no unmute |
| Mechanical button | Release after at least 2 s, including 5 s and longer | One-way software mute |
| MPR121 base | Qualified stationary short release | Stop |
| MPR121 base | Second short contact | Stop, with double-tap classification |
| MPR121 base | Qualified stationary hold, released after at least 2 s | One-way software mute |
| TTP223 known head pads | Completed grouped touch/pet | Advisory Pet, never Stop or mute |

Stop and software mute are separate from physical privacy. No gesture opens
capture. There is no implicit wake, unmute, voice-mode toggle, Harness entry,
shutdown or reset. This deliberately changes V1's wake-on-tap and long-hold
actions. The 2 s and 5 s feedback tiers both mean mute on release; the 5 s
tier does not arm shutdown. Feedback carries duration and intent, not an old
red shutdown cue. First must provide a separate explicit owner-side clear
action before enabling software mute; the tested transaction is described
below. Repeated panic taps remain safe while muted.

Mechanical double-tap classification uses release-to-release **≤400 ms**.
Base double-tap classification uses previous release to next press **<400 ms**.
Those match the two V1 paths' different timing rules; neither delays the first
Stop. A release without an accepted press cannot emit an action. Fresh holds
show feedback while held but never commit before release.

Base touches require three selected pads simultaneously qualified for 10 ms;
separate short intervals cannot add up. Qualification survives staggered lift.
A moving contact is consumed without a tap, mute or swipe action. Stationary
qualified release uses 30 ms grace, the existing V1 fast-tap floor, instead of
V1 Automatic's 120 ms; ambiguous/moving contacts retain 120 ms. Spatial swipe
actions and the full V1 directional recognizer are deliberately absent. This
must be tried against real handoffs and noisy electrodes on the assembled base.

Head pads are explicitly known installed pads. There is no probing or learning
of candidate pins; V1's 20 ms LOW rule was for learning an unknown pad, not for
every touch. Both edges extend the 200 ms contact timer, and each completed
session restarts the 1.2 s decision timer. An isolated pulse resolves as Pet
1.4 s after its last edge. Directional labels, early reversal classification,
the first-session chirp and spoken phrases are not ported. State publishes
head-active edges promptly for an optional small visual reaction; it cannot
establish listening readiness. Any brain notification must remain limited to
one per 60 s by its consumer, as in V1; the Pet event is not permission to speak.

## The adapter contract

`PinReader::read(now_us)` returns immediately with at most one `Observation`,
`None` for no new acquisition, or a bounded `InputError`. `None` is never a
fresh released level. A full observation contains `ClockStamp`, reader session
identity, all configured named button/head levels, and an optional 12-bit base
mask. Logical `active:true` means pressed/touched. Configuration lists at most
eight buttons, four known head pads and one explicitly selected base mask.

Use the shared `lamp_ipc::monotonic_us()` clock and Linux boot ID. Preserve
actual acquisition times. A queued older edge must not receive the time it
was drained. Consecutive full observations have strictly increasing source
times and sequence numbers increasing by exactly one; batch simultaneous
changes into one snapshot. A restarted reader gets a new session ID. Report
kernel event loss, read errors, unknown levels and invalid MPR121 status as
faults, never as a released button or a zero touch mask. In particular,
[MPR121 overcurrent](https://www.nxp.com/docs/en/application-note/AN3892.pdf)
is a fault. Its chip debounce and software filters are different delays.

An observation or inter-observation gap of 100 ms is stale. Unknown, missing,
future, duplicate, out-of-order, wrong-clock, lost-sequence and changed-reader
input cancels recognition and withdraws hold feedback. Recovery requires a
new released baseline; a still-held contact cannot become a fresh press.
Clock reversal and exhausting the 32-retired-reader fence require a new
engine. Accepted Stop/mute records survive input failure independently of
pending recognition. All feedback duration comes from observed acquisition
time; an advancing timer cannot invent a held button during a reader stall.

Read the mechanical/head GPIO edges without discarding intervening transitions.
For an unchanged level, periodically acquire it again rather than replaying a
cached observation with a fresh stamp. Base polls normally run at 10 ms; the
adapter must describe honestly when each mask was read. Full observations are
a contract above those individual interfaces, not a claim that hardware reads
are simultaneous. If one configured input cannot be read, report the fault;
do not fill its value with the previous sample. Omit truly absent optional
inputs from configuration before starting instead of silently changing it.

The runtime targets one step every 2 ms, drains at most 64 observations per
step, ages a sample after the adapter returns, and faults on a full drain.
These are software budgets, not hard scheduling guarantees. `controls.lock`
allows one process per bus root. The OS releases the lock when a process dies.

## The board's reader (first builder, 2026-10-14)

`src/pins.rs`. `lamp-controls serve --pins FILE` reads the board's GPIO
lines named in FILE in place of the stand-in that reports Missing.
Without `--pins` nothing changes.

The file is the board's wiring and nothing else: the chip, and for each
button and head pad its name (the same names, in the same order, as the
controls' own configuration), its line, whether a press or a touch pulls
it low, and how it is held when nothing drives it. A field nobody knows
is refused. `deploy/lamp-4ace/controls-pins.json` and
`deploy/lamp-4ace/controls.json` are lamp-4ace's.

Every configured line is read together every `every_ms` (5 on lamp-4ace)
and given as one complete observation stamped with the moment of the
reading, numbered one after another, under a session of the reader's own
that is new at every start. A line that cannot be read is reported as
Missing, never as released. The lines are taken for this program alone:
if another holds one, it does not start. The privacy slide is not read
here; it is the voice's.

`lamp-controls probe --pins FILE` reads each line of the file once with
a pull-down and once with a pull-up, 10 ms each, and serves nothing: a
line that reads the same both ways is driven by something, and one that
follows the pull is not.

**On lamp-4ace, 2026-10-14, about 10:35, nobody touching it.** The older
software's record names the primary button on chip 0, line 99, and four
places a head pad may be wired, lines 37, 96, 97 and 98
(`robots/lamp/docs/physical-controls.md`). The probe:

| Line | With a pull-down | With a pull-up | So |
|---|---|---|---|
| 99, the button | high | high | Held high by the board: a button to ground, pressed is low |
| 37 | low | low | Driven low: a pad is wired here |
| 96 | low | low | Driven low: a pad is wired here |
| 97 | low | high | Follows the pull: nothing wired |
| 98 | low | high | Follows the pull: nothing wired |

So this lamp has the button and two head pads. The two pads rest low,
where the older software's page says a wired pad rests high: lamp-4ace's
file takes a touch to be high. **That is read from the lines at rest and
not from a touch.** Twenty seconds of `serve` with the file, watching
only: 3,232 observations taken, one every 6.2 ms, none refused, no
reading failed, the longest step 0.5 ms, and all three inputs at rest
throughout. `controls/state` was published.

**Running on lamp-4ace since 11:03 the same day**, as its own unit
(`deploy/lamp-4ace/lamp-controls.service`), watching only: no `--enable`, so
its gestures say `enabled:false` and nothing acts on them. Its two files sit
beside its build in `/opt/lampOS/controls/<sha>/`, because `/etc/lampOS` is
root's alone and the first start, with them there, failed on "Permission
denied". It says `controls/state` on the bus, and since 11:17 it writes one
line to its journal (standard error; standard output stays the closing
summary) whenever a hand changes something: the button pressed or let go,
the head touched or let go, a gesture, a fault. Changes only, at most 20
lines in ten seconds, the rest counted. Its first line was
`hand: button primary released, head not touched`. So the first real press
or pat is kept whenever it comes, with
`journalctl -u lamp-controls | grep hand:`. Polling every 5 ms it took 11.5 s
of processor time in its first 14 minutes, 1.4 % of one core.

Not shown: a press or a touch. Nobody's hand has been on the lamp for
this. The first press of the button and the first pat of the head, with
`serve --record`, will show which way each line goes and how long a
touch lasts, and the file is to be corrected from that before anything
acts on it. Nothing does yet: the voice does not listen to
`controls/state`.

## Topics and the receiver

Both types live in `lamp-msgs` and are additive; the old first-topic table and
all actuator contracts are unchanged.

| Topic | Kind | Meaning |
|---|---|---|
| `controls/gesture`, `ControlGesture` v1 | Event, 500 ms lifespan | Source, gesture, one-way intent, original release/duration, enabled flag and stable event identity |
| `controls/state`, `ControlsState` v1 | Latest, 100 ms heartbeat, 300 ms lifespan | Input health, observed button/base hold feedback, head activity, independent retained Stop and mute records |
| `diagnostics`, `Diagnostic` | Event, once a second | Reader faults, counts, drops, publication/recording errors and explicit lack of physical confirmation |

State is also published on feedback transitions and gestures. Its session is
published before that step's event; consumers still handle loss and order.
Quiet gesture topics renew their publisher liveness once a second. A later
pet never overwrites a Stop/mute. Retained commands keep their original event
identity and time, so a new heartbeat does not create another action.

Voice and Body subscribe to `controls/state`; optional pet/expression consumers
also subscribe to `controls/gesture`. `receiver::Receiver` validates the type's
fields, boot clock, enabled flag, monotonic state, session and freshness, and
deduplicates requests. It fences 32 retired publisher sessions, then refuses
further input until recreated with the saved mute. A Stop expires against its
original release and publication times. An older retained one-way mute can
still close input, but cannot clear anything. `Receiver::pet` requires a fresh
known owner and deduplicates advisory events. Missing/stale Controls means no
new hand gestures; the lamp's other healthy parts continue as before.

The helper does not grant hardware permissions, authenticate arbitrary local
processes, or supply an actuator acknowledgement. Use the existing trusted
bus directory and validate the typed bus header/expiry before calling it.
Never use the bus header's publisher-relative `at_us` as the shared clock.

## Voice and physical privacy

The existing Voice GPIO worker remains the only reader and authority for the
physical slide switch. Controls has no privacy-switch field and cannot forge
Voice's `PhysicalMute` restart token. The fresh 50 ms observations, stable
60 ms unmute, verified shutdown, new runtime identities and input-readiness
checks in [service privacy](service-privacy.md) remain unchanged.

First's Voice subscription goes in the local coordinator loop independently
of provider traffic and presentation. On Stop, revoke the current reply and
queued output using the existing owner/cancellation path, including late
provider chunks, music and any active following intention. Do not wait for a
network response or a cue, and do not cancel underlying agent work merely
because its speech was stopped. On software mute, inhibit new microphone,
camera and output permissions locally, cancel owners, then perform bounded
cleanup. Use a distinct software-inhibit outcome; do not manufacture a fresh
physical-switch observation or reuse `PhysicalMute` to reopen the service.

`mute::MuteRecord` is an optional owner helper. Load it **before** starting
capture/provider/output workers, from a private file in a trusted 0700
directory. Corrupt, future, exposed or interrupted state is an error, not an
unmuted default. On accepted mute, revoke immediately in memory, persist with
atomic replace plus file/directory sync outside the urgent loop, and acknowledge
only after success. Keep inhibition on a write failure and make that failure
visible; do not automatically restart into an older preference. A crash before
an unacknowledged request reaches durable storage is not claimed recoverable.
The physical switch is still the independent privacy control.

The record is **boot-scoped**: an owner/service restart preserves it; a new
machine boot starts a fresh software preference, still subject to physical
privacy. Successful explicit clear advances the original-release watermark,
so a retained or delayed pre-clear hold cannot re-mute after restart.
`Receiver::clear_after_explicit_user_request(now_us, path)` stages a clone,
saves/syncs it, and only then changes its live software latch. A save failure
keeps the live latch. Invoke it only for a separate explicit user request;
never for a tap, timer, process start, Controls loss, or an open switch reading.
Even successful clear supplies no capture permission. Voice must pass its
ordinary fresh privacy, cleanup and readiness checks before opening anything.

Before enabling this mapping, First must wire and test that explicit clear
operation in its local operator control, including an applied-state receipt.
There is intentionally no file-editing CLI that bypasses the live owner.
Body consumes Stop/mute at its final intention boundary, clears following and
expression work, and uses its locally safe controlled stop. A gesture never
commands raw joint targets, torque-off, or a new motion for which clearance is
unknown. Feedback uses the existing ring owner; no spoken listening cue is
emitted by Controls and no feedback state claims capture readiness.

## Replay and local records

One written scenario exercises both button taps, a base tap, a released hold,
head petting, lost input during another hold, recovery, and a moving base
contact. From the standalone `lampOS/` root with the binary on `PATH`:

```sh
lamp-controls replay --config crates/controls/fixtures/controls.json --input crates/controls/fixtures/hand-controls.jsonl --clock-id written-controls --enable --output /var/tmp/controls-written-20261014.jsonl
```

Use a fresh output filename. The fixture has 901 receipt records, 900 complete
acquisitions and one explicit overflow. It yields six requests with no rejected
acquisitions; the held contact spanning the fault produces no mute. Replay
never opens a bus or hardware, and `--enable` only labels simulated output.
It has a supplied 2 ms clock, at most ten simulated minutes, 32 MiB input,
100,000 records and 16 KiB per line. It emits changes/gestures and 100 ms
snapshots. EOF creates no fresh samples; feedback becomes stale.

After First installs its adapter, start with shadow and record real acquisitions:

```sh
lamp-controls serve --config /etc/lampos/controls.json --record /var/tmp/controls-real-20261014.jsonl --seconds 60
lamp-controls replay --config /etc/lampos/controls.json --input /var/tmp/controls-real-20261014.jsonl --clock-id "$(cat /proc/sys/kernel/random/boot_id)" --enable --output /var/tmp/controls-real-replay-20261014.jsonl
```

Without the adapter, that serve command truthfully records Missing faults.
The optional writer has a 32-item queue, creates a new 0600 file, and cannot
block the reader. It shares replay's size/count/time limits and reserves room
for an overflow marker. Lost acquisitions interrupt replay recognition; a
terminal drop cancels pending timers at the last recorded time. Invalid large
observations become a bounded Invalid record. Writer drops/errors and a finish
timeout are reported; a recording with any of these is incomplete evidence.
Shutdown gives the optional writer 100 ms to finish; disk synchronization is
not a hard userspace bound. Replay output errors are returned, never called a
complete replay. Keep the printed summary with a real recording.

## Qualification and evidence

The unchanged workspace built on Third's x86_64 development Mac before this
task. The new tests exercise pure clocks, historical/written touch masks, local
Unix datagrams, source/owner restarts, input loss, private mute files, a real
CLI child and SIGTERM. The final gate and standalone-copy evidence are recorded
in [Third's log](work-log-third.md) and the
[host evidence](verification/controls-host-20261014/results.json); none is an
on-lamp result. The Controls crate has 121 tests and the new message contracts
have 13. The source-only copy passed all 159 Controls/message checks, including
the pre-existing message tests, and built the whole workspace independently.

Written nominal 2 ms polling cases cover 16 first/repeated GPIO releases at
eight phases: release-to-recognizer dispatch is 0–1,999 simulated microseconds.
Base pulse cases at 10 ms polling give 32.45–39.95 simulated milliseconds from
authored release to recognition, including sampling phase; accepted cases are
exactly 30 ms after the first sampled full release. Neither includes electrical
noise, chip processing, bus/owner delay, buffered audio or actual joint motion.

One release-mode invocation ran the written replay 100 times on Third's Intel
Core i7-10700K development Mac (macOS 26.6.2, Rust 1.96.0). The measured boundary
was input parsing, engine construction, injected-clock replay and JSON
serialization to a sink: first iteration 3.340848 ms, median 2.975859 ms, worst
3.340848 ms. Configuration parsing preceded timing. This excludes process
startup, the bus, disk writing, scheduling jitter and all physical response;
there is no before/after comparison. Reproduce that host workload with
`cargo run --offline --release -p lamp-controls --example replay_cost -- 100`.

First checks the configured lines and electrodes, then observes real taps,
holds, repeated panic taps, head pats and untouched motor/speaker activity.
Measure from a physical input observation to owner revocation, then separately
to acoustic silence and motion stopping. Try a press during provider/network
stall, a held boot, reader overflow/unplug, Controls SIGKILL/restart, Voice
restart while muted, a physical-switch cycle and an explicit software clear
while the switch is closed. No old reply or captured audio may resume. Faults
must remain faults; a stopped component or a successful bus send is not proof
that an actuator stopped.

## Not shown

No GPIO/I²C adapter, lamp build, installed subscription, physical privacy
transition, electrode-noise acceptance, touch-to-silence latency or safe motion
stop was run by Third. No hardware feedback or applied mute/clear receipt is
provided by this process. The most likely lamp mismatch is base qualification
and release timing under its actual electrodes/noise, followed by lost GPIO
edges or an acquisition clock converted incorrectly. The shipped process
cannot operate a hand control until First supplies the real reader and the
two owners' integration.
