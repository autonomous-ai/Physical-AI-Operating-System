# A new session when the known person changes, on lamp-52e6, 2026-10-15: raw readings

Four spoken turns on lamp-52e6 between 13:01 and 13:04 +07 with the voice
built from `a858fa0e9` on the development Mac with `cargo zigbuild --release
--target aarch64-unknown-linux-gnu.2.36 -p lamp-voice` (SHA-256
`f0ec94c1fcbda3474909e483093ec01e7bf6846e0d5aa13aac6a9c59b78eceb4`; the
ARM64 run's binary was not ready yet; the first lamp run of such a build),
under `/opt/lampOS/voice/<sha256>/`, lent by the fifth builder between the
sixth's runs. The voice ran lamp-52e6's own unit with one drop-in: that
binary, `LAMP_MEMORY_DAYS=on` and `LAMP_EYES_WHO_SOCKET`. Only `lamp-voice`
was restarted; the eyes, recording for #763, were left alone.

The face was a stand-in, as in
[the first #764 run](../memory-per-person-lamp-52e6-20261015/): a loop
writing `/run/lampos-eyes/known` once a second (lamp-52e6's eyes build does
not write that file). The speech was real (`say -v Daniel` from a Mac's
speaker).

| Turn | Known | Said | What happened |
|---|---|---|---|
| 1 | Minh (appeared after the start) | "Hey Lamp, please remember that my locker code is four one seven." | new session opened between turns ("the person at the lamp has changed"); `remember` into Minh's scope: "Sure! I've kept your locker code safe for you." |
| 2 | nobody, for 40 s | "Hey Lamp, what is my locker code?" | new session opened, told nobody is there: "I only remember things you've asked me to keep track of." No 417. |
| 3 | Minh again | same | new session with Minh's material; `recall` (6 ms): "It's 417." |
| 4 | nobody for 10 s, then Minh | "Hey Lamp, and what was the code again?" | no new session (under the 30 s for someone gone): "It's 417." |

Before this change, step 2 was answered "It's 417" from the open session
([real-face run](../memory-per-person-real-face-lamp-52e6-20261015/)).

- `voice-journal.txt`: the voice's own lines for the run.
- `transcripts.jsonl`: the lamp's own words and the test's spoken lines.
- `memory-status.json`: the memory's `status` after turn 4.

Afterwards the drop-in was removed, the memory store put back from the copy
taken before, and `lamp-voice` restarted on its recorded unit.

Not shown: a real face for this change; two different known people one
after the other (A to B); a change arriving while someone is speaking.
