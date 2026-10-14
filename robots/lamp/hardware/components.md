# Components

The lamp's bill of materials: every part in one lamp, how it is connected,
and what reads or drives it. **Kept true by lampOS's first builder** (the
session with lamp-4ace); see "Keeping this list true" at the end.

The electronics were checked on **lamp-4ace on 2026-10-14** from the running
lamp (USB, I2C, GPIO, SPI, sound, video and power devices) and, for the
touches, with a person's hand. Parts software cannot see (speakers,
amplifier, power, fan, printed and machined parts) are as the build list
says and marked "not checked". Parts tagged **optional** or
**decorative** are not needed for a working lamp.

## Computer

| Part | Model | Connection / identity | Checked | Read or driven by (lampOS) |
|---|---|---|---|---|
| Single-board computer | Xunlong **Orange Pi 4 Pro** | Allwinner **A733** (`sun60iw2p1`): 2 x Cortex-A76 at 2.0 GHz + 6 x Cortex-A55 at 1.8 GHz | 2026-10-14 | everything |
| Memory | 6 GB on the board (4 GB boards exist) | 5.7 GiB usable | 2026-10-14 | |
| NPU | On the A733 (VeriSilicon VIP) | `/dev/vipcore` | present 2026-10-14 | nothing yet |
| Storage | 64 GB microSD (`SN64G`) | `/dev/mmcblk1` | 2026-10-14 | |
| Wi-Fi and Bluetooth | AIC8800 on SDIO (`C8A1:0082`) | `wlan0`, `hci0` | 2026-10-14 | Wi-Fi: network; Bluetooth: nothing yet |
| Ethernet | On the board, brought out by an RJ45 extender | `end0` | interface present | network |
| Real-time clock | HYM8563 | I2C bus 15, 0x51 | bound 2026-10-14 | system clock |
| Power management | AXP515 and AXP8191 | I2C bus 13, 0x34 and 0x36; AXP8191's power key is `axp8191-pek` | bound 2026-10-14; no battery reported | power key: nothing |
| On-board audio codec | ES8389 (Linux names the device es8388) | I2C bus 7, 0x10; sound card `sndi2s4` | bound 2026-10-14 | not used: sound goes through the C-Media adapter |
| Status LED | On the board | GPIO `gpio-38` | 2026-10-14 | the board |
| Operating system | Orange Pi 1.0.6 Bookworm (Debian 12), kernel 5.15.147-sun60iw2 | | 2026-10-14 | |

## Senses

