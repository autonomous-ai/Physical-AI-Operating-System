# Setup for a home (#806)

A lamp in a home is set up by its person, with a phone and no engineer: the
home's Wi-Fi, the voice's service, the person's name and time zone. Holding the
reset button for 5 s takes it back to setup and wipes the person's data. The
first builder decided (2026-10-15) that V2 ships with **V1's setup flow**, not
a new one. This page studies that flow and says, part by part, what V2 takes.

The study read V1 on `origin/main` at `be040af6704d`. Paths below are V1's,
on that commit. Nothing here was run on a lamp.

## What a person does in V1

1. **Power on.** The light breathes dim white, then shows "booting". The Go
   server (`os-server`) reads `set_up_completed` in `/root/config/config.json`;
   while it is false it starts the hotspot and turns the light solid white
   (`system/server/config_watch.go`). The lamp says nothing during setup.
2. **The phone joins the open hotspot** `lamp-xxxx` (no password; the suffix is
   the last four characters of the board serial or the Ethernet address). It
   gets an address in `192.168.100.50–150`; the lamp is `192.168.100.1`.
3. **The setup page.** Normally the autonomous.ai account page opens it as
   `http://192.168.100.1/setup?llm_api_key=…&device_id=…&parent_origin=…`.
   Any address also reaches the lamp: the hotspot's DNS answers every name with
   the lamp, and `/` redirects to `/setup`. The phones' sign-in checks
   (`/generate_204`, `/hotspot-detect.html`, …) get 204, so no sign-in sheet
   pops up; the person opens a browser.
4. **The wizard** shows one step in normal use: pick the home network, type
   its password, press Setup. The page's admin password defaults to the
   sticker suffix. The AI, channel, language and voice steps appear only with
   `?debug=true`.
5. **"Your device is joining Wi-Fi…"** The light blinks blue. About 2 s after
   Setup the hotspot drops (the lamp has one radio). The page follows the lamp
   to its new address if it caught it.
6. **"Preparing your robot's voice…"**, up to 5 minutes. Then the light goes
   to rest and the agent greets the person.
7. Optional: record "My Voice" (a name and a recording) and a face.
   **Time zone is not in setup**: it is in Settings, behind the admin login.

## How V1 does it

| Part | V1 code and files |
|---|---|
| Server | `system/server/device/delivery/http/handler.go` (`Setup`, `SetupStatus`, `WifiProvision`), `system/device/setup.go`, `system/network/{service,recovery,setup_failure}.go`. Behind nginx on port 80, which serves the web bundle and proxies `/api` to `127.0.0.1:5000`. |
| API | `POST /api/device/setup` (open until setup is done; body `SetupRequest`: `device_id`, `llm_api_key`, `llm_base_url`, optional `ssid`/`password`, …), `GET /api/device/setup/status` (`phase: idle/connecting/connected/failed`, `lan_ip`, `error`, `mac`, `run`, `set_up_completed`, `runtime_phase`), `GET /api/network` (scan), `POST /api/device/wifi-provision` (hotspot clients only), `POST /api/system/factory-reset`. |
| Page | `system/web/src/pages/setup/*`, `hooks/setup/useSetupStatusPolling.ts`, `pages/wifi-provision/WifiProvision.tsx`. |
| Radio | Image scripts written by `scripts/imager/build-orangepi.sh`: `device-ap-mode` (hostapd, open 2.4 GHz channel 6, dnsmasq answering every name with `192.168.100.1`), `device-sta-mode`, `connect-wifi SSID [PASS]` (rewrites `wpa_supplicant-wlan0.conf`). The image removes NetworkManager. |
| Done | `set_up_completed: true` in `config.json`, written only after the agent runtime is ready. Also there: the Wi-Fi name and password in plain text, `llm_*`, `device_id`, the admin password's bcrypt hash. |
| Time zone | `system/device/timezone.go`: writes `/etc/localtime` and `/etc/timezone`, runs `timedatectl` when present. |
| Reset | Orange Pi: a separate button on chip 0 line 100 (header pin 37), pull-up, active low, held 5 s (`robots/lamp/gpio_button.json`). HAL says "Factory reset starting", waits 3 s, releases the servos, then calls `POST /api/system/factory-reset`, which wipes `config.json`, the agent's state, users and strangers, snapshots, the Wi-Fi configuration, logs and every runtime's persona and memory, keeps `bootstrap.json` and the image, and reboots. At most once in 5 minutes. |

**The Wi-Fi switch.** After Setup: 2 s, `connect-wifi`, then up to 60 s for the
named network **and** the Internet; two wrong-key reports from wpa_supplicant
end it early as `wrong_password` (others: `ssid_not_found`, `no_dhcp`,
`no_internet`). On failure nothing is saved and the hotspot comes back in about
5–8 s. **The stuck-screen fix:** while the hotspot is still up, a loop
publishes the new LAN address every second; the page polls status every
600 ms, calls the hotspot lost after 5 s of silence, tries the new address
first, then `<host>.local` every 2 s, and shows failure at 80 s. On a first
setup the hotspot often drops before DHCP finishes, and on routers that block
mDNS nothing can redirect: the page can show failure after a join that worked.
After setup, 90 s without Wi-Fi brings back a recovery hotspot, which retries
the saved network every 2 minutes only while no phone is on it.

