# Lamp hardware test (bench / factory)

Factory / bench hardware test for the Orange Pi 4 Pro (Allwinner A733) lamp.
One command on the Mac tests a lamp through its setup hotspot and saves a Markdown
report to `~/lamp-reports/lamp-xxxx-YYYYMMDD-HHMMSS.md`.

```
Mac (USB-LAN en6, 192.168.1.233) ==cable==> hub opidev1 end0 (192.168.1.10)
                                              hub wlan0 --> lamp-xxxx hotspot --> lamp 192.168.100.1
```

The hub's own Wi-Fi is borrowed to join each lamp's hotspot, so reach the hub over the
cable (tailscale/LAN to the hub drop while a test runs).

## Run a test (on the Mac)

```bash
lamp-test                      # find a lamp-* hotspot (1 = auto; several = highlighted list, 10 s countdown, n/2 clicks = next, Enter/1 click = take)
lamp-test 'lamp-0c*'           # wildcard (quote it)
lamp-test lamp-b6c9            # exact
lamp-test --all                # every lamp in range, one report each
lamp-test --scan               # list hotspots only
lamp-test -- --skip servo      # args after -- go to lamp_fulltest.py
lamp-test -- --only led        # LED colour test (off by default)
lamp-test tune                 # MPR121 factory calibration (see below), in the hub's tmux "lamprun"
lamp-test hal-bundle [repo]    # build + copy HAL/web from the lampOS checkout to the hub for tune to deploy
lamp-test dash                 # live view of the hub (tmux "dash" tails ~/hwtest/fleet/live.log; Ctrl-b d to leave)
```

Every run on the hub is also written to `~/hwtest/fleet/live.log`; the `lamp-dash` service keeps a
tmux session `dash` following it from boot, so a run started any way (Mac, or later a GPIO button
on the hub) shows up there.

### Hub push button (no Mac needed)

Button between header **pin 40** (PL13) and **pin 39** (GND) on the hub, internal pull-up.

| When | 1 click | 2 clicks (< 0.6 s) |
|---|---|---|
| idle | test, skipping servo | full test incl. servo |  — **3 clicks: MPR121 factory calibration** |
| MPR121 calibration "Save ...? [Y/n]" | redo | save |
| choosing between several lamps (10 s countdown) | take the highlighted lamp | next lamp (countdown restarts) |
| a Y/n question on screen | No | Yes |
| `hal.calibrate` running (old calibration is overwritten automatically) | ENTER (arm in the middle / stop recording) | ENTER |
| test running (any step) | ignored | ignored |  — **3 clicks: skip the current step** (also `s` + Enter in the terminal) |
| test running, no question | ignored | ignored |

Button runs live in tmux session `lamprun` and show on the LCD like any run. Reports and the
camera frame wait in `~/hwtest/fleet/outbox/`; `lamp-test fetch` on the Mac collects them.

### MPR121 factory calibration (`lamp/mpr121_tune.py`)

