# Rules a person gives, second run on lamp-52e6, 2026-10-15: raw readings

After the fixes for [the first run](../rules-lamp-52e6-20261015/) and the
reviewer's findings. lamp-52e6, 18:55 to 18:58 +07, lent by the fifth
builder. `lamp-voice` and `lamp-memory` built from `b7a4dd344` on the
development Mac with `cargo zigbuild --release --target
aarch64-unknown-linux-gnu.2.36` (SHA-256
`e46093f96ce7651a259a500ece6b2d8c83f5c7867bb66cc19164f719857f1d97` and
`d25d188dc60f07af06dad62d414fd4026251b8ada4f08ab712ec56cd49301251`), each from
lamp-52e6's own unit with one drop-in (the voice's also sets
`LAMP_MEMORY_DAYS=on`, `LAMP_MEMORY_RULES=on`, `LAMP_EYES_WHO_SOCKET`). The
known person was a stand-in (a loop writing `/run/lampos-eyes/known`);
speech was real (`say -v Daniel` from a Mac's speaker, a busy office). The
memory store was copied before and put back after.

| Step | Known | Said | What happened |
|---|---|---|---|
| 1 | Minh | "Hey Lamp, from now on please keep your answers to one short sentence." | `keep_rule` (30 ms): "I'll keep my answers to one short sentence from now on." (the first run kept nothing here) |
| 2 | Minh | "… please stop calling me buddy, I don't like it." | `keep_rule` (29 ms), kept as "Never call you Bobby." (the model heard "Bobby") |
| 3 | Minh | "… never tell me the weather unless I ask for it." | `keep_rule` (41 ms): Minh's rules [1] short answers, [2] Bobby, [3] weather |
| 4 | Minh, a new session (voice restarted) | "Hey Lamp, tell me about the moon." | **one sentence, unprompted by anything but the rule**: "The moon is Earth's only natural satellite and it always shows us the same face." |
| 5 | Minh | "… you can tell me the weather again, forget that rule." | `forget_rule` (33 ms): **rule [3] (weather) removed**, [1] and [2] stay (the first run removed the wrong one) |
| 6 | Sam (a new session) | "Hey Lamp, what have I asked you not to do?" | `list_rules` (2 ms): "You haven't asked me to avoid doing anything." |
| 7 | Minh (a new session) | same | `list_rules` (2 ms): "You've asked me to keep my answers to one short sentence and never call you Bobby." |

- `journal.txt`: the voice's and the memory's own lines (requests, scopes, counts; no words of what was kept).
- `transcripts.jsonl`: the lamp's own words and the test's spoken lines.
- `memory-rules-topic.txt`: the kept `memory/rules` value at the end.

Not shown: a rule about speaking up read by the Policy (the second builder's
side); a seventh rule refused on the lamp (host test only); the rule-linked
journal deletion on the lamp (host and real-store tests only).
