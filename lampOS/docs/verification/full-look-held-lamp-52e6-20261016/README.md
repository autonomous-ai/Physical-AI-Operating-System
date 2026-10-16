# A full look at things held up by a person, lamp-52e6, 2026-10-16

lamp-52e6 was lent by the fifth builder from 07:44 to 08:01 and from 08:22 to 08:26 +07. `lamp-voice` and `lamp-eyes` were built on the development Mac from `1a515c39f` with `cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36`. The head `523e32b52` differs from that build only by a comment. Voice SHA-256 `5a055c6a3fb899181d0db8e7c5fd286949eb00a659b87003824ef1b931140e2d`, eyes SHA-256 `ddbe583c9948cde166ad541760051f317853a57675cb622a2f05ce03cfe06200`.

Each part ran from lamp-52e6's own unit with one drop-in:
- voice: `LAMP_LOOK_FULL=on`, `LAMP_EYES_FULL_LOOK_FILE=/run/lampos-eyes/look.jpg`, and a copy of `provider.json` that names `lampOS/deploy/character/lamp.txt`. lamp-52e6's own instruction file says the lamp cannot see.
- eyes: `--full-look 1280x960 --full-look-file /run/lampos-eyes/look.jpg`.

The camera opened at 1280 by 960 in 52–139 ms.

**How.** A person sat at the lamp. For each of ten rounds the MacBook played a chime. Within 6 s the person held one thing up in front of the lamp, and the MacBook's speaker said "Hey Lamp, what am I holding?" (`say -v Daniel`). The office was noisy and other people were talking.

**What happened**

| Round | Look asked | Still | Ask → still | What the lamp said |
|---|---|---|---|---|
| 1–4 | no | — | — | Not an answer to the question: talk from earlier speech in the room (calculators' history) |
| 5 | yes | 59,626 B | 862 ms | "a small white note… your name is written on it" |
| 6 | yes | 58,272 B, 1024×768, made in 71 ms | 833 ms | not captured: the turn was cut by speech and the log read the previous answer |
| 7 | yes | 60,822 B | 872 ms | "a black power adapter" |
| 8 | yes | none: "a full look asked for with the camera shut" | 2,601 ms (gave up) | "I can't quite make out what you're showing me" |
| 9 | yes | 62,016 B, 1024×768, made in 71 ms | 673 ms | "a yellow, U-shaped plastic object" |
| 10 | yes | 60,826 B, 1024×768, made in 71 ms | 773 ms | "a small green tool, it looks like a screwdriver" |

The person confirmed that every answer given from a still named what they were holding. In rounds 1–4 the lamp took other people's speech for the question; the person put those down to the noisy room.

**Readings**
- Ask → still was 673–872 ms in five of five rounds that had a still, measured from `look_full_asked` to `look_full_still` in the voice's events. The 0.3 s aim stays open.
- With no still (round 8), the lamp said it could not make the thing out rather than guess.
- Every still fitted 64 KiB at 1024 by 768.

**Not shown in this run.** The head turning toward the person: `lamp-body` was down for the whole run. It stopped at 07:55:48 and refused restart with "base_yaw already carries load 212". So there were no joint states, and every still was taken at once ("the head waited for 0 ms").

**Second run, 08:22–08:26: the head turns first.** `lamp-body` was back on the fifth's unit (`04e02caf`, PR #952). Same build, same drop-ins, same question from the MacBook. The person sat well to one side of the lamp. Three rounds, three looks asked, three stills taken; the person saw the head turn to them.

| Round | Head waited | Ask → still | What the lamp said |
|---|---|---|---|
| 1 | not logged | 953 ms | "Let me look." (the answer came after the round's window) |
| 2 | 836 ms | 1,011 ms | "a small, dark cone-shaped tool with an orange band on it" |
| 3 | 366 ms | 1,009 ms | "a light pink wireless computer mouse" |

With the head turning, ask → still is about 1 s, because the settle wait (366–836 ms) comes before the frame.

**Not shown:** question → first word, per request.

**Privacy.** No picture left the lamp. `/run/lampos-eyes/look.jpg` was deleted at the end, and no copy was made. The answers above are the lamp's own words; the person is not named.

**Restored** at 08:01, and again at 08:26 after the second run (the fifth's body drop-in left as it was). The drop-ins, the provider copy and the character copy were removed, and voice and eyes were restarted on their recorded units. `lamp-body` was not touched.
