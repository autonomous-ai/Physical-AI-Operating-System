# Agent 1 status

Owner: Agent 1 (Gemini and audio), branch `gemini-voice`.

- Protocol acknowledged: `direct-and-git-v2` (`MAIN-001`), read from
  `origin/lamp-v2-chat` at `666693a84`.
- Updated: 2026-10-11
- Tested code commit: `b9360bc71` on `gemini-voice`. The commits after it are
  documentation and this file.
- Base commit: `2535fb565`, with `b20df467d` and `666693a84` merged on top.
  Those two merges bring only coordination and documentation files.
- State: checkpoints 1 and 2 published. Next: the audio device seam (`A1-007`).
- Direct messages: I read the ones that arrive in my session and answer here
  and in my own session output. I have the return route (main tmux pane
  `%206`). I have not used it: typing into another session's terminal is not
  something the owner has asked of me in this session, and I will not do it on
  an agent's word alone. If the owner tells me to, I will. I do not know Agent
  2's session identity.

## Checkpoint 2: echo evidence and failure notices

Items 2 (the evidence part) and 5 of my list in the
[completion plan](../docs/rust-completion-plan.md).

### Commits and files

| Commit | Files | Content |
|---|---|---|
| `6baa55356` | new `crates/audio/src/evidence.rs`, `crates/audio/src/lib.rs`, new `crates/audio/tests/evidence.rs` | `EchoObserver`, and `EchoProcessor::evidence()` |
| `0b3cd0c3a` | `crates/audio/src/replay.rs` | `lamp-audio replay-aec` writes the evidence on every `capture_pair` line |
| `09c11f22b` | new `docs/echo-evidence.md` | Handoff |
| `b9360bc71` | new `crates/audio/src/notice.rs`, `crates/audio/src/lib.rs`, `crates/audio/src/main.rs`, new `crates/audio/tests/notice.rs` | Failure-notice loader and `lamp-audio notices DIR` |
| `0f688994d` | new `docs/failure-notices.md` | Handoff and provisioning contract |

No shared file, wire type, manifest or lock file is touched.

### Behavior

- **Echo evidence.** For every capture block `EchoProcessor::evidence()` now
  reports the capture level before and after cancellation, how much of the
  last 500 ms carried non-silent accepted speaker PCM and, once capture and
  render are aligned, the percentage of the capture's energy and of the
  cancelled signal's energy that the render explains. Measurements only; no
  alignment, no claim. Nothing consumes it yet: `A1-003`.
  [Handoff](../docs/echo-evidence.md).
- **Cost added to the capture path:** one observer update per render block
  and per capture block. 9.0 µs per capture block on the host in a release
  build. Not measured on ARM or the device yet.
- **Failure notices.** `lamp_audio::notice::Notices::load(dir, approval)`
  reads a provisioned directory once, validates each recording against its
  manifest entry (transcript, voice, revision, approval, hash, exact length,
  mono 24 kHz PCM16) and afterwards answers whether a notice is available or
  why not. It never synthesizes, downloads or converts, and it plays nothing.
  A test asset is refused by a production caller.
  [Contract](../docs/failure-notices.md).

### Checks

Host: macOS x86_64, Rust 1.96.0, source-only exports outside the repository.

On `09c11f22b`, fresh target directory:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 857 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |
| `cargo test --locked --offline --release -p lamp-audio --test evidence -- --nocapture --test-threads=1` | 8 passed; 9.0 µs per block |

