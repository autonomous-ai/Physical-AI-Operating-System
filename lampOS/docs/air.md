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
sensor does not know the value yet, and it is left out. CO2 says it as
either: for its first seconds after a start it reads 0x7FFF (lamp-4ace,
17 Oct 18:3x), which taken as 32,767 ppm would make every start Stuffy.

**Filler, a failed name, and a cause not reproduced.** On lamp-4ace on 17
Oct every look failed its CRC from at least 11:00 until 18:36, when the
reads came back after the hardware team swapped the sensor and a start was
sent by hand. The cause is not reproduced: at 20:13 to 20:19 the swapped
sensor, measuring, gave no `0xff` filler at 400 kHz or at 100 kHz (0 in
3,360 bytes each), and at 20:42 to 20:44, stopped (`0x0104`) and idle, it
gave none either (0 in 480) and its name read clean; the installed
`lamp-air` restarted it as it should (the first builder, #705 18:28 to
20:44). So the filler decode and the start when the name fails are guards,
not a shown fix; what is shown is that a sensor just started reads CO2
`0x7FFF` (the old `lamp-air` took it as 32,767 ppm, very stuffy). What was
seen at 18:2x, in the swapped sensor's first minutes on the bus: the SEN6x
speaks I2C standard mode only (100 kHz) and stretches the clock; lamp-4ace's
bus 0 runs at 400 kHz (`twi@2510000`'s `clock-frequency`), the master read
`0xff` while the sensor still held the line, and `0xff` bytes came between
the real ones,
up to two inside one word (the first builder, #705 17 Oct 18:28 and
18:30). So each answer is read at three
times its length and decoded by its CRCs (`words_among_filler`): each
word is two bytes and a CRC that matches, a `0xff` is taken as filler
first and as data only where no decode with it skipped holds,
backtracking over every `0xff` (at most 8 skipped in one place), so a
real 0xFFFF still reads as "not known yet". Filler first, because filler
can make a word: 0xFF00 is `ff 00 00` (its CRC is 0x00), and `ff 00 ff 00`
around a null word, with the inner `ff` skipped, reads as it. A reply that reads straight (its first words all with good CRCs) is taken as it is, before any backtracking, so with no filler a real `0xff` is never skipped for the bytes after the answer, whatever the sensor sends there (the reviewer on #1366: T 0x1BFF then CO2 0x0370, with `0x00` after, read 28,756 ppm without it). When the name still fails its
CRC (something answered), the measurement is started anyway and the
values tried, a guard for a name that fails while the values would read
(on lamp-4ace on 17 Oct the name failed all day; whether a start would
have brought the values back is not shown, and the branch has not run on
a lamp). The journal then says
"the sensor answers: an air sensor whose name failed (…); its
measurement started". A start it refuses (one already measuring may)
still leaves the values to try, and the loop starts it again after 15 s
with no reading. Nobody acknowledging is never started, so a lamp
without the sensor makes no more transactions. The journal says it once
a run: "the air sensor's answer to 0x… had N filler bytes (0xff)
between its words, skipped by their CRCs". The bus at 100 kHz (#1368) is the sensor's own
limit, not shown to be the fix.

A sensor that does not answer, or answers its name but gives no reading
(a half-seated plug, a CRC fault), is looked for three times, five seconds
apart, then once every ten minutes (`src/watch.rs`); only a good reading
counts it as back. Each look is an I2C transaction, and the kernel logs
every one nobody acknowledges ("ACK not received"): on lamp-cb94, which
has no sensor, a look every 200 ms made about 300 kernel lines a minute,
and the journal kept only about 90 minutes. The journal hears of it twice,
the first miss and when the sensor counts as absent. The health report
says it steadily: a sensor that never answered since `lamp-air` started is
`ok`, "not fitted" (a lamp without one is well), and one that answered, by
name or with words that failed their CRC, and gives no reading is a
warning with the last error. A reading already kept on `air/state` this
boot counts as an answer, so a restart of `lamp-air` never takes a fitted
sensor that now only refuses for an absent one (#1324). A sensor that stops answering is
looked for again 5 s later, then the same way; one plugged back in is found
within ten minutes, or at once after `systemctl restart lamp-air`. A bus
that cannot be opened (no `/dev/i2c-0`, or no permission) makes no
transaction, so it is looked at again every five seconds.

The temperature and humidity are those inside the base, warmed by the
lamp itself (33 °C on 2026-10-14 in a cooler office; on 17 Oct it climbed
from 35.5 to 44 °C in 6 s after a start, and the humidity fell with it):
they are not the room's, and nothing should speak of them as if they
were. Where the sensor sits in the base is the hardware team's question
(`lamp-4ace.md` question 5).

## What it says

| Topic | Type | Kind | What |
|---|---|---|---|
| `air/state` | `AirState` | latest, every 5 s | PM1, PM2.5, PM4, PM10 (ug/m3), humidity (%), temperature (°C), CO2 (ppm), and `feel`: fresh (under 800 ppm), fine (to 1,200), stuffy (to 2,000), very stuffy, or unknown |
| `diagnostics` | `Diagnostic` | every second | `air`: ok, or a warning with why the sensor does not answer ("the sensor does not answer: …" in its first three looks; then, for a sensor that has never answered since `lamp-air` started, an **`ok`** report, `not fitted: no air sensor has answered since start` (the sensor is optional, so a lamp without one is well; the journal keeps `no air sensor answers on I2C bus 0 (…); looked for again every 10 min`), or `the air sensor answered but gives no reading on I2C bus 0 (…)` for one that has, by name or with a reading that failed its CRC. The self-test reads these words, kept as `watch::LOOKING`, `NOT_FITTED` (and `ABSENT`) and `NO_READING`: the first is `not_checked`, the second `not_fitted`, the third a `fail` (#1324)); the feel, CO2 and PM2.5 |

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
`LAMP_AIR_FACTS=on` in its unit (off unless set; `crates/voice/src/air_told.rs`;
on lamp-4ace since 2026-10-14, 18:46, when the room read 1,560 ppm, "stuffy";
first told at 19:45 that evening, beside a played request for a story, and
the model ended the story with "By the way, the air here is getting pretty
stale; should we open a window for some fresh air?"):
it reads `air/state` every five seconds; once the air has felt stuffy (or
very stuffy) for ten minutes, the model is told so beside the next thing a
person says to the lamp, in a room that is not busy, with leave to suggest
a window once if it fits; not again for three hours. A reading older than
30 s counts as no reading. The quiet droop and the light a shade dimmer
are the Stuffy air skill (`skills/stuffy-air`, #1021, `docs/skills.md`),
off unless installed and not yet run on a lamp.

## Not shown

- The PM figures against a reference instrument: one sensor, one office.
- What a power cut does to the sensor's measuring state (it is started
  again after 15 s without a reading; not seen on a lamp yet).
