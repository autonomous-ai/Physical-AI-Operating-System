# mic1 live with the capture hold against the earlier live run

Agent 1, 2026-10-12. Read-only analysis of two native Gemini duplex runs
by Main on lamp `4ace` through `mic1` (kept-start, capture-clocked
reference, mixers unchanged). No audio is copied here.

| | Earlier | With the hold |
|---|---|---|
| Run | `lamp-mic1-gemini-executed-o7lfc33c` | `lamp-mic1-gemini-timing-executed-_r4egoic` |
| `lamp-live` SHA-256 | `14fca820…8941e` | `33f2eb21…9800` |
| Lamp sounding: blocks, before and after cancellation, dBFS | 702, -14.0, -15.6 | 614, -13.9, -36.5 |
| Removed | 1.7 dB | 22.7 dB |
| Lamp-sounding blocks the detector scored 0.8 or over | 404 of 702 | 8 of 614 |
| Of them in the reply's first second, and after it | 0 of 75, 404 of 627 | 8 of 76, 0 of 538 |
| Yields | 2, each after the lamp's own voice | 0 |
| Candidates rejected | 2 | 0 |
| The reply: gaps in playback | 2 | 0 |
| A frame processed after its own read: least, median, most | 0.5, 1.0, 5.7 ms | 39.3, 40.4, 46.9 ms |
| Reference's capture count minus the frame's sequence | 0 in every row | 4 in every row |
| The lamp's own margin figure in the trace | none | -33.2 dB |
| Sunset question, lamp silent: level, scored 0.8 or over | -44.9, 134 of 188 | -44.8, 124 of 188 |
| Admissions; input transcripts | 1; the sunset question | 1; the sunset question |

**The lamp no longer yields to itself.** Its own voice is scored as speech
in 8 blocks, all of them 0.6 s into the reply while the canceller is
still settling (output -21.5 and -29.2 dBFS there for 200 ms), and none in
the 538 blocks after. No yield followed those 8.

**The clip over the reply is still not heard.** It was placed 0.29 s after
the reply began. No candidate, no yield and no transcript came of it. At
`mic1` it is expected at -50 dBFS under a lamp voice at -14.5 dBFS.

## Limits

- One run each, with different replies from the service: the reply is not
  the same sound in the two.
- **My stopped VM job overlapped this recording.** Container
  `agent1-arm64-assert45` ran from 07:12:12Z to 07:13:39Z. The room
  recording's file was last written at 07:14:45Z and is 100 s long, so
  about its first half minute has that host load under it. Both prompts
  were delivered with no fault, no gap and the usual 10.7 ms largest host
  gap. Everything in the table is from the lamp's own bundle. The room
  recording is not examined here.
- Per-block echo evidence is not in the bundle; "after the lamp's own
  voice" for the earlier run's yields is from the six blocks before each.

`inputs.sha256`: digests of the files read.
