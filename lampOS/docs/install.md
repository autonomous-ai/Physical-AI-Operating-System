# Installing lampOS on a new lamp

Task [#811](https://github.com/autonomous-ai/lampOS/issues/811) (the ninth
builder), for shipping to homes on 27 November (Queue #705). Today every part
reaches a lamp by hand: the first builder's scripts and unit files
(`deploy/lamp-4ace/`, fourteen `*_deploy.sh` kept off the repository), or the
fifth's on lamp-52e6 (`deploy/lamp-52e6/`). A lamp in a home needs one
install that puts every part, its unit and its settings on a blank lamp from
a signed build, and keeps them current.

This page is step 1 of the task: what a lamp runs today, part by part, and
which of it is the product and which is one lamp's own. The product units
(`deploy/product/`) and the install command come after it and are written
against it.

**Kind:** a document. Nothing on any lamp changes.

## What exists and what we take

Read on 2026-10-15 at `lamp-voice` `462d04cee`.

- **lamp-4ace's units** (`deploy/lamp-4ace/`, 15 units and a timer, the
  controls' and Expression's settings, `RUNBOOK.md`). Every part is its own
  unit under `multi-user.target`, run as the board's user `orangepi` (the
  voice as root), with no `lampos.target`, no slices and no keeper
  (`LAMP.md`: "each its own systemd unit, no keeper yet"). Taken: every
  option and switch as the product's starting values, below. Not taken: the
  host name in each unit, the board user, settings kept inside a build's
  folder.
- **lamp-52e6's units** (`deploy/lamp-52e6/`, 12 units and its own
  `lampos.target`), the fifth's. Closer to the templates: `PartOf=` and
  `WantedBy=lampos.target` on every unit, the body `Type=notify` with
  `WatchdogSec=5`. Taken as the second lamp to compare with: what two lamps
  share is the product, what differs is either one lamp's own or one lamp
  being behind (below).
- **The templates** (`deploy/units/`, 12 units, the target and two slices):
  `@NAME@` placeholders filled by hand for each lamp
  ([README](../deploy/units/README.md)). Taken: the target, the slices, the
  order and the limits ([keeper.md](keeper.md)). Not taken: the
  placeholders. They mix three things: the build's hash (which an update
  must change), a lamp's measurements (which only that lamp has), and values
  every lamp shares; and `@VOICE_OPTIONS@` holds the voice's options
  whole, though the two lamps' are the same.
