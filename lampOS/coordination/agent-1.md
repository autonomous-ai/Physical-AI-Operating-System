# Agent 1 status

Owner: Agent 1 (audio and Gemini). This file on `lamp-a1-onset-retention`
is the status of one task: your request of 2026-10-12, after the native
serve trial, for a causal audit of onset retention. The branch is your
`a75a8ddad` plus the commits below. My other branches are as they were;
the last status of the continuation and capture-order work is on
`lamp-a1-capture-order` at `6c311b4d1`.

- Updated: 2026-10-12, 02:50 UTC
- **Answer: the opening of the coffee request was never admitted.** The
  admitted request begins 2.3 s after the stimulus's speech began. The
  provider, the kept start and continuation were handed nothing earlier.
- **Cause of the late yield on the device: not established.** No
  per-block hearing and no microphone audio were recorded. A lab with the
  same cached clip over recorded `4ace` replies reproduces the device's
  timing in 4 of 50 trials and points at the canceller's output while the
  lamp sounds.
- **No correction proposed.** One native diagnostic is, and two controls
  are added. Nothing in production code changes on this branch.

The audit, its numbers, limits and scripts:
`lampOS/docs/verification/onset-retention-20261012/README.md`.

## Commits

| Commit | Files | What |
|---|---|---|
| `1090991e2` | `docs/verification/onset-retention-20261012/` | The audit: native timeline per turn, the stimulus's speech stretches, 50 lab trials pooled, the scripts. No audio |
| `7f395d0b6` | `crates/live/src/coordinator/tests/duplex.rs` only | **Shared file, one proposal commit.** Two tests, no production change |
| this commit | `coordination/agent-1.md` | This status |

## What is established

| | |
|---|---|
| Native, all eight turns | A request reaches back 288 to 291 ms before its trigger block: the detector's pre-roll. Nothing is trimmed at admission |
| Native, the seven turns without a reply being spoken | Trigger 231 to 330 ms after the clip's speech onset; the request starts from 57 ms before to 39 ms after it, before any output latency is counted |
| Native, coffee | One yield, 2,607 ms after the clip's speech onset; commit 478 ms later; request starts at 2,316 ms of the clip. The lamp had been speaking for 3.8 s and had 4.7 s still queued |
| Native, provider | Start 3.8 ms after admission, ordinary interruption, nothing kept back, nothing discarded. The last sentence, wholly inside the admitted audio, is what the service transcribed |
| Native, continuation | Turn 5 to 6 under `kept-start`: the corrected question was answered. One attempt |
| Lab, 50 trials | Whole request retained in 6. Opening not retained, 41 committed: median 1,240 ms. Yield at 2,530 to 2,610 ms then commit in 4, losing 2,240 to 2,320 ms: the device's figures |
| Lab, why | While the lamp sounded, before the first yield, 81.8% of blocks were heard as nothing speech-like. The added speech is 22.6 dB down in the canceller's output there, against 3.5 dB down with the lamp silent |

## What is not

- How each block was heard on the device before the yield.
- The microphone before and after cancellation in that interval.
- When the clip's sound reached the microphone; bounded near 0.2 s.
- Samples written to the service per request.
- Whether the device's canceller does what the cold one in the lab does.

The lab is a software mix of a clean clip over other replies. It is not
a device result and chooses no gate.

## Why no correction

- Reaching further back at a duplex commit: in the lab the opening is
  not in the cancelled signal to reach back to, and it would need audio
  older than the transport's one second.
- Yielding sooner, or not giving a yield up at a short pause: changes to
  what is heard as a person, on one native negative and a replay.
- The provider and the kept start: the opening never reached them.

## For you

| Id | |
|---|---|
| `A1-028` | **The one next native diagnostic.** Repeat only `sunset` then `coffee`, same cached clips, same duplex settings, in a run that records the microphone bundle with `--diagnostics`, as the `lamp-connected-*` runs of 2026-10-11 did. The serve runner refuses raw audio diagnostics, so not that runner. Give me the bundle path; the lab's replay then shows how every block of the overlap was heard on the device, what the canceller left of the speech, and when it arrived. No further device time |
| `A1-029` | If raw audio may not be recorded there: a bounded per-block hearing history in the trace at each yield, audio-free. Not built; it would be one coordinator proposal commit. Say if you want it |
| `A1-013` | Still open from the duplex work and the same thing: a recording of speech over the lamp through the real acoustic path |

## Verification

| | |
|---|---|
| `7f395d0b6`, focused, from a source export of the exact commit, host, 2 jobs, 02:48:06Z to 02:48:52Z, all exit 0 | `cargo fmt --all -- --check` clean; strict Clippy `-p lamp-live --all-targets` clean; `lamp-live` lib tests `coordinator::tests::duplex`, `duplex_through_the_worker`, `turn_taking`, `activity::` 63 passed, 0 failed, 0 ignored |
| The two new controls alone, on the working tree before the commit | 2 passed |
| The lab, `crates/live/examples/duplex_lab.rs` at `a75a8ddad`, release, SHA-256 `1bb4dc7d46198e32f4e0183c0496cb8ca4d84ddc8fa51cc9727265c1c6e85b98` | 50 trials and 3 negative controls, offline; outputs in the scratchpad, pooled in the audit directory |

Not run: the workspace suite, ARM, anything on a device or against the
service. The lab runs are offline on the host.

## My faults here

- My first measure of how much of the added speech survives the
  canceller compared the two signals with no delay between them and gave
  −27 dB even with the lamp silent. The canceller delays its output by
  144 samples. Found by checking the measure where the answer is known,
  before publishing; the figures above are at that delay.
