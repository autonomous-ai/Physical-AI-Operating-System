# Agent 2 status

Owner: Agent 2. Workstream MAIN-051 (fast, reliable end-of-turn processing) on
`lamp-a2-endpoint-runtime`. Earlier work stays on its own branches:

- `lamp-a2-endpoint` (`ee1eaad8`): MAIN-043 candidate and evidence;
- `lamp-v2-agent2` (`95029a03`) and `lamp-v2-agent2-sim` (`1a89a9cb`):
  evaluator work.

## MAIN-051 checkpoint

- **Base.** Integration `e566db15`, merged at `32465121` without force push.
- **Tested code head.** `61112832`. This status file and
  [the endpoint detection doc](../docs/endpoint-detection.md) follow it.
- **State.** An opt-in candidate and integration proposal. Nothing is
  activated or deployed, and nothing ran on a device or the shared ARM VM.

### Delivered

1. **Isolated worker.**
   - Inference runs in the `lamp-endpoint-worker` process with pinned model
     bytes.
   - The core links `lamp-endpoint-ipc`, which has no model runtime;
     `lamp-live`'s dependency tree has no tract.
   - Bounded binary frames over pipes, one request at a time, utterance and
     worker identities, startup and hang deadlines.
   - Bounded replacement, with the old worker reaped first.
   - Linux parent-death kill; reap on runtime teardown.
2. **Fallback.**
   - Missing, loading, failed, hung, busy or closed workers keep the 1,200 ms
     window.
   - A test proves the fallback ends every turn on the baseline's exact block.
   - A full coordinator session with a missing model, and one with a hung
     worker, pass.
3. **Resumed speech.** Any speech-scored block ends a pause and obsoletes its
   verdict; opening and resumed audio are retained; late verdicts never end a
   later turn.
4. **Policy.** Three bands, with defaults: complete ≥ 0.9 ends at 1,000 ms;
   clearly unfinished < 0.01 holds to 1,500 ms; otherwise 1,200 ms. Every
   decision records its basis, fallback reason and verdict provenance.
5. **Evidence.**
   - Recorded human clips: development shards 0–1, fresh held-out shards 4–5.
   - The synthetic `say` cohort.
   - Profiles on macOS and the local Linux ARM64 VM.
   - All in the doc, cohorts separate.
6. **Integration proposal.** Shared coordinator, options and test changes are
   in separate commits (see the doc).

### Gates

- **macOS** (M2 Max, Rust 1.96.0):
  - `cargo fmt --all --check` passes.
  - `cargo clippy --workspace --all-targets -- -D warnings` is clean.
  - Endpoint crates and `lamp-live` tests pass, except two of Main's
    `coordinator::optional_ring_tests`. Those fail on macOS only: a datagram
    send to a dropped peer is not refused there. They pass on Linux and do
    not involve endpoint code.
- **Linux ARM64**: my own Colima VM on this Mac, not the shared office VM;
  Docker `rust:1.96.0-bookworm`, at `61112832`:
  - `cargo fmt --all -- --check` passes.
  - `cargo clippy --locked --workspace --all-targets -- -D warnings` is clean.
  - `cargo test --locked --workspace --no-fail-fast`: 1,363 passed, 0 failed,
    8 ignored, plus 9 self-hosted process tests passed.
  - With the model mounted: the `lamp-endpoint` parity and real-worker tests
    pass, and `real_worker_session_records_verdict_provenance` passes.

### Results (digital replay, not physical)

| Fresh held-out, 1,167 human clips | Added splits | Finished turns ending earlier | Mean delay |
| --- | --- | --- | --- |
| parity (proposed opt-in) | 6 (0.5% of clips) | 77% (1.2 → 1.0 s) | 1,047 ms |
| responsive (400 ms) | 338 (271 clips) | 77% | 583 ms |
| MAIN-043 candidate | 434 (333 clips) | 82% | 545 ms |

Endpoint savings are reported separately from cloud and output delay. The
parity profile removes at most 200 ms; it does not reach the reply target on
its own.

### Requests to Main

1. Compose or reject the proposal commits listed in the doc. The coordinator
   change adds one `endpoint` field to `Rig`, which touches every test `Rig`
   literal.
2. Device measurement when scheduled: worker startup, inference, RSS and CPU on
   the A733, and the coordinator loop under load.
3. Decide, with Agent 1, whether speech that resumes before the reply's first
   speaker write can continue the same request. Under the responsive profile,
   98% of split speakers resumed within 1 s of the endpoint. That is the only
   route to faster endpointing found here.
4. Physical cohorts are yours: synthetic loudspeaker replay, recorded-human
   loudspeaker replay and direct human at the Lamp.

### Remaining gaps

- No device or physical evidence. Human clips are digital, English and
  Spanish only, labelled only at the end, without transcripts.
- The parity gain is small, at most 200 ms of the endpoint wait.
- The worker's memory (59–190 MB resident by host) and its 18 MB stripped
  binary are unmeasured on the A733.
- Agent 2's tools could not deliver through the M2-to-office tmux route; the
  owner was told.
