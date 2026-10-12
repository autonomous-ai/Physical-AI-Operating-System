# Agent 1 status

Owner: Agent 1 (audio and Gemini). This file on `lamp-a1-onset-retention`
is the status of one task: your request of 2026-10-12, after the native
serve trial, for a causal audit of onset retention. The branch is your
`a75a8ddad` plus the commits below. My other branches are as they were;
the last status of the continuation and capture-order work is on
`lamp-a1-capture-order` at `6c311b4d1`.

- Updated: 2026-10-12, 05:29 UTC. Corrected at 06:55 UTC: this line said 05:55,
  a time I did not read off the clock. Nothing else in this file changed
  then. Later work is on `lamp-a1-mic1-reference` and its status file.
- **Newest: your R4, gain control off, is analysed against the first
  device run.** The control was costing a person 10.6 dB of level once
  the lamp had spoken; off, 1.8 dB, and the run reached a commit, 0.9 s
  sooner. **In both runs the lamp yielded where its own speech paused,**
  and the request began at the last sentence. While the lamp is actually
  sounding the person is still 12 to 15 dB weaker in the microphone
  signal than the same loudspeaker is in silence; not the gain switch,
  not compression of the lamp's voice, not the emission. See "The second
  device recording" in the audit.
- **One inference of mine from the first run is withdrawn:** the quiet
  floor as evidence of the gain control. With the control off the floor
  fell by the same 10 dB while speech kept its level.
- **Newest: your device recording is analysed. The interruption is lost
  at the microphone input, before any software.** While the lamp speaks,
  the microphone signal the software receives holds the coffee clip at
  least 20 dB weaker than the same loudspeaker delivered five seconds
  earlier with the lamp silent; the clip was emitted, at the same
  strength, by the room recording. The microphone's hardware
  `Auto Gain Control` is on: its quiet floor drops 7 to 10 dB at the
  lamp's first sound and takes more than 30 s to return. The device's
  own margin read -22.1 dB. Canceller, detector and hearing then did
  what that input allowed: 86% of the clip's speech blocks scored under
  0.5, one yield a quarter second before the clip ended, candidate
  rejected. See "The device recording" in the audit.
- **This withdraws a finding of mine from the duplex work:** that on
  `4ace` a person is about as loud as the lamp's own voice at the
  microphone. Prompts were recorded at the microphone's high gain and
  replies at its low one. Every lab overlap on `4ace`, then and in this
  audit, was at least 10 dB too loud. `docs/duplex-listening.md` carries
  the correction.
- **Answer: the opening of the coffee request was never admitted.** The
  admitted request begins 2.3 s after the stimulus's speech began. The
  provider, the kept start and continuation were handed nothing earlier.
- **Cause of the late yield on the device: not established.** No
  per-block hearing and no microphone audio were recorded. A lab with the
  same cached clip over recorded `4ace` replies reproduces the device's
  timing in 2 of the 39 trials that can be judged and points at the
  canceller's output while the lamp sounds.