- **The updater** (`lamp-update`, `crates/update`, [updates.md](updates.md),
  #737/#741). It installs a signed build's binaries part by part into
  `/opt/lampOS/<folder>/<sha256>/` and switches a part by writing the new
  hash into its unit (`crates/update/src/unit.rs`, `with_build`). It knows a
  part only by a unit in `/etc/systemd/system` whose `ExecStart=` names such
  a folder, never adds a part, never enables a unit, and carries files
  beside a binary from the old folder to the new (the controls' settings).
  Taken: the signed manifest, the install beside, the quiet moment, the
  watch and the way back, all as they are. To change: what a switch writes
  (below, "One question for the updater").
- **The keeper's package** (`crates/keeper/src/package.rs`, keeper.md, "One
  service for the lamp"). The old runtime's updater installs one package,
  `/opt/lampOS/lamp-os/<version>/` with `bin/service` (the keeper),
  `VERSION` and `units/`; the keeper links every unit in `units/`, enables
  each part, ties `lampos.target` to its own unit and starts it. Shown on
  lamp-52e6 on 2026-10-14. Taken: linking the release's units, the bus
  root made as lamp-4ace has it, the old runtime's units masked. Open: this
  is a second way for software to reach a lamp, beside `lamp-update`
  (below, "Two ways in").
- **Setup** (#806, the third; `docs/setup.md` on `third/setup`). V2 ships
  with V1's setup flow. Setup writes the Wi-Fi, the voice's credential file
  and the person's name and time zone, and a marker; nothing of lampOS
  listens before it. Taken: install writes none of what setup writes, and
  starts nothing before the marker. The update key (`/etc/lampOS/update/key.pub`)
  comes with the image, not from setup (the third's proposal on #806).

## The parts

Every binary with a unit on a lamp today, and the three outside the lamp
itself. "User" is lamp-4ace's; the template's is in brackets where it
differs. Folders are what the unit makes or names.

| Part | Binary | User | Folders it keeps | Settings it reads | Sockets and files it makes |
|---|---|---|---|---|---|
| voice | `lamp-voice serve` | root | `/var/lib/lampOS/voice/` (sessions, first output, reminder delivery); `/run/lampos-voice/` (0700, kept across restarts) | `/etc/lampOS/voice/provider.json` and the credential it names; the character file | the thread `/run/lampos-voice/thread.json` |
| body | `lamp-body` | orangepi, `dialout` | `/run/lampos-body/` (0755); the servo lock folder | none: its poses are options | `mood.sock`, `gaze.sock` |
| eyes | `lamp-eyes` | orangepi, `video` | `/var/lib/lampos-eyes/` (0700, `people.json`); `/run/lampos-eyes/` (0755); the camera lock folder | two models, `/opt/lampOS/eyes/*.onnx`, outside the build's folder | `permit.sock`, `who.sock`, `present`, `known`, `sight.jpg` |
| name | `lamp-name` | orangepi | none | models, `/opt/lampOS/name/models`, outside the build's folder | the bus |
| expression | `lamp-expression serve` | orangepi | none | a catalogue, `/opt/lampOS/expression/catalogues/*.json` | the bus |
| actions | `lamp-actions` | orangepi, `audio` | `/run/lampos-actions/` (0755) | none | its folder; the bus |
| memory | `lamp-memory` | orangepi (`@MEMORY_USER@`: its own) | `/var/lib/lampos-memory/` (0700); `/run/lampos-memory/` | none | `memory.sock` |
| policy | `lamp-policy` | orangepi (template: `DynamicUser=yes`) | `/var/lib/lampos-policy/` (0700) | the judge's key, `/etc/lampOS/policy/jev.key`, as a systemd credential | the bus |
| health | `lamp-health serve` | orangepi | none | none: what it expects is options | the bus |
| controls | `lamp-controls serve` | orangepi | none | `controls.json` and `controls-pins.json`, inside the build's folder | the bus |
| ears | `lamp-ears` | orangepi, `audio` | none | none | the bus |
| schedule | `lamp-schedule serve` | orangepi | `/var/lib/lampos-schedule/` (0700); `/run/lampos-schedule/` (0700) | none | `call.sock` |
| air | `lamp-air` | orangepi, `/dev/i2c-0` | none | none | the bus |
| tasks | `lamp-tasks serve` | orangepi | `/var/lib/lampos-tasks/` (0700); `/run/lampos-tasks/` (0700) | `targets.json` in its state folder; the Harness program in the user's home | `request.sock`, `voice.sock` |
| records | `lamp-records check` (a timer, hourly) | root | `/run/lamp-records/` | `/etc/lampOS/records.json` (it does nothing without it) | `last.json` |
| update | `lamp-update serve` | root | `/var/lib/lampos-update/` (0700) | `/etc/lampOS/update/key.pub` | none |
| keeper | `lamp-keeper serve` | root | its service data folder | none | `lampos.target.d/keeper.conf`; the bus root |
| surround | `lamp-surround serve` | `@SURROUND_USER@` | not read for this page | `/etc/lampos/surround.json` (lower case, unlike every other part) | not read |

The bus root, `/run/lampos-bus`, is made by the keeper (root, the parts'
group, mode 2770) or by whichever part comes first; every part above but
update, records and surround publishes or reads there.

Binaries the build carries with no unit, which an install leaves alone:
`lamp-turn-end` (a worker the voice starts from its own folder), `lamp-bus`
(a tool), `lamp-audio`, `lamp-observer`, `lamp-voice-eval`,
`lamp-sound-fixtures` ([deploy/units/README.md](../deploy/units/README.md)).

What else a part needs that no build carries: the voice's turn-end model
and ONNX Runtime library (beside the voice in its folder,
[endpoint-detection.md](endpoint-detection.md)), the eyes' two ONNX models,
the name's models, Expression's catalogue, the controls' two files, the
character file, `provider.json`, `records.json`. lamp-update's manifest
carries binaries only ([updates.md](updates.md), "Not shown": "models and
libraries a build does not carry, configuration under `/etc/lampOS`").

## Order at boot

From [keeper.md](keeper.md) ("The parts and their order"), which names
eleven parts; the four it does not name are placed by what they serve
(proposed here, for the fifth to correct):

| Step | Parts | Why |
|---|---|---|
| 1 | memory, actions, health; **schedule, tasks** | The voice calls each of their sockets from its first turn (`LAMP_SCHEDULE_SOCKET`, `LAMP_TASKS_DIR`), as it does memory's |
| 2 | voice | |
| 3 | body, name, controls, policy; **ears, air** | Ears and air only publish on the bus, as name does |
| 4 | eyes, expression | |

Outside `lampos.target`, each started by `multi-user.target` on its own:
the keeper (which starts the target), update, surround, and the records
timer. Stop is the reverse.

Only a `Type=notify` part makes an `After=` wait. Today that is the body on
lamp-52e6 and in the template; every lamp-4ace part is `Type=exec` or
`simple`, so its order is only an order of forks.

## Product, or one lamp's own

Compared line by line: lamp-4ace's units against lamp-52e6's, with every
build hash taken out. Same board (Orange Pi 4 Pro), two lamps.

**The same on both: the product.**

- The voice's options: `--noise-suppression off --turn-taking duplex
  --faint-floor-db -28 --microphone mic2 --reference-clock capture
  --reference-recovery off --provider-continuation kept-start
  --playout-start-ms 350 --ring-channel-ceiling 24 --end-silence-ms 600
  --controls-stop --endpoint smart-turn-fast`. The template's
  `@VOICE_OPTIONS@` calls them "the measured options of this lamp"; on two
  lamps they are one set.
- Every option of name, ears, air, actions, memory, schedule and controls
  (`lamp-name --microphone mic2 --least 0.9`, `lamp-ears --device
  plug:device_micro1 --rise-db 30`, `lamp-air --bus-number 0`,
  `--mixer-card allwinnerhdmi --mixer-control Speaker --volume-ceiling 100`,
  `--keep-days 30 --memory-days`, …), and `CPUAffinity=6 7` (the board's
  two fast cores; keeper.md).
- The body's speeds and limits: `--reach 150,15,1,250 --speed 120
  --servo-speed 1000 --servo-acceleration 60 --gaze-reach 480,120
  --stand-share 100 --stand-seconds 6 --awake-seconds 90 --idle on
  --force-limit 600 --wake on --rest-glance on --look-at-name on`.
- The controls' wiring, `controls-pins.json`: a property of the board.

**Different: one lamp's own.** Only these, and each is measured on its lamp
([body.md](body.md)):

| What | lamp-4ace | lamp-52e6 |
|---|---|---|
| Body `--gaze` | `727,553` | `782,557` |
| Body `--stand` | `1772,2043,2015,2657,2602` | `1772,2035,2035,2657,2602` |
| Body `--folded-arm` | `2048,2047` | `2053,2069` |
| Body `--home` | `1950,2043,2015` | `1771,2035,2035` |
| Body `--pitch-window`, `--torque on` | not given | `1670,1995`, `on` |

The camera's USB path, `--usb 01da:5875@3-1.1.4`, is the same on both: a
property of the board and its cable, so the product's, as long as every
lamp's camera goes in the same port.

**Different: one lamp behind the other.** lamp-4ace has switches lamp-52e6
lacks: the voice's `LAMP_TASKS_DIR`, `LAMP_SCHEDULE_TOOLS`,
`LAMP_SCHEDULE_SPEAK`, `LAMP_SCHEDULE_DELIVERY_DIR`, `LAMP_MEMORY_DAYS`,
`LAMP_EYES_WHO_SOCKET`, `LAMP_SPEAK_FIRST`; the actions'
`LAMP_SCHEDULE_TOOLS`; health's `--expect policy`. By joining.md's rule
("The switch is for that first minute, not for life") a switch on for
lamp-4ace becomes the product's default, so the product takes lamp-4ace's
set. Expression's catalogue differs too (`expression-alive-name-speech` on
lamp-4ace, `expression-alive` on lamp-52e6): the product takes lamp-4ace's.

**Neither: the person's.** The time zone: `--utc-offset-minutes 420` in
memory and Expression, `--zone Asia/Ho_Chi_Minh` in schedule. Setup writes
the system's zone (#806); a fixed offset is wrong across daylight saving,
as the third found. Until memory and Expression read a zone name, the
install writes the offset from the system's zone, and the product unit
does not hold it.

**Paths and users that are lamp-4ace's by history, not by need:** the board
user `orangepi` and things in its home (the servo and camera lock folders
under `~/.local/share/lampOS-voice/`, the Harness program in
`~/.local/bin`); the voice under `/opt/lampOS/releases/` where every other
part is under `/opt/lampOS/<part>/`; `ConditionHost=` in every unit.

## The product layout (proposed)

What steps 2 and 3 build, unless an answer on #811 changes it.

- **One user**, `lampos`, a system user with no login, in `audio`, `video`,
  `dialout` and the group that may read the GPIO chips and `/dev/i2c-0`;
  memory its own (`lampos-memory`) as the template says; policy
  `DynamicUser=yes`; the voice, update, records and the keeper root, as now.
  `lampos` is then the parts' group the keeper gives the bus root.
- **Builds** under `/opt/lampOS/<part>/<sha256>/` as now, and the voice
  under `/opt/lampOS/voice/`. Each part's unit names
  `/opt/lampOS/<part>/current/lamp-<part>`, `current` being a link to the
  build's folder, so an update changes the link and not the unit.
- **What a build does not carry**, never inside a build's folder: models in
  `/opt/lampOS/<part>/models/` (the eyes' two, the name's), beside
  `current`; the product's settings in `/etc/lampOS/<part>/` (decided by the
  first builder on #811, 2026-10-15 17:37): Expression's catalogue as
  `/etc/lampOS/expression/expression.json`, the controls' two files in
  `/etc/lampOS/controls/`. `/etc/lampOS` is made 0755 and each of these
  folders 0755, files 0644: readable by `lampos`. lamp-4ace's `/etc/lampOS`
  is root's alone; `install` does not open one made closed (credentials may
  be below it) and says so. The voice's turn-end model and ONNX
  Runtime library stay beside the voice, where the voice looks for them,
  and lamp-update carries them from build to build as it does now.
- **A lamp's own values** in one file per part, `/etc/lampOS/<part>/lamp.env`,
  read with `EnvironmentFile=` and put into `ExecStart=` as `$NAME`
  (systemd splits an unquoted `$NAME` into words): today only the body's,
  `BODY_POSES`. The file is required: without it the body does not start,
  so a body never moves on poses measured on another lamp. The product unit
  never changes for one lamp; a lamp's file is written once, when the lamp
  is measured, and no update touches it.
- **The zone**, `/etc/lampOS/zone.env`: `LAMP_ZONE` (schedule) and
  `LAMP_UTC_OFFSET_MINUTES` (memory, Expression), written by the install
  from the system's zone and again by setup.
- **Locks and the bus**, `/etc/tmpfiles.d/lampos.conf`: `/run/lampos-bus`
  as the keeper makes it, and `/run/lampos-locks/{motor,camera}` in place
  of lamp-4ace's lock folders under the board user's home.
- **The body's unit id** is the host name (`--unit-id %H`), and no unit
  says `ConditionHost=`.
- **Switches** are in the product unit as `Environment=` lines, on; a lamp
  that must have one off says so in a drop-in
  (`/etc/systemd/system/lamp-<part>.service.d/`), which no update touches.
- **The person's settings** (zone, name, Wi-Fi, credential) are setup's, in
  setup's files.
- **Nothing starts before setup:** every part in `lampos.target`, and the
  target itself, has `ConditionPathExists=/var/lib/lampos-setup/done`,
  setup's marker (the path agreed with the third on #811, 2026-10-15
  17:07). The condition is on each part because systemd checks a unit's
  condition only when its own job runs, after the jobs it pulls in are
  queued: the target's alone held back none of its parts (the reviewer on
  #815, from systemd 252's `systemd.unit(5)`, "Conditions and Asserts").
  `lamp-update.service` and `lamp-records.timer` are outside the target
  and listen to nothing; they run before setup as after it. The install
  enables the target and the parts and starts none of them. On a lamp brought up by
  the keeper, the keeper's `--setup-marker` (#816, the fifth's) starts only
  `lamp-setup.service` until the marker is there; the target's condition
  keeps the same rule on a lamp without the keeper. `lamp-setup.service`
  joins `deploy/product/` when the third hands back its unit (#806).

## The updater and the link

Decided on #811 (the first builder 17:37, the second 17:46): a round knows
a product part by `current/lamp-<name>` and switches it by replacing the
link, keeps the old build in its journal and `previous/`, and puts the link
back to go back; a part that names its build in its unit (lamp-4ace,
lamp-52e6) is switched as before, both in one round
([updates.md](updates.md), "One round").

## Two ways in

A lamp can get lampOS as the keeper's package through the old runtime's
updater (lamp-52e6, 2026-10-14), or part by part through `lamp-update`
(lamp-4ace, build 188, 2026-10-15). For a lamp in a home there should be
one. Which one is the first builder's and the owner's call; the install
command is written for `lamp-update`'s layout, since signed updates are on
the ship list (#737). It does not install the keeper's unit or setup's yet
(below, "What the image must hold").

## What the image must hold

`install` puts lampOS on a lamp; it does not make the image. It expects to
find, from the third's list on #811 (2026-10-15 17:07) and from what it
reads itself:

- Debian bookworm (arm64) with systemd, `useradd`, `systemd-tmpfiles` and
  `date`; the groups `audio`, `video`, `dialout` (and `i2c`, `gpio` if the
  board has them).
- The update trust root, `/etc/lampOS/update/key.pub`, put there at the
  factory. Setup does not write it.
- The zone data in `/usr/share/zoneinfo`, and `/etc/timezone` or
  `/etc/localtime`.
- For setup (#806): nginx installed and not enabled, with the setup site;
  V1's built setup page in `/usr/share/nginx/html/setup`; V1's radio scripts
  `device-ap-mode` and `device-sta-mode` with hostapd and dnsmasq, as
  `scripts/imager/build-orangepi.sh` writes them; no
  `/var/lib/lampos-setup/done`.
- None of the old runtime's units running (`hal`, `os-server`,
  `led-boot`, `led-shutdown`): the voice refuses to start beside them
  (keeper.md).
- **Setup's unit, `lamp-setup.service`, and what starts it while the
  marker is missing** (the keeper's `--setup-marker`, #816 and #817).
  `deploy/product/` carries neither yet (#806 hands back setup's unit), and
  setup's unit has no `[Install]` of its own. Until they are added here, a
  blank lamp with only this install never enters setup, and so never
  starts the lamp: the parts wait for setup's marker (the reviewer on
  #815).

## The install command

```text
lamp-update install --channel-url URL [--channel NAME] [--key-file PATH]
```

As root, on a lamp with the base image (`crates/update/src/install.rs`):

1. Reads the channel's newest build and checks it with the image's key
   (`/etc/lampOS/update/key.pub` unless `--key-file` says otherwise), as a
   round does. No key, no install.
2. Makes the users `lampos` (in `audio`, `video`, `dialout`, `i2c` and
   `gpio`, each only if the image has that group, and says which it has
   not) and `lampos-memory` (in `lampos`), unless they are there.
3. Checks that the build carries a binary for every product part, and the
   voice's `lamp-turn-end`, before writing anything.
4. Fills each part's folder, `/opt/lampOS/<part>/<sha256>/`, under a
   temporary name, each file hashed as it comes and compared with the
   manifest; renamed only when whole. A folder already there and holding
   exactly the build's files is kept. Only when every folder is whole does
   any `current` link move, each replaced whole by a new link renamed over
   it.
5. Writes each product unit and `lampos.conf`, each only if it is not
   already exactly that. The product settings (Expression's catalogue, the
   controls' two files) are written only where one is missing or is still
   exactly what an install wrote before, by the SHA-256 kept for it in
   `state.json`, so a newer build's default reaches a lamp that never
   changed it. A setting changed on the lamp since, or there before any
   install (lamp-4ace's own pins), is the lamp's: it is kept, and said
   (`kept /etc/lampOS/controls/controls-pins.json: …`). Rerunning install is
   how the updater upgrades itself, so this holds at every run (the
   reviewer on #815). What it wrote is recorded in `state.json` at once,
   even when a later setting fails.
6. Writes `/etc/lampOS/zone.env` from the system's zone if it is not there.
   It never writes `/etc/lampOS/body/lamp.env`, and says that the body waits
   for it.
7. Records the build, the folders it made and, if anything changed, that
   systemd is owed (`systemd_owed`) in `/var/lib/lampos-update/state.json`,
   as a round does, under the same lock: all of it before systemd is asked
   anything.
8. If systemd is owed: `systemd-tmpfiles --create`, `systemctl
   daemon-reload`, and `systemctl enable` of every part, `lampos.target`,
   `lamp-update.service` and `lamp-records.timer`; then records the debt
   paid. It starts nothing and never writes setup's marker. A run whose
   enable fails leaves the debt, and the next run pays it though it writes
   nothing (the reviewer on #815, 19:17).

Run again with nothing new and nothing owed, it writes nothing and asks
nothing of systemd.
With a newer build, it installs the new folders beside the old and moves
the links; the old folders stay. It does not restart a running part:
switching a running lamp at a quiet moment is a round's work.

The product units and settings are carried inside the program
(`include_str!` of `deploy/product/`), so they are those of the commit the
program was built from; the binaries are the channel's newest build.

## What was not shown

- `install` was run only in tests, on a lamp made of files with a channel
  and a machine of the test's own (`crates/update/tests/install.rs`): a
  blank lamp gets every part, link, unit and setting, and nothing is
  started; a second run changes nothing, byte for byte, and asks nothing of
  the machine; a file that does not match its SHA-256 leaves no link and no
  unit; a build without a part is refused before anything is written; a
  newer build moves each link and keeps the old folders. `useradd`,
  `systemctl` and `systemd-tmpfiles` themselves were not run.
- No build carries the voice's turn-end model and ONNX Runtime library, the
  eyes' two models or the name's models, and `install` does not place them.
  A lamp installed by it has a voice without its endpoint model and eyes
  and name that cannot start.
- Nothing here was run on a lamp. The units were read from the repository,
  not from the lamps, and lamp-4ace's `LAMP.md` and its unit files already
  disagree on builds (updates.md, "Most likely wrong").
- Two lamps, one board. A third lamp may show another line as its own.
- lamp-4ace's `lamp-cmedia-speaker.service` (the speaker's level, from V1)
  is named by its voice unit and is not in the repository.
- Which group may read the GPIO chips and the I²C bus on the base image was
  not checked.
- `lamp-surround` was not read beyond its unit, and `lamp-records` has two
  unit texts (lamp-4ace's names `/opt/lampOS/records/`, the crate's
  `/opt/lampOS/bin/`), while `deploy/units/README.md` says it has no unit on
  any lamp.
