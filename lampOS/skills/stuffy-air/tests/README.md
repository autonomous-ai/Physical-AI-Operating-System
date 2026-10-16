# Stuffy air's tests

**None of these has run.** They wait for the runtime's test command
(`lamp-skills test`, #938) and for recordings of the lamp's bus, made on a lamp
with its owner's yes (`lamp-bus record`): `air_state.jsonl`,
`humans_faces_tracked.jsonl` and `lamp_pause.jsonl` in each case's folder. A
stuffy spell can be made by closing the door of a small office; lamp-4ace's
office reached 1,870 ppm on 2026-10-14.

| Case | The recording it needs |
|---|---|
| `acts-when-stuffy-ten-minutes` | CO2 above 1,200 ppm for 25 minutes, a person at the desk |
| `quiet-on-a-short-rise` | CO2 above 1,200 ppm for 4 minutes, then under |
| `quiet-when-nobody-there` | CO2 above 1,200 ppm for 25 minutes, nobody at the desk |
| `once-per-stuffy-spell` | CO2 above 1,200 ppm for 50 minutes, a person at the desk |
| `quiet-when-paused` | CO2 above 1,200 ppm for 25 minutes, a person at the desk, the lamp paused |
