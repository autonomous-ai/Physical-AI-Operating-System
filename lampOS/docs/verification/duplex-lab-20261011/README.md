# Duplex lab results, 2026-10-11

Every closed-loop trial behind `docs/duplex-listening.md`, failed ones
included. These files hold block numbers and outcomes. They hold no audio
and nothing that was said. The recordings they were computed from are
private and stay on the office Mac.

> **Read with the correction of 2026-10-12** in
> `docs/duplex-listening.md` and
> `docs/verification/onset-retention-20261012`: on `4ace` the gain levels
> below are at least 10 dB louder than a person over the lamp, because
> the microphone's hardware gain control was at a higher gain when the
> prompts were recorded than when the replies were.

## Files

For each session and part there are three files.

| File | Holds |
|---|---|
| `NAME.summary.txt` | What the evaluation printed |
| `NAME.negatives.tsv` | One row for each time the lamp stopped for its own voice: reply, block, outcome, pause |
| `NAME.trials.tsv` | One row for each synthetic overlap: level, seed, reply, onset, first stop and its delay, first outcome, commit and its delay. `never` is an overlap that was not heard. `excluded` is a trial in which an earlier stop in the same reply had already moved the clock |

| Name | Session | Lamp | Part |
|---|---|---|---|
| `six-dev`, `six-held` | `lamp-4ace-varied-12io918n` | `4ace` | even replies, odd replies |
| `ten-dev`, `ten-held` | `lamp-rust-v1-ten-evidence-20261011T070623Z`, `trial-01-ns-off-target` | `a0ae` | even replies, odd replies |
| `ext-all` | `lamp-connected-extended-20261011-01` | `4ace` | development, all replies |
| `plain-all` | `lamp-connected-plain-20261011-01` | `4ace` | held-out, all replies |
| `ns-six-dev`, `ns-ext-all` | as above | `4ace` | development only, software noise suppression on, three levels |

Blocks are 10 ms. Delays are milliseconds from the first block of the
prompt's speech. A reply's number is its position among the session's
stretches of playback of 1.5 s or more.

## Inputs

SHA-256 of the two files each session's lab directory is made from.

| Session | `capture-audio/pre_aec.pcm16le` | `render-audio/render_accepted.pcm16le` |
|---|---|---|
| `lamp-4ace-varied-12io918n` | `b32af9dfa437ac6ce3f2c188c01edb3314c0faa962aa91b1f8bb1d3b01959d94` | `27409c4fc7b4513a95a25655b8e538bcb4c24ebbced8193128b6588ff1cecf88` |
| `lamp-rust-v1-ten-evidence-20261011T070623Z` | `b6b5de3c624bbeea1121915aea95172d764701cf9c39f4036c4b07b19aaeeaf7` | `b8a590917f6793dcc8af1738680ce620c3cd10b5b69d7636a1587b33018ea973` |
| `lamp-connected-extended-20261011-01` | `7797c6f2be19f6b1ddf52e5dec5557d51496740046b46dba990ac6b3be76cfde` | `f4c675a7ecd496550dd9f6cb2cd56484d1b322e72aef91d37685340774fe2ff5` |
| `lamp-connected-plain-20261011-01` | `d5c759817d21fefc3f54a97b58888dd8cdea75ce1088107d8ee55ff364da8325` | `1d97ae8411cb88f87c06e66678753bcd967b8f535dfc1f900dda316309ab21fb` |

## What ran

The decision is `Hearing::EVALUATED` and `Confirmation::EVALUATED` as
committed at `feb560767`. The lab is
`crates/live/examples/duplex_lab.rs`, release build.

| Binary | Built from | SHA-256 |
|---|---|---|
| Frozen evaluation | `feb560767` | `d594d5496bd5ba9cfc39118bf74851ad510ca14790b156c39c4b984dfbcdc0be` |
| Noise suppression comparison | `5ca2cd081`, which adds only the lab's `--noise-suppression` switch | `727adfb5e6ce80485712bd84c7698e7a09d987efab76a446221237870c2615b7` |

The drivers are Python 3.12 with NumPy and are not in Git. They are in
`m050-handoff/` of the session scratchpad on the office Mac, with
`manifest.sha256`. They prepare a lab directory from a diagnostic bundle
(`prepare_lab.py`), choose where each overlap goes and which recorded
prompt it uses (`mix.py`), run the lab once per trial and write these
files (`evaluate.py`), and pool them (`pool.py`).

```sh
python3.12 -I prepare_lab.py BUNDLE/runtime lab/NAME
duplex_lab lab/NAME lab/NAME/base
for g in 6 0 -6 -12 -18; do for s in 1 2 3; do
  python3.12 -I mix.py lab/NAME lab/NAME/base lab/NAME/near_${g}_s$s/near.pcm16le \
    lab/NAME/near_${g}_s$s/truth.tsv $g $s
done; done
python3.12 -I evaluate.py duplex_lab lab NAME dev|held|all OUT_PREFIX
```

The same inputs and binary give the same files: the lab reads no clock and
the overlap positions come from a seeded generator.

## Order of events

1. Thresholds chosen on `six-dev` and `ten-dev`, open loop, negatives and
   positives together.
2. Closed loop built and run on `six-dev`, `ten-dev` and `ext-all`.
3. Rule frozen at `feb560767` without change.
4. `six-held`, `ten-held` and `plain-all` run once. `plain-all` had not
   been opened for any decision before this.
5. Noise suppression comparison, development material only.

## Paced check

`paced/` holds what `crates/live/examples/reference_pace.rs` printed, one
file a run: two host processes, the real clock and scheduler, no audio
device. Runs in a round were started together so that they share the
host's stalls.

| Round | Built from | Ring | Wait | Binary SHA-256 |
|---|---|---|---|---|
| 1 | `feb560767` | eight blocks | sleep | `42472bded7e957e9a26ec7d893dbcc2f37d067f9edccfc24103bda656ab96a46` |
| 2 | `b09f69767` | ten blocks | sleep | `db32630e534a44dcb22c4875441e243e4a8372b421a60b6a68dfc30630a2d98f` |
| 3 | `bc2f1a7df` | ten blocks | spin | `3444578abc7d76662494b8d887becbafba641cf4745c773b9b17be05e07dd79f` |

`capture_blocks` is how many 10 ms blocks ran before `fault`. No run
reached `seconds_asked`; `docs/duplex-listening.md` says why.
