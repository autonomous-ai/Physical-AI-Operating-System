# Attended 4ace pilot package

This package directly runs the existing Rust `lamp-live serve` on **lamp-4ace**.
Its source base is `55d7af98e`; its binary is
`738503a803d3718e52ca9e8886a669c0e41c007f7fe35873642afd4a83f5b8d0`.
Native05 passed: **16/16 owned replies over 1,295.074642 seconds**, clean Stop,
exit 0, verified stationary rollback, and all three Mac recordings valid. Results
were published in `6691132718b3bf3f1adc2a6c3ada26d08a027e6a`, at
`lampOS/docs/verification/native05-20261011/README.md`. Acoustic quality remains
unscored; that run did not qualify physical mute/reopen or all-day reliability.
A later [installed-service check](verification/installed-service-20261011/README.md)
completed two more exchanges, then verified all five observed processes gone,
empty cgroup/hardware owners, clean metadata finish and stationary restoration.
The 180-second room recording is valid; offline ASR corroborates both replies.
Physical mute/reopen and acoustic listening acceptance remain separate gates.
This is an attended desk pilot, not an unattended or boot-enabled release.

Remote staging and installation of the **stopped unit** are already authorized
and do not require onsite attendance. Starting needs exclusive device ownership
and the preflight below. Handoff to a facilitator additionally needs actual
physical mute/reopen qualification and desk audibility checks. Keep those stages
separate; installing this template neither starts it nor performs handover.

The package consists of [the unit](../deploy/4ace-attended/lamp-live-pilot@.service)
and [a private manifest template](../deploy/4ace-attended/manifest.template.json).
The template records evidence; the runtime does not enforce its fields. Unknown
pins or failed gates must not be filled with guessed values. No private Python
lab harness, parent-repository file, HAL bridge or background helper is shipped.

## Prerequisites and explicit boundaries

- Keep Lamp stationary. This command has no camera, ring or motor worker, and
  issues no serial, torque, home or park command. The serial namespace mask is
  only an additional access restriction; it does not stop an already moving
  joint or establish safe placement.
- Keep verified administrative access available for stop and manual restoration.
  An onsite facilitator is required for the attended user pilot, not for remote
  staging or installing the stopped unit. Do not start while another trial,
  recording controller, rollback timer or hardware owner is active. Retain
  same-boot legacy backups and the existing stationary guard.
  Do not stop HAL normally: its shutdown may park/release joints. Main must
  complete the separately reviewed stationary handover first. The unit has no
  `Conflicts`, legacy hooks, automatic restart or boot target.
- The release needs a compatible Linux ARM64 loader/C runtime, `libasound` and
  its configured plugins, `/usr/bin/systemctl`, normal DNS/network access to the
  approved provider, `/dev/urandom`, writable `/tmp`, and the qualified sound and
  GPIO devices. It does not need Cargo, Python, Go, model downloads, libclang or
  development headers at runtime. Verify the binary's actual dependency list;
  this package does not guess the installed library versions.
- Voice input is `plug:device_micro2` (Jieli), output `plug:device_speaker`
  (the qualified C-Media soft-volume chain). Do not swap routes, change capture
  gain/AGC, or infer real volume from a saved `.volume` file. Workers clear their
  environment, so a unit-level `ALSA_CONFIG_PATH` is not passed to them. Pin the
  existing **system** ALSA configuration and card identities.
- The privacy worker exclusively requests `/dev/gpiochip1`, line 9, pull-up,
  active-low. Do not run a competing GPIO reader. Root is retained as the
  qualified service UID; a dedicated user is a separate permissions change.

## Main's installation and preflight

Run these root commands on 4ace through the already-authorized administration
path. Main coordinates the device reservation; no additional onsite attendance is
needed to install the stopped unit. Installation can precede handover while
legacy remains running. Steps 2 and 3's handover evidence must be fresh before
**start**, not before copying the binary or unit. The commands below are manual
instructions, not steps executed or enforced by the package. Use a root-only
evidence directory and a new manifest copy. Do not print private
provider JSON, credentials, legacy unit environment or complete legacy config.
Commands using `timeout`, `sha256sum`, `readelf`, `jq`, `find`, `awk`, `curl`,
`nsenter`, ALSA utilities and `systemd-analyze` are **operator tools**, not Rust
worker dependencies. Missing tools are a preflight gap, not permission to skip a
check. Commands in this document assume Bash with `set -euo pipefail`.

