# Each person's memory kept apart on lamp-52e6, 2026-10-15: raw readings

Five spoken turns on lamp-52e6 between 11:48 and 11:51 +07, the voice built
from `19e94334c` (ARM64 run 38679434978, `lamp-voice` SHA-256
`b8ebd5960699b3b24c0ea4ae2394130e60bdeeb50fcc616ec896a49d2b0dcb7b`, installed
under `/opt/lampOS/voice/<sha256>/`), lent by the fifth builder for the test.
The voice ran lamp-52e6's own unit with one drop-in: that binary,
`LAMP_MEMORY_DAYS=on` (as lamp-4ace runs) and
`LAMP_EYES_WHO_SOCKET=/run/lampos-eyes/who.sock`. The memory was lamp-52e6's
own (`eea06c64…`, `--memory-days`), unchanged.

**What was not real: the face.** lamp-52e6's eyes do not yet remember faces,
and the test needs two people the lamp knows. A small shell loop on the lamp
stood in for the eyes: once a second it wrote the name in
`/tmp/seventh-who` to `/run/lampos-eyes/known` (removed the file when that
was empty), exactly the file the eyes keep fresh while a known person sits
there. The speech was real: `say -v Daniel` from a Mac's speaker beside the
lamp, heard through the lamp's microphone, ears and model.

| Turn | Known | Said | Function | Answer |
|---|---|---|---|---|
| 1 | Minh | "Hey Lamp, please remember that my locker code is four one seven." | remember (30 ms) | "Got it, locker code 417 saved." |
| 2 | Sam | "… my locker code is nine nine two." | remember (22 ms) | "… code 992 saved." (said twice: the first was not heard) |
| 3 | Sam | "Hey Lamp, what is my locker code?" | recall (2 ms) | "You told me your locker code is 992." |
| 4 | Minh | "Hey Lamp, what is my locker code?" | recall (3 ms) | "You told me your locker code is 417." |
| 5 | nobody | "Hey Lamp, what is my locker code?" | recall (2 ms) | "I don't remember you telling me your locker code." |

After the turns the store held Minh's kept line only in
`p-be9688998f085098` and Sam's only in `p-e96e02d8e47f2a7c` (`p-` and the
first 16 hex digits of SHA-256 of the lower-cased name), `everyone` none of
either, and each known person's exchanges in their own journal
(`memory-status.json`).

- `voice-journal.txt`: the voice's own lines from the journal for the run.
- `transcripts.jsonl`: from the session's event record, the lamp's own words
  and the test's spoken lines; everything else the microphone picked up in
  the room is left out.
- `memory-status.json`: the memory's `status` for every person after turn 5.

Afterwards the drop-in was removed, the memory store put back from the copy
taken before the test, and `lampos.target` restarted on its recorded units.

Not shown: real faces through the eyes; two people in one conversation
without a pause; lamp-52e6's legacy memory client (`LAMP_MEMORY_DAYS` unset),
covered only by the host tests.
