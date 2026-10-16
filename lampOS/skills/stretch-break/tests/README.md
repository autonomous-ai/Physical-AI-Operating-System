# Stretch break's tests

**None of these has run.** They wait for the runtime's test command
(`lamp-skills test`, #938) and for recordings of the lamp's bus, made on a lamp
with its owner's yes (`lamp-bus record`): `humans_faces_tracked.jsonl`,
`voice_state.jsonl` and `lamp_pause.jsonl` in each case's folder.

| Case | The recording it needs |
|---|---|
| `acts-after-50-minutes` | A person at the desk for 55 minutes, looking away twice for under a minute |
| `quiet-when-short-sitting` | A person at the desk for 40 minutes, then leaving |
| `quiet-until-they-leave` | A person at the desk for 3 hours without leaving |
| `quiet-while-talking` | A person at the desk for 55 minutes, talking with the lamp from minute 48 to 56 |
| `quiet-when-paused` | A person at the desk for 55 minutes, the lamp paused throughout |
