# Agent 2 status

Owner: Agent 2 (end-of-turn and whole-thought workstream), on M2.
Communication with Main and Agent 1 is Harness Agent Messages and Git.

## Endpoint pilot composition on `675c71c62`

- **Branch.** `lamp-a2-endpoint-pilot`, proposal only, base `lamp-v2-chat`
  `675c71c62`. `lamp-a2-pilot` and `lamp-a2-pilot-limits` are unchanged.
- **Tested code SHA.** `b1f43e50a`. The later docs commit changes only
  `docs/endpoint-pilot.md` and this file.
- **Content.**
  - The ten package commits and their sharing are listed in
    [the source closure](../docs/endpoint-pilot.md#source-closure).
  - Defaults are unchanged. Without `--endpoint smart-turn` no worker starts,
    and the baseline trace records keep their exact fields.
  - `smart-turn-early` is absent.
  - New on this base:
    - smart-turn also on duplex, which gates the same baseline detector;
    - `--endpoint` as a typed voice-eval runtime flag beside `--microphone`;
    - an [opt-in trial configuration](../docs/endpoint-pilot.md#opt-in-latency-trial-on-cached-speech).
- **Gates.**
  - macOS on `549daeb7`:
    - endpoint crates with the model, and IPC 12/12;
    - live `endpoint_session`, `pilot_faults`, `duplex`, options and activity
      90/0, and the real-worker session test with the release worker;
    - `endpoint_confirmation` 22/0;
    - voice-eval flags 8/0 and `serve_plan` 6/0.
  - Final ARM gate on `b1f43e50a` (M2 Linux ARM64 VM, Rust 1.96.0):
    - fmt and strict workspace Clippy;
    - `lamp-endpoint` with the model 16/0 and IPC 16/0;
    - real-worker `endpoint_session` 14/0 and `endpoint_confirmation` 22/0;
    - fallback suites (`pilot_faults`, `duplex`, options, activity) 79/0;
    - voice-eval flags 8/0 and `serve_plan` 6/0;
    - release builds and three `--check-model` runs, all exit 0.
  - No full workspace test run.
- **A733 bundle.**
  `lampOS/.cache/endpoint/artifacts/b1f43e50a/lamp-endpoint-check-b1f43e50a.tar`
  on M2, SHA256 `71d5506e…162d`:
  - worker `58852eb0…d7ec`;
  - model `ab8dc64b…39be`;
  - commands are in
    [the bundle section](../docs/endpoint-pilot.md#a733-check-bundle-ready-to-copy).
- **Not established.** A733 cost, real conversation, acoustic latency and the
  continue-before-output barrier path. My recommendation is unchanged:
  baseline first, and the candidate only in a separately labelled session.

## MAIN-057: conservative endpoint pilot package

- **Branch.** `lamp-a2-pilot`, base `lamp-v2-chat` `892ec0a1`.
- **State.** Ready for Main to compose; not enabled by default.
- **Package.** [docs/endpoint-pilot.md](../docs/endpoint-pilot.md):
  - source closure and asset/dependency manifest;
  - preparation and `--check-model`;
  - measured bounds and stress evidence;
  - review of the six held-out splits;
  - the acceptance and rollback rule.
- **Subsystem reference.** [docs/endpoint-detection.md](../docs/endpoint-detection.md).

Commits (each applies alone, in order):

| Commit | Content |
| --- | --- |
| `7b76b04b` | Endpoint worker and IPC crates (reviewed code from `052eb703`, `425a76a7`) |
| `2c4ee1a7` | Workspace members, `lamp-live` → `lamp-endpoint-ipc`, lockfile (65 added entries, none removed or changed) |
| `47a59d02` | Detector seam, baseline-exact pauses, parity policy; no early end |
| `13495b3a` | Shared coordinator integration (`--endpoint smart-turn`), confirmed-reap teardown, full-session pilot stress tests |
| `fa33308f` | Worker `--check-model` self-check; Linux core-file, address-space and priority bounds; damaged-model and idle tests |
| `4402caeb` | Emulated endpoint sessions run one at a time |
| this commit | Docs and this status |

### Not in the pilot

- `--endpoint smart-turn-early` and the evaluation crate stay on
  `lamp-a2-continuation` (`a86546cf`).
- Main's strict continuation probe failed with `BarrierTimeout`, so the early
  mode stays off. No barrier, provider, camera or motor code changed.

### Recommendation

Run the first attended sessions on the baseline. Enable
`--endpoint smart-turn` only in a separately labelled session under the
rollback rule.

- Parity takes the continue-before-output barrier path in 16 of 1,167
  held-out clips; the baseline takes it in 9.
- On that path a real Lamp run and the strict probe ended in
  `BarrierTimeout`. Main's one successful trial continued during output;
  before-output remains unresolved, and every case parity adds is before
  output.
- The candidate saves at most 200 ms.

### Remaining blockers

1. **Composition onto current canonical.** The closure applies cleanly on
   `892ec0a1`. A trial merge onto `55d7af98` (no branch changed) conflicts
   in two root-owned files; I have not merged them, as Main asked.
   - `coordinator.rs` has two conflicts:
     - Detector setup: canonical adds `TurnTaking::Duplex`, its own
       `endpoint_policy` trace and the duplex admission/mask.
       `EndpointRig::attach` must take that match and keep the duplex code.
     - The test module list: keep both `duplex` and `endpoint_session`.
   - `options.rs` has three additive conflicts: the fields, the validation
     and the test literal. Canonical adds the `reference_clock` and
     `reference_recovery` options; keep both.
   - `smart-turn` stays limited to `v1-baseline`, not `duplex`.
2. **No A733 measurement.** `--check-model` on the device is the first
   startup, inference and memory evidence there.
3. **Provider before-output settlement** is unresolved. This is why I
   recommend the baseline for the first attended sessions.

### Validation

Tested tree: snapshot `5d1bfcfc`, whose code is identical to `4402caeb`
(`git diff 5d1bfcfc 4402caeb` outside docs and coordination is empty). The
600 s idle evidence comes from snapshot `3038ab18`; since then the endpoint
crates have changed only in one comment.

**macOS, M2 Max:**

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: clean.
- `cargo test --locked --workspace --no-fail-fast`, before the session
  serialization: 1,395 passed, 1 failed.
  - The failure is in Main's `process::ring_startup_tests`.
  - Main's `optional_ring`, `ring_startup` and `ring_trace` tests fail
    intermittently in parallel on macOS. The untouched base `892ec0a1` fails
    4–6 of them in each of five runs.
  - They pass serially (`ring_startup`, 11/11, twice).
  - After the session serialization, three `lamp-live` runs had no
    `endpoint_session` failure. Two were fully green; the third failed 7 of
    Main's tests.
- `cargo test --locked --release -p lamp-endpoint -- --include-ignored`:
  9/9, including the real model and worker.
- `cargo test -p lamp-endpoint-ipc --test process`: 12/12.

**Linux ARM64, my own M2 Colima VM** (`rust:1.96.0-bookworm`, 6 vCPUs):

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: clean.
- `cargo test --locked --workspace --no-fail-fast`: exit 0, no failure.
- `cargo test --locked -p lamp-live --lib`: three more runs, each 398 passed,
  0 failed.
  - Before the session serialization, one of three runs failed five session
    tests with "physical privacy closed or lost freshness", together with
    Main's provider timing tests.
  - Base `892ec0a1` failed one of Main's ring tests in two of three runs.
- `cargo test --locked --release -p lamp-endpoint -- --include-ignored`: 9/9
  with the real model.
- `cargo test --locked --release -p lamp-endpoint-ipc`: all pass; process
  tests 12/12.
- `LAMP_ENDPOINT_WORKER=… cargo test --locked -p lamp-live --lib endpoint_session -- --include-ignored`:
  12/12, including the real worker and model.
- `LAMP_ENDPOINT_IDLE_SECS=600`, both idle tests pass:
  - the real worker: same process, no kill; VmRSS 59,772 KiB and VmHWM
    102,648 KiB, unchanged after the idle and after 50 more checks;
    `/proc` shows core limit 0, address space 268,435,456 and niceness 10;
  - the scripted worker: 600,031 ms, then it answers.
- `lamp-endpoint-worker --check-model`, quiet:
  - load 157–175 ms, first classification 96–98 ms;
  - warm 94–96 ms median, 96–101 ms max; 95–97 ms CPU per check;
  - VmPeak 116,176 KiB, VmHWM 102,520 KiB.
- The same check right after a release build: load 334 ms, warm max 130 ms.
- Under a 96 MiB service cap the worker aborts (exit 134) with no core file.
- Sizes: `lamp-live` 8,406,016 → 8,506,584 bytes; worker 24,233,112 bytes
  (stripped 17,776,360).

Not run: no Lamp, shared office VM, cloud service or physical audio cohort.

## Earlier checkpoints (unchanged)

- MAIN-051: `lamp-a2-endpoint-runtime` `350bd572`.
- Review fixes: `lamp-a2-endpoint-review` `052eb703`.
- MAIN-052 early-end experiment and evaluation crate: `lamp-a2-continuation`
  `a86546cf`.
- MAIN-043: `lamp-a2-endpoint` `ee1eaad8`.