1. Check `hostname` is exactly `lamp-4ace` and record
   `/proc/sys/kernel/random/boot_id` in the manifest. Stop on a boot change; do
   not reuse a previous snapshot's PIDs or restoration authority. Inventory the
   six legacy units (`hal`, `led-shutdown`, `led-boot`, `os-server`,
   `hermes-gateway`, `bootstrap`, all `.service`) and every known lab trial/timer.
   The Rust coexistence guard checks only the first four. Main must keep the
   updater and gateway unable to relaunch old owners throughout the pilot.
2. Obtain a fresh, private same-boot handover snapshot and reviewed restoration
   list. Preserve actual unit/drop-in bytes, original active states, config hash,
   same-boot muted sidecars and exact owned temporary stop-overlay hashes.
   Existing guard details are in the restoration section below. **There is no
   standalone automated cutover/rollback command in this package.** Do not start
   until Main has completed handover and approved that manual restoration path.
3. Record the actual board state before handover; these commands list settings,
   they do not start a recording or playback:

   ```sh
   cat /proc/asound/cards
   aplay -l
   arecord -l
   aplay -L
   arecord -L
   sha256sum /etc/asound.conf
   amixer -c device_micro2 contents
   amixer -c device_cmedia contents
   stat -Lc '%F %u:%g %a %t:%T' /dev/gpiochip1 /dev/ttyACM0
   readlink -f /sys/class/sound/card*/device
   ```

   Save outputs privately with hashes. Match card IDs to the actual USB devices
   and inspect `/etc/asound.conf` for both aliases and the complete soft-volume
   route. Card numbers may change; a successful open is not identity proof.
   Resolve any changed route, mixer, GPIO node or permission before starting.
4. Install the already-qualified binary from a trusted local staging path. Set
   `STAGED_BINARY` to that **explicit local path**, not a URL or shell download.

   ```sh
   SHA=738503a803d3718e52ca9e8886a669c0e41c007f7fe35873642afd4a83f5b8d0
   RELEASE=/opt/lampOS/releases/$SHA
   printf '%s  %s\n' "$SHA" "$STAGED_BINARY" | sha256sum --check --status
   test ! -e "$RELEASE"
   test ! -L "$RELEASE"
   install -d -m 0755 -o root -g root "$RELEASE"
   install -m 0755 -o root -g root "$STAGED_BINARY" "$RELEASE/lamp-live"
   printf '%s  %s\n' "$SHA" "$RELEASE/lamp-live" | sha256sum --check --status
   readelf -l "$RELEASE/lamp-live"
   readelf -d "$RELEASE/lamp-live"
   ldd "$RELEASE/lamp-live"
   ```

   All dependencies must resolve; retain this report. Do not execute an unknown
   binary through `ldd`. Existing release directories must be verified and
   reused explicitly, not overwritten by this procedure.
5. Provision `/etc/lampOS/pilot-4ace` as root:root 0700; put `provider.json` and
   `credential` there as separate, non-symlink, root:root 0600 regular files.
   Required JSON fields are `endpoint`, `model`, `voice`, `credential_file`;
   optional fields are `authentication`, `thinking_level`, `language_code`.
   Authentication is `api_key`, `bearer` or `ephemeral`. Preserve the qualified
   model/voice/settings; do not add an unsupported language field. The credential
   path must be `/etc/lampOS/pilot-4ace/credential` in this package. Copy values
   through the approved private provisioning process; never place a token in
   argv, a shared report or source. Record the new config hash after this path
   migration. Runtime checks reject unexpected fields, non-private/wrong-owner
   files, symlinks and files over 16,384/4,098 bytes respectively.

   Using the existing cloud authorization, the validator can also check setup:

   ```sh
   timeout 30s "$RELEASE/lamp-live" provider-check /etc/lampOS/pilot-4ace/provider.json
   ```

   It contacts the provider but opens no mic and sends no audio. A successful
   setup is not conversational readiness. Retain only its bounded setup receipt.
