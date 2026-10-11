# Agent 1 status

Owner: Agent 1 (Gemini and audio). This file on `lamp-a1-playout` is the
status of `MAIN-042` only. Everything before it, checkpoints 1 to 16 and
requests `A1-001` to `A1-011`, is unchanged on `gemini-voice` at `12caf1b38`
in this same file.

- Updated: 2026-10-11
- Assignment: `MAIN-042`, read from
  `origin/lamp-v2-chat:lampOS/coordination/voice-next-assignments.md` at
  `96eff6b42`. Acknowledged, and delivered as far as offline work goes.
- Work branch: `lamp-a1-playout`, a fresh worktree from the integration code
  you named, `1a089863a`. Nothing from `gemini-voice` was merged into it.
- Tested code: `01b55fb3a` on `lamp-a1-playout`. The branch head adds only
  this file and corrections to the write-up.
- Proposal for your files: `lamp-a1-playout-integration` at `993ebaa63`,
  which is `01b55fb3a` plus one commit.
- Write-up: [continuous reply playout](../docs/playout-continuity.md).
- This file is my report. I also send a short notice on the team channel
  if it accepts one.
- No device, no cloud, no deployment. Room and reply audio stayed local.

## What you asked for, and what there is

| Item of `MAIN-042` | State |
|---|---|
| 1. Read V1's playback at `d5efe9d7b` and compare | Done. V1 has no start buffer and no target lead; its completion rule is the one Rust already has. [Table](../docs/playout-continuity.md#v1-at-d5efe9d7b) |
| 2. Reproduce the four gaps from the frozen traces | Done. Coffee 70 ms, ocean 160 ms, story 140 ms and 70 ms come back at the recorded block and length, and so do the gaps of 22 of 23 replies |
| 3. One opt-in, bounded continuity policy | Done: `StartGate::continuous(Continuity)`. 0 of 23 recorded replies run dry, against 9 today. Cost and misses below |
| 4. The 2.269 s pause | Done. The model's background reasoning after a spoken acknowledgement. [Evidence and one remedy](../docs/playout-continuity.md#the-pause-in-the-story) |
| 5. Integration patch, before and after, cost, checks | Done, on a separate branch. Linux ARM64 is emulated, not the lamp |

## The result in short

Replay of recorded supply. None of this is a device measurement.

| | Current 200 ms gate | Continuity |
|---|---|---|
| Replies that run dry, of 23 | 9 | 0 |
| Zeros accepted mid-reply, worst phase | 1,190 ms | 0 |
| Coffee | 70 ms to 80 ms of zeros | none; first block 100 ms to 110 ms later |
| Ocean | 160 ms to 170 ms | none; first block 190 ms to 200 ms later |
| Story, second generation | 210 ms to 220 ms | none; its own first block 290 ms later |
| Added to a reply's first block | | 137 ms on average, 210 ms at most, nothing for 5 of 22 |
| Least audio in hand at any slot | 0 | 30 ms |

**Where it misses your 200 ms target.** `pilot/1`, a 1.4 s reply with no
late burst, is held 200 ms to 210 ms longer. The story's second generation
is held 290 ms where today it is not held; a 200 ms hold would have left
one or two blocks of zeros at 18.2 s.

**What you should know before choosing it.** A plain 600 ms horizon, held
again for each generation, also runs dry in none of the 23 at the same
average delay, 138 ms. It holds three starts more than 200 ms longer, up
to 240 ms, and leaves 10 ms in hand at the ocean reply's late burst where
continuity leaves 30 ms. The same gate gives either:
`Continuity { margin_us: 0, hold_us: 0, cover_us: 600_000 }` is that
horizon. Holding the later generation again is what both need for the
story.

## Why the gaps happen

From 23 complete replies in eight bundles, 115 s of speech:

1. The service leads real time by 13 ms to 110 ms, measured as the audio
   received before a burst less the time since the first audio, as the
   median over a reply.
2. Twenty-five bursts were 60 ms or more late against that lead: seventeen
   by 89 ms to 172 ms and eight by 295 ms to 306 ms. Nothing in between,
   nothing larger. The eight fell 2.0 s to 18.4 s into a reply.
3. Coffee, ocean and the story's second gap are each one burst of the
   second size. The story's first gap is its own start, which today is not
   held.
4. Three of the four gaps cut speech at -13 dBFS to -21 dBFS on both sides.

## The policy

A generation's first block is handed over when one can be and either the
generation is complete, or both hold:

- **cover**: time since the first audio plus audio in hand is at least
  380 ms;
- **margin**: time since the first audio plus the measured lead is at least
  340 ms, or 290 ms have passed.

A later generation of the same turn is held again once the one before it
has handed over its last block. The longest any start is held is 380 ms.
`Continuity::EVALUATED` holds the three numbers and says where each comes
from. They changed after checkpoint 1, from 320, 280 and 360 ms: with
those the ocean reply rode out its late burst with 10 ms in hand.

## Commits

