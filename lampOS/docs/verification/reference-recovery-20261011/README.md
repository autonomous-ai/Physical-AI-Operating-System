# Reference recovery evidence, 2026-10-11

Behind `docs/reference-recovery.md`. Numbers and outcomes only: no audio
and nothing that was said. The recordings and the native run they were
computed from are private and stay on the office Mac.

| File | Holds |
|---|---|
| `native-run03-margin.txt` | The fixed margin over the native continuous run that ended at 378.6 s, from its once-a-second `reference_clock` reports, and the fitted drift |
| `lamp-loop-timing.txt` | How late the capture loop and the reference ran in four recorded lamp sessions: gaps between capture reads and between reference packets, the age of the newest reference at a capture read, microphone backlog |
| `paced/*.json` | What `crates/live/examples/reference_recovery_pace.rs` printed, one file a run: two host processes, the real clock and scheduler, no audio device |
| `capture-clock-cancellation.txt` | Four recorded sessions replayed through the real canceller with the recorded interleave and with the capture-clocked ring |

## Paced runs

Binary built from the tree committed as `5df4248c3` plus the example,
which is `7ebd9af6b`. SHA-256
`a7555ff4d728d60518c8a0ac0a73ea4c12d68f9776db33f49340ff668dccbb0e`.

```sh
reference_recovery_pace SECONDS SKEW_PPM fixed|capture off|bounded [SCRIPT...]
```

| File | Arguments |
|---|---|
| `sender-short` | `30 157.3 capture bounded sound sender-pause=15@5 sender-pause=30@12` |
| `sender-45` | `30 157.3 capture bounded sound sender-pause=45@8` |
| `sender-70-100` | `40 157.3 capture bounded sound sender-pause=70@8 sender-pause=100@25` |
| `capture-short` | `30 157.3 capture bounded sound capture-pause=15@5 capture-pause=27@12 capture-pause=38@20` |
| `capture-70` | `30 157.3 capture bounded sound capture-pause=70@8` |
| `loss-dup` | `30 157.3 capture bounded sound lose=1000 twice=2000` |
| `privacy` | `30 157.3 capture bounded sound lose=800 privacy=8` |
| `exhaustion` | `40 157.3 capture bounded lose=500 lose=1000 lose=1500 lose=2000` |
| `timeout` | `30 157.3 capture bounded sender-pause=2500@8` |
| `off-loss` | `30 157.3 capture off lose=1000` |
| `long-capture-bounded` | `600 157.3 capture bounded sound` |
| `long-fixed-bounded` | `600 157.3 fixed bounded` |
| `long-fixed-off` | `600 157.3 fixed off` |

The three long runs were started together and share the host's stalls.
The ARM gate and then the host test suite were running on the same
machine during them.

## Cancellation comparison

`duplex_lab LAB OUT` against `duplex_lab LAB OUT --capture-clock`, built
from the tree committed as `1d24e3288`, lab binary SHA-256 `fd89feb455130d5d1aa22478d0f00961bd8c7d3c33bfc0b32989b6ef7ef5eb5e`, on the four sessions
whose input digests are in `../duplex-lab-20261011/README.md`.
