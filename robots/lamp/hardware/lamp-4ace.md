# lamp-4ace as built

One unit, as measured from the running lamp on 2026-10-14 by lampOS's first
builder (the session that works on lamp-4ace), and, for the touches, with
the owner's hand. The design every unit follows is the hardware team's:
[`components.md`](components.md) and [`wiring.md`](wiring.md). This file
says what this one unit has, how each part shows itself to software, and
where it differs from that design, for the hardware team to confirm or
correct. It is kept by lampOS's first builder and updated when the unit
changes.

## Where lamp-4ace differs from the design

The hardware team answered on the evening of 2026-10-14 (through the owner);
their answers are in the last column. Where an answer settles a row, the
design files are theirs to change.

| # | Part | Design (`components.md`, `wiring.md`) | lamp-4ace | Hardware team's answer |
|---|---|---|---|---|
| 1 | Primary button | header pin 35, chip 0 line 99, pull-up, low when pressed | The owner: there is no button on this lamp. Line 99 reads high (released); it was not pressed with a hand | No user button in this lamp. (The team's parts list for the design still gives a primary button on pin 35: the design has one, this unit does not.) |
| 2 | Reset button | header pin 37, chip 0 line 100 | Not read by lampOS; not checked | In the design: pin 37, held 5 s. Not said whether lamp-4ace has one; still to check by hand |
| 3 | Mic privacy slide switch | header pin 11, chip 1 line 9, low = muted | Reads high (unmuted); the owner did not mention one | On header pin 11, as designed |
| 4 | Head pads (TTP223) | two of lines 37, 96, 97, 98; pull-up, rest high, a touch is the falling edge | Lines **37 and 96** answer a touch; read with pull-down they rest **low and go high** on a touch | Two pads per lamp, on two of header pins 27, 29, 31, 33, found at start; the older list of four pads was wrong. Not answered: active-high or low on this unit (it is high, as measured) |
| 5 | Base strip (MPR121) | all 12 electrodes register at the bench; per-unit thresholds in `/var/lib/hal/mpr121.factory.json` | Ten answer a finger: 0-3, 5-7, 9-11. **E4 and E8 never did**, and their charge currents are 31 where the others are 41 to 57. The per-unit file is gone (the old software was removed); the chip still holds touch 6 / release 4 on every electrode (`lampOS/deploy/lamp-4ace/mpr121-registers-20261014.txt`) | **E4 and E8 are faulty**, not disabled on purpose. Sensing through the 3D-printed shell is uneven; the team wants a calibration for each pad and a uniform touch surface with the electrodes right behind it. Thresholds are set per lamp |
| 6 | Environment sensor | optional; declared but disabled | A **SEN63C is fitted and measuring** (I2C bus 0, 0x6B) | Fitted in the pro builds, inside the lamp in a thermally insulated housing, meant to measure the room: temperature, humidity and air. Still open: at 33 °C at the sensor it may read warmer than the room; not yet compared with a thermometer in the room |
| 7 | Camera microphone | not selected: near-digital silence, needs repair | Two channels at 22.05 kHz, 8 bits; the left is silent, the right faint (RMS 0.71 of 127) | As the design says |
| 8 | Touch controller GT9271 | not in the design | Declared in the board's device tree on I2C bus 5 at 0x14; no driver bound and a read failed | Not in this lamp; the team did not know where it came from. It comes from the Orange Pi image's device tree, not from anything fitted |

**Coming:** the team has a custom HAT that carries these parts on one board
plugged into the Orange Pi 4 Pro's header and power rails, with much less
speaker hiss. lamp-4ace does not have it; when a lamp does, this file gets
a row for it and the pins are measured again.

## Asked of the hardware team (2026-10-14, 20:20, through the owner)

1. **Echo.** The voice microphone hears the lamp's own voice about as loudly
   as a person, on a clock of its own, so a person speaking over the lamp
   is missed about half the time (`lampOS/docs/duplex-listening.md`). On the
   new board: are the voice microphone and the speaker on one audio chip and
   one clock, is there a loopback of the speaker's signal to use as the echo
   reference, and can the voice microphone sit farther from the speakers or
   be isolated from them?
2. **Amplifier.** Are the PAM8610's mute or shutdown pins on a GPIO, on
   lamp-4ace or on the new board, and which? The lamp would silence it when
   it is not speaking.
3. **Base strip.** The bench settings for each pad (touch and release
   thresholds, charge current and time), the calibration procedure, and a
   copy of lamp-4ace's old per-unit file, if kept.
4. **Reset button.** Is one fitted on lamp-4ace (pin 37), and what should a
   five-second hold do?
5. **Air sensor.** Is its insulated housing open to the room's air, and is
   there an offset or a reference reading for its temperature?
6. **Fan.** Always on, or controllable from software, and from which pin?
7. **The new board.** When do lamp-4ace and the other test lamps get it, and
   do any buses, pins or USB audio devices change? Its pinout early, please.

## Parts the design does not list

| Part | On lamp-4ace |
|---|---|
| Single-board computer | Orange Pi 4 Pro, Allwinner A733 (`sun60iw2p1`): 2 x Cortex-A76 at 2.0 GHz + 6 x Cortex-A55 at 1.8 GHz; 6 GB (5.7 GiB usable) |
| NPU | On the A733 (VeriSilicon VIP), `/dev/vipcore` |
| Storage | 64 GB microSD (`SN64G`), `/dev/mmcblk1` |
| Wi-Fi and Bluetooth | AIC8800 on SDIO (`C8A1:0082`): `wlan0`, `hci0` |
| Real-time clock | HYM8563, I2C bus 15, 0x51 |
| Power management | AXP515 and AXP8191, I2C bus 13, 0x34 and 0x36; power key `axp8191-pek`; no battery |
| Onboard audio codec | ES8389, I2C bus 7, 0x10, card `sndi2s4` (unused, as `wiring.md` says) |
| A declared touch controller | Goodix GT9271 on I2C bus 5, 0x14 in the device tree; no driver bound, a read failed: probably not fitted |
| USB hubs | Terminus `1a40:0101` then QinHeng `1a86:8091`, 4 ports each, in a chain (two chained 4-port hubs, the team confirms); on the second: Jieli mic (port 1), servo adapter (port 2), C-Media adapter (port 3), camera (port 4) |
| Status LED | `gpio-38` |
| Operating system | Orange Pi 1.0.6 Bookworm (Debian 12), kernel 5.15.147-sun60iw2 |

Parts software cannot see, from the hardware team's list (not checked on
lamp-4ace):