## Verdicts for V2

| Part | Verdict | Why |
|---|---|---|
| Open hotspot `lamp-xxxx`, `192.168.100.1`, every name answered with the lamp | **Take as is** | The same on a V2 lamp, which runs on V1's image. |
| `device-ap-mode`, `device-sta-mode` on the image | **Take as is** | Field-tested on these boards; NetworkManager is not on the image. |
| `connect-wifi SSID PASS` | **Take with a change** | It puts the password on a command line. V2 writes `wpa_supplicant-wlan0.conf` itself, private, then runs `device-sta-mode`. |
| 204 answers to the phones' sign-in checks | **Take as is** | Keeps phones from fighting the page; the person opens a browser, as now. |
| The new address published early, the `run` counter, the status reply's shape | **Take as is** | It is the fix for the stuck screen. |
| The page's redirect order and its 600 ms / 5 s / 2 s / 80 s timings | **Take as is** | Field-tested. |
| Join rules: 60 s, two wrong keys end it, named failures, hotspot back, nothing saved on failure | **Take as is** | They handle the most common failure. |
| 90 s loss to a recovery hotspot, retry every 2 min only with no phone on it | **Take as is** | `lamp-surround` already has the same numbers. |
| The setup page (React bundle served by nginx) | **Take with a change** | The same Wi-Fi step and progress screens, served by V2 from the same paths with the same API shapes; the agent, channel and MQTT parts removed; a name and a time zone added. |
| Credentials only through the account page's URL | **Question for the owner** | V1 receives an OpenAI-style key and base URL from the autonomous.ai account. V2's voice needs a Gemini credential (`provider.json` names a key file; it can also take a bearer or ephemeral token). Where a home lamp's credential comes from is a product choice. |
| Admin password = sticker suffix | **Take as is** | The person has the sticker; refuse to start if the hardware ID is unreadable. |
| `set_up_completed` | **Take with a change** | One marker of V2's own, written only after the voice is healthy (not an agent); `lampos.target` starts only when it exists. |
| Agent runtime, MQTT, channels, the backend ping | **Leave** | V2 has none of them. |
| Name and time zone | **Take with a change** | V1 does not ask during setup. V2 asks for both; the zone is written the way `timezone.go` does it. |
| Reset button: line 100, 5 s, a spoken notice, the wipe, a reboot | **Take with a change** | Same button and hold. V2 wipes its own stores (below) and the Wi-Fi configuration, then restarts into setup. |
| MQTT soft reset | **Leave** | Cloud only, and it keeps the Wi-Fi. |
| Running V1's whole `os-server` for setup | **Leave** | After Wi-Fi it waits up to 120 s for an agent runtime and drives the light through HAL; neither exists beside lampOS, and both would need V1 changes. |

## What setup must write for lampOS

What lamp-4ace's units read today (`deploy/lamp-4ace/`):

| Item | Today | What setup writes |
|---|---|---|
| Wi-Fi | V1's `wpa_supplicant-wlan0.conf` | The same file, mode 0600. |
| The voice's service | `/etc/lampOS/voice/provider.json`, which names a credential file | The credential file; `provider.json` comes with the release. |
| Update trust | `/etc/lampOS/update/key.pub` | Nothing: a trust root belongs in the image, not on a page anyone near the lamp can open. |
| Time zone | Schedule takes `--zone Asia/Ho_Chi_Minh`; Memory and Expression take `--utc-offset-minutes 420` | `/etc/localtime` and `/etc/timezone`; the parts read the system zone. A fixed offset is wrong across daylight saving, so Memory and Expression need a zone name, not minutes, before a lamp ships outside Vietnam. |
| The person's name | Not read by any part yet | A private file the voice gives the model with its instruction. |

## The person's data, which reset wipes

From lamp-4ace's units: memory `/var/lib/lampos-memory`, faces
`/var/lib/lampos-eyes/people.json`, tasks `/var/lib/lampos-tasks/store` (and its
targets), schedule `/var/lib/lampos-schedule/store`, voice sessions
`/var/lib/lampOS/voice/sessions`, reminder delivery
`/var/lib/lampOS/voice/schedule-delivery`, the conversation thread
`/run/lampos-voice/thread.json`, the policy's log `/var/lib/lampos-policy`, the
credential file, the name, the Wi-Fi configuration and the setup marker. Kept:
the software, its releases, the update key, the touch calibration and other
per-lamp facts, and physical privacy.

## Open questions (asked on #806)

1. Where does a home lamp's voice credential come from: the autonomous.ai
   account page, as in V1, a key the person types, or a token service?
2. Is `key.pub` put in the image at the factory (proposed), not by setup?

## Not shown

No lamp, phone, router or hotspot was used for this page. The V1 behaviour
above is read from source and V1's own documentation, not measured.
