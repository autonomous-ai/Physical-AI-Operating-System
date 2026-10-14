# Ears: the room microphone

`lamp-ears` (`crates/ears`) listens to the lamp's room microphone and says
two things on the bus: how loud the room is against its own quiet, and when
a sound rises suddenly out of that quiet. It decides nothing and keeps no
sound: only levels leave it. With it stopped the lamp talks on as before.

## Why a part of its own

The lamp has two microphones ([microphone-topology.md](microphone-topology.md)).
The voice uses the second (mic2, the Jieli pair), whose chip filters echo,
steers its beam and evens loudness out: good for speech, useless for how
loud anything is. The first (mic1, the plain capsule on the C-Media
adapter in the front of the base) has none of that. V1 used it for the
room (noise, rain, thunder); nothing in V2 read it until now. Two things
the owner asked for need it:

- **Noise.** A knock, a clap, a door, something dropped: a living thing
  hears it and glances up, then lets it go (the repository's rule 8, and
  V1's ladder for a loud noise, kept as cases in
  [v1-verdicts.md](v1-verdicts.md), sheet HAL-3.5).
- **Who is close.** mic1 rose only 2 to 8 dB over its resting -51.5 dBFS
  for colleagues talking in the office, and heard the owner at the desk
  plainly ([brain-build.md](brain-build.md), "Two things follow for the
  architecture"). If that holds, a voice that is loud on mic1 is a voice at
  the lamp: the sign the busy-room rule lacks when nobody faces the camera
  ([attention.md](attention.md)).

The voice keeps mic2 to itself; the two parts never share a device.

## What it says

| Topic | Type | Kind | What |
|---|---|---|---|
| `ears/level` | `RoomSound` | latest, every 100 ms | The last 100 ms in dBFS; the room's quiet (the tenth percentile of the last 30 s heard while the lamp was not speaking; none for the first two seconds); whether the lamp's own voice is in it |
| `ears/sudden` | `SuddenSound` | event | Its loudest 20 ms, the quiet it rose out of, how long it stayed 10 dB over the quiet in the 300 ms it was watched, whether that was under 200 ms ("short": a knock, a clap) or not (a voice, a chair, music), and how long ago it began |
| `diagnostics` | `Diagnostic` | every second | `ears`: ok, or an error with why the microphone does not open; the quiet now, how many sudden sounds, overruns |

**When a sound is sudden.** A 20 ms block at least 30 dB over the quiet
(`--rise-db`, 10 to 60) and at least -40 dBFS, after 200 ms that averaged under 10 dB over the quiet.
It is watched for 300 ms and then said. After one, none for a second.
While the voice says it is speaking (`voice/state`, if no older than 3 s)
and for half a second after, nothing is sudden and the quiet is not
learned: the lamp's own voice reaches mic1 at -13 to -17 dBFS.

These numbers are first choices except the 30 dB: with 20 dB, the office at
work gave 2 to 7 sudden sounds a minute between 16:15 and 16:33 on
2026-10-14 (52 in 18 minutes, all but 7 short, peaks -28 to -33 dBFS:
keys, cups, the start of a word); a glance at those would be a twitch, not
a living thing. One of the 52 rose more than 30 dB (-18.1 dBFS out of
-52.7).

## On the lamp

    lamp-ears --device plug:device_micro1 --rise-db 30 --bus-root /run/lampos-bus

Unit `lamp-ears.service` (in the first builder's deploy folder for
lamp-4ace). `lamp-ears check FILE...` runs the same listening over
recordings (16 kHz, one channel, 16 bits) and prints each sudden sound.

**First look, lamp-4ace, 2026-10-14, 16:13 (first builder, build
`b81c2df7…` of `bb4ad51e4`).** It reads 50 blocks a second with no overrun,
at 1.3 % of a core and 6 MB. The office's quiet on mic1 was -53 to -54
dBFS; twenty seconds of the office at work just before gave -50.7 at the
tenth percentile, -44.3 at the median and -32.6 at the loudest 100 ms.
macOS's "Bottle", "Pop" and "Glass" played from the office iMac about a
metre away: 2 of 6 were said as sudden (peaks -32.4 and -32.8, both short);
those sounds peak about -37 to -39 dBFS over 100 ms at mic1, at the edge of
the first rule of 20 dB and under the present one. A "Hey Lamp" played from the same place reached about -37 to -45
dBFS: no louder at mic1 than the office's own talk.

**Who is close, with people (2026-10-14, 17:12 to 17:42).** With the owner
at lamp-4ace and colleagues around it, the room microphone's rise over its
quiet in the 4 s before each line the voice heard (19 lines, the lamp's own
voice left out): the owner speaking to the lamp at the desk, 10 lines, all
answered, **19.5 to 26.5 dB**; people farther off, 4 lines at 17:41, none
answered, **9.8 to 15.1 dB**; 4 lines at 17:29 to 17:30, not answered,
14.7 to 23.5 dB (likely someone at the desk talking to someone else); one
word at 17:21, 8.9 dB. So the level says someone is close, not that they
are speaking to the lamp: a sign to put beside the name, the words and a
face, not one to decide by. The camera reported no face during the whole
half hour. One session, one office; not a rate.

## What exists, and what is taken

From general knowledge, not a fresh survey:

- **Level and onset** (energy over a running floor, as here): what every
  sound-level meter and V1's environment sensing do. Cheap, explainable,
  and it cannot tell a knock from a word.
- **Sound event classifiers**: Google's YAMNet (521 classes from AudioSet,
  about 3.7 million weights) and PANNs; consumer products use the same idea
  for glass breaking and alarms (Alexa Guard, Nest's sound detection). They
  would name what was heard ("door", "dog", "speech"). A later step if a
  reaction needs to know what it was.
- **Direction**: needs two or more capsules with their raw sound. mic2's
  pair is behind its chip; mic1 is one capsule. No direction here.

Taken: level and onset now; a classifier only when a behaviour needs one.

## Not shown

- Whether mic1 tells a person at the lamp from colleagues across the room
  in general: one half hour with people (above) separated the owner at the
  desk from people farther off, and not from someone at the desk talking to
  someone else.
- Any reaction: nothing listens to `ears/sudden` yet. The glance is
  Expression's, behind a switch, next.
- The thresholds: chosen, then to be set from what the lamp hears in the
  office.
