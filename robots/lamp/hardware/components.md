# Components

Running list of parts that go into one lamp. Models / links filled in as we lock things down.

Parts tagged **optional** or **decorative** in the notes column are **not required** for a functional build — the lamp works fine without them.

| Part | Model | Notes |
|---|---|---|
| Mic 2 (voice) | Jieli USB composite mic (`4c4a:4155`) | dual-mic ENC module; delivers one mono 48 kHz channel; ALSA `device_micro2` (see `wiring.md`) |
| Mic 1 (sensing) + speaker DAC | C-Media USB audio adapter, Unitek Y-247A (`0d8c:0014`) | card `device_cmedia`: capture = sensing mic `device_micro1`, 3.5 mm line-out → PAM8610 (`device_speaker`) |
| Camera | USB ZV A016 V4 | FOV 78° |
| Speaker 3W x2 | | |
| Speaker amplifier | PAM8610 v2 | |
| OrangePi 4 Pro | 4GB or 6GB RAM | |
| Servo x5 | STS3215 ST-3215-C018 | |
| Servo control board (USB) | Waveshare Bus Servo Adapter | https://www.waveshare.com/product/modules/motors-servos/drivers/bus-servo-adapter-a.htm |
| RGB LED ring | WS2812B 32-LED ring | 32 px is what the driver and `presets.json` assume (see `wiring.md`, `power.md`) |
| User button | Tactile button switch 6 mm | https://www.adafruit.com/product/367 or equivalent; header pin 35 |
| Reset button | Tactile button switch 6 mm | header pin 37; hold ≥ 5 s and release = factory reset |
| Mic privacy switch | Slide switch | header pin 11; LOW = mic muted (also disables camera, mutes speaker) |
| Capacitive touch controller | MPR121 breakout (12 electrodes, I²C 0x5A) | touch strip in the base: tap / swipe / hold; per-unit thresholds calibrated at the bench (see `wiring.md`) |
| RJ45 extender | | extends the OrangePi 4 Pro Ethernet port |
| Wire, screw, header, USB-C female | | to be updated |
| 3D printed body | | STL / STEP files: see `cad/` |
| Ball bearing | | STL / STEP files: see `cad/` |
| Aluminium CNC parts | | holds the STS3215 servos; STL / STEP files: see `cad/` |
| Wooden CNC parts | | **decorative** — STL / STEP files: see `cad/` |
| Limited edition tag | | **decorative** — units 1–100, CNC aluminium or laser engraving on leather |
| Power adaptor | 12V 5A LiteOn | |
| DC-DC step-down (12V → 5V for OrangePi) | MP2482 | needs replacement — produces audible audio hiss |
| Fan 5V | Nidec | |
| Touch button x4 | TTP223 | head pads for petting; four candidate positions, only two are wired per unit (HAL detects which) |
| Environment sensor | Sensirion SEN55 / SEN63C, or SCD41 | **optional** — declared but disabled (`sen55.json`, `sen63c.json`, `scd41.json`) |
| Steel plate (dead weight) | 5 mm thick steel disc, ~3 kg | **optional** — ballast for base stability |
| Silicon pad | | **optional** — stuck to the bottom of the lamp base to reduce slipping |
