# Air: the air sensor

`lamp-air` (`crates/air`) reads the lamp's Sensirion SEN63C every five
seconds and says on the bus what the air is like. It decides nothing. With
it stopped the lamp runs on as before.

## The sensor

A SEN63C on I2C bus 0 at 0x6B (beside the base's touch strip on the same
bus), measuring particles (PM1, PM2.5, PM4, PM10), relative humidity,
temperature and carbon dioxide (`robots/lamp/hardware/components.md`).
Commands used: product name (0xD014), data ready (0x0202), read measured
values (0x0471, seven words, each with Sensirion's CRC-8), and start
measurement (0x0021), sent only when no new reading has come for 15 s,
as after a power cut. Every word's CRC is checked; a bad one is no value.
0xFFFF (and 0x7FFF for the signed humidity and temperature) means the
sensor does not know the value yet, and it is left out.

The temperature and humidity are those inside the base, warmed by the
lamp itself (33 °C on 2026-10-14 in a cooler office): they are not the
room's, and nothing should speak of them as if they were.

## What it says

| Topic | Type | Kind | What |
|---|---|---|---|
| `air/state` | `AirState` | latest, every 5 s | PM1, PM2.5, PM4, PM10 (ug/m3), humidity (%), temperature (°C), CO2 (ppm), and `feel`: fresh (under 800 ppm), fine (to 1,200), stuffy (to 2,000), very stuffy, or unknown |
| `diagnostics` | `Diagnostic` | every second | `air`: ok, or a warning with why the sensor does not answer; the feel, CO2 and PM2.5 |

The bands follow carbon dioxide because that is what a person at the desk
feels as a stuffy room; they are round figures, not a standard.

## On the lamp

    lamp-air --bus-number 0 --bus-root /run/lampos-bus
    lamp-air once        # the sensor's name and one reading, no bus

Unit `lamp-air.service` in the first builder's deploy folder. Running on
lamp-4ace since 2026-10-14 17:43: CO2 1,856 to 1,872 ppm ("stuffy"), PM2.5
102 to 105 ug/m3 in the office at work.

## What the creature does with it

See [creature.md](creature.md). The voice's part, behind
`LAMP_AIR_FACTS=on` in its unit (off unless set; `crates/voice/src/air_told.rs`):
it reads `air/state` every five seconds; once the air has felt stuffy (or
very stuffy) for ten minutes, the model is told so beside the next thing a
person says to the lamp, in a room that is not busy, with leave to suggest
a window once if it fits; not again for three hours. A reading older than
30 s counts as no reading. Still to build: the quiet droop and dimmer eye
before anything is said (Expression, with the eye, #712, and the idle
layer, #714).

## Not shown

- The PM figures against a reference instrument: one sensor, one office.
- What a power cut does to the sensor's measuring state (it is started
  again after 15 s without a reading; not seen on a lamp yet).