6. Create `/var/lib/lampOS/pilot-4ace/sessions` as root:root 0700. Keep the
   completed manifest root:root 0600 outside the session leaf. From the standalone
   lampOS source root, install and verify the template:

   ```sh
   install -d -m 0700 -o root -g root /var/lib/lampOS/pilot-4ace/sessions
   test ! -e /etc/systemd/system/lamp-live-pilot@.service
   test ! -L /etc/systemd/system/lamp-live-pilot@.service
   install -m 0644 -o root -g root deploy/4ace-attended/lamp-live-pilot@.service /etc/systemd/system/lamp-live-pilot@.service
   systemd-analyze verify /etc/systemd/system/lamp-live-pilot@.service
   systemctl daemon-reload
   sha256sum /etc/systemd/system/lamp-live-pilot@.service
   ```

   Check the effective instance with `systemctl show`: no additional drop-ins,
   `Slice=system.slice`, direct `ExecStart`,
   `Restart=no`, `KillMode=mixed`, SIGTERM then SIGKILL, five-second stop limit,
   root UID, 0077 umask, no ExecStop/ExecStopPost and the serial restriction.
   The five seconds are an outer service timeout, not measured acoustic stop
   latency. Native unit verification remains a start-preflight gate; physical
   mute/reopen remains a facilitator-handoff gate, not a stopped-installation
   prerequisite.

## Start and facilitate

For the physical-mute qualification and facilitator handoff, keep the switch
muted while starting and have the facilitator verify its actual position. A remote
start under the existing authorization does not establish that physical test.
Choose a fresh ID of 1–48 ASCII lowercase letters, digits or hyphens; never reuse
an output leaf:

```sh
PILOT_ID=20261012-attended-01
[[ "$PILOT_ID" =~ ^[a-z0-9][a-z0-9-]{0,47}$ ]]
UNIT=lamp-live-pilot@${PILOT_ID}.service
OUTPUT=/var/lib/lampOS/pilot-4ace/sessions/$PILOT_ID
test ! -e "$OUTPUT"
test ! -L "$OUTPUT"
systemctl start "$UNIT"
systemctl show "$UNIT" --property=ActiveState,MainPID,ControlGroup,Result
journalctl -u "$UNIT" -f -o cat
```

`systemctl start` returning does not mean ready. While muted, only the Rust GPIO
worker exists. Observe the current invocation's lifecycle, then let the person
unmute. The runtime requires stable fresh permission, a new provider session and
retained microphone data before printing `{"status":"listening_ready",...}`.
Do not reuse readiness from an old invocation or from before a provider fault.
There is no installed status UI; an absent cue listener is intentional.

Read back and compare both mixer snapshots after startup. No change is expected.
If they differ or replies are not clearly audible at the actual desk, mute and
stop; record the failure rather than silently changing gain during the pilot.
Do not claim acoustics from ALSA timestamps. Keep feedback on missed words,
cutoffs, relevance, pauses, false starts and perceived pace.

Physical mute revokes authority and closes the active runtime. Only verified
cleanup permits another privacy-only wait; unmute must create fresh owners and
readiness. Context continuity across mute is not promised. Before handing the
pilot to a facilitator, Main must verify this actual mute/reopen cycle and
main-PID stop. Neither was established by static package checks.

V1-baseline deliberately suppresses prospective input during observed own
playback plus its declared tail; this is not qualified natural interruption.
No wake-word, addressee recognition, software mute/volume API, tap-to-talk or
touch-button support is exposed here. Use physical mute and the service stop.
Rotating local metadata may contain transcripts; it is bounded best-effort
observation, not complete benchmark evidence. No raw PCM diagnostics are enabled.

## Stop and prove release before restoration

Save the instance's nonempty, specific cgroup path **before** stopping. When a
person is present, ask them to hold the physical switch muted. Remote shutdown
uses the same coordinator-first path; it does not prove a physical switch test.
Do not resume if privacy or cleanup is unknown. Use `systemctl stop`, whose
`KillMode=mixed` signals only the coordinator first. Do not send a process-group-
wide SIGTERM or call legacy HTTP endpoints to stop Rust. The coordinator revokes permissions and sends Stop to all workers
before its shared 300 ms cleanup grace. Any forced kill remains a runtime failure.

```sh
CG=$(systemctl show "$UNIT" --property=ControlGroup --value)
[[ "$CG" == /system.slice/lamp-live-pilot@*.service ]]
timeout 8s systemctl stop "$UNIT"
systemctl show "$UNIT" --property=ActiveState,SubState,MainPID,Result,ExecMainCode,ExecMainStatus
```

Require inactive, MainPID 0, successful result/exit, and no surviving cgroup
members. On cgroup v2, inspect every descendant `cgroup.procs`; an absent saved
cgroup after verified exit is also empty. Capture the commands/output privately:

