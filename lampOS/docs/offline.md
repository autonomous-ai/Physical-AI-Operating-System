# No internet at home

Task [#830](https://github.com/autonomous-ai/lampOS/issues/830) (the ninth
builder), for shipping to homes on 27 November. A home's internet drops: the
router reboots, the Wi-Fi password changes, the provider is down. lamp-4ace has
never run without the internet. This page is step 1 of the task: what each
part does with no internet, and with a clock that was never set, read from its
code at `lamp-voice` `4ad0ec5c3` on 2026-10-15. Steps 2 to 4 (mend what
breaks, a part that says whether the lamp is online, show it) come after it.

**Kind:** a document. Nothing on any lamp changes.

## What exists and what we take

- **The voice's link** (`crates/gemini/src/recovery.rs`,
  [gemini-session-reliability.md](gemini-session-reliability.md)): once
  serving, it redials Gemini without end, 0.5 s doubling to 4 s apart, says
  it is down once per outage, and a session that comes back is told the
  conversation so far. Taken as it is.
- **The voice's report on the bus** (`crates/voice/src/bus_out.rs`): on
  `diagnostics`, `link: "down"` and "it cannot reach the speech service, and
  keeps trying" after 5 s; `lamp-health` shows it. Taken as the signal a
  watcher of reachability can use, before any probe of its own.
- **`voice/link`** (`crates/msgs/src/lib.rs`, `VoiceLink`: ready or
  unavailable, with why) is declared and registered, and nothing publishes
  it. The place for the voice's own link state.
- **Recorded failure lines** ([failure-notices.md](failure-notices.md),
  `--failure-notice-dir`): built, packaged nowhere, no wording approved, and
  played only for a turn whose request failed. The way to say "I can't reach
  the internet" with no internet: a recording on the lamp.
- **Schedule's trusted clock** (`crates/schedule/src/time.rs`,
  [schedule.md](schedule.md)): elapsed timers run on the boot clock; calendar
  reminders wait (`waiting_for_clock`) until the wall clock is trusted, which
  is systemd-timesyncd's marker `/run/systemd/timesync/synchronized` or
  `--trust-clock`. Taken: the rule. Not taken: `--trust-clock` on every lamp
  (below).
