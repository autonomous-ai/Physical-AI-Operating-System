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
| 1 | Primary button | header pin 35, chip 0 line 99, pull-up, low when pressed | The owner (14 Oct): there is no button on this lamp. On 17 October, 17:43–18:11, the hardware team pressed buttons at the lamp and Controls counted 7 events; the latest was a 142 ms tap on `primary` (source `button`), read as stop (item 12) | No user button in this lamp (14 Oct). Where the button that was pressed on 17 October sits on this unit is still to be said |
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

   **Measured on 17 October:** on lamp-4ace, neither microphone carries a
   person's word said over the lamp's own speech, whatever the software
   after it does (#828).
   - **Voice microphone (Jieli, mic2):** the echo canceller's linear stage,
     before its suppressor, takes only 3 to 4 dB of the lamp's voice out,
     and its suppressor takes 11 to 15 dB, the person's words with it. A
     person 10 dB under the echo stays 4 to 7 dB under it after the linear
     stage. "Lamp, stop." played from the office Mac over a story barely
     raises the linear stage's level (−18.8 against −18.1 dBFS), the final
     signal falls silent under it, and on lamp-52e6 the raw signal's
     speech-to-text returned only the story. A canceller can
     remove only an echo it can model, and this one has its own clock, no
     loopback and processing on the chip.
   - **Room microphone (the C-Media speakerphone, mic1, which is also the
     speaker):** recorded while the lamp told a story and "Lamp, stop.",
     a second request and "Stop." were played over it, the speech-to-text
     returned the whole story and none of the three. The speakerphone does
     not take its own voice out.
   - **How loud the lamp's own voice is on the voice microphone** (17 October,
     afternoon; the raw mic2 signal the stop words were sent, its last 10 s,
     over one short story per setting): about −13 to −14 dBFS RMS, the 99th
     percentile at −2 to −2.6 dBFS, and 12 to 61 samples at full scale in
     those 10 s, at every setting tried: `Mic` capture 147 with Auto Gain
     Control on (as found), 147 with it off, and 120 (5 dB lower) with it
     off. The ALSA controls hardly change what the Jieli delivers. A voice
     from the room arrives about 15 dB under the lamp's own (−17 to −20
     dBFS). "Lamp, stop." said alone over a story got no yield from the
     hearing in any of 11 runs, and the stop words' speech-to-text, checked
     in 6 of them and in 2 more at the lower gain, never heard it. These
     voices were played from the office Mac's speaker, which misjudges this
     microphone; a person close to the lamp may do better. The settings were
     put back to 147 with Auto Gain Control on.
   - **What the new board needs for a person to be heard over the lamp:** a
     voice microphone on the speaker's clock with a hardware loopback of the
     speaker signal (the echo reference), or a microphone the speaker's
     sound reaches far more weakly than a person's.
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

## Asked of the hardware team (2026-10-17, lampOS's first builder)

8. **The voice microphone falls off USB.** On 17 October the Jieli (port 1
   of the second hub) disconnected for 4 to 5 s twice: at 12:29:20 and at
   12:49:06 ICT (kernel `usb 3-1.1.1: USB disconnect`, back as a new
   device number). Both times a 47 MB voice build was being copied onto the
   lamp's SD card over its Wi-Fi (`wlan0`, the only link). There was no
   other disconnect since boot (42 hours earlier, from `uptime`), and at
   12:49 the arm was resting and still (the body's own log). The old voice
   was stopped after each disconnect, not before: 3.2 s after the 12:49 one
   (kernel and voice clocks), about 3 s after the 12:29 one (the kernel's
   wall time, to the second). Its stop did not cause them.
   Each time the new voice could not open the microphone and came back
   muted. Is the hub chain powered from the
   MP2482 rail with the computer, ring and fan, and could a load step on it
   drop the hub's first port? Is the cable or its plug worth reseating?
9. **The air sensor's reads fail.** Since at least 11:00 on 17 October
   `lamp-air` has logged 20 to 30 failed reads of the SEN63C a minute
   ("word 0 fails its CRC", "command 0xd014: Invalid argument (os error
   22)"), on I2C bus 0 with the MPR121 (how many reads still succeed was
   not counted). Its wiring and pull-ups, please, or whether this unit's
   sensor is faulty. **Answered on 17 October by swapping the sensor (item
   12): neither. Not the bus speed either, as first written here: put back
   at 400 kHz at 20:15, it read 20 of 20. The cause is not shown; it is
   lampOS's to fix.**
