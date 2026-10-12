# Agent 2 status

Owner: Agent 2 (end-of-turn and whole-thought workstream), on M2.
Communication with Main and Agent 1 is Harness Agent Messages and Git.

## Opt-in fast endpoint on `7e28dc7ac`

- **Branch.** `lamp-a2-endpoint-fast`, proposal only. Its base is
  `lamp-v2-chat` `7e28dc7ac` (with `f9c6b0ea8`) plus the pilot through
  `ad0b2e329`.
- **Tested code SHA.** `57ed58110`. The docs commit after it changes only
  `docs/` and this file.
- **Package.** [docs/endpoint-fast.md](../docs/endpoint-fast.md). Commits:

  | Commit | Content | Shared? |
  | --- | --- | --- |
  | `209ae425f`, `9e23f60bf` | `EarlyEnd`, provisional confirmation, continuation events and hold (`activity.rs`, `tests/endpoint_confirmation.rs`) | Shared (`activity.rs`) |
  | `97d939b7b` | `--endpoint smart-turn-early`, refusal rules, `Reply::input_end_held`, the hold and release, traces, the test-only removed-hold switch (`coordinator.rs`, `coordinator/{cloud,endpoint}.rs`, `options.rs`, `activity.rs`, one test field in `coordinator/tests/turn_taking.rs`, voice-eval `physical/session.rs`) | Shared |
  | `d7e164df8`, `57ed58110` | Real-loop sessions (`coordinator/tests/endpoint_fast.rs`), test visibility in `endpoint_session.rs`, one `mod` line in `coordinator.rs`, the test-only `Termination::shared` | Agent 2 tests (one shared line) |
  | `1b7cf291b` | `crates/endpoint-eval` with the kept-start seam; workspace member and one lock entry, no third-party crate | New crate |
  | docs | `docs/endpoint-fast.md`, `docs/endpoint-detection.md`, `docs/endpoint-pilot.md`, this file | Agent 2 files |

- **Defaults unchanged.**
  - `silence` stays the default.
  - `smart-turn` keeps its 1,000 / 1,200 / 1,500 ms profile and its exact
    records.
  - `smart-turn-early` is refused without `--provider-continuation
    kept-start` and the baseline detector.
- **Gates (M2 macOS only).**

  - fmt and strict Clippy (`-D warnings`, all targets) for `lamp-live`,
    `lamp-voice-eval` and `lamp-endpoint-eval` on `57ed58110`. The same
    checks also pass on each intermediate commit, `97d939b7b` and `d7e164df8`.
  - `lamp-live`, everything except the motor and optional-ring groups, run
    in parallel: lib 562/0 (5 ignored); integration suites 188/0, including
    `endpoint_confirmation` 30/0.
  - The motor and optional-ring groups, serially: 46/0.
    - In a parallel run they time out starting children on this host.
    - They fail the same way at the unchanged base `7e28dc7ac`, so the cause
      is the host, not this change.
  - `endpoint_fast` (17 real-loop sessions): 17/0 in each of three
    consecutive runs.
  - voice-eval: 285/0.
  - Held-out turn evaluation: shards 4 and 5 at verdict costs of 130, 400
    and 550 ms ([results](../docs/endpoint-fast.md#held-out-evaluation)).
  - Not run: the Linux VM, the office VM, any device or service, or audio.

- **Not established.**
  - Gemini settling a request interrupted before output, and keeping both
    parts of a continued thought in context.
  - Real provider latency, Lamp acoustics, A733 verdict time, and physical
    Stop and mute.
  - Main schedules every service and device trial.

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
