# Conversation and ring ownership

The directed Rust runtime now has an optional, separately supervised ring
writer. The interaction Controller owns the conversation; a small local policy
derives light cues from that authority. This is the first connected expressive
output, not a finished character performance. Head/body motion, visual social
understanding and expressive variation remain implementation and physical
qualification work.

## What triggers a cue

| Observed state | What the ring shows |
|---|---|
| Startup, no admitted turn or completed turn, in a bounded `directed` run | Black. Worker readiness alone does not imply listening readiness. |
| The same in `serve`, while the microphone is permitted and input is available | The resting face: a slow, dim, warm breath (5.2 s) with a faint brighter patch that wanders around the ring. Black as soon as privacy closes, input is unavailable or the authority goes stale. |
| Fresh retained input admitted to the current turn | The listening face: cool, at once, brighter with the person's voice. |
| The same while the internet cannot be reached (`LAMP_OFFLINE_VOICE=on`, the link down 5 s or more, #830) | The offline breath: one slow, dim breath in warm amber, about 2.5 s a breath, never more than a third of the listening face's least, no red, no flashing, the same all round. Only listening changes; idle, thinking and speaking are as always, so an empty room sees nothing different. |
| Local input ended; the turn is unfinished without active playback | The thinking face: an amber point circling the ring once every 1.15 s with a fading tail. No spoken waiting filler. |
| Matched speaker acceptance followed by `playback_started` | The speaking face: warm, as bright as the lamp's own voice is loud. Provider audio arrival alone does not start it. |
| A duplex yield in force (`--turn-taking duplex`: the lamp has stopped handing reply audio to the speaker for someone speaking over it) | The listening face, as bright as that person's voice is loud, on frames that are still speaking frames in phase, owner and permit (`RingFrame.attending`). It ends with the yield: the speaking face again if the reply resumes, the ordinary listening phase if the interruption is taken. |
| Playback's actual final sample retired | End the speaking phase; completion clears the owner and cue, and in `serve` the resting face eases back in. |
| New admitted question, privacy closure, expired authority or stop | Retire the old cue under its original identity. A successor can obtain its own cue. |

### The face

`crates/voice/src/ring_face.rs` holds what the ring shows and nothing else: a
mood (resting, listening, thinking, speaking), a voice level and a time go in,
32 colours come out, and no channel exceeds the configured ceiling. It has no
I/O, no clock and no authority. The ring worker keeps one face and renders
every frame it is allowed to write from it, so a change of mood eases over
about 70 ms instead of jumping, and the first frame of a cue is already
visible. Every pattern but the eye's (below) is symmetric around the ring or
travels around it, so none depends on which pixel is at the top of the head.

The voice level is one byte in each `RingFrame`: the root mean square of the
last 10 ms block, 0 at -48 dBFS and 255 at -14 dBFS, of the person's captured
audio while they speak and of the lamp's own speech as it is handed to the
speaker. It shapes a frame and grants nothing: when a frame is requested, and
whether it may be written, are decided exactly as before. In the face the
level rises within 20 ms and falls over about 170 ms.

One sequence has a look of its own. When thinking ends in rest with nothing
said in between (the service chose not to answer, or the speech was not for
the lamp), the face closes for a moment: it goes dark over the first fifth of
0.9 s and opens again over the rest, as eyes close on "not for me". A turn
that was answered never does this.

A level between two brightness steps is shown by rounding each pixel at its
own fixed point, so some pixels sit one step higher. That keeps a dim breath
smooth where a ceiling of 24 leaves the resting face two or three steps to
work with. Nothing is dithered in time.

### A hand kept on the head: the countdown (#711)

Off unless set: `LAMP_HEAD_TOUCH=on` in the voice's unit, which the voice
passes to its all-day ring worker as `--head-touch`
(`crates/voice/src/ring_head.rs`); it needs `LAMP_BUS=on` and Controls run
with `--touch`. The ring worker reads the head's state (`controls/touch_state`,
the head's `held_us`) every 50 ms. From one second of a hand kept on the
head (Controls' stroke) to three (its sleep), the whole face dims to 15 % of
itself, eased like every other change: a countdown the person can see, and
can stop by letting go, when it brightens again. Under a second nothing
changes; a hand a push from the body spoiled has no time and dims nothing.
It never makes the ring dark: what is shown still says the lamp hears. The
sleep that follows at three seconds is not drawn here yet (#711): the ring
worker is gone while the voice is muted.

### A tap on the head at rest: the listening face (#711)

Same switch and same worker. A tap on the head (Controls' `controls/touch`,
a fresh, enabled head Tap from `controls`) and the head's hand time are read
from the bus by a thread of the head's own (`ring_head.rs`), never by the
ring's loop, which only reads the two numbers it last found, after each
tick's Stop, Authority and expiry (#849 review, twice):
- One receive from the bus passes over any number of messages it refuses, so
  no count of receives would bound it. The loop does no bus work at all.
- The test `a_flood_of_head_requests_never_holds_back_revocation_or_expiry`
  floods `controls/touch` from four senders: valid taps through the bus, and
  messages of another type written straight to the subscriber. While it goes
  on, a 50 ms permit must blank within 150 ms and a Stop sent then must end
  the worker within 50 ms.
- It failed on the first head, which read the bus before the controls. On
  the second head (at most four receives a tick, after the controls) this
  fixture passes on Third's Mac. The reviewer's own fixture, a different
  procedure on a Mac too, made that second head miss Stop by 485 ms. Why the
  two procedures differ is not established. What this head guarantees is
  structural: the ring's loop does no bus work, so no receive can stand
  between it and its Stop, Authority and expiry.
- The head's thread keeps the hand's time with its expiry, the state's stamp
  plus Controls' 300 ms. The ring's loop checks that expiry itself, so if
  the thread stalls, the hand lapses on time and stops dimming the face
  (#849 review).
- **Shown:** at rest, the resting face becomes the listening face, the one a
  turn shows while it listens, for `TAP_LISTENS_US` (2 s, as long as a call
  waits for the rest on lamp-4ace, #891).
- **Speech in the window** is a turn, whose own listening face goes on from
  there.
- **Afterwards:** if nothing comes, the face settles back to its own light,
  most of the way in about half a second (`SETTLE_EASE_US` 170 ms, 600 ms in
  all), slower than the ring's usual easing, so it does not snap.
- **The hum's own turn** owns the lamp for its 0.3 s, when no resting frame
  may be shown. Each of its frames is the listening face instead
  (`RingFrame::listening`, stamped by the coordinator for a `HeadCue` turn,
  from the pass that admits it), so the face does not wait for the hum to end.
- **Not changed:** other turns' faces, and the ring's refusal of a resting
  frame when nobody may listen.

It is one gesture with the voice's "hm?" ([attention.md](attention.md#a-tap-on-its-head-is-a-call-711))
and Expression's look-up ([alive.md](alive.md)), all from the one tap event
(First, #711, 05:05).

### The eye (task 34, #712)

Off unless set: `LAMP_RING_EYE=on` (or `strong`, below) in the voice's unit, which the voice
passes to its all-day ring worker as `--eye` (`crates/voice/src/ring_sight.rs`).
Without it every frame is what it was (a test renders every mood with and
without the switch, with a face seen, and compares them pixel for pixel). With
it the face is the lamp's eye in two ways:

- **It looks at the person while it listens.** The ring worker reads the
  eyes' newest look (`humans/faces/tracked`) ten times a second, takes the
  largest face no older than 0.7 s, and the face eases its look toward it
  over about a quarter of a second. Looking to one side, the half of the ring
  away from the person dims, by up to three quarters, and the half toward them
  keeps its light; a person within 6 % of the middle of the camera's picture,
  or no face at all, has the whole ring, as before. So when the body turns the
  head to a face, the look should lead and fade as the face comes to the
  middle of the picture (not yet seen on a lamp). Only listening looks;
  thinking and speaking keep their own faces.
- **It blinks at rest,** now and then: every 7 to 16 s, not on a beat, the
  resting face dims to a quarter for 150 ms, eased in and out.

**The strong eye** (`LAMP_RING_EYE=strong`, passed as `--eye --eye-strong`;
first builder, 2026-10-16). On lamp-4ace on 15 Oct the owner found the eye too
subtle: the body turns the head to a face, so the face sits near the middle of
the picture, inside the 6 %, and the eye rarely leant. The strong eye is the
eye with two changes, and `on` is as before:

- **It leans sooner and further:** from 3 % off the middle, fully at 14 %
  (6 % and 22 % for the plain eye), and its far side dims by up to nine
  tenths (three quarters).
- **It notices a person who arrives:** when a face is seen after none for
  3 s, the whole face brightens by a third at once and settles back over
  about a second (0.3 s to a third of the way), listening or at rest, as eyes
  widen at someone who comes in. A face lost for a moment and found again is
  not noticed again.

Tests: the strong eye leans toward a person near the middle whom the plain eye
looks straight at; it notices an arrival once, against a plain eye run frame
for frame beside it, and again only after 3 s without a face; with the eye off
it changes nothing. Not yet seen on a lamp: it is for the owner's face and the
iMac's camera on 16 Oct.

Which way the ring counts was measured on lamp-4ace on 2026-10-14 at 19:54:
with the voice stopped, `crates/ring/examples/pixel_map.rs` lit pixel 0 red,
1 and 2 orange and yellow, 8 green, 16 blue and 24 white, and a photograph
from in front showed them at the top, just after it, and at three, six and
nine o'clock. So pixel 0 is at the top and they count clockwise as a person
facing the lamp sees them. The camera sits in the middle of the ring looking
out, so a person on the left of its picture is on the lamp's left, which that
person sees on their right; the look goes there.

On lamp-4ace since 2026-10-14, 20:15 (build `d3e9cc3d2`, the ring worker
started with `--eye`). Played a request at 20:16 with no face in view, the
lamp stood and listened with the whole ring lit, as before. The look toward
a face is not yet seen on the lamp: a face photograph on the screen in front
of the lamp, which the eyes found in every look on 2026-10-13, was found in
none that evening, because the screen was locked and showed only its clock
(the eyes' own frame showed it), so the photograph never appeared. It needs
the screen unlocked, or a person at the lamp. The blink at rest has not been watched on the lamp either.

Not done, from the task: the mood's pace and warmth (from the inner state,
#713, when it publishes). The muted look is the asleep face, below.

### The resting face and its authority

A resting frame belongs to no turn and claims nothing about one. The ring
worker paints it itself, forty times a second, and only in `serve` (the
coordinator starts the worker with `--resting CEILING`, the same ceiling as
its cues). `GuardedRing::show_resting` writes it only when the installed
authority is fresh, the microphone is permitted, input is available and no
turn owns the lamp, and never over a displayed cue. `tick` blanks it when the
authority expires, privacy closes or input stops being available. An owner
appearing does not blank it: the last resting frame stays until the first cue
frame is painted over it, so a turn begins with no dark frame. The coordinator
receives no receipt for a resting frame; a blank that ends one is reported
like any other.

So the resting face means one thing: the lamp is on and able to hear. It is
dark when the privacy switch is closed.

### The asleep face (#712)

A lamp that cannot hear, because it is muted or its privacy switch is
closed, has no ring worker, so without this its ring is dark, which reads as
off or broken. With `LAMP_RING_ASLEEP=on` in the voice's unit, the service
shows the asleep face while it waits for the lamp to hear again: a faint warm
glow on the five pixels at the bottom of the ring (at most 12, 5, 1 of 255,
and never over the resting ceiling), breathing once every 6 s between three
tenths and all of that. It fades in over 1.5 s, starting 1.5 s into the wait,
so the short wait at the start of every cycle never shows it. It is not red
and not a line: a sleeping thing closes its eyes and keeps breathing.

It is painted by `ring_asleep` from a thread of the voice, through
`lamp_ring::asleep::AsleepRing`, the one writer that needs no permit because
the one picture it can show (and black) says only that the lamp cannot hear.
It opens the device after the cycle's ring worker has gone, and it is black
with the device closed before the wait returns, so the next cycle's ring
worker is again the only writer. If the device cannot be opened it says so
once in the journal and the ring stays dark, as before.

### The lamp's own light

The resting face is also the lamp's light: a colour and a level that
someone can ask for ("make your light blue", "a bit brighter") and that
stays until it is asked for again. The faces of listening, thinking and
speaking are the lamp's expression and do not change with it; when a turn
is over the ring eases back into its light.

| | |
|---|---|
| Colours | Ten, by name: yellow, red, green, blue, cyan, purple, orange, pink, white, warm. `warm` is the amber the lamp has always rested in |
| Levels | 1 to 5: about 7, 24, 44, 68 and 90 hundredths of the ceiling, each with a slow breath of 6 to 10 more on top. The fifth reaches the ceiling at the top of a breath |
| Before anyone asks | On, warm, level 1: exactly the resting face described above |
| Off | Not dark. An ember: one pixel in four at three steps of red and one of green, the faintest amber the ring can show, and the others dark. About a quarter of the lowest light in all |

The ember is a few lit pixels and not the whole ring glowing fainter,
because the ring cannot show that: under the ceiling of 24 that lamp-4ace
runs with, the lowest light is already two or three steps of red and one
or two of green, and less than one step of each can only be shown as some
pixels red and others green. That was the first version, and on the lamp
it was a patchwork of pink and green.

Off is an ember because of what the resting face means. A ring that went
dark on "turn your light off" would look like a lamp that cannot hear,
while it can. What is answered says so: "My light is off. The faint glow
that is left shows I am still listening." The privacy switch still makes
the ring dark, whatever light is set.

The ring worker is the only writer of the ring, so it is also the one
that takes the light (`src/ring_light.rs`). Started with
`--bus-root DIR` (the coordinator adds it when the voice's unit has
`LAMP_BUS=on`), it:

- begins with the light last asked for, which the bus keeps as the newest
  value of `light/command`, or with the first light if there is none;
- looks every 50 ms for a light asked for on `light/command`
  (`lamp_msgs/Light`) and eases the ring into it. A request that is not a
  light there is (a colour that does not exist, a level outside 1 to 5,
  something that is not a `Light`) is passed over;
- says what it shows on `light/state` when that changes and once a
  second, each good for three seconds, so that a ring that has stopped is
  not a light that can be asked for.
- takes a self-test's colour on `light/test` (`lamp_msgs/Light`, asked
  less than a second ago) and shows it for 2 s at most, then the light
  asked for on `light/command` again. A `light/command` ends a test at
  once, even one for the light already shown. A test never touches
  `light/command`, and `light/state` goes on saying the person's light, so
  a test cannot overwrite what the person asked for (#812);
- says the light it drew on `light/drawn` (`lamp_msgs/Light`), from the
  ring's own write receipt for each resting frame, timed by when the write
  finished, when that changes and once a second, each good for three
  seconds. A frame the ring refused (the privacy switch closed, a turn
  owning the lamp) says nothing, so `light/drawn` is what was on the ring,
  where `light/state` is what was taken;
- with `--light-requests` (the coordinator adds it when the voice's unit
  has `LAMP_LIGHT_REQUESTS=on`, #940) takes lights asked for that lapse on
  `light/request` (`lamp_msgs/LightRequest`, `src/ring_requests.rs`): a
  skill's glow, or anything else that must not leave the person's light
  changed. Each asker (`claim.source`) has one request, its newest. The
  ring shows the highest priority that holds on the host's boot clock
  (`/proc/sys/kernel/random/boot_id`), the newest of equal priority, and
  the person's light again once none does, with no one giving it back. An
  asker gives the light back early by asking with a time already passed.
  A claim holds 30 s at most, so a skill renews what it keeps. A
  `light/command` ends every request below `person` that holds, and it
  stays ended while its asker renews it; the asker's next claim, after
  that one lapsed or was given back, is heard again. At most 16 askers
  are kept and 8 requests read a look. A self-test's colour still comes
  first. `light/state` goes on saying the person's light; `light/drawn`
  says the request's while it is drawn. A boot clock that cannot be read
  is said once and the ring takes the person's light alone.

Whoever asks never writes the ring: the actions program publishes the
light wanted and reads the state back ([actions.md](actions.md)). A bus
that cannot be used is said once in the voice's log and the ring runs as
it did, in its first light. The light is kept by the bus, which lives in
memory: after the board restarts the lamp is back in its first light.

**On lamp-4ace, 2026-10-14, about 03:00, nobody in the office.** Called
directly on the actions program's socket, each change was answered in 15
to 108 ms with the state read back, and a colour that does not exist was
refused. Stills through the office Mac's camera showed the ring blue at
levels 1, 3 and 5, red at 5, and pink, white, purple and green at level
1, each readable as its colour; and the ember, in its second form, as
eight amber dots. Asked aloud by a played voice, ten requests: nine did
what was asked and said so ("There, blue." "Red it
is!" "Better? That should be a bit easier on the eyes." "Back to warm,
just how I like it."), one of them a follow-up with no name in it. The
tenth, "make your light blue", reached the model as something else and it
made the light brighter, and said that it had.

Before that, the first four requests found a fault. The light changed
each time and the lamp said nothing: the model did what was asked and
then called its function for "that was not said to me", and the
character file still told it that it could control nothing. One of the
four, misheard, turned the volume up. The character now says what it can
change and that it is to say so, and a decline is no longer taken in a
request the lamp has acted on ([tools.md](tools.md)).

Not shown: a person asking. How the levels look to someone in the room,
and whether level 5 is a useful light under a ceiling of 24, a camera
cannot say: it saturates on the ring. The light is not restored after the
board restarts.

Seen through the office iMac's camera on lamp-4ace on 2026-10-12 with a
ceiling of 40: across eight stills of the resting face the dark centre of
the head, which reflects the ring, varied between 45 and 59 of 255 while a
patch of floor stayed at 167 to 170 in seven of them; during one exchange the
centre read blue while a question was played, warm while the lamp answered,
and fell back to the resting level afterwards. The ring itself saturates that
camera in every state, so these stills say nothing about brightness as a
person sees it, and the circling point was not caught in a still.

The cue signals accepted conversation state, not proven
social understanding: the directed voice harness still has false interruptions
from playback and does not know whether ambient speech addresses Lamp.
ALSA acceptance/retirement are software/driver boundaries, not optical or
acoustic measurements. Stillness, silence and an unlit ring are valid behavior.

## Explicit qualification option

Append `--ring-channel-ceiling 0..120` to `directed`, `directed-fixture` or `serve` to
include the ring worker. A value of zero still starts the guarded writer but
caps its output at black. Without the option no ring device is opened. Repeated,
missing, negative or out-of-range values fail argument validation. The option
is independent of audio diagnostics and software noise suppression.

```sh
target/release/lamp-voice directed /absolute/private/provider.json 45 /new/private/run --diagnostics --ring-channel-ceiling 24
```

This flag isolates qualification experiments; light remains part of the full
release scope. The command requires exclusive hardware ownership and does not
stop legacy services itself. It opens no motor bus.

## Ordering and deadlines

The coordinator publishes input-end, matched playback-start/end and turn
completion immediately. New input, interruption and privacy already have an
authority path. A cue cannot authorize itself: the parent publishes its
snapshot on priority control before sending its frame on the separate data
socket. The worker rereads control after receiving data. All phase publication
also runs in an audio-only session; that mode avoids SPI but is not identical
to the earlier control-message timing.

The policy retains one pending immutable frame. It creates a new phase-bound
Light permit, normally renews the static cue every 20 ms, and waits at most
100 ms for its matching receipt. This is lease maintenance, not an animation.
The original permit is clipped by actual input/authority leases. A heartbeat
cannot extend an already displayed frame's permit. A phase change can replace
the last cue after the outstanding receipt retires; it never relabels old work.

The writer's cooperative tick is 2 ms. Each tick shares a budget of 16 priority
control packets, retains at most one data frame and uses its original request
time. Data expires after 100 ms. An older heartbeat cannot suppress a newer
privacy revocation. A newer expired authority packet fails closed; it is not
silently ignored in favor of older allowed state. Malformed/unknown control
fields, mismatched lineage and feedback backpressure are faults.

The software qualification target is at most 50 ms from policy request to
write completion, with no microphone/speaker work waiting for SPI. A slow
receipt fails ring qualification and disables further ring presentation; otherwise
healthy voice continues. This boundary excludes phase recognition
before the request and physical light onset afterward. The release's visible
transition target therefore still needs a real-device measurement.

Finite sessions give the ring worker a 630-second total lifetime cap, covering
the maximum 600-second directed run plus startup/shutdown reserve. In `serve`,
the coordinator selects its private continuous-worker lifetime while keeping
parent, privacy, authority and frame-expiry watchdogs unchanged. Authority expiry
remains independent and much shorter: Controller snapshots are bounded to
250 ms, and active input leases are 100 ms. Audio diagnostics retain their
separate 600-second wall/sample cap; diagnostic runs must leave startup room.

## Bounded renewal evidence

The summaries below describe finite directed sessions. Continuous `serve` emits
append-only ring observations into its bounded best-effort metadata writer; it
keeps no mutable index into an evicted log batch. Choreography and cleanup state
remain independent of logging loss. See [continuous chat core](serve-runtime.md).

Lease renewal still sends a real authority packet and frame every 20 ms when
eligible, and validates every returned receipt. Trace retention is separate:
logging two events per renewal previously filled the 20,000-event cap after
about 200 seconds with a continuously active cue. The cap is unchanged.

The first request and receipt for each owner/phase/ceiling remain individual
`ring_requested` and `ring_feedback` events, as do new playback occurrences
within the same turn. A cycle back to the same phase also stays individual
when its presentation lineage changed, even if no intermediate cue was sent.
During an unchanged cue, the first pair of each second
also remains individual. Intervening renewals share one `ring_renewals` record
and one raw `ring_requested` tail sample for that interval. This reduces a
stable cue from about 100 to four retained events per second without changing
any worker/control action or deadline.

Each summary retains the original owner and first/last full frames, request
count, validated presentation/rejection counts, unacknowledged count, exact
first/last presentation receipts and request-to-write latency minimum, maximum
and total. Its `at_us` is the first included request time;
`last_updated_at_us` is the latest actual request/receipt observation. Neither
replaces the worker's original write timestamps. A submitted frame without a
validated receipt remains unacknowledged, including on timeout or shutdown.
Receipts drained only during shutdown remain separate shutdown events.

The coordinator updates those two fixed slots in its append-only in-memory
trace. The tail sample always carries the latest actual request timestamp and
full original frame, including before its receipt arrives. It is marked
`trace_role: renewal_tail`; normal requests use `cue_or_periodic_head`. To count
commands, add head requests to summary `requested` counts; do not count the
tail samples again. Updating a tail can move its timestamp past interleaved
voice events, but never moves indices or changes those events. Consumers must
use original timestamps rather than infer time from array position. It keeps
no renewal history outside the bounded trace and one pending request. Final shutdown needs no summary flush to preserve the last partial
interval. Rejections, black writes, invalid receipts and service faults remain
individual events with the original pending-frame identity where available.
An unvalidated worker claim never increments successful presentation counts.

The voice evaluator currently checks individual `ring_requested` phase/voice
consistency and does not score `ring_feedback` or renewal summaries. Keeping
all transitions plus exact first/last request samples preserves the interval
boundaries used by those checks, including a stale tail exposed by a later
retirement receipt with an earlier driver timestamp. Summary
counts are additional operational evidence, not proof of physical light or
per-renewal acoustic synchronization. Individual renewal percentiles cannot be
reconstructed from min/max/total; do not describe the retained samples as all
writes. A different high-rate event source or exceptionally frequent real
transitions can still exhaust the unchanged global cap and fail the run.

Host regression coverage uses the real coordinator service, Controller,
choreographer and private Unix sockets with a virtual clock and synthetic
worker receipts. It includes more than 10,000 renewal pairs, a 600-second
schedule at 20 ms with dense voice events, phase/playback changes, interruption,
and fault injection. This proves bounded software retention for that schedule,
not physical SPI timing or a ten-minute acoustic trial.

## Failure and cancellation

Light receipts are evidence; they cannot advance the conversation or revive
an old owner. The policy checks request identity, write ordering and the
original permit. An expected supersession rejects old work without erasing a
new valid cue. A bare delayed `off()` callback is never queued for a turn.

Revocation and expiry reach `GuardedRing` even with no new frame. Stop is sent
to every worker before waiting for any one worker, within the existing shared
300 ms cooperative grace (750 ms with diagnostics). A clean ring shutdown
requires an explicit black-write receipt whose timestamps follow that stop
request, plus successful process exit. A zero exit alone is insufficient.

Ring startup and post-handshake failure are optional-capability failures. The coordinator latches the ring unavailable
before one nonblocking Stop attempt and stops sending authority or frames to
it. This does not change the current input, speech owner, playback occurrence,
or audio readiness. Mandatory audio/privacy failures remain terminal. A late
ring event cannot create listening readiness or revive its presentation policy.

The live loop observes at most 16 cleanup messages and one nonblocking child
status per pass. After 300 ms without observed exit, it requests termination
once, without waiting. The child remains owned until reaped; there is no restart
or replacement writer in this path. The latency target is zero added wait from
observed ring fault to the next audio/control work, within the existing 2 ms
cooperative tick target. Kernel signal/status calls have no hard userspace
execution deadline. Host channel tests establish ordering and bounded work,
not loaded-device timing.

`ring_degraded` records unavailability and an initially unconfirmed cleanup.
`ring_degraded_blank` retains validated SPI completion, and
`ring_degraded_exit` separately reports actual process exit. Prior presentation
receipts become historical `ring_degraded_event` records. A validated early
shutdown black write keeps its original stop timestamp through final shared
shutdown; a later Stop does not invalidate that evidence. Failed stop delivery,
malformed cleanup, forced termination, abnormal exit, or missing black-write
proof still fails final cleanup. A clean degraded shutdown may complete the
voice run; its trace still records that ring qualification failed.

SPI I/O remains in the child. Linux cannot promise a userspace deadline for a
stalled kernel SPI write; the existing 10 ms write budget is checked on return.
A crashed/killed writer cannot itself blank a latched physical ring. Software
unavailability therefore does not claim darkness or another visible state.
There is no automatic replacement. An unavailable executable or failed startup
handshake disables only the ring; an already-created unready child remains owned
through the same bounded cleanup path. Required-worker and termination errors
during that wait stay terminal. Startup has a separate 16-event optional evidence
allowance and explicit overflow count. A fresh private ring socket namespace is
removed only with exit and inode checks; unsafe replacements remain visible
cleanup failures. Startup cleanup is not an optical-darkness receipt. See
[private socket startup](worker-startup.md) for the boundary and host tests.

## Verification without Lamp

The policy tests exercise real Controller transitions, stale and duplicate
feedback, phase changes, privacy, original leases and time bounds. Worker tests
use local sockets and fake sinks to cover cross-channel ordering, delayed
authority, cancellation, expiry, malformed controls, bounded service and both
primary/cleanup failures. Test fixture children acknowledge shell exec through a
bounded one-byte pipe before a closed-peer test can drop its socket. Under the
local ARM/QEMU setup, returning from child spawn alone left inherited peer
socket descriptors briefly alive; a successful send then meant kernel enqueueing,
not a ring-policy failure. The barrier changes test setup only: production uses
its existing worker handshake, and authority-send errors remain strict. Missing
or malformed fixture markers fail setup and kill/reap only that owned child.
The process probe additionally starts a separate child with a memory-only sink:

```sh
cargo run --locked -p lamp-voice --example ring_process_probe -- --samples 120
```

It uses synthetic conversation transitions, including simulated accepted
playback. Its JSON retains ordered samples, first/warm timing, startup and
shutdown evidence. It opens no microphone, speaker, SPI, camera, motor or cloud
connection. Host test and timing results do not establish Linux/ARM64 behavior,
physical darkness, color quality, speech/light synchronization or an end-to-end
voice improvement. Keep those results separate in qualification reports.

See [reuse decisions](reuse-decisions.md) for the V1 behavior retained and
changed, and [the ring driver](../crates/ring/README.md) for protocol and safety
boundaries.

### Host qualification, 2026-10-10

The source-only copy outside the parent repository passed **535 workspace
tests**, with zero failures or ignored tests, strict workspace/all-target
Clippy, formatting and a host release build of `lamp-voice`. The Rust source
manifest SHA-256 is
`50cb160bc188b910b35117617448337265b2ab104f85c133c2b2b91772c91d97`.
Exact commands, logs, ordered timing samples and file hashes are retained
privately in `artifacts/ring-live-20261010-50cb160b/`.

| macOS process probe, development profile | Result |
|---|---|
| Samples | 120, synthetic phases and memory-only sink |
| Worker startup | 7.049 ms |
| First request to write completion | 2.342 ms |
| All-sample p50 / p95 / maximum | 1.758 / 3.177 / 3.391 ms |
| Warm p50 / p95 / maximum, 119 samples | 1.758 / 3.189 / 3.391 ms |
| Shutdown | Matching post-stop black-write receipt and successful child exit |

The local Linux VM did not remain running, so this slice has **no new Linux or
ARM64 qualification**. It has not been deployed. The earlier 518-test native
audio checkpoint does not cover this source. Real SPI duration, physical light
onset/off, startup/crash recovery, brightness and concurrency with actual audio
remain unmeasured. No before/after voice speedup follows from this probe.

### Renewal evidence qualification, 2026-10-11

Before the correction, the production service with a virtual clock and private
Unix sockets failed at zero-based renewal 9,999, retaining 19,999 events
(199.98 simulated seconds). The same 10,050-pair workload now completes with
804 retained events. A separate 600-second simulated schedule completes
30,000 socket request/receipt pairs and retains 19,735 events, including
16,800 voice events, 256 startup/shutdown reserve events, 19 cancellations and
a visible terminal worker fault. These are synthetic workload results, not
a physical ten-minute conversation or a claim that every event rate fits.

The exact integrated standalone source passed **768 host tests, zero
failed and zero ignored**, formatting, strict Clippy and both release builds:

```sh
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1
cargo build --locked --offline --release -p lamp-voice -p lamp-voice-eval
```

Rust/Cargo/toolchain manifest: `962c876dae67917da4c831afb29978bb4d011f0b55b04b3257e4ea176033b374` (167 files;
documentation excluded). Host: macOS x86-64. Sources were unchanged throughout
the sequential gates; local socket tests ran outside the network sandbox.
Independent review checked integration with playback occurrence identity,
whole-turn completion, lifecycle cues and the evaluator's original timestamps.
The original cap failure, hidden presentation-lineage and stale-tail red
regressions, scoped receipts, combined gate logs and source archive are retained
in `artifacts/ring-renewals-20261011/` with a separate durable backup.

This checkpoint does not qualify ARM64/Linux, Gemini, physical SPI timing,
light visibility, real interruption behavior or end-to-end voice latency.
No actor command, lease, timeout, brightness limit or trace cap was relaxed.

### Renewal bookkeeping cost on the host

A separate release probe compared baseline `e8b961c79` with the same ring patch
as the integrated checkpoint. It ran four fresh processes in before/after/
after/before order, each with 2,000 complete request/receipt pairs over real
private Unix sockets. The serialized request payloads matched across all four
runs, including three cancellations and twelve phase changes per process.
Both versions used the same virtual interaction clock and synthetic receipts.

| Repeated service call | Before p50 / p95 / max, µs | After p50 / p95 / max, µs |
|---|---:|---:|
| Request: authority checks, control/data sends and trace mutation | 9.146 / 15.379 / 72.499 | 37.916 / 54.210 / 159.709 |
| Feedback: receive, decoding, validation and trace mutation | 9.724 / 16.054 / 55.640 | 38.804 / 54.336 / 146.246 |
| Idle: no due renewal or queued receipt | 0.939 / 1.600 / 19.441 | 1.220 / 1.812 / 17.649 |

Each cell uses 3,998 samples, excluding only the first call per process.
First requests were 83.921 and 103.477 µs before, versus 93.366 and 134.917 µs
after. First feedback calls were 34.274 and 35.676 µs before, versus 38.021 and
58.929 µs after. No outliers were removed. Each run retained 4,000 events before
and 160 after; event count is not a byte-memory measurement.

The richer summary costs about 29 µs more per typical request or feedback call
on this Mac. Every measured call stayed under 2 ms, including first use. This
does not establish a deadline for a loaded coordinator or ARM hardware.
Timers exclude fixture construction, planned Controller transitions, mock-peer
actions, SPI, worker scheduling and file serialization. First use starts after
fixture construction, not at cold OS startup. CPU affinity, thermal state and
real-time scheduling were uncontrolled; compilation had finished before timing.

Raw nanosecond samples, first-use results, category breakdowns, outliers,
instrumented probe sources and source/build/binary hash receipts are retained
with the checkpoint evidence. This comparison measures bookkeeping cost;
speech response time and physical light timing remain separate open checks.
