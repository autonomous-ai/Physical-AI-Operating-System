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
   **6 of 10** in the trial they belonged to (key, book, scissors, glasses,
   apple, umbrella), plus one **ambiguous**: "a yellow banana" was captured
   in the headphones trial's window, late, with no request id or timestamp
   kept to tie it to the banana trial, so it is not counted. Cup, phone and
   headphones got no answer bound to their trial. Pictures 59.8–65.1 KB.
   `looks-after.txt`. The before run's 4 of 10 was scored the same way.

**Time** (`look-timing.txt`, the session's own events). From the voice's
`look_full_asked` to its `look_full_still`: 622–1,060 ms (median about
800 ms). That is **ask to the coordinator accepting the still**, not ask to
capture: the coordinator records `look_full_still` only once the lamp's
previous reply ("Let me look.") has ended and the next turn may start, so a
still already taken waits there. The eyes logged a head wait of 0 ms for each
full look in `eyes-journal.txt`, so the head's settle was not the cause; the
capture's own time was not kept. From "Let me look." to the first word of the
answer: 2.0–3.6 s in seven looks; in three, no answer came before the next
question (about 32 s later). Per-request camera-ready and first-word
timestamps were not kept, so these are not yet comparable with the issue's
~300 ms aim. The eyes' working cost at 1280x960 was 87 ms a look (64–81 ms
at 320x240).

Not shown: objects held up by a person, the head turning toward a person
(nobody was in view, so it did not turn), lamp-4ace.