- **Corrected at 03:15 UTC: the lab figures.** I had pooled all 50 trials.
  In 11 the recorded reply paused or ended before any yield, where the
  ordinary path would admit the person and the lab's yield decides
  nothing. The figures below are for the other 39. See
  [My faults here](#my-faults-here).
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
| Lab, the 39 trials in which the lamp sounded throughout | Whole request retained in 6. Opening not retained, 36 committed: median 675 ms, at least 1 s in 17. Yield at 2,580 and 2,610 ms then commit in 2, losing 2,290 and 2,320 ms: the device's figures |
| Lab, why | Before the first yield, 83.5% of blocks were heard as nothing speech-like. The added speech is 20.8 dB down in the canceller's output there, against 3.5 dB down with the lamp silent |

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
| `A1-031` | **Where the 12 to 15 dB go while the lamp sounds.** One recording without a conversation: the lamp plays a fixed sound while the same cached clip is played to it, once per microphone setting or capture device that can be chosen (`device_cmedia` has a capture control too). It separates the microphone's own processing from everything after it. Yours to decide and run |
| `A1-030` | First half done by you in R4, thank you. Still owed: the pilot's precondition, two complete replies before the overlap, gain control off, recorded the same way, to see what a canceller that has heard the lamp leaves of a person. As first written: **The next native comparison, and nothing in software before it.** The same two clips with (1) the microphone's `Auto Gain Control` mixer switch off and its capture volume fixed where the lamp's own voice stays below full scale, and (2) two complete replies before the overlap, as in the pilot, so the canceller has heard the lamp; recorded with `--diagnostics`. It shows whether the floor holds still, how far below the lamp's voice a person really is, what the converged canceller leaves of them, and whether the lamp yields sooner. A mixer setting and a longer scene, not a threshold |
| `A1-028` | Done by you, thank you: `/private/tmp/lamp-duplex-diagnostic-executed-fhld3dr3`. It was not the pilot's scene (first playback of the session, clip 0.45 s into it), which the plan says. What it was: **The one next native diagnostic.** Repeat only `sunset` then `coffee`, same cached clips, same duplex settings, in a run that records the microphone bundle with `--diagnostics`, as the `lamp-connected-*` runs of 2026-10-11 did. The serve runner refuses raw audio diagnostics, so not that runner. Give me the bundle path; the lab's replay then shows how every block of the overlap was heard on the device, what the canceller left of the speech, and when it arrived. No further device time |
| `A1-029` | If raw audio may not be recorded there: a bounded per-block hearing history in the trace at each yield, audio-free. Not built; it would be one coordinator proposal commit. Say if you want it |
| `A1-013` | Still open from the duplex work and the same thing: a recording of speech over the lamp through the real acoustic path |

## Verification

| | |
|---|---|
| `7f395d0b6`, focused, from a source export of the exact commit, host, 2 jobs, 02:48:06Z to 02:48:52Z, all exit 0 | `cargo fmt --all -- --check` clean; strict Clippy `-p lamp-live --all-targets` clean; `lamp-live` lib tests `coordinator::tests::duplex`, `duplex_through_the_worker`, `turn_taking`, `activity::` 63 passed, 0 failed, 0 ignored |
| The two new controls alone, on the working tree before the commit | 2 passed |
| The lab, `crates/live/examples/duplex_lab.rs` at `a75a8ddad`, release, SHA-256 `1bb4dc7d46198e32f4e0183c0496cb8ca4d84ddc8fa51cc9727265c1c6e85b98` | 50 trials and 3 negative controls, offline; outputs in the scratchpad, pooled in the audit directory. 39 of the 50 are used |
| The analysis for the coming device bundle, on a synthetic bundle made from one lab trial | Arrival delay 0.0 ms where it is 0; canceller delay 8.75 ms where it is 9.0; level before cancellation 8.2 dB where it is 8.2. Script in the scratchpad, not in Git until it has a real bundle |

Not run: the workspace suite, ARM, anything on a device or against the
service. The lab runs are offline on the host.

## My faults here

- **I read the microphone's quiet floor as its gain.** It fell 7 to
  10 dB after the lamp's first sound in the first device run, the gain
  control was on, and I put the two together and told you so. In R4,
  with the control off, the floor fell the same way and speech did not.
  The gain was shown by the level of the same clip in silence before and
  after, which I also had; the floor was not evidence and I do not know
  what it follows.
- **I took two recordings made at two microphone gains for one
  acoustic fact.** In the duplex work I measured prompts spoken with the
  lamp silent and replies with the lamp speaking, found them about as
  loud, and built the lab's levels and a headline on it. The owner's side
  had said the microphone board has its own processing, and I wrote that
  down as a caveat instead of checking the mixer, where
  `Auto Gain Control` is `on`. Your device recording showed it.
- **The lab figures I first published pooled trials the lab cannot
  judge.** Whole request in 6 of 50, median 1,240 ms lost and the
  device's timing in 4 of 50 became 6 of 39, 675 ms and 2 of 39. I had
  written that the lab does not model ordinary admission after a reply
  ends and then counted such trials all the same. Found while validating
  the bundle analysis on a synthetic bundle; corrected in the audit, with
  the first figures kept in the data.
- My first measure of how much of the added speech survives the
  canceller compared the two signals with no delay between them and gave
  −27 dB even with the lamp silent. The canceller delays its output by
  144 samples. Found by checking the measure where the answer is known,
  before publishing; the figures above are at that delay.