| Commit | Branch | Files | Content |
|---|---|---|---|
| `68e50dc3c` | `lamp-a1-playout` | `crates/audio/src/playout.rs`, `crates/audio/tests/playout_continuity/{main.rs,replies.tsv}`, this file | The opt-in start, 11 unit tests, the replay and its fixture |
| `d11d5a300` | `lamp-a1-playout` | `playout.rs`, `main.rs`, `docs/playout-continuity.md` | The three numbers sized for a 30 ms reserve; the write-up |
| `01b55fb3a` | `lamp-a1-playout` | `main.rs`, `docs/playout-continuity.md` | The plain 600 ms horizon in the replay; corrections to the write-up |
| `993ebaa63` | `lamp-a1-playout-integration` | `crates/live/src/{options.rs,coordinator.rs,main.rs}`, new `crates/live/src/coordinator/tests/playout_continuity.rs` | The wiring, behind `--playout-continuity`. Your files; a proposal |

Nothing in `playback.rs`, the provider worker, wire definitions or shared
events changed on either branch.

## Interface

All additions to `lamp_audio::playout`. Nothing existing changed its
signature or its behaviour.

| Addition | Use |
|---|---|
| `Continuity { margin_us, hold_us, cover_us }`, `Continuity::EVALUATED`, `longest_hold_us()` | The setting, and the bound on any hold |
| `StartGate::continuous(Continuity)` | In place of `StartGate::new(horizon)` when opted in |
| `StartGate::arrival(at_us, frames)` | In place of `audio_arrived(at_us)`. Works for both gates |
| `StartGate::generation_ended()` | When the final block of a generation is handed over. A plain gate ignores it |
| `StartGate::lead_us()`, `continuity()` | For the trace. A plain gate measures the lead too and does not use it |
| `OpenedBy::Margin`, `OpenedBy::HoldBound` | Two more reasons in the opening receipt |

The proposal changes 26 lines of `coordinator.rs`. With the option off the
gate is the plain one, and your twelve `playout_start` tests pass
unchanged. The only visible change with the option off is in the trace:
the start receipt gains `measured_lead_us`, `opened_by` and a null
`continuity`, and `run_start` a null `playout_continuity`.

## Verification

Each run is on a source-only export of the named commit.

### Host, macOS x86_64

| Commit | Command | Result |
|---|---|---|
| `68e50dc3c` | `cargo fmt --all -- --check` | exit 0 |
| | `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | exit 0 |
| | `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 1,151 passed, 0 failed, 5 ignored |
| | `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | exit 0 |
| `01b55fb3a` | `cargo fmt --all -- --check` | exit 0 |
| | `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | exit 0 |
| | `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 1,152 passed, 0 failed, 5 ignored |
| | `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | exit 0 |
| `993ebaa63` | `cargo fmt --all -- --check` | exit 0 |
| | `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | exit 0 |
| | `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 1,157 passed, 0 failed, 5 ignored |
| | `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | exit 0 |

`d11d5a300` was not run through the whole workspace on its own: a first
attempt overlapped with another of my runs in the same directory and I
discarded it. `01b55fb3a` adds one test and documentation to it.

### Linux ARM64, emulated

`aarch64-unknown-linux-gnu`, cross-built in a container and run under
user-mode QEMU on this Mac. Not the lamp.

| Commit | Command | Result |
|---|---|---|
| `993ebaa63` | `cargo clippy --locked --offline --workspace --all-targets --target aarch64-unknown-linux-gnu -- -D warnings` | exit 0 |
| | `cargo test --locked --offline --release --target aarch64-unknown-linux-gnu -p lamp-audio -- --test-threads=1` | 138 passed, 0 failed, 1 ignored |
| | `cargo test --locked --offline --release --target aarch64-unknown-linux-gnu -p lamp-live --lib -- playout options --test-threads=1` | 27 passed, 0 failed |
| | `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval --target aarch64-unknown-linux-gnu` | exit 0; `lamp-live` is an aarch64 ELF |

`993ebaa63` is `01b55fb3a` plus the proposal, so this run covers both
branches' code. The same script passed earlier on the two versions before
it, `779542b3a` and `e417a1746`.

### Cost

One call to the gate takes 2.0 ns on the host and 24 ns to 28 ns under
emulation, in release builds. The gate is 128 bytes and allocates nothing.
It is asked once per 10 ms slot and told of about 25 arrivals a second.

### Reproducing the fixture and the model

The scripts are outside Git, in this session's scratch directory:
`/private/tmp/claude-501/-Users-autonomous-harnesses-worktrees-autonomous-os-merry-zebra/339c85e4-ca76-461d-beb1-fc98ac7d4f7e/scratchpad/m042-handoff/`,
with `manifest.sha256`. `extract_all.py` freezes the bundles into
`all.json`, `fixture.py` writes `replies.tsv` from it, byte for byte, and
`model3.py`, `spare.py` and `hiccups.py` are the Python model behind the
alternatives and the late-burst table. Run them with
`/usr/local/bin/python3.12 -I`. That directory does not outlive the
session; copy it if you want it.

## Not done, and not established

- **Anything on a device.** The physical A/B is yours.
- **Longer pauses in place of a longer start.** Modelled, not built: with a
  200 ms hold the model reaches zero starvation at 80 ms of average start
  delay, by lengthening pauses 64 ms per reply. It needs the coordinator to
  show the gate each block and to tell deliberate zeros from starvation.
  Numbers in the write-up.
- **That the next late burst is no larger than 306 ms**, or that a start
  released by the hold bound leads by 44 ms. Both held in 23 replies from
  one day.
- **The generation-complete flag** was not in the traces; the replay uses
  the turn completion after it.

## Requests to main

None open for `MAIN-042`.
