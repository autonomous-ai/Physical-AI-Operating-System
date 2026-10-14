# The early-end probe on lamp-4ace, 2026-10-14, 23:10

The first builder ran `early_end_probe` (built from `d95287bd5` for the lamp, SHA-256
`2fbd7c91…`) once, as root, from lamp-4ace's network: the five recordings made with
macOS `say -v Daniel` as `docs/early-end-taken-back.md` says (1.2 to 2.1 s each), and
a provider file reduced to the six fields the probe takes (`endpoint`, `model`,
`voice`, `credential_file`, `authentication`, `language_code`) from the voice's own,
written private to root on the lamp, because the probe refuses the voice's file's
other fields. 15 requests in 95 s. `probe.jsonl` is its output as printed: timings
and the service's own words, no handle, credential or path.

| Trial | The answer | Resumed, and how long setting up took |
|---|---|---|
| `control` | "You asked me to remember orange and purple." | no |
| `resume` | "You asked me to remember the words orange and purple." | yes, 2,209 ms |
| `resume-late` | "You asked me to remember orange and purple." | yes, 2,360 ms |
| `whole-resumed` | "The capital of Japan is Tokyo." | yes, 2,134 ms |
| `whole-labelled` | "The capital of Japan is Tokyo." | no |

Read by the document's table: (a) does not hold, since a session resumed from the
handle given before the piece still named purple, and a resumed setup took over two
seconds; (b) holds, since the whole question with its note was answered as one and
"the capital of" was not answered as a question of its own. One run.
