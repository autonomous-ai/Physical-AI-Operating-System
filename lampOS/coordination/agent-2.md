# Agent 2 status

Owner: Agent 2 (camera, perception and evaluation), branch `lamp-v2-agent2`.

- **Protocol:** `direct-and-git-v2` acknowledged (`MAIN-001`), read from
  `origin/lamp-v2-chat` at `666693a84`, re-read at `51bd815e9` (MAIN-010).
  Direct route: Harness team "Lamp Rust implementation", member `agent2`;
  two-way delivery with `main` is confirmed.
- **Updated:** 2026-10-11
- **Tested head:** `8856dac9` on `lamp-v2-agent2` (`MAIN-034` importer fix).
  Gate in the Linux ARM64 container:
  - Workspace `fmt` clean; workspace tests 1018 passed, 0 failed, 4 ignored.
  - Strict workspace clippy: the first invocation ended in a transient
    container TLS/network error; the rerun on the same commit was clean.
  - Main integrated the evaluator subset through `9ed94e42` as `2a1522eb`
    (`MAIN-034`). `9ed94e42` passed 1015, failed 0.
- **State:**
  - MAIN-014 and MAIN-015 fixes are published: runner P2, MAIN-013 evaluator
    alignment and context-frame privacy P1.
  - `MAIN-023` final-dispatch fix: `d3ada11e`. `MAIN-025` runner and backend
    eligibility fix: `37b398a7`. Root's four audit regressions are included
    and green (see answers).
  - `MAIN-021` delivered as three commits (see answers):
    - certified target diagnostics verification: `02529e7d`;
    - room evidence from a plain WAV and recorder receipt: `0a0cfdcf`;
    - automatic boundary proposals, kept separate from annotations:
      `fb096dce`.
  - Process simulation is published as a draft branch, not for
    integration: `lamp-v2-agent2-sim` @ `1a89a9cb`, based on `b7fbe352`
    (see `MAIN-018` below and `docs/process-simulation.md` on that branch).
    Gate on `1a89a9cb` in the Linux ARM64 container: fmt and strict
    workspace clippy clean; voice-eval 155 passed, 0 failed. With the unmodified
    `lamp-live directed-fixture`, the real `physical-run` validated every
    real cue with 0 protocol errors.
- **Full report:** [Agent 2 handoff](../docs/agent2-handoff.md), checkpoint 2
  section, covering commits, red/green evidence, exact checks and open items.

## Checkpoint 2 summary

| Check on `8a1cc9b3` | Result |
|---|---|
| macOS: `fmt` / strict workspace clippy / `cargo test --locked --workspace --no-fail-fast` | Clean / clean / 921 passed, 0 failed, 4 ignored |
| Linux ARM64 container: same | Clean / clean / 960 passed, 0 failed, 4 ignored |
| Linux ARM64 container on `0253e5a5` (corrections only, before the merge) | Clean / clean / 948 passed, 0 failed, 3 ignored |

No device, camera, real person or audio was involved.

## Requests

### A2-001 — Main: accept or reject shared edits on this branch

1. The `worker camera` arm of `crates/live/src/main.rs` passes
   `--frame-consumer ROLE=BOOT` to `camera_worker::run` (`197fa0b9`).
2. The `crates/perception` workspace member (`5663f75a`).
3. `Cargo.lock`: 66 third-party packages from `tract-onnx =0.23.8` and
   `zune-jpeg =0.5.15`, plus the dev-only `jpeg-encoder` (`5663f75a`).

### A2-002 — Main: allow the `perception` worker role in `transport.rs` (done here as approved: `7912fa07`)

`WorkerChannels::bind` rejects `perception`. The worker binds
`perception.{c,d}.w` with `Channel` directly, using the same names and envelopes,
so `Worker::spawn` will work unchanged once the role is allowed.

### A2-003 — Main: process-simulation seams for plan item 3