- **The board's clock** (`crates/health/src/board.rs`, `clock_synced`):
  read from the same marker only, so `None` on the real board, which has no
  timesyncd (health.md: "The real board has neither `/proc/net/wireless` nor
  `/run/systemd/timesync`"). Nothing but `lamp-health show` reads it.

## Each part with no internet

| Part | Uses the internet | With none | Crash-loop or flood | Back by itself |
|---|---|---|---|---|
| voice, **already running** | Gemini Live, one link per session (connect 10 s, setup 10 s) | Redials without end, 0.5 → 4 s apart (`recovery.rs:93-94,1263-1270`); `diagnostics` Warn after 5 s; speech heard meanwhile is dropped silently (`coordinator.rs:1490-1510`); the ring shows nothing different (`ring_wire.rs`: listening, waiting, speaking only) | No: one Unavailable per outage, no line per attempt | Yes, within 4 s of the link (plus up to 20 s for a dial that hangs), the conversation told again |
| voice, **starting** (power-up, restart) | as above | Not ready within 25 s (`INPUT_READINESS_TIMEOUT`, `coordinator.rs:82`): "voice input never became ready", `exit(1)` (`main.rs:19-20`) | **Yes.** lamp-4ace's and lamp-52e6's units have `StartLimitIntervalSec=0`: a new voice every ~29 s for as long as the internet is down. The template and product units (10 starts in 10 min) stop it for good after ~5 min, and nothing starts it again | Only by its next start |
| second voice (#778) | Cloud too: speech-to-text, a chat model and speech (`docs/second-voice.md` on `sixth/second-voice`, not on `lamp-voice`) | Covers Gemini being down, not the internet: every turn fails | Not known | — |
| name | No (local models, `crates/name`) | Works | No | — |
| turn end | No (local model, `crates/turn-end`) | Works | No | — |
| policy | Jev, HTTPS, 3 s a question (`decide.rs:36`) | The rule decides each utterance; `diagnostics` Warn "the decision model is not answering" | No: one line when it starts failing (`policy.rs:293-296`) | Yes, at the next question |
| tasks | Through the Harness program, not itself | A task is `NotSent` or `Uncertain` and not tried again; reads every 30 s / 120 s fail quietly | No | Reads yes; a task not sent stays not sent. Its health stays Ok while Harness cannot reach anything (`runtime.rs:858-906`) |
| update | The channel, HTTPS, every 15 min | "nothing changed: the channel cannot be reached"; next round | No: an outcome the same as the last is not said again | Yes, at the next round. **Its name lookup has no timeout** (`fetch.rs:89-91`): a resolver that hangs holds a round |
| surround | Only when an operator stages a release | That command fails | No | — |
| memory, eyes, schedule, health, actions, body, controls, expression, air, ears, keeper | No (no network crate, no socket; checked in each `Cargo.toml` and `src`) | Work | No | — |

## Each part with a clock not set

The lamp has no battery clock. After power it starts at its last saved time
(58 minutes behind on lamp-4ace at 18:15 on 2026-10-15) until NTP sets it,
about 70 s later; with no internet it stays wrong.

| Part | Uses the wall clock for | With the clock wrong, then set |
|---|---|---|
| schedule | Calendar reminders (`time.rs:334-341`); "today" for a new request (`time.rs:255-262`) | With `--trust-clock` (both lamps' units): reminders fire up to the error late; when the clock jumps forward every one passed becomes due at once, marked late (`engine.rs:317-335`). Without it they wait, and a new request, a timer too, is refused ("trusted wall clock required", `time.rs:229-235`) |
| voice | The time told to each new session (`local_time.rs:168-182`); the `the_time` tool ("this one is exact", `tools.rs:259-283`) | Says a wrong time, as exact. Never says the clock is not set |
| expression | Night, 23:00 to 06:00 (`alive.rs:601-617`) | Sleeps and wakes up to the error late; right within a minute of the clock being set |
| memory | Day boundaries, expiry (`store.rs:104-107,825-835`) | Entries stamped with the wrong day; a jump forward moves "today" and expiry forward; a backward step revives nothing (`days.rs:310-317`) |
| records | Age of recordings (`plan.rs:70`) | A jump forward may make recordings old enough to delete |
| policy | The hour for speaking unasked (`progress.rs:357`) | Not used: no unit gives it `--utc-offset-minutes` |
| body, eyes, name, air, ears | — | Not affected |

## What breaks, in order

1. **The voice never starts with no internet**, and on today's lamps
   restarts every ~29 s for as long as it lasts. Wanted: it starts, listens
   and keeps its name and turn-taking with the link down, as it does once
   running; with a limit, a part that cannot start is not started forever.
2. **Nothing is said or shown.** Speech is dropped in silence and the ring
   looks as always. Wanted: one recorded line, once per outage, and a calm
   look of the light (#830).
3. **A clock not set is used as if set,** by schedule (through
   `--trust-clock`), the voice's `the_time`, Expression's night and memory's
   days; and the board's `clock_synced` is never known on a real board.
   Wanted: one reading of "the clock is set" that is true on the board
   (`timedatectl`'s `NTPSynchronized`, as the first builder asked), on the
   bus; reminders and the night wait for it; the voice says the clock is not
   set instead of a time.
4. **Smaller:** update's name lookup has no timeout; tasks says it is well
   while every task fails to be sent; `voice/link` is declared and never
   published.

## Step 2: the voice starts with no internet (`LAMP_START_OFFLINE`)

With `LAMP_START_OFFLINE=on` in a voice service's environment, a service
started with no internet no longer exits after the 25 s startup budget
(`INPUT_READINESS_TIMEOUT`) once its microphone is retained: it keeps
listening, says once on its log "listening with no link to the speech
service; it keeps trying" (trace record `waiting_for_link`), and becomes
ready (`listening_ready`) when the link comes up. The serving link
(`RecoveryPolicy::endless`) already redials its first connection like any
other outage, so nothing else waits. A service whose microphone, privacy or
capture never comes up still fails at 25 s, and so does every finite
(`directed`) session; with the switch off or anything but `on`, the budget
covers the link as before (`crates/voice/src/session_lifetime.rs`, tests
`serve_started_offline_*` and `started_offline_without_*`).

Kind: off unless set. Not yet shown on a host with its network cut or on a
lamp.

## Step 2: the board knows when its clock is set (`lamp-health --clock kernel`)

`lamp-health serve --clock kernel` reads `clock_synced` from the kernel
(`adjtimex`, as `timedatectl`'s `NTPSynchronized`), so it is known on a
board with no timesyncd; timesyncd's marker is still believed first. On
lamp-75c9 (2026-10-16) the clock is kept by chrony, there is no
`/run/systemd/timesync`, and `timedatectl` says `NTPSynchronized=yes`.
Kind: off unless set (`timesync`, the default, reads as before).

`lamp-schedule serve --kernel-clock` trusts the wall clock also when the
kernel says it is set, so a lamp can drop `--trust-clock` (both lamps' units
have it, and it trusts a clock never set): calendar reminders and new
requests then wait (`waiting_for_clock`, "trusted wall clock required")
until chrony sets the clock. One reader for both, `lamp_ipc::clock_synced`.
Kind: off unless set. The units are not changed here: swapping
`--trust-clock` for `--kernel-clock` is each lamp's change, after its
minute. Next: Expression's night and the voice's time.

## What steps 2 to 4 will build

- A part that watches reachability from the links' own state first (the
  voice's link on `diagnostics`, or `voice/link` once published), not a ping
  every second, and publishes `lamp/online` on the bus; and the clock's
  state beside it. Behind a switch first.
- The voice: start without the link (#1), say the recorded line and show
  the light (#2), say the clock is not set (#3).
- Schedule and Expression: wait for a set clock (#3).

## What was not shown

- Everything here was read from the code, not run. No part was run with its
  network cut yet, on a host or a lamp.
- The second voice was read from its branch's document only.
- Whether lamp-4ace's board reports `NTPSynchronized` through `timedatectl`
  (its units' comments say it does) was not checked on a lamp.