| Part | From the hardware team |
|---|---|
| Power in | 12 V, 5 A |
| 5 V supply | MP2482 step-down, for the computer, the LED ring and the fan |
| Amplifier | PAM8610 v2, on 12 V |
| Speakers | two, 3 W each |
| Sensing mic and speaker DAC | the C-Media part is a Unitek Y-247A USB adapter |
| LED ring data | header pin 19 (SPI 3) |
| MPR121 | on header pins 1, 3, 5, 6 (power, SDA, SCL, ground) |

## Parts as they show themselves to software

| Part (design name) | Identity on lamp-4ace | Read or driven by (lampOS) |
|---|---|---|
| Camera, USB ZV A016 V4 | "ZVCAI USB_Camera" `01da:5875`, `/dev/video0`, MJPEG up to 3264 x 2448 | Eyes |
| Mic 2 (voice), Jieli | `4c4a:4155`, card `device_micro2`; also shows a small USB disk and a key device | Voice, Name |
| Mic 1 (sensing) + speaker DAC, C-Media | `0d8c:0014`, card `device_cmedia`; on 2026-10-14 its Speaker control at 9 of 37 (-28 dB), Mic capture 4 of 35 (-8 dB), Auto Gain Control on; playback goes through the softvol alias `device_speaker` (`/etc/asound.conf`, control "Speaker" on card `allwinnerhdmi`, -30 to 0 dB) at 75 % (-7.5 dB) | Ears (capture), Voice (playback) |
| Servo control board, Waveshare | QinHeng `1a86:55d3`, serial 5CE4141883, `/dev/ttyACM0` | Body |
| Servos x5, STS3215 | on the servo bus | Body |
| RGB LED ring, WS2812B 32 px | SPI bus 3, `spidev3.0` | Voice (ring) |
| Head pads, TTP223 | chip 0 lines 37, 96 | Controls |
| Touch strip, MPR121 | I2C bus 0, 0x5A, running (ECR 0x8F) | nothing yet (lampOS task #711) |
| Environment sensor, SEN63C | I2C bus 0, 0x6B, serial 071F78D689E4A407; 2026-10-14 17:36: CO2 1,931 ppm, PM2.5 92.5 ug/m3, 31.7 % RH, 33.0 °C at the sensor | Air (`lamp-air`), running since 17:43 |

## How each part was checked

Read-only, from the running lamp: `lsusb`, `lsusb -t`; `/sys/bus/i2c/devices/*/name`;
for parts read from user space, a known register (MPR121 0x5E, SEN63C product name
0xD014); `/sys/kernel/debug/gpio` for claimed lines; `/proc/asound/cards` and a short
recording; `v4l2-ctl --list-devices`; `/sys/class/power_supply`, `/sys/class/net`,
`/proc/device-tree/compatible`. The two touches were confirmed with the owner's hand
while the lines and the MPR121's touch status were logged (raw log in lampOS issue
#711). Speakers, amplifier, power supply, fan and the mechanical parts cannot be seen
from software and were not checked; what is listed for them above is the hardware
team's.
