# Each person's memory with a real face on lamp-52e6, 2026-10-15: raw readings

The same build as [the first run](../memory-per-person-lamp-52e6-20261015/)
(voice `b8ebd596…` from `19e94334c`, `LAMP_MEMORY_DAYS=on`), now with the
eyes from the same ARM64 run (`lamp-eyes` SHA-256
`9ed93ede3af8824e5fb01af9cd4dd20e3335cbec3b315aa57706ce187f446c4a`) given
`--recogniser` (OpenCV SFace, SHA-256 `0ba9fbfa…` checked), `--people`,
`--who-socket` and `--known-file` as lamp-4ace runs them. A real person sat
at the lamp; the speech came from a Mac's speaker (`say -v Daniel`), so the
face and the voice were two different sources. The name given was a
stand-in, "Minh". 12:06 to 12:23 +07. The office around was talking.

| Step | At the lamp | Said | What happened |
|---|---|---|---|
| 1 | the person | "Hey Lamp, I am Minh, please remember my face." | `remember_my_face`, eyes answered in 1,370 ms: "I'll know you now, Minh." (two earlier tries were not answered: the end of speech was cut, then no answer) |
| 2 | Minh (known file fresh) | "… remember that my locker code is four one seven." | kept in `p-be9688998f085098` only; `everyone` none |
| 3 | nobody (the person stood away; no known file) | "Hey Lamp, what is my locker code?" | two tries held back by the busy-room rule; the third: "It's 417." **without calling recall** |
| 4 | Minh again | same question | "Your locker code is 417.", also without recall |
| 5 | Minh | "Hey Lamp, please forget my face." | `forget_my_face`, 11 ms; `people.json` empty |

**What step 3 shows.** The memory kept the code apart, but the provider
session was the same one in which Minh had kept it four minutes earlier, so
the model answered from the conversation it was in, not from the memory.
Keeping each person's memory apart does not reach what is still in the
session's own context. Not mended here; a question on #764.

- `journal.txt`: the voice's and the eyes' own lines for the run.
- `transcripts.jsonl`: the lamp's own words and the test's spoken lines.
- `memory-status.json`: the memory's `status` after step 5.

Afterwards the face was forgotten, both drop-ins, `/var/lib/lampos-eyes` and
the model file removed, the memory store put back from the copy taken
before, and `lampos.target` restarted on its recorded units.