```sh
if test -d "/sys/fs/cgroup$CG"; then
  find "/sys/fs/cgroup$CG" -name cgroup.procs -exec cat '{}' +
fi
```

No PID may remain. Also inspect hardware ownership, including GPIO anonymous
line descriptors; PCM control-only descriptors are not audio stream owners:

```sh
find /proc/[0-9]*/fd -maxdepth 1 -type l -printf '%p %l\n' |
  awk '$2 ~ /^\/dev\/snd\/pcm/ || $2 ~ /^\/dev\/gpiochip/ || $2 ~ /^anon_inode:\[gpio/ || $2 == "/dev/spidev3.0" || $2 == "/dev/ttyACM0"'
```

Require no relevant surviving owner before HAL starts. A permission error or
incomplete/racing inventory is not an empty result. Resolve it before proceeding.
Retain `service_metadata_finished` separately from the process result; lost
metadata does not justify claiming a complete record. If stop times out, cleanup
fails, the cgroup is nonempty or ownership is unknown, **do not restart Rust or
legacy HAL**. Leave the switch muted and call Main. Do not erase failed evidence.

## Manual stationary legacy restoration

This is an operator fallback to the previously installed system, not a runtime
dependency or an automated command supplied here. Main must have retained and
verified the same-boot snapshot, backup and original active-unit list. Do not
guess that all six services were active. Keep any old rollback timer stopped
while this new unit owns devices; those timers know different trial unit names
and must never restore HAL behind this pilot.

The inspected 4ace guard is exactly:

```ini
# /etc/systemd/system/hal.service.d/99-lampos-4ace-stationary.conf
[Service]
InaccessiblePaths=/dev/ttyACM0
EnvironmentFile=/var/lib/lampos-4ace-audio-onsite-20261011/stationary.env
```

Its root-owned environment file contains only:

```text
HAL_SERVO_PORT=/var/lib/lampos-4ace-audio-onsite-20261011/no-servo-device
```

Preserve both existing files; never remove/rewrite them to make startup succeed.
Check their SHA-256 against the fresh manifest, compare the current boot with
the snapshot, and require the disabled path to remain absent. Verify effective
`systemctl show hal.service --property=InaccessiblePaths,EnvironmentFiles`:
the serial path is inaccessible and the exact environment file is the final
required file (`ignore_errors=no`). Do not print other environment values.

If temporary stop overrides remain from Main's handover, remove **only** each
root-owned regular file whose exact path and SHA match the fresh manifest.
The original helper used `/run/systemd/system/UNIT.d/99-lampos-4ace-stop.conf`;
later cohorts use distinct names. A filename is not ownership proof. Never use
a wildcard `rm`, `systemctl revert` or `unmask` to erase unrelated state. The
override clears ExecStop/Post and uses SIGKILL to bypass V1 park/release; do not
reintroduce ordinary HAL stop during this step. Reload only after those exact
approved removals, then **start**, never restart, each saved originally-active
unit in the saved order:

```sh
systemctl daemon-reload
# Only when the snapshot says HAL was active and all release checks above passed:
timeout 12s systemctl start hal.service
HAL_PID=$(systemctl show hal.service --property=MainPID --value)
[[ "$HAL_PID" =~ ^[1-9][0-9]*$ ]]
test "$HAL_PID" -gt 1
nsenter --target "$HAL_PID" --mount -- stat -Lc '%t:%T %s' /dev/ttyACM0
```

The namespace result must be `0:0 0`; no process may own the real serial node.
Save the HAL PID/start identity and ensure it is unchanged during verification.
The old configuration hash must still match; never overwrite a newer config to
force rollback. Check `hal-mic-state.json` and `hal-speaker-state.json` under
the snapshotted HAL state directory (qualified `/tmp` on 4ace). Each must be a
root-owned, non-symlink regular file of at most 4,096 bytes with **`"muted": true`**
and **`"boot_id"`** exactly equal to the saved snapshot's `boot_id`, which must
still equal `/proc/sys/kernel/random/boot_id`. The key is `boot_id`, not a generic
boot timestamp. Pinned V1 `d5efe9d7b73cc529b34cd4abe97624682a82ca94` defines it
in `hal/app_state.py:393` (`_save_boot_sidecar`), and the reviewed restore helper's
`muted_sidecars` checks the same key. Malformed, missing, wrong-boot or unmuted
sidecars fail verification; never rewrite them to fabricate restoration.