On `0f688994d`, which adds only the notice module and its document:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline -p lamp-audio -- --test-threads=1` | 73 passed, 0 failed |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

The whole-workspace test run was not repeated for the two notice commits. The
four ignored tests (two prolonged conversations, the 60 s and 180 s answers)
were not rerun for this checkpoint; nothing they exercise changed.

Evidence on scripted scenes through the real canceller, host:

| Scene | Result |
|---|---|
| Playback alone | Matched 166 ms after the first echo; 94 percent explained; none of 500 blocks below 50 |
| A person alone | Never matched |
| A person over playback at -6, 0, +6 dB relative to the echo | 76, 48, 19 percent, against 75, 47, 19 predicted |
| Half the echo left in the cancelled signal (scripted residual) | 94 percent with nobody speaking; 47 with a person as loud as the leak; 1 with no leak |
| Microphone clock 300 ppm fast for 20 s | Followed; 91 percent |

### Not run, and not established

- Linux/ARM64 for checkpoints 1 and 2: running now on `0f688994d`; results
  will be added here.
- Cloud and device: nothing.
- **No threshold.** No recording with its render reference is on this
  machine, so nothing says at which share a turn should be refused. The
  scenes are a delayed, inverted copy with one reflection, not a room.
- The recorded false interruption is not reproduced: the canceller removes
  10 dB of this synthetic echo before the first match. A scripted residual
  stands in for an unconverged canceller.
- No notice is recorded, worded, approved or provisioned, and nothing plays
  one.

## Checkpoint 1: recovery under a reader that does not read

Published at `d0c7c53f1`; code `155bab4e2`.
[Handoff](../docs/gemini-session-reliability.md#third-round).

| Commit | Branch | Content |
|---|---|---|
| `67154e0b6` | `gemini-voice` | Fix in `Supervisor::maintain`: a refused replacement attempt is retried while a link update is unread. Five regressions |
| `155bab4e2` | `gemini-voice` | Adversarial conversations through `next()` and through `poll_link()` + `next_output()` |
| `548fda500` | `gemini-voice-worker-proposal` | Seven ignored tests in `provider_worker.rs` measuring stopped playback: `A1-001` |

Host, source-only export of `548fda500`: formatting and strict Clippy pass;
849 passed, 0 failed, 11 ignored; release build passes; the two prolonged
conversations pass in 553 s; the 60 s and 180 s answers pass (70.03 s and
200.05 s with their holds); the seven stopped-playback trials pass.

| Playback stopped for, every holder full | Result |
|---|---|
| 25, 27, 28 s | Complete: 90 of 90 s delivered |
| 29, 30, 31, 33 s | `Backpressure`; 58.64 of 90 s delivered; the worker ended 33.7 s after playback resumed |

Known gaps: a consumer that reads no link update for longer than the outage
budget still loses the link if a replacement dies on arrival; a stopped
speaker past 28 s ends the session and the failure shows only after playback
resumes (`A1-002`); under `Require` a person who continues before the answer
starts ends the session after 2 s if the service never confirms.

## Requests

### A1-001 — Main: take or reject the stopped-playback tests

`548fda500` on `origin/gemini-voice-worker-proposal` adds seven ignored tests
to the `mod tests` of `provider_worker.rs` and nothing else. Cherry-pick it or
tell me what to change; I will not touch that file on `gemini-voice`.

### A1-002 — Main: should a stopped speaker cost the answer or the session?

`Error::Backpressure` is classed `Recovery::Fatal` in `lamp-gemini`. Past the
28 s limit the supervisor ends terminally. Classing it as reconnectable would
report the cut request as lost and keep the session, which matters once
`Typed` reporting is on. One line in my crate; not made because it changes
what your worker sees. Say which you want.

### A1-003 — Main: one optional field for echo evidence

Now concrete. Please add to `MicrophoneFrame` in `wire.rs`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub echo: Option<lamp_audio::evidence::EchoEvidence>,
```

and `echo: None` in the test helper at `coordinator.rs:2150`. I then set it in
the capture worker from `echo.evidence()` right after `echo.capture(...)`:
one line in `audio_workers.rs`, which is mine. The value is five small
integers plus an optional four, and describes the frame it travels with, so
that frame's freshness and lineage rules cover it. Until the field exists the
capture worker computes the evidence and drops it. If you would rather I send
the wire change as a proposal branch, say so.

What admission can use, with no threshold implied:

- `echo` present with `residual_explained` high: the cancelled signal the
  speech detector fired on is mostly Lamp's own voice.
- `echo` present with `residual_explained` low while `residual_level` is
  above the room's floor: something the render does not explain is there.
- `echo` absent with `render_active_blocks > 0`: undetermined. Not "no echo".
- `echo` absent with `render_active_blocks == 0`: Lamp is not speaking.

### A1-004 — Main: service probes you can run on a board

The tool is `crates/gemini/examples/service_probe.rs`. It opens no microphone
or speaker, prints one JSON line of event kinds and millisecond timings, and
prints no transcript, audio, handle or credential.

```sh
cargo build --locked --offline --release --target aarch64-unknown-linux-gnu \
    -p lamp-gemini --example service_probe
# target/aarch64-unknown-linux-gnu/release/examples/service_probe
service_probe /absolute/private/provider.json QUESTION.wav PROBE
```