| Part | Model | Connection / identity | Checked | Read by (lampOS) |
|---|---|---|---|---|
| Camera (the eye) | USB ZV A016 V4 (USB name "ZVCAI USB_Camera", `01da:5875`); FOV 78° | USB 2.0 (hub port 4); `/dev/video0`; MJPEG up to 3264 x 2448; brightness, gain, white balance and more as controls. In the head, at the centre of the light ring | 2026-10-14 | Eyes (`lamp-eyes`) |
| Camera's microphone | In the same camera | USB audio, card `USBCamera`: two channels at 22.05 kHz, 8 bits, but only the right carries sound (one capsule) | 2026-10-14 | nothing |
| Voice microphone (mic2) | USB dual microphone HK JZMIC v1.0 (Jieli, `4c4a:4155`), its own echo cancelling, beam and noise filtering; also shows a small USB disk and a key device | USB (hub port 1); card `device_micro2` | 2026-10-14 | Voice, Name |
| Room microphone (mic1) | The input of the C-Media USB audio adapter (below) | card `device_cmedia`, capture; a plain capsule in the base | 2026-10-14 | Ears (`lamp-ears`) |
| Touch, head | TTP223 capacitive touch pads; the build list says four on the head | Two answer a hand: GPIO chip 0 **line 37** and **line 96** (high when touched) | the owner's hand, 2026-10-14 | Controls (`lamp-controls`) |
| Touch, base (a strip) | **MPR121** capacitive touch controller, 12 electrodes | I2C **bus 0, 0x5A**. Ten pads answer, in order along the strip: 0-3, 5-7, 9-11 (4 and 8 never did). Sees taps, holds and slides | the owner's hand, 2026-10-14 | nothing yet (#711) |
| Air sensor | Sensirion **SEN63C** (particles PM1-PM10, humidity, temperature, CO2) | I2C **bus 0, 0x6B**; serial 071F78D689E4A407; measuring | 2026-10-14 (CO2 1,931 ppm, PM2.5 92.5 ug/m3, 31.7 % RH, 33.0 °C inside the base) | Air (`lamp-air`): built, not yet running on the lamp |
| A declared touch controller | Goodix GT9271 | Declared on I2C bus 5, 0x14 by the device tree; no driver bound; a read failed | 2026-10-14 | probably not fitted |
| Microphone privacy switch | V1's design: a slide switch on GPIO chip 1 line 9 (`gpio-361`, pull-up, low = mute) | Reads high; no switch is fitted on lamp-4ace | 2026-10-14 | Voice (as its privacy input) |
| User button | Tactile button 6 mm (V1's list) | **Not fitted on lamp-4ace** (the owner, 2026-10-14). GPIO chip 0 line 99, once named for it, never changes | 2026-10-14 | |

## Body and light

| Part | Model | Connection / identity | Checked | Driven by (lampOS) |
|---|---|---|---|---|
| Servos x5 | Feetech **STS3215** (ST-3215-C018): base yaw, shoulder, elbow, head roll, head pitch; each reports position, load and temperature | Serial bus | in use 2026-10-14 | Body (`lamp-body`) |
| Servo bus adapter | Waveshare Bus Servo Adapter (QinHeng `1a86:55d3`, serial 5CE4141883) | USB (hub port 2); `/dev/ttyACM0` | 2026-10-14 | Body |
| Light ring (the eye's ring) | **WS2812B, 32 pixels**, around the camera | SPI bus 3, `spidev3.0`; brightness capped in software (see `power.md`) | 2026-10-14 | Voice (ring), Expression (planned) |

## Sound out

| Part | Model | Connection / identity | Checked | Driven by (lampOS) |
|---|---|---|---|---|
| USB audio adapter | Unitek Y-247A (C-Media, `0d8c:0014`): speaker output and the room microphone's input; a key device | USB (hub port 3); card `device_cmedia` | 2026-10-14 | Voice (playback) |
| Speaker amplifier | PAM8610 v2 | from the adapter's output | not checked | |
| Speakers x2 | 3 W | on top of the base, either side of the arm | not checked | |

## Inside

| Part | Model | Connection / identity | Checked | Notes |
|---|---|---|---|---|
| USB hubs | Terminus `1a40:0101` (4 ports), then QinHeng `1a86:8091` (4 ports) in a chain | USB 2.0 on the board | 2026-10-14 | the four USB devices above hang off the second |
| Power adaptor | 12 V 5 A LiteOn | | not checked | single 12 V input (`power.md`) |
| DC-DC step-down (12 V to 5 V for the board) | MP2482 | | not checked | needs replacement: audible hiss in the audio |
| Fan | 5 V Nidec | | not checked | |
| Wire, screws, headers, USB-C female | | | not checked | to be listed |

## Mechanical

| Part | Notes |
|---|---|
| 3D printed body | STL / STEP: `cad/` |
| Aluminium CNC parts | hold the STS3215 servos; `cad/` |
| Ball bearing | `cad/` |
| Wooden CNC parts | **decorative**; `cad/` |
| Steel plate (dead weight) | 5 mm steel disc, about 3 kg; **optional** ballast for stability |
| Silicon pad | **optional**, under the base against slipping |
| Limited edition tag | **decorative**; units 1-100, CNC aluminium or laser-engraved leather |

## Keeping this list true

- **Who:** lampOS's first builder keeps it; any builder who finds a part on
  a lamp writes it here in the same change, with the lamp's name and date.
- **When:** whenever a part is found, fitted, removed or replaced, and once
  a week on lamp-4ace.
- **How a part is checked:** from the running lamp, read-only: `lsusb` and
  `lsusb -t`; `/sys/bus/i2c/devices/*/name`, and a read of a known register
  for user-space parts (MPR121 0x5E, SEN63C product name 0xD014);
  `/sys/kernel/debug/gpio` for claimed lines; `/proc/asound/cards`;
  `v4l2-ctl --list-devices`; `/sys/class/power_supply`, `/sys/class/net`.
  Any input is confirmed with a person's hand or voice while its raw line or
  register is logged, before anything is built on it.
- **Per lamp:** this list is lamp-4ace's. Another lamp (lamp-52e6) is
  checked the same way by its builder, and a difference is written in a
  "differs on" note beside the part.
