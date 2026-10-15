# A full look on lamp-52e6, 2026-10-15: raw readings

lamp-52e6, lent by the fifth builder, three runs between 21:00 and 21:45 +07.
`lamp-voice` and `lamp-eyes` built on the development Mac with `cargo zigbuild
--release --target aarch64-unknown-linux-gnu.2.36`; the final run from
`4956b840f` (SHA-256 voice
`e9f609aad8c9941813b75d3ac355ab9d488601c28322fbd2323c99009520eed4`, eyes
`66a0b0f0935f56d75ee8fd1b0527a6476ea98fe8ccf288454bbfb5a03808f8b5`), each from
lamp-52e6's own unit with one drop-in (voice: `LAMP_LOOK_FULL=on`; eyes:
`--full-look 1280x960`).

**The character.** lamp-52e6's own instruction file is from 2026-10-14 and
says the lamp cannot see, so it never asked to look. For every run the voice
read a copy of `provider.json` naming `lampOS/deploy/character/lamp.txt` from
`lamp-voice` (the "Let me look." rule), for the before run too, so before and
after differ only in the build.

**The objects** were ten pictures (cup, key, phone, book, banana, scissors,
glasses, apple, umbrella, headphones), each drawn full-screen on the MacBook
the lamp faces from about a metre, side-on; "Hey Lamp, what am I showing you?"
was said from the MacBook's speaker for each (`say -v Daniel`). The office was
busy, and other speech reached the lamp between turns; such answers are not
counted. No hands; nobody in front of the lamp.

**Runs**

1. Before (the recorded build, the 320x240 working frame): named 4 of 10
   (book, scissors, glasses, umbrella; the umbrella after first saying a
   bicycle; the banana as "a single yellow curved line"; headphones as a
   clothes hanger). Pictures 15.9–16.7 KB. `looks-before.txt`.
2. `5f7387b90`: the camera never opened at 1280x960. "InvalidData None": the
   camera crate refused any height over 720 before touching the device.
   Mended in `b3e7dee42`.
3. `b3e7dee42`: "InvalidData … stage: SetFormat": the camera answered 1280x960
   MJPG with a 2,457,600-byte buffer (probed with `v4l2-ctl`), over the
   crate's 2 MiB frame bound. Mended in `4956b840f`.
4. After (`4956b840f`): "camera open, 1280 by 960" 0.25 s after start. Named
   **7 of 10** (key, book, banana, scissors, glasses, apple, umbrella); cup,
   phone and headphones got no answer within the run's window. Pictures
   59.8–65.1 KB. `looks-after.txt`.

**Time** (`look-timing.txt`, the session's own events): the eyes' still came
622–1,060 ms after the voice asked for it (median about 800 ms), over the
issue's 300 ms aim; most of it is the wait for `body/joint_states` to settle
(2 s cap) and the next 1280x960 frame at 10 fps. From the lamp's "Let me
look." to its first word of the answer: 2.0–3.6 s in seven looks; in three the
answer did not come before the next question (about 32 s later). The eyes'
working cost at 1280x960 was 87 ms a look (64–81 ms at 320x240,
`eyes-journal.txt`).

Not shown: objects held up by a person, the head turning toward a person
(nobody was in view, so it did not turn), lamp-4ace.