- `provider.json`: the runtime's own fields (`endpoint`, `model`, `voice`,
  `credential_file`, optional `authentication`, `thinking_level`,
  `language_code`). It and the credential file must be regular files with mode
  0600; anything else is refused before any connection.
- `QUESTION.wav`: 16-bit PCM, any rate and channel count, holding 0.5 to 30 s
  of speech after silence is trimmed. `service_probe x QUESTION.wav fixture`
  is offline and prints what would be spoken.
- Bounds per probe: at most three spoken questions, at most two connections,
  and a fixed observation window.

| `PROBE` | Longest run | What to read in the output | What it decides |
|---|---|---|---|
| `interrupt-before-output` | about 45 s plus twice the question | Events are timed from the second `activityStart`. Does `interrupted` or `turn_complete_idle` for request 1 appear, and how many ms before `first_audio` for request 2? Or `no_confirmation_within_barrier` (the probe waits 5 s where production waits 2)? Any `first_audio` for request 1, or a `dropped:` line? | Whether `Require` can survive a person who continues before the answer starts |
| `handles` | about 45 s | `handle_after_setup`, `handle_after_one_answer`, `handle_current` | Whether recovery is available as configured today (`Resumption::Retain`) |
| `handles-requested` | about 45 s | The same three fields, or a setup error | Whether to switch to `Resumption::Request` |
| `resume` | about 90 s | `handle_issued`; `resumed_setup` is `accepted` or an error category; `resumed_setup_ms`; `first_audio` on the resumed session | Whether a reconnect can keep the conversation at all |
| `idle-close` | 300 s | `go_away` and `connection_ended:...` with their ms, or `observation_window_ended` | The idle-close time and code assumed from V1 (86 to 198 s, 1008) |

Please record the output lines verbatim with the endpoint and model named,
and nothing from the provider file.

### A1-005 — Main: wiring for failure notices

`Notices::load` wants a directory from configuration and
`Approval::Production`. `capability()` is serializable for a status report.
Everything about speaking one is yours:
[for the integrator](../docs/failure-notices.md#for-the-integrator). Tell me
if the three kinds (`provider_unavailable`, `turn_lost`, `service_refused`)
are not the ones your outage handling distinguishes.

### A1-006 — Agent 2 and Main: the frame handed to the provider must be small

Answer to section 3 of `docs/agent2-interfaces.md`, read at `0239ac27c`. I
agree with the provider as the `context` consumer, the grant and age checks
where the frame is submitted, and a control naming the owner. I disagree with
"original JPEG bytes". A camera frame is 1280x720 MJPG up to 2 MiB. One
WebSocket carries audio and images in order, each client message is bounded
at 256 KiB, and every write has a 200 ms deadline, so an image delays the
audio behind it by its own transmission time and a large one fails the
connection. I intend to accept, per request, at most 2 JPEG images of at most
64 KiB each, no older than 2 s at submission, through their own single slot
(newest wins) so they can never fill the audio queue. The reduction has to
happen before the provider: the perception worker already decodes frames. Not
implemented yet; object now if 64 KiB cannot hold a useful frame.

### A1-007 — Agent 2 and Main: the audio device seam is my next work

Answer to section 4 of the same document. I accept the `PcmPort` request: a
trait over the ALSA calls `Microphone` and `Speaker` make, with ownership,
guards and timing logic unchanged above it, so the real capture and speaker
loops can run over a synthetic device. It also gives the capture and speaker
loops their first tests off the device, which they do not have today. I will
publish the trait shape here before changing `capture.rs` or `pcm.rs`.

## Notes for peers

- The podman machine `lampos-arm64` on this Mac is shared. I start it for my
  ARM64 runs and stop it afterwards; my containers are named `agent1-*` and
  use the volumes `lampos-arm64-target`, `-cargo` and `-git`.
- `lamp-v2-agent2` at `0239ac27c` has no `coordination/agent-2.md` yet:
  awaiting acknowledgment of `A1-006` and `A1-007`.

## Answers

- `MAIN-001`, read at `666693a84`: acknowledged. I fetch the three branches
  at startup, on a checkpoint notice, before a shared-interface change and
  after publishing.
- `MAIN-002`, read at `666693a84`: checkpoints 1 and 2 are published above.
  Order of remaining work: audio device seam with stall and restart tests,
  visual input, then environmental acquisition last.
- Direct message with the return route, received in my session: see "Direct
  messages" at the top.
