# Agent 1 status

Owner: Agent 1 (Gemini and audio). This file on `lamp-a1-playout` is the
status of `MAIN-042` only. Everything before it, checkpoints 1 to 16 and
requests `A1-001` to `A1-011`, is unchanged on `gemini-voice` at `12caf1b38`
in this same file.

- Updated: 2026-10-11
- Assignment: `MAIN-042`, read from
  `origin/lamp-v2-chat:lampOS/coordination/voice-next-assignments.md` at
  `96eff6b42`. Acknowledged and started.
- Work branch: `lamp-a1-playout`, a fresh worktree from the integration code
  you named, `1a089863a`. Nothing from `gemini-voice` was merged into it.
- The team channel returned nothing new; this file is my report.
- No device, no cloud, no deployment. Room and reply audio stayed local.

## Checkpoint 1: the start policy and its replay

### What is published

| Path | Content |
|---|---|
| `crates/audio/src/playout.rs` | An opt-in continuity start on the existing `StartGate`. The plain gate is unchanged |
| `crates/audio/tests/playout_continuity/main.rs` | Replay of recorded supply through the real gate, 9 tests and one ignored timing |
| `crates/audio/tests/playout_continuity/replies.tsv` | 23 recorded replies as numbers: enqueue times, frame counts, completion, recorded start and recorded gaps. No audio, no text |

The write-up, `docs/playout-continuity.md`, and the integration patch for
your files follow in the next checkpoint. The interface below is what that
patch will use.

### What the evidence says

From the six-turn run and seven earlier bundles: 23 complete replies, 115 s
of speech, 2,888 enqueue records.

1. **The service leads real time by 13 ms to 110 ms and no more.** Measured
   as the audio received before each burst, less the time since the first
   audio, and taken as the median over a reply. That lead is all a reply has
   in hand when a burst is late, unless the start added to it.
2. **Late bursts come in two sizes.** Twenty-five bursts were at least 60 ms
   late against their own reply's lead: seventeen between 89 ms and 172 ms,
   and eight between 295 ms and 306 ms. Nothing in between and nothing
   larger. The eight fell 2.0 s to 18.4 s into a reply, in 8 of the 23.
3. **The four gaps you named are that second size.** Coffee: a burst 303 ms
   late against a 44 ms lead, with a start 190 ms after the first audio.
   Ocean: 299 ms late, 104 ms lead, start at 47 ms. Story: its own start
   was not held at all, so it ran dry 30 ms in when its second burst came
   131 ms behind, and again on a burst 295 ms late.
4. **The replay reproduces what the lamps recorded.** Started where each
   speaker started, the model gives the recorded gaps block for block in 22
   of 23 replies: all four of yours, and 21 of the 23 recorded runs of zeros
   overall. The other two runs are one reply, where one block moves between
   its two gaps and the total is the same.
5. **The four gaps were inside words.** In the accepted render the speech is
   at about -13 dBFS to -21 dBFS on both sides of three of them. The fourth
   is 30 ms into the story.

### The policy

`StartGate::continuous(Continuity)` releases the first block when a block
can be handed over and either the reply is complete, or both of these hold:

- **cover**: time since the first audio plus audio in hand is at least
  360 ms. This is your gate's rule with a longer horizon. It decides only
  when almost nothing has arrived.
- **margin**: time since the first audio plus the measured lead is at least
  320 ms, or 280 ms have passed.

It is re-armed for a later generation of the same turn, and only once the
generation before it has handed over its last block.

320 ms is the largest late burst recorded, 306 ms, plus one 10 ms slot.
280 ms is the longest it waits for evidence of the lead. A start released
that way has 280 ms plus whatever the service leads by, which was 44 ms or
more in every recorded reply that had audio by then. These numbers are
`Continuity::EVALUATED`. They are not a live default and nothing here was
measured on a device.

### Before and after, on the recorded supply

Over twenty slot phases per reply. "Current" is the 200 ms gate with the
story's generation not held, as deployed in `6e6c58b42`.

| | Current gate | Continuity |
|---|---|---|
| Replies that run dry | 9 of 23 | 0 of 23 |
| Zeros accepted mid-reply, worst phase | 1,190 ms | 0 ms |
| Coffee | 70 ms to 80 ms | 0, first block 90 ms to 100 ms later |
| Ocean | 160 ms to 170 ms | 0, first block 170 ms to 180 ms later |
| Story, second generation | 210 ms to 220 ms | 0, its own first block 280 ms later |
| First block of a reply, added delay | | median 160 ms, most 210 ms |
| Replies not delayed at all | | 5 of 22, by 0 ms to 10 ms |

**Where it misses your target.** One first generation is held 200 ms to
210 ms longer, the first pilot's 1.4 s reply, which had no late burst. And
the story's second generation is held 280 ms where today it is not held at
all; it needed 204 ms in hindsight, so 200 ms would have left a hole of one
or two blocks at 18.4 s. That delay lands after the 2.27 s pause, which is
a different matter and stays visible as its own interval.

**It is a delay, not a free gain.** Riding out a 306 ms late burst with a
service that leads by 44 ms to 110 ms means playing about 210 ms to 280 ms
behind the first audio. Replies that arrive late anyway, with one frame and
then the audio 340 ms to 390 ms after it, pay nothing.

### Interface, for your patch

All additions. Nothing existing changed its signature or its behaviour.

| Addition | Use |
|---|---|
| `Continuity { margin_us, hold_us, cover_us }`, `Continuity::EVALUATED`, `longest_hold_us()` | The setting, and the bound on any hold: 360 ms |
| `StartGate::continuous(Continuity)` | In place of `StartGate::new(horizon)` when opted in |
| `StartGate::arrival(at_us, frames)` | In place of `audio_arrived(at_us)`, in `append_provider_audio`. Works for both gates |
| `StartGate::generation_ended()` | When `next_speaker_chunk` returns the final block. A plain gate ignores it |
| `StartGate::lead_us()`, `continuity()` | For the trace. A plain gate measures the lead too and does not use it |
| `OpenedBy::Margin`, `OpenedBy::HoldBound` | Two more reasons in the opening receipt |

Three call sites in `coordinator.rs` and one option. I will publish that as
a separate branch with tests through the real `Reply` methods, and not on
this one.

### Verification

Host, macOS x86_64, on the working tree before this commit:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --locked --offline -p lamp-audio --no-fail-fast -- --test-threads=1` | 117 passed, 0 failed, 1 ignored |
| `cargo test --locked --offline -p lamp-live --lib playout` | 12 passed, 0 failed |
| `cargo test --locked --offline --release -p lamp-audio --test playout_continuity -- --ignored --nocapture` | 2.0 ns per call, 128 bytes, no allocation |

Linux ARM64 and the whole workspace's tests are not run yet for this
checkpoint. They come with the next one.

### Not done yet

- `docs/playout-continuity.md`: the V1 comparison, the full tables, the
  2.27 s pause, limits.
- The integration branch.
- Linux ARM64 build and tests.
- An alternative I measured and did not build: holding less at the start and
  lengthening pauses inside the reply instead. Numbers in the write-up.

## Requests to main

None open for `MAIN-042`. I have not changed `playback.rs`, the coordinator,
the provider worker, options or wire definitions.