HAL's initial physical-switch reconciliation can unmute software despite a saved
muted sidecar. This was an observed V1 restoration defect. If a person is present,
keep the switch muted and verify that actual position. Record initial responses
and verify the direct HAL endpoints with bounded **GET** requests:

```sh
curl --fail --silent --show-error --connect-timeout 1 --max-time 1 http://127.0.0.1:5001/voice/status
curl --fail --silent --show-error --connect-timeout 1 --max-time 1 http://127.0.0.1:5001/audio/status
```

Allow at most 20 seconds from HAL start for connection-refused/502/503/504
startup availability, with each request bounded to one second. Do not retry an
explicit bad/unknown privacy response as if it were successful. These are direct
HAL JSON objects: `/voice/status` supplies `mic_muted` and
`hw_mic_switch_muted`; `/audio/status` supplies `speaker_muted`. Do not unwrap a
Go API `data` field or accept strings/numbers as booleans.

The reviewed helper allows only this disclosed repair after a newly restored,
still-identical HAL: fresh status reads show **`mic_muted=false`,
`hw_mic_switch_muted=false`, and `speaker_muted=true`** exactly. Preserve the
failed receipt first, verify the same PID/start identity, then issue **one**
`POST /voice/mute` with JSON `{}` and `Content-Type: application/json`:

```sh
curl --fail --silent --show-error --connect-timeout 1 --max-time 1 -X POST -H 'Content-Type: application/json' --data '{}' http://127.0.0.1:5001/voice/mute
```

This is an operator instruction, not an automatic restoration action in this
package. Other combinations, including an unmuted speaker or unknown hardware
state, do not qualify for that narrow repair. Do not open a physical mute switch
to satisfy it. Require the returned `status` to be `ok` or `already_muted`, then
recheck unchanged HAL identity, both GET endpoints and both sidecars. Final
`mic_muted` and `speaker_muted` must be exactly true; `hw_mic_switch_muted` must
be a known boolean. The reviewed helper accepts either hardware boolean and
never overwrites a newer physical mute. If someone is holding mute, additionally
confirm that the reported value is true and matches the actual position.

Retain `startup_mute_preserved=false` and `continuous_privacy_verified=false`
when repair was needed; later readback does not erase the startup gap. Never call
an unmute endpoint automatically. Unknown state, changed identity or failed
readback means **restoration failed**. This contract was checked against the
reviewed `rollback.py::wait_for_muted_privacy`, `restore_bench_privacy` and
`muted_sidecars`, file SHA-256
`307579e16050590fa56e2407ed6c131e42a723360804adda14f2d08d6f2a5e9c`.
V1 source also confirms `hal/routes/voice.py:501,600` and
`hal/routes/music.py:288` at the pinned commit above. The helper is provenance,
not a packaged dependency or an automatically invoked rollback.

Only after stationary HAL/privacy verification may Main start the other saved
originally-active units, confirming each result and retaining the guard. Do not
restart an updater until Main approves its interaction with the retained guard.
Keep Rust stopped. The manual recovery is not complete until ownership, privacy,
unchanged configuration and all approved restored-unit states have receipts.

## Reuse and verification limits

This package reuses Rust's `serve`, private-file checks, physical privacy gate,
bounded worker cleanup and existing systemd mixed-kill pattern. It preserves the
V1 operational lesson that normal HAL shutdown invokes hardware lifecycle hooks:
the inspected guard/rollback contract deliberately prevents those hooks during
handover. No V1 prompt, audio algorithm, mixer mapping or motor limit is changed.
Relevant Rust source: `main.rs:50`, `coordinator.rs:672,829,1474,1734,1865,2222`,
`process.rs:201`, `config.rs:40`, `physical_privacy.rs:15` and
`service_privacy.rs:64`, all under `crates/live/src/`.

Host static checks can validate unit/manifest/argv consistency and documentation
shell syntax. They do not qualify systemd on 4ace, device ownership, placement,
GPIO response, mute/reopen, actual current mixer state or desk audibility.
Native05's passing sustained-conversation result is separate evidence on the
pinned binary, not validation of this new systemd unit or physical mute/reopen.
Main records installation, start preflight and facilitator handoff separately;
there is no manifest enforcement or automatic restoration here.
