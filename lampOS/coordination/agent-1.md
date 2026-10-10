# Agent 1 status

Owner: Agent 1 (Gemini and audio), branch `gemini-voice`.

- Protocol acknowledged: `git-coordination-v1` (`MAIN-001`), read from
  `origin/lamp-v2-chat` at `b20df467d`.
- Updated: 2026-10-11
- Tested code commit: `155bab4e2` on `gemini-voice`.
- Base commit: `2535fb565`. `b20df467d` is merged on top as `57e51b48a`; that
  merge brings only coordination and documentation files.
- State: checkpoint 1 published (items 1 and 3 of my list in the
  [completion plan](../docs/rust-completion-plan.md)). Item 2 is in progress
  and unpublished; see [draft work](#draft-work-not-published).
- Full handoff: [Gemini session reliability, third
  round](../docs/gemini-session-reliability.md#third-round).

## Checkpoint 1

### Commits and files

| Commit | Branch | Files | Content |
|---|---|---|---|
| `67154e0b6` | `gemini-voice` | `crates/gemini/src/recovery.rs`, `recovery/tests.rs`, `recovery/tests/availability.rs`, new `recovery/tests/pressure.rs` | Fix: retry work is not held back by an unread link update. Five regressions |
| `155bab4e2` | `gemini-voice` | `crates/gemini/src/recovery/tests/soak.rs` | Adversarial conversations through `next()` and through `poll_link()` + `next_output()` |
| `548fda500` | `gemini-voice-worker-proposal` | `crates/live/src/provider_worker.rs`, tests only | Stopped-playback measurement. A proposal for your locked file: request `A1-001` |

### Behavior

One runtime change, in `Supervisor::maintain`. With a link update unread, a
refused replacement attempt was never retried; the 15 s outage budget ran out
on the clock and the link ended as `BudgetExhausted` when the update was
finally read. The provider worker reads nothing from the supervisor while its
queue is above its high-water mark, so a stopped speaker put it there. A
failed attempt is now harvested, backed off and redialled whether or not
anything is read, inside the same attempt and budget bounds. A completed
setup still waits to be announced, as before.

Found by the new adversarial conversation, reproduced by two focused tests on
`2535fb565` (one attempt made where two, and three, were due), fixed in
`67154e0b6`.

### Checks

Host: macOS x86_64, Rust 1.96.0, source-only export of `548fda500` outside
the repository, fresh target directory. `548fda500` is `155bab4e2` plus seven
ignored worker tests, so this run covers both.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 849 passed, 0 failed, 11 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |
| `cargo test --locked --offline -p lamp-gemini --lib soak -- --ignored --nocapture --test-threads=1` | 2 passed in 553 s |
| `cargo test --locked --offline -p lamp-live --lib follows_output_capacity -- --ignored --nocapture --test-threads=1` | 2 passed in 270 s: 60 s answer played out at 70.03 s, 180 s answer at 200.05 s |
| `cargo test --locked --offline -p lamp-live --lib stopped -- --ignored --nocapture --test-threads=7` | 7 passed in 118 s; table below |

`lamp-gemini` contributes 136 of the 849. The earlier prolonged conversation
gives the same tallies as before this work: 843 completed, 417 cancelled, 107
settled, 232 lost, 477 recoveries, 145 assumed barriers over four seeds. The
adversarial one, four seeds of 400 turns through each interface: 679
completed, 386 cancelled, 225 settled, 309 lost, 249 refused, 1,453
recoveries, 234 refused attempts, 231 replacements dead on arrival, 341
speaker holds, identical rows through both interfaces.

Measured, real time, scripted service, worker through output capacity to a
modeled speaker, 2 s provider messages, every holder full when playback stops:

| Playback stopped for | Result |
|---|---|
| 25, 27, 28 s | Complete: 90 of 90 s delivered |
| 29, 30, 31, 33 s | `Backpressure`; 58.64 of 90 s delivered; the worker ended 33.7 s after playback resumed |

### Not run

- Linux/ARM64: not built or tested for these commits.
- Cloud: nothing. See `A1-004`.
- Device: nothing. I take no device action without a grant in `main.md`.
- The real coordinator: every test drives the supervisor or the worker
  directly.

### Known gaps

- A consumer that reads no link update for longer than the outage budget
  still loses the link if a replacement completes setup and then dies, and
  input stays refused until the earlier update is read. Both go away when the
  worker polls the link on its control tick.
- A stopped speaker past 28 s ends the session, not just the answer, and the
  failure surfaces only after playback resumes. See `A1-002`.
- `Require` in production: a person who continues before any answer has
  started ends the session after 2 s if the service never confirms the cut-off
  request. Only the `interrupt-before-output` probe can say whether it does.

## Requests

### A1-001 — Main: take or reject the stopped-playback tests

`548fda500` on `origin/gemini-voice-worker-proposal` adds seven ignored tests
to the `mod tests` of `provider_worker.rs` and nothing else. They produce the
table above. Cherry-pick it or tell me what to change; I will not touch that
file on `gemini-voice`.

### A1-002 — Main: should a stopped speaker cost the answer or the session?

`Error::Backpressure` is classed `Recovery::Fatal` in `lamp-gemini`. Past the
28 s limit the supervisor therefore ends terminally. Classing it as
reconnectable would report the cut request as lost and keep the session, which
only matters once `Typed` reporting is on; under `Legacy` the worker ends on
the lost request either way. It is a one-line change in my crate. I have not
made it because it changes what your worker sees. Say which you want.

### A1-003 — Main: one optional field for echo evidence

Draft, see below. When the estimator is published I will ask for
`MicrophoneFrame.echo: Option<lamp_audio::evidence::EchoEvidence>` in
`wire.rs`. There are two constructors of that struct: the capture worker,
which is mine, and a test helper at `coordinator.rs:2150`. The type is
serializable, 5 small integer fields plus an optional 4-field match, and
describes the frame it travels with, so the frame's own freshness rules apply.
Tell me if you would rather receive it another way.

### A1-004 — Main: service probes you can run on a board

The tool is `crates/gemini/examples/service_probe.rs`. It opens no microphone
or speaker, prints one JSON line of event kinds and millisecond timings, and
prints no transcript, audio, handle or credential.

Build, on the host or the board:

```sh
cargo build --locked --offline --release --target aarch64-unknown-linux-gnu \
    -p lamp-gemini --example service_probe
# target/aarch64-unknown-linux-gnu/release/examples/service_probe
```

Run, one probe at a time:

```sh
service_probe /absolute/private/provider.json QUESTION.wav PROBE
```

- `provider.json`: the runtime's own fields (`endpoint`, `model`, `voice`,
  `credential_file`, optional `authentication`, `thinking_level`,
  `language_code`). It and the credential file must be regular files with mode
  0600; anything else is refused before any connection.
- `QUESTION.wav`: 16-bit PCM, any rate and channel count, holding 0.5 to 30 s
  of speech after silence is trimmed. A short cached question is right.
  `service_probe x QUESTION.wav fixture` is offline and prints what would be
  spoken.
- Cost and bounds per probe: at most three spoken questions, at most two
  connections, and a fixed observation window.

| `PROBE` | Longest run | What to read in the output | What it decides |
|---|---|---|---|
| `interrupt-before-output` | about 45 s plus twice the question | Events are timed from the second `activityStart`. Does `interrupted` or `turn_complete_idle` for request 1 appear, and how many ms before `first_audio` for request 2? Or `no_confirmation_within_barrier` (the probe waits 5 s where production waits 2)? Any `first_audio` for request 1, or a `dropped:` line? | Whether `Require` can survive a person who continues before the answer starts |
| `handles` | about 45 s | `handle_after_setup`, `handle_after_one_answer`, `handle_current` | Whether recovery is available as configured today (`Resumption::Retain`) |
| `handles-requested` | about 45 s | The same three fields, or a setup error | Whether to switch to `Resumption::Request` |
| `resume` | about 90 s | `handle_issued`; `resumed_setup` is `accepted` or an error category; `resumed_setup_ms`; `first_audio` on the resumed session | Whether a reconnect can keep the conversation at all |
| `idle-close` | 300 s | `go_away` and `connection_ended:...` with their ms, or `observation_window_ended` | The idle-close time and code assumed from V1 (86 to 198 s, 1008) |

Please record the output lines verbatim with the endpoint and model named,
and nothing from the provider file. I will fold the answers into the handoff.
A probe for image input will follow with item 4.

### A1-005 — Agent 2 and Main: bound for a camera frame given to Gemini

Draft, not implemented. One WebSocket carries audio and images in order, so an
image delays the audio behind it by its own transmission time. I intend to
accept, per request, at most 2 JPEG images of at most 64 KiB each, no older
than 2 s at submission, through their own single slot (newest wins) so they
can never fill the audio queue. Camera frames are 1280x720 MJPG up to 2 MiB,
so the frame handed to the provider must already be scaled and re-encoded to
that bound, with its `FrameId` and `CameraGrant` so lineage and privacy can be
checked where it is submitted. Object if 64 KiB is too small for a useful
frame; I would rather learn that now.

## Draft work, not published

Item 2, echo evidence. Local and uncommitted; nothing here is a result yet.

- `crates/audio/src/evidence.rs`: an observer beside the echo canceller that
  reports, per capture block, the capture level before and after cancellation,
  whether accepted speaker PCM was recently non-silent, and, once capture and
  render are aligned, the share of the capture's energy and of the cancelled
  signal's energy that the render explains. Measurements, not a verdict: no
  alignment, no claim.
- On scripted scenes through the real canceller the share follows the
  mixture within a point (person at -6, 0, +6 dB relative to the echo: 76, 48,
  19 percent against 75, 47, 19 predicted), and a person alone is never
  attributed to the speaker.
- No device recording with its render reference is on this machine, so no
  threshold is chosen and none will be claimed from synthetic scenes.
- `lamp-audio replay-aec` will emit the evidence per capture block, so any
  recorded bundle can be scored offline, including the recorded false
  interruption once its raw files are available.

## Answers

- `MAIN-001`, read at `b20df467d`: acknowledged. I fetch the three branches
  at safe boundaries and read `main.md` and peers' files from the fetched
  refs.
- `MAIN-002`, read at `b20df467d`: published above. Order of work is as you
  set it: continuous audio and echo evidence, probes (`A1-004`), cached failure
  audio, then visual input, with environmental acquisition last.
- `lamp-v2-agent2` at `197fa0b96` has no `coordination/agent-2.md` yet:
  awaiting acknowledgment.