Phantom touches come from pads whose idle noise/drift reaches the touch threshold. Each lamp
gets its own per-pad thresholds, saved on the lamp in `/var/lib/hal/mpr121.factory.json`.
HAL (lampOS PR #693 and later) merges that file over the device package and keeps it across
updates. The owner can adjust it later in the web UI (Settings → Touch pad), and "Reset to factory"
goes back to this file.

1. On the Mac: `lamp-test hal-bundle` builds the web UI and copies HAL from
   this lampOS checkout (or `$LAMPOS_REPO`) to the hub (`~/hwtest/hal-bundle`). Do it again after
   pulling new HAL code.
2. Start a lamp: **3 clicks** on the hub button, or `lamp-test tune` on the Mac. The lamp picker
   lists lamps not yet calibrated first.
3. The hub joins the lamp's hotspot. If the lamp doesn't run the bundle yet, it installs the bundle:
   - `/opt/hal` is backed up to `/opt/hal.prev` and the web UI to `setup.prev`;
   - hal is restarted;
   - if hal doesn't answer within 150 s, everything is rolled back automatically.

   New Python packages can't be installed offline, so a bundle needing them rolls back. The tool
   then calibrates in direct I2C mode; the saved file applies once the lamp updates.
4. `mpr121_tune.py` on the lamp. **hal is stopped while it runs**, so touches trigger nothing on the lamp; hal is started again at the end and picks up the new file:
   1. **hands off**: 10 s idle noise per pad;
   2. **suggested thresholds**: touch = max(6, 2 × noise + 2), release about half;
   3. **slide over all 12 pads**: each must register, and weak pads are flagged;
   4. **Save? [Y/n]**: `y` = save, `n` = redo, `c` = check again, `t 8` / `t 3 10` / `r 4` = adjust;
   5. **60 s hands-off soak**: any touch is a phantom touch, so FAIL, and it offers a redo.
   Before it starts, it asks the **lamp colour**: gray-charcoal, green-matcha, white-cloud or
   sand-dune. The shells are different 3D-print materials, which change the touch signal. The
   button cycles colours like the lamp picker (2 clicks = next, 1 click = take); the last colour
   is the default.
   After a PASS, a **hands-off phantom watch starts on the lamp itself**. It runs detached in the
   lamp's root tmux session `mpr121-watch`, with hal running on the new calibration, for 30 min
   by default (`--watch SECONDS`, 0 = off). It logs to `~/hwtest/mpr121/<lamp>-watch-*.log`. The
   hub moves straight on, so you can calibrate the next lamp while earlier ones are being watched.
   - **Collect:** `lamp-remote-test <lamp> --watch-fetch`, or `--all --watch-fetch` for every lamp
     in range. It copies the log and result into `~/hwtest/fleet/<lamp>/` and the stats pane.
   - **Watch without re-calibrating:** `--watch-only`.
   - **Look live on a lamp:** `sudo tmux attach -t mpr121-watch`.
   The bottom-right pane shows `lamp-mpr121-stats --follow` (also `lamp-test stats` on the Mac):
   per colour, idle noise, thresholds, real-touch strength, the weakest pad's margin, and
   watch/phantom counts.
5. The report goes to `~/hwtest/fleet/<lamp>/<lamp>-mpr121-*.md/.json` and the outbox. Every run
   adds a row to `~/hwtest/fleet/mpr121-tune.csv` (the batch tracker; the summary shows how many
   lamps are calibrated).

`lamp-remote-test <lamp> --deploy-hal` only installs the bundle; `--tune --no-deploy` calibrates
without installing it.

### What the test does (`lamp/lamp_fulltest.py`)

Order: camera -> mic -> speaker -> mpr121 -> button -> ttp223 -> **servo (last)**.

| Step | Pass when |
|---|---|
| hal.service | stopped first (owns servo/GPIO/LED); left stopped, comes back on next boot (`--restart-hal`) |
| camera | full-res frame via `camera_capture_test.sh`; chafa preview in the terminal + Y/n |
| mic | mic1 + mic2 record 4 s while you speak; peak > -50 dBFS and not clipping |
| speaker | plays both recordings on `device_speaker`; Y/n |
| mpr121 | all 12 electrodes register (touch 6 / release 3); fail shows best delta per pad |
| button | header pins 11, 35, 37 all pressed |
| ttp223 | >= 2 of pins 27, 29, 31, 33 touched |
| servo | current pose = middle (2047); torque off; rotate every joint through its range (live min/max shown); press **any 2 of the 3 buttons** to confirm; ranges saved to the calibration file (no backup). Unmoved joints get 2047 +/- 1024. No confirm in 180 s = saved anyway, FAIL |
| end | "Power off the lamp now? [y/N]" auto-N in 3 s, `y` = power off (after the hub has the report) |
| then | "Calibrate the servos now (hal.calibrate)? [y/N]" auto-N in 10 s; `y` runs `python -m hal.calibrate --id lumi --port <servo> --follower-only` from `/opt/hal/.venv` interactively (a requested power-off waits for it) |
| after hal.calibrate | starts `hal.service` so you can watch the servos idle; "Power off the lamp now? [Y/n]" auto-**Y** in 30 s (`n` / 1 click keeps it on). If calibration failed: hal started, no auto power-off |

- Y/n prompts count down 3 s, default Y, single key (`n` = No). Power-off prompt default N.
- Voice: each step is announced from `lamp/lamp_voice/*.wav` (`--no-voice` to mute). The WAVs are not in git: make them with `mac/make_voice.sh` (macOS `say`) before `hub/install-hub.sh`.
- LED ring (32+24+12): left half of each ring = progress, right half = one segment per test
  (green pass, red fail, amber skip, blue running). Final state stays lit.
- Backends picked automatically: HAL lamps (`/opt/hal/.venv`, calibration
  `/opt/hal/calibration/robots/hal_follower/hal.json`) or the lelamp bench
  (`~/lelamp_runtime/.venv`, `~/.cache/huggingface/lerobot/calibration/robots/lelamp_follower/lumi.json`).
- Audio uses the `/etc/asound.conf` aliases `device_micro1`, `device_micro2`, `device_speaker`.

## Layout

| Path | What |
|---|---|
| `lamp/` | copied to every lamp by the hub: `lamp_fulltest.py`, `lamp-test` (picks python/sudo), `servo_test.py`, `mpr121_opi_test.py`, `camera_capture_test.sh`, `lamp_voice/` |
| `hub/lamp-remote-test` | runs on the hub: scan, join hotspot, copy files, run, collect log+report to `~/hwtest/fleet/<ssid>/` and `outbox/`, power off on request, restore hub Wi-Fi (self-heals if a run was killed) |
| `hub/install-hub.sh` | set up a hub |
| `hub/dhcpcd-end0.conf` | end0 fallback 192.168.1.10/24 (no gateway) |
| `mac/lamp-test` | Mac launcher, fetches reports into `~/lamp-reports/` |
| `mac/install-mac.sh` | Mac setup (launcher, USB-LAN IP, SSH key to hub) |
| `mac/make_voice.sh` | regenerate the voice prompts with macOS `say` |
| `tools/` | single-component tests (servo, mic, TTP223, buttons, RGB, SEN5x, ...) |

## Setup

### Hub (Orange Pi 4 Pro, e.g. opidev1)
```bash
git clone <lampOS> && cd lampOS/robots/lamp/hardware/test
bash hub/install-hub.sh
```
Plug the Ethernet cable into the board's own RJ45 (`end0`).

### Mac
```bash
bash mac/install-mac.sh          # needs the USB ethernet adapter plugged in
```

### Lamp prerequisites
Hotspot `lamp-xxxx` at 192.168.100.1, `orangepi` user with sudo, `/etc/asound.conf`
aliases, `/opt/hal/.venv` (HAL lamps), I2C bus 0 for the MPR121 (`overlays=... i2c0` in
`/boot/orangepiEnv.txt`), SPI3 for the LED ring. Tailscale on the boards runs in
userspace mode (`FLAGS="--tun=userspace-networking"`, no /dev/net/tun).

## Devices

| Device | Address | Notes |
|---|---|---|
| Test hub (an Orange Pi 4 Pro) | cable 192.168.1.10 (`end0`) | runs `lamp-remote-test`, the button and the LCD view |
| Mac USB-LAN (en6) | 192.168.1.233/24 manual | cable to the hub |
| Lamp in setup mode | hotspot `lamp-xxxx`, 192.168.100.1 | user `orangepi`; hotspot PSK default `orangepi` |

## Known issues
- Lamps in hotspot mode have no NTP, so the lamp clock (and the report date) can be wrong.
- The ES8389 onboard mic only records at 44.1 kHz; the Jieli USB mic (noise cancelling)
  reads very quiet unless you actually speak.
- MPR121 idle noise reaches +7 on some pads at touch 6 - a pad may pass without a touch;
  use `--mpr-touch 7` if that happens.