10. **The board runs hot.** From 16 October 16:03 to 17 October 14:21, one
    reading a minute, `cpul_thermal_zone` and `cpub_thermal_zone` read within
    1 °C of each other. Their hourly means were 74 to 81 °C at night (19:00 to
    08:00) and 83 to 91 °C by day: 88 to 91 °C on Friday from 16:00 to 19:00
    with the office busy, 83 to 86 °C on Saturday morning. The lowest reading
    was 63 °C, the highest 100 °C (`cpul`, 16 October 18:02). The hourly load
    averages were 1.2 to 2.3 on 8 cores. When read on 17 October at 14:07 the
    clocks were at their top (1.8 and 2.0 GHz, `ondemand`); on 16 October at
    16:40 the big cores were throttled to 1.79 of 2.0 GHz. The CPU zones' trips
    are 70, 90 and 110 °C; `skin_zone` read 36 to 43 °C against its 50 °C. No
    fan shows to software as a cooling device (read 17 October 14:07: only CPU
    idle and frequency). lamp-cb94 read 62 to 67 °C
    on its zone0 on Saturday morning (Thirteen's soak, #705, 17 October 14:20),
    with its voice at 38 to 39% of a core; its load average isn't given. On
    lamp-4ace zone0 is `cpul_thermal_zone` (read 17 October 14:2x), and it read
    83 to 86 °C that morning, so lamp-4ace ran about 20 °C hotter the same
    morning (assuming cb94's zone0 is the same zone). Is its fan running, is
    the heatsink in contact, and what should this load read? **Answered on 17
   October (item 12): a second fan, about 20 °C cooler under load.**
11. **The SD card's host timed out once since boot** ("re-tuned and timed out",
    the self-test's `storage` item on 17 October at 13:51). Such stalls reach
    the audio: on lamp-4ace the voice's exits lined up with the SD/MMC hosts'
    retunes to the millisecond (#784), and on lamp-cb94 the same timeout came
    in the same second as the voice's capture worker ending (04:53:53 UTC,
    #1178). Which SD cards are fitted, and is another card or slot setting
    worth trying?

## Done at the lamp by the hardware team (2026-10-17, 17:12–18:15)

12. **What was changed and what the lamp showed** (read from lamp-4ace while
    it was done; the base was open and the lamp was restarted at 17:18 and
    17:43):
    - **A second cooling fan was added** (about 17:15). Idle, at the same load
      (about 1.2), the CPU zone read 62 °C over 11 minutes after it, against
      79 °C over last night's 619 minutes before it. Under the same
      test (the hourly battery, load about 1.7), the CPU zone averaged 67.1 °C,
      at most 69, against 86.9 °C, at most 89, on the old fans that afternoon:
      about 20 °C cooler, and under the first trip at 70. Item 10 is answered.
    - **The inner USB hub (`3-1.1`) dropped twice at 17:12:29 and 17:12:36**
      while the base was open, and the voice microphone, the body's servo port,
      the speaker and the camera with it, and once more at 18:15:33 under hands.
      That was the work, not a fault: none has dropped by itself since.
    - **The elbow servo was tested and its cable readjusted.** The camera's
      cable was unplugged and replugged between 17:39 and 17:40; the camera
      is on USB again (`01da:5875`).
    - **A button press is read:** see row 1.
    - **The air sensor was swapped for another SEN63C, which failed the same
      way, and that settled item 9: not the sensor, not the wiring.** Read raw
      (`0xD014`, product name), the sensor answers "SEN63C" correctly, with
      `0xff` filler bytes wedged between the real ones (`53 ff 45 83 ff 4e 36
      ff 06 …`). Stripped of them, every CRC matches. The board runs I2C bus 0
      (`twi@2510000`) at 400 kHz (`clock-frequency` 400000), above the SEN6x's
      standard mode (100 kHz). I first took that for the cause, and it is not
      shown. At 20:15, with the bus put back at 400 kHz, the `lamp-air`
      without a filler decode read 20 of 20, and 3,360 raw bytes held no
      filler. The rate change is real: a 48-byte read takes 1.3 ms at 400 kHz
      and 4.6 ms at 100 kHz. The reads came back at 18:36, after a start was
      sent by hand to the swapped sensor, so a sensor left not measuring is
      the likelier cause. lampOS's fixes: a reader that starts a sensor it
      finds idle and skips filler (#1366), and the bus at the sensor's own
      100 kHz, at run time (#1368, as V1's HAL did) and in the next image
      (#1372). Since 18:36 `lamp-air` reads the air (CO2 about 470 ppm), and
      the self-test passes its air item.

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
| Mic 2 (voice), Jieli | `4c4a:4155`, card `device_micro2`, Mic capture 147 of 147 (-0.94 dB), Auto Gain Control on (read 2026-10-16); also shows a small USB disk and a key device | Voice, Name |
| Mic 1 (sensing) + speaker DAC, C-Media | `0d8c:0014`, card `device_cmedia`; on 2026-10-14 its Speaker control at 9 of 37 (-28 dB), Mic capture 4 of 35 (-8 dB), Auto Gain Control on; playback goes through the softvol alias `device_speaker` (`/etc/asound.conf`, control "Speaker" on card `allwinnerhdmi`, -30 to 0 dB) at 75 % (-7.5 dB), the control `lamp-actions` changes for "louder" and "quieter". From 2026-10-15 14:49 to 2026-10-16 02:56 it was at 95 %: a word overheard in a busy office ("Thailand") ran `volume_up`, and at that level the lamp's own voice clipped Mic 2 on 9 % of its speech (under 1 % at 75 %). Both controls are back at the 14 Oct values (`alsactl store`) | Ears (capture), Voice (playback) |
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
