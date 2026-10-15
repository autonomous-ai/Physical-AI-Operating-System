# Rules a person gives, kept and obeyed, on lamp-52e6, 2026-10-15: raw readings

lamp-52e6, 17:37 to 17:40 +07, lent by the fifth builder. `lamp-voice` built
from `d381ef9c2` on the development Mac with `cargo zigbuild --release --target
aarch64-unknown-linux-gnu.2.36` (SHA-256
`153a934d3b032e32843451cb80d2bb4b0281caaa680360782b629ceed1b6b829`), run from
lamp-52e6's own unit with one drop-in: that binary, `LAMP_MEMORY_DAYS=on`,
`LAMP_MEMORY_RULES=on` and `LAMP_EYES_WHO_SOCKET`. The known person was a
stand-in (a loop writing `/run/lampos-eyes/known` with "Minh" or "Sam");
speech was real (`say -v Daniel` from a Mac's speaker, a busy office). Only
`lamp-voice` was restarted; the memory store was copied before and put back
after.

| Step | Known | Said | What happened |
|---|---|---|---|
| 1 | Minh | "Hey Lamp, from now on please keep your answers to one short sentence." | "Got it, I'll stick to short, single-sentence answers." — **no `keep_rule`**: obeyed in that session only |
| 2 | Minh | "Hey Lamp, a rule for you: never tell me the weather unless I ask for it." | `keep_rule` (27 ms): "Understood, no weather reports unless you ask for them." |
| 3 | Minh | "Hey Lamp, please stop calling me buddy, I don't like it." | `keep_rule` (24 ms); Minh's rules: [1] weather, [2] "Do not call Minh buddy." |
| 4 | Minh, after a restart (a new session) | "Hey Lamp, what have I asked you not to do?" | `list_rules` (3 ms): "You've asked me not to share the weather unless you ask for it, and not to call you buddy." |
| 5 | Sam (a new session) | same question | `list_rules` (2 ms): "You haven't asked me to avoid anything." |
| 6 | Minh (a new session) | "Hey Lamp, you can tell me the weather again, forget that rule." | `forget_rule` (38 ms): "You got it…" — **but rule [2] (buddy) was removed and the weather rule stayed**: with no number, `forget_rule` took the rule given last |

`memory/rules` after step 6: `{"person":"p-be9688998f085098","quiet":false,"quiet_mornings":false,"rules":1}`.

Mended in 985dacd2f: `forget_rule`'s default to the
last rule (step 6). Not mended: an implicit "from now on…" (step 1) is
followed in the session but not kept.

- `journal.txt`: the voice's own lines.
- `transcripts.jsonl`: the lamp's own words and the test's spoken lines.
- `memory-rules-topic.txt`: the kept `memory/rules` value after step 6.

Not shown: a rule obeyed unprompted in a later session (for example no
weather in a morning note); the Policy reading `memory/rules`.