These are needed to run the actual coordinator and worker graph with synthetic
devices. Details are in [interfaces §4](../docs/agent2-interfaces.md#4-synthetic-hardware-ports-for-process-level-tests).

- `HardwareProfile { worker_executable: Option<PathBuf>, legacy_owner_check: bool }`
  on the coordinator entry. Defaults stay as they are: current executable,
  check on.
- `provider_worker::run_with_connector<C: Connect>(channels, boot, connector, options)`.
- `physical_privacy::run_with<S: SwitchInput>(channels, switch)`.

I will not edit those files without your decision. If you prefer, I can submit
them as separate single-purpose commits for your review.

### A2-004 — Main: coordinator wiring for camera and perception

- **Spawn order:** provider, then `lamp-perception worker perception DIR PARENT
  WORKER --model-dir ABS_DIR [--frame-consumer context=PROVIDER_BOOT]`, then the
  camera worker with `--frame-consumer perception=PERCEPTION_BOOT`.
- **Authority:** send authority to perception only after `Ready`.
- **Events:** decode `lamp_perception::observation::PerceptionEvent` from its data
  channel and track it with `ObservationTracker::new(perception_boot)`.
- **Restarts:** a frame link accepts each consumer identity once, so a restarted
  consumer needs a new producer incarnation.
- **Visual context:** add a `VisualContext { owner, max_age_us }` control toward
  the provider (see `A1-006`).

## Answers

- **`MAIN-034`** (importer review of `02529e7d` / `0a0cfdcf` / `fb096dce`):
  fixed in `8856dac9`, importer-owned files only.
  - **Red first on `7edd8d94`:**
    - receipt frames of `u64::MAX` panicked (multiply overflow,
      `import.rs:498`);
    - a correctly hashed diagnostics marker with absurd frames panicked
      (`target_audio.rs:164`);
    - cached evidence rebound to a different valid WAV, with the receipt
      unchanged, passed review.
  - **Fix:**
    - Receipt frames are bounded to 600 s and diagnostics marker frames to
      the stream byte bound, before any arithmetic (checked multiply). Render
      span sums are checked.
    - At review, the receipt is read again, its hash checked and the selected
      input parsed and validated by the same function as at import. WAV path,
      digests and frames are re-derived from it, cached values that disagree
      are refused, and the WAV is verified against the receipt's claims.
  - **Green:** three new regressions pass; voice-eval 196 passed, 0 failed.

- **`MAIN-030`** (review of `ec84cf72`): fixed in `9ed94e42`.
  - **Red first on `c2efaad1`:** root's two tests from
    `lamp-voice-eval-review2-20261011` (`cdc0ec26`), with unchanged bodies:
    the overdue target started once (`late_us` 274947, `withheld` none), and
    a consumed post-exit `run_end` trace still started the player.
  - **Fix in `PhysicalBackend::inject`:**
    - Immediately before the player starts, the target's 100 ms bound is
      rechecked on the local clock, even with no new event; a new
      `Withheld::Stale` gives `trigger_stale`.
    - The final drain is bounded at 1024 relay lines; when it runs out,
      `revalidate` with `drain_exhausted`.
    - The first relayed trace record latches `session_ended`, because the
      relay streams the trace only after the runtime exits. The trace
      evidence is kept.
    - The 1 ms wait uses saturating arithmetic.
    - Terminal causes still take precedence.
  - **Tests:** a new budget-exhaustion test with a 2000-line flood, then a
    single start. The test rig now measures scene timing once, as the runner
    does, instead of inside the target window.
  - All 13 `final_dispatch` tests and 193 voice-eval tests pass.

- **`MAIN-021` / `MAIN-027`:** delivered offline and file-only. No device,
  transport or audio output was used.
  - **`02529e7d` `target_audio`:** verifies `render-audio/` and
    `capture-audio/` against the trace's `audio_diagnostics_started` and
    `audio_diagnostics_certified`.
    - It calls lamp-live's own `diagnostic_control::certify`.
    - It re-hashes every PCM and record file against the completion marker
      and checks lengths against the certified frame counts.
    - It reads the render timeline: contiguous offsets, ALSA acceptance host
      times, silence kinds and owning turns.
    - A missing, uncertified or tampered stream is unverified, with the
      reason, and never adapted.
    - Tests use real `Recorder` output.
  - **`0a0cfdcf` `import-trace --room-receipt FILE --room-input NAME`:**
    validates root's exact receipt schema. The fixture is
    `recording-import-examples.json` from `3998a8dc`.
    - Checks: a completed, valid capture with privacy never revoked; the
      recorder accepted the input; an owned, graceful stop; mono 16 kHz
      S16_LE; consistent frame counts and digests; a plain sibling WAV
      matching the file hash, the frame count and the received-PCM digest.
      The WAV's data chunk is hashed separately for that last check.
    - Only bounded, nonsecret fields are kept, never stderr.
    - At review time the receipt must be unchanged and the WAV is re-read.
    - Root's real invalid receipt (microphone permission closed) is retained
      and refused. The attempt stays, with unmeasured acoustics.
    - The import also records `target_audio`.
  - **`fb096dce` `propose-boundaries RUN_DIR NEW_FILE [--stimulus-wav FILE]`:**
    proposals in a separate file. They are never scored, `listened` stays
    false, and the file does not parse as annotations.
    - Each certified render reply segment is resampled from 24 to 16 kHz and
      located in the room recording: envelope correlation, then waveform
      refinement within ±20 ms, each segment separately so clock drift stays
      local.
    - Each reply reports its status (located, ambiguous or not found), score,
      margin, alternatives, polarity, clipped fraction and uncertainty.
    - Thresholds are a minimum score of 0.2 and a minimum margin of 0.05,
      both unqualified.
    - The reply onset is the earliest rendered reply audio, never a later,
      clearer word.
    - The stimulus speech end comes only from an exact stimulus WAV;
      otherwise it is `unavailable` and never taken from transcript text.
    - End to end on synthetic signals: reply onset and end, and stimulus
      start and end, all within 2 ms.
  - **Not done:** proposals use the room WAV only. They do not yet map
    target host events onto room time through the capture stream. No real
    recording has been run through them.

- **`MAIN-025` / `MAIN-026`:** fixed in `37b398a7` on top of `d3ada11e`.
  - **Root's tests:** the four audit regressions from
    `lamp-voice-eval-review-20261011` (`60f98cb9`) are copied as
    `tests/audit_final_dispatch.rs` and `tests/audit_runner_dispatch.rs`.
    One diagnostic print was adapted to the new type.
    - On `fac55a5a`, the two backend cases were already green (`MAIN-023`).
    - The two runner cases were red: injections `[501000, 3200000]`, and an
      injection after 200 ms of drain work. All four are green now.
  - **Runner:** after the final drain, `ineligible()` rechecks every condition
    at the side-effect boundary:
    - session open;
    - provider available;
    - lateness within 100 ms (`trigger_stale`);
    - Lamp still speaking for overlap steps (`precondition_lost`).
    The drain is bounded at 256 events.
  - **Backend:** if its own final drain took any event the runner has not
    observed, nothing starts. It returns `Withheld::Revalidate` with
    `unobserved_events`; the runner observes those events, rechecks and
    retries, at most 8 times, otherwise `trigger_stale` with the reason.
    Terminal causes keep precedence.
  - **New tests:**
    - backend: an unobserved event in the drain, then a single start;
    - runner (scripted): harmless revalidation dispatches once (receipt
      `revalidations: 1`); a retirement taken in the backend drain gives
      `precondition_lost`; a dispatch that never settles gives `trigger_stale`.
  - **Contract change:** `Injection.withheld` is `Option<runner::Withheld>`
    (`ProviderFailed`, `SessionEnded`, `ProtocolFailed`, `Revalidate`), not
    `Option<String>`. A `Backend` that never drains returns `None` as before.

- **`MAIN-023`** (final-dispatch blocker): fixed in `d3ada11e`.
  - **Red first:** 7 new regressions drive the real `PhysicalBackend` over a
    local pipe transport with a recording-only player. On `b7fbe352` all 7
    started the player once.
  - **Cases covered:**
    - known `session_end`, invalid cue and `run_end` cue;
    - `session_end`, invalid cue, relay framing error and transport close
      first seen in the final drain;
    - first-cause precedence with a real `provider_failed` completion.
  - **Fix:**
    - The backend latches the first terminal cause (provider failure,
      session ended, or protocol failure) and withholds with
      `withheld_cause` and a precise `reason`.
    - Only provider failure maps to `provider_unavailable`; the other causes
      map to `session_ended`, with the cause in `detail`.
    - The final drain now detects a closed transport.
    - A relay framing error now ends live scheduling as a protocol stop.
  - **Gate:** see the status above.
- **`MAIN-021`** (answers `A2-005`): accepted. No second live device driver.
  - Building an offline evidence import: plain WAV plus a validated recorder
    receipt and target diagnostics. Format, length, digest and provenance are
    required; missing certification or clock mapping stays unverified.
  - Building automatic boundary proposals with confidence and alternatives,
    kept separate from confirmed annotations (`listened` stays false), with
    no acoustic p50/p95 from proposals.
  - Root's stimulus is not SHA-pinned, so proposals say when exact source PCM
    is unavailable.
  - Cue answer noted: deployed `51bd815e9` directed mode emits the seven
    ordinary cues.
- **`MAIN-020`:** acknowledged; the contract was read at `dccc0fa3` (below).

- **`MAIN-013` / `MAIN-016`:** acknowledged, read at `lamp-v2-chat`
  `dccc0fa3` (`coordination/failure-cue-contract.md`). The candidate status
  is retained until the combined commit.
  - Already aligned in `9b82031c` and `4ea4fb49`:
    - `turn_completed` / `turn_finished` with `provider_failed`, with
      `provider_session` and `provider_outcome` preserved verbatim.
    - The unowned trace-only `input_discarded_endpoint`, read from
      `last_block_host_read_us`.
    - No re-ready: unavailability latches for the attempt.
    - Failure is never in the success numerator.
- **`MAIN-018`:** acknowledged. Simulated EPIPE/ReferenceLate reproductions
  are retained with a source SHA and no acoustic claim. Full record:
  `docs/process-simulation.md` on `lamp-v2-agent2-sim`.
  - **Clean rerun** of source `0b00407f`: 1/20 trials failed at echo 0.3 and
    1/20 with echo off. A confirmation run gave 2/3 and 0/3.
    - Echo is not required for the failure.
    - Whether echo raises the rate is unresolved: pooled, 3/23 vs 1/23.
    - My earlier 3/8 vs 0/8 claim is retracted.
  - **Behaviour:** one speaker underrun (EPIPE in `snd_pcm_avail_delay`)
    ends the whole session. ReferenceLate follows when the reference stops.
  - **Scheduling:** in the VM, a 1 ms sleeper never waited more than 13.6 ms,
    even during failing trials. The cause, simulated audio path or a
    lamp-live speaker stall, is open.
  - **Runner against real cues:** 9 attempts with 0 protocol errors.
    - Triggers fired on real `listening_ready` and `speaker_first_write`.
    - Clock mapping within ±1.1 to ±2.8 ms.
    - 4 of the 9 attempts ended on these underruns. The 2 topic-change
      attempts were correctly withheld because the 2 s simulation reply
      ended before the interruption was admitted.
  - **For the pilot:** a `run_end failed` with this EPIPE or ReferenceLate
    on the Lamp matches the simulated repro.

- **`MAIN-005`**: all five findings fixed, each with a regression shown
  failing first.
  - (1) Exhausted perception drains: `343f9a41`.
  - (2) Dependency cycle: `29c17173` and `0253e5a5`, with the shared
    registration kept narrow.
  - (3) Blocking connect: `206813b5`.
  - (4) Raw-byte relay bound: `5220ce20`.
  - (5) Freshness contract: `206813b5`.
  - B: the seams remain proposals, behind an explicit test-support feature.
  - C: the next target is `run_directed_session` with scripted Gemini.
- **`MAIN-006`**: the relay group kill after an early return is fixed in
  `5220ce20`. On the old code the regression left a worker running.
- **`MAIN-009`**: accepted and implemented provisionally in `8bfff000`. Final
  field names wait for the frozen producer schema. Please include the exact
  names for `provider_failed` and `input_discarded_endpoint`. (My direct reply
  expired before delivery; this is the durable answer.)
- **`MAIN-014`**: acknowledged. Camera, perception and image work stays
  inactive and does not gate the first audio-only run. The narrow runner fix is
  `b010aa0b`.
- **`MAIN-015`** (review of `bb7e61ae`):
  - **P2:** `4ea4fb49`. A provider failure latches for the attempt. It is
    rechecked while waiting for a trigger, during the delay and before
    dispatch. `PhysicalBackend` also checks before the player starts. Your
    exact repro is a test.
  - **MAIN-013 alignment:** `9b82031c`. `provider_session` and
    `provider_outcome` are kept. `provider_failed` is scored before Late,
    and lateness stays a separate finding. Raw failure and drain records are
    kept as evidence.
  - **P1:** `ddc771a9` and `d5b848b3`. A context tail whose grant changed,
    closed or lapsed is abandoned before any continuation write. The
    reconnect policy is defined, the send buffers are small, and there is
    `ContextServed` provenance. Denial, newer-grant, lapse and queued-Stop
    regressions cover it.
  - **MAIN-013 itself was never received.** Please resend it or commit it to
    `main.md`.
- **`MAIN-010`**: acknowledged by direct reply. `51bd815e9` is merged in
  `8a1cc9b3`.

- **`MAIN-001`**, read at `666693a84`: acknowledged. I fetch all three branches
  at startup, on a checkpoint notice, before a shared-interface change and
  after publishing.
- **`MAIN-003`**, read at `666693a84`: published above and in the handoff.
  Interface decisions needed from Main are `A2-001` through `A2-004`.
- **`MAIN-004`** (direct): acknowledged by team reply.
- **`A1-006`**, read at `origin/gemini-voice`: agreed.
  - The provider never receives camera-sized frames. The perception worker will
    serve **reduced context frames** over its own frame link
    (`perception.frames`, role `context`, the provider's boot given by the parent).
  - Each frame is the newest frame perception already holds, re-encoded so that
    it is at most 64 KiB. Perception downscales to at most 640 pixels wide and
    steps JPEG quality down until it fits; if it never fits, nothing is sent.
  - `FrameMeta` keeps the original camera frame identity, grant and dequeue
    time, and adds the reduced size.
  - The provider applies its own 2 s age bound, its 2-image limit and its
    single slot.
  - Being implemented now. Object if you need a different width.
- **`A1-007`**: acknowledged; thank you. I will consume the `PcmPort` shape in
  the simulation harness's room model (stimulus in, accepted speaker PCM back
  at a configured echo gain and delay) once you publish it, and I will review
  the shape when you post it.
