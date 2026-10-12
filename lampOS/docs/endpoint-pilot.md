# End-of-turn pilot package (MAIN-057)

This is a conservative, opt-in endpoint package for Main to compose into the
supervised pilot ([supervised-pilot.md](supervised-pilot.md)).

- **Base.** The current composition is on `lamp-v2-chat` `675c71c62`, branch
  `lamp-a2-endpoint-pilot`. The first package (`lamp-a2-pilot`, base
  `892ec0a1`) and its limit fix (`lamp-a2-pilot-limits`) are unchanged.
- **Where it ran.** Nothing here ran on a Lamp, the shared office VM or a
  cloud service. Every measurement comes from an M2 Max and an ARM64 Linux VM
  on that host. Neither is an A733.
- **Default unchanged.** Without `--endpoint smart-turn`, no endpoint worker
  starts and the detector, admission, playback and provider paths are the
  base's. The `endpoint_policy` and `local_endpoint` trace records keep
  exactly their existing fields.

## What the pilot runs

| | Command line | Endpoint |
| --- | --- | --- |
| Baseline (unchanged default) | `lamp-live directed … --turn-taking v1-baseline`, or `lamp-live serve … --turn-taking duplex` | 1,200 ms of quiet; no endpoint worker starts |
| Candidate | The same command plus `--endpoint smart-turn` | Parity profile: a confident complete verdict ends at 1,000 ms of quiet, a clearly unfinished one at 1,500 ms, anything else and every failure at 1,200 ms |

Duplex gates the scores that the same baseline detector sees, so the
candidate confirms duplex pauses exactly as V1-baseline pauses. `vad-only`
has no silence fallback and refuses `--endpoint smart-turn`.

- **Rollback.** Restart without `--endpoint smart-turn`. Nothing else changes:
  - no rebuild is needed;
  - the worker and model may stay installed, unused;
  - no default, threshold or provider setting differs.
- **Excluded from the pilot**, and absent from this branch:
  - `--endpoint smart-turn-early` and the continuation hold
    (`lamp-a2-continuation`). Main's strict continuation probe failed with
    `BarrierTimeout`
    ([result](verification/strict-continuation-20261011/README.md)).
  - The evaluation crate `lamp-endpoint-eval`.
  - Any change to the barrier policy, provider, camera or motor code.
- **Expected effect.** At most 200 ms less endpoint wait on confident finished
  turns: 77% of finished held-out turns. It does not shorten the 1.1–1.6 s from
  the endpoint to the first ALSA write.

### Recommendation

The package is ready to compose. **I recommend that the first attended
sessions run the baseline**, with the candidate enabled only in a separately
labelled session under the rule below. The reason is the continue-before-output
path:

- A person who resumes after the endpoint but before the reply is audible
  takes the interruption barrier.
- On this path one real Lamp run and Main's strict probe both ended in
  `BarrierTimeout` ([real-device-audio-pilot.md](real-device-audio-pilot.md),
  "Repeated trial").
- Main later reported one real-service trial in which a continuation
  *during* reply output succeeded under `Require` and the 2 s bound. That
  is a different path: before any output, settlement remains unresolved, and
  Agent 1 is correcting the at-commit timeout separately. Every case parity
  adds is before output.
- In the held-out turn model, parity takes that path in 16 of 1,167 clips
  against the baseline's 9.
- The saving is at most 200 ms, while a barrier failure ends the provider
  connection.
- The [review of the six added splits](#review-of-the-six-added-held-out-splits)
  shows the cases behind this.

This is Main's decision; nothing in this package enables the candidate by
default.

## Opt-in latency trial on cached speech

The trial compares the candidate with the existing cached-speech cohorts and
changes nothing else.

- **Configuration.**
  - Use `fixtures/voice-eval-serve-endpoint-trial.example.json`. It is the
    installed pilot configuration plus `"endpoint": "smart-turn"`.
  - A dry-run test proves the two differ in nothing else and that the runner
    forwards exactly the pilot flags followed by `--endpoint smart-turn`.
  - Keep the same plan (`fixtures/voice-eval-serve-v1.json`), cached WAVs and
    manifests (no synthesis), Lamp, room, microphone selection and output
    volume as the baseline cohort it is compared with.
  - Loudspeaker and direct-human cohorts stay separate.
- **Pairing.** Alternate baseline and candidate attempts in one sitting,
  labelled per attempt. Earlier sittings are context, not a control.
- **Per turn, from the runtime's own records:**
  - `local_endpoint.includes_silence_wait_ms`: the wait actually used, 1,000,
    1,200 or 1,500 ms.
  - `local_endpoint.endpoint`: the basis (`model_complete`,
    `model_incomplete`, `model_uncertain`, `silence_fallback`) and the
    fallback reason.
  - `endpoint_worker` availability changes, and `endpoint_worker_final`
    (`reaped` must be true).
- **Measures.**
  - The distribution of waits.
  - Endpoint to first ALSA-accepted write, the boundary already reported.
  - Their sum, the same estimate Main reports (for example 1.2 s + 1.275 s in
    [pilot readiness](verification/pilot-readiness-20261012/README.md)).
  - Acoustic timing comes only from verified, listened room recordings.
- **Expected effect, and its limits.**
  - At most 200 ms less on confident completed turns.
  - The reply path after the endpoint is unchanged.
  - Different generated replies prevent a causal reply-latency claim.
- **Fallbacks kept:**
  - A missing, loading, failed, hung or late worker ends on the 1,200 ms
    window, with its reason.
  - Interruption admission, the duplex yield, playback and the provider are
    unchanged code.
  - Their suites pass on this branch: `duplex`, `pilot_faults`, and the
    endpoint full-session tests under V1-baseline and duplex.
- **Stop.** The [rollback rule](#pilot-acceptance-and-rollback-rule) applies
  unchanged. An unreaped worker fails runtime cleanup, so no later service
  cycle starts beside it.

## Source closure

Base `675c71c62`. Each commit applies alone in order. The first seven are
the reviewed `lamp-a2-pilot-limits` commits replayed onto this base; only the
fourth needed conflict resolution, against the newer duplex, notice and
motor code.

| Commit | Files | Content | Shared? |
| --- | --- | --- | --- |
| `42a07701` | `crates/endpoint-ipc/**`, `crates/endpoint/**` | Worker, IPC, supervisor, features, pinned model loading, process and parity tests, fixture provenance | New crates only |
| `1d94414d` | `Cargo.toml`, `Cargo.lock`, `crates/live/Cargo.toml` | Two workspace members; `lamp-live` → `lamp-endpoint-ipc`; 65 added lock entries, none removed or changed | Shared |
| `ae5fea33` | `crates/live/src/activity.rs`, `crates/live/tests/endpoint_confirmation.rs` | Detector seam, baseline-exact pauses, parity `ConfirmPolicy`; no early end | Shared (`activity.rs`) |
| `3de3b636` | `coordinator.rs`, `coordinator/endpoint.rs`, `options.rs`, `coordinator/tests/endpoint_session.rs`, and one `endpoint: None` field in the test `Rig` literals of `camera.rs`, `optional_ring_tests.rs`, `provider_terminal.rs`, `service_mute.rs`, `termination.rs`, `motor/tests.rs`, `tests/pilot_faults.rs` | `--endpoint smart-turn`, worker start in `spawn_rig`, traces, confirmed-reap teardown, full-session stress tests; the baseline `endpoint_policy` record keeps its fields | Shared |
| `fc55a090` | `crates/endpoint/{Cargo.toml,src/main.rs,src/lib.rs,src/reference.rs,tests/parity.rs}`, `crates/endpoint-ipc/tests/process.rs` | `--check-model` self-check, Linux resource bounds, damaged-model and idle tests | New crates only |
| `afd2c52b` | `crates/live/src/coordinator/tests/endpoint_session.rs` | Emulated endpoint sessions run one at a time | Agent 2 test file |
| `eea0ac0c` | `crates/endpoint/src/main.rs` and tests | Worker bounds never raise an inherited soft or hard limit (MAIN-059) | New crates only |
| `e053b100` | `crates/live/src/coordinator.rs` | Baseline `local_endpoint` records keep their exact fields | Shared |
| `98075207` | `crates/voice-eval/src/{main.rs,physical/session.rs}`, `crates/voice-eval/tests/serve_plan.rs`, `fixtures/voice-eval-serve-endpoint-trial.example.json` | `--endpoint` as a typed runtime flag beside `--microphone`, validated by lamp-live's parser; the trial configuration | Agent 2 crate |
| `549daeb7` | `crates/live/src/options.rs`, `coordinator/tests/endpoint_session.rs` (and formatting in `coordinator.rs`, `coordinator/endpoint.rs`) | Smart-turn on duplex, with duplex full-session tests | Shared (`options.rs`) |
| docs commits | `docs/endpoint-detection.md`, `docs/endpoint-pilot.md`, `coordination/agent-2.md` | This package | Agent 2 files |

The hash of the docs and status commit is in
[coordination/agent-2.md](../coordination/agent-2.md).

## Asset and dependency manifest

### Model

| Field | Value |
| --- | --- |
| Source | `https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/f766f81d3cfdf7737ac64aad813d91bbfd56bf93/smart-turn-v3.2-gpu.onnx` |
| Revision | `f766f81d3cfdf7737ac64aad813d91bbfd56bf93` |
| Installed name | `smart-turn-v3.2-gpu.onnx`, beside `lamp-live` and `lamp-endpoint-worker` |
| Size | 32,411,198 bytes |
| SHA256 | `ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be` |
| License | BSD-2-Clause |
| Pinned in | `lamp_endpoint_ipc::SMART_TURN_MODEL_SHA256` (core) and `lamp_endpoint::SMART_TURN_V3_2` (worker) |
| Not used | The int8 `smart-turn-v3.2-cpu.onnx`: tract 0.23.8 mis-executes its uint8 quantize/dequantize pairs |

The core refuses a worker whose `Ready` reports any other SHA256. The worker
refuses any other bytes before the ONNX parser reads them.

### Executables

| Binary | Links | Linux ARM64 VM size, release / stripped |
| --- | --- | --- |
| `lamp-live` | `lamp-endpoint-ipc` (no third-party crate added) | 9,665,032 bytes at `b1f43e50` (first package: 8,406,016 at `892ec0a1` → 8,506,584 bytes / 6,165,560 → 6,231,096 bytes) |
| `lamp-endpoint-worker` | tract 0.23.8 and the crates below | 24,233,112 / 17,776,360 bytes |

### Rust dependencies

- **Toolchain.** Rust 1.96.0 (`rust-toolchain.toml`); build with `--locked`.
- **Direct dependencies of `lamp-endpoint`:**
  - `tract-onnx =0.23.8` with default features off;
  - `rustfft =6.4.1`;
  - `sha2` (workspace);
  - `rustix =1.1.5`, with features `process` and `time`.
- **`lamp-endpoint-ipc`** depends only on `lamp-ipc`. It adds nothing to
  `lamp-live`'s third-party tree:
  `cargo tree --locked -p lamp-live -e normal --target all` lists no new
  third-party package.
- **Lockfile.** The 63 third-party lock entries added against `892ec0a1` are
  listed in the [appendix](#appendix-added-lock-entries), with versions,
  licenses and checksums.
  - 55 of them are compiled into the Linux ARM64 worker.
  - The other 8 are lock entries for other targets, build scripts on other
    hosts, or optional features that are not activated.
  - All are MIT, Apache-2.0, Zlib, Unlicense, BSD-2-Clause or BlueOak-1.0.0
    licensed, alone or as alternatives.

## A733 check bundle (ready to copy)

The final ARM gate built the worker from `b1f43e50` on M2's Linux ARM64 VM
(Rust 1.96.0, Debian bookworm, glibc 2.36). The worker needs glibc 2.34 or
later. The bundle has no credentials and no audio. It is stored on M2 at
`lampOS/.cache/endpoint/artifacts/b1f43e50a/lamp-endpoint-check-b1f43e50a.tar`
(Git-ignored):

| File | Bytes | SHA256 |
| --- | --- | --- |
| `lamp-endpoint-check-b1f43e50a.tar` | 56,650,752 | `71d5506e371ae9415a5eba7ef928c3f3bd2febe9c0c059c54d197d0ab317162d` |
| `lamp-endpoint-worker` | 24,233,112 | `58852eb0697628d293af0d118614bd5fcf04c9d24a940044adcce1471b73d7ec` |
| `smart-turn-v3.2-gpu.onnx` | 32,411,198 | `ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be` |
| `README.txt` | 1,352 | `0fed97e9fa1b71c146a68f95098a60c3aa6f12d1cd4e45baced5e85ea1c22196` |

The same gate's `lamp-live` is 9,665,032 bytes, SHA256
`e0c306772c4ff6cd175894e8face40b6d7f3c9f7ecc74412e31d987abe61dc62`. It is not
in the bundle: root builds and composes the shared runtime. Embedded build
paths may change the hash of another build of the same commit.

On the idle Lamp, as the service user and under the service's limits:

```sh
tar -xf lamp-endpoint-check-b1f43e50a.tar && cd lamp-endpoint-check-b1f43e50a
sha256sum -c SHA256SUMS
cat /proc/loadavg; grep -E "address space|core file" /proc/self/limits
for i in 1 2 3; do ./lamp-endpoint-worker --check-model ./smart-turn-v3.2-gpu.onnx; echo "exit $?"; done
```

Before handing over, the bundle was extracted and verified in the VM and
checked once more there; it exited 0. On the VM the three gate checks gave:
- load 158–160 ms and cold 93–96 ms;
- warm median 93–96 ms and warm max 97–101 ms;
- 92–96 ms CPU per check;
- VmPeak 116,216 KiB under the 262,144 KiB address-space limit, at niceness 10.

These are not A733 numbers.

## Preparation and check (Rust only)

```sh
cargo build --locked --release -p lamp-live -p lamp-endpoint
curl -fsSL --max-filesize 33000000 -o target/release/smart-turn-v3.2-gpu.onnx \
  https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/f766f81d3cfdf7737ac64aad813d91bbfd56bf93/smart-turn-v3.2-gpu.onnx
target/release/lamp-endpoint-worker --check-model target/release/smart-turn-v3.2-gpu.onnx
```

Install `lamp-endpoint-worker` and the model in the same directory as
`lamp-live`. Then run `--check-model` there, as the service user, under the
service's own limits. It runs these steps:

1. Applies the worker's Linux bounds.
2. Verifies the size and SHA256 before parsing.
3. Loads the model with tract.
4. Classifies the `voiced_9s` reference signal ten times. Every probability
   must match ONNX Runtime 1.31.0's 0.9685323 within 1e-4.

Exit 0 prints one JSON line. Exit 2 names the problem on stderr: a missing,
truncated, corrupt or different file, a load failure, or a parity mismatch. An
allocation beyond the address-space cap aborts the check (exit 134), as it
would abort the worker. Delete the file after any failure.

Example JSON line, from the Linux VM:

```json
{"model":"smart-turn-v3.2-gpu.onnx","bytes":32411198,"sha256":"ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be","probability":0.96853244,"reference":0.9685323,"load_ms":157.3,"load_cpu_ms":157.1,"cold_ms":96.3,"warm_median_ms":96.4,"warm_max_ms":101.4,"cpu_per_check_ms":96.5,"vm_peak_kib":116176,"vm_hwm_kib":102520,"vm_rss_kib":59076,"address_space_limit_kib":262144,"nice":10}
```

Record the device's line with the pilot evidence: it is the first A733
measurement of this worker.

## Measured bounds

All rows are from the M2 Max (macOS) or its ARM64 Colima VM (6 vCPUs, Docker
`rust:1.96.0-bookworm`, NEON only). The A733 is unmeasured.

| Bound | How it is enforced | macOS | Linux ARM64 VM |
| --- | --- | --- | --- |
| Cold start: spawn to `Ready`, as the core sees it | Startup limit 10 s, then `Failed` (never restarted) and the 1,200 ms window | 164–165 ms (first launch of a new binary 1,022 ms) | 241–257 ms in 5 profiled starts; 298 ms in the gate while another test compiled |
| Model load with hash check | Inside the startup limit | 130–131 ms | 157–175 ms |
| First classification of a full 8 s window | Inside the startup limit | 36–39 ms | 96–98 ms |
| Warm classification, 8 s window, median / max of 9 | Result timeout 500 ms after submission (fallback); hang limit 2 s (kill, reap, replace at most twice) | 34–37 / 35–38 ms | 94–96 / 96–101 ms |
| Worker CPU per classification | One thread; one request at a time; Linux niceness +10 | 34–37 ms | 95–97 ms |
| Worker memory | Linux address-space cap 256 MiB; an allocation beyond it aborts the worker | 190–198 MB peak resident | 113.5 MiB peak virtual, 100 MiB peak resident, 58 MiB resident when warm |
| Repeated checks | Buffers reused | n/a | peak resident unchanged after 50 more full-window checks (`worker_process_stays_ready_and_bounded_across_idle_and_repeated_checks`) |
| Idle | Only a request in flight is timed | 5 s (default test) | Real worker idle 600 s: same process, no kill; resident 59,772 KiB and peak 102,648 KiB both when warm, after the idle and after 50 more checks. Scripted worker idle 600 s, then answers |
| Core | Three client threads and at most one 256 KB request frame; `submit` never waits | submit p50 / max 67 / 346 µs | 72 / 148 µs |
| Core files | Linux core file limit 0 | n/a | `/proc/PID/limits` checked by the idle test |

The memory cap was checked on the VM: under a 96 MiB service cap the self-check
aborted with "memory allocation of 2359296 bytes failed" (exit 134) and wrote no
core file. Under the worker's own 256 MiB cap every check passed.

Contention raises these costs. One self-check ran on the VM right after a
release build: load 334 ms, first classification 204 ms, warm median 95 ms,
warm max 130 ms and 111 ms of CPU per check. The peak memory was unchanged.

**CPU budget.**

- The detector submits at most one request per pause, 200 ms into the pause.
  Each request costs about 95 ms of one VM core.
- Ten pauses a minute would cost about 1.6% of one VM core on average, with
  bursts of one core for about 0.1 s.
- On the A733, measure the device's `cpu_per_check_ms` before judging
  contention. Under Linux's fair scheduler, niceness +10 gives each capture,
  playback and provider thread about nine times the worker's CPU share when
  they compete for a core. A late verdict falls back to 1,200 ms.

## Stress evidence

Each row is a real process, a real pipe or the real `conversation()` loop.
Session tests run with emulated capture, privacy, provider and speaker peers.
They keep the production freshness bounds, so they run one at a time: twelve
concurrent sessions on the 6-vCPU VM starved each other's 50 ms privacy
lease.

| MAIN-057 path | Result | Tests |
| --- | --- | --- |
| Missing file | `Failed` once, never restarted; every pause 1,200 ms; session ends normally | `worker_process_refuses_missing_or_wrong_models_without_blocking` (real binary), `missing_model_is_final_and_never_restarted`, `missing_model_session_ends_on_the_unchanged_silence_fallback` (session) |
| Wrong model | A different file is refused by size or hash; a worker reporting another model is refused before any request | same real-binary test, `a_worker_reporting_another_model_is_refused_before_any_request` |
| Malformed or truncated model | A half-length copy fails "unexpected size"; a same-size copy with one flipped byte fails "pinned SHA256"; neither reaches the parser, both reaped, no restart | `worker_process_refuses_truncated_and_corrupt_models_once`, `check_model_verifies_the_installed_model_and_refuses_damaged_copies` (real binary, real model), `model_load_refuses_wrong_bytes_before_parsing` |
| Loading | A worker still loading for the whole session leaves `silence_fallback` (`loading`) at 1,200 ms | `a_worker_still_loading_leaves_the_baseline_window` (session) |
| Repeated timeout | Every verdict 700 ms late: every turn ends `silence_fallback` (`timeout`) at 1,200 ms | `verdicts_after_the_result_timeout_fall_back_on_every_turn` (session) |
| Hang | Capture, endpoint and session end are never delayed; the worker is killed, reaped and replaced | `hung_worker_never_delays_capture_endpoint_or_session_end` (session) |
| Inference crash | The failed pause falls back (`failed`), the next falls back while recovering (`recovering`), and the replacement's verdict is used again (`model_complete`) | `a_worker_failure_falls_back_through_recovery_and_then_resumes` (session), `serving_failures_are_replaced_within_budget`, `protocol_violations_are_replaced` |
| Rapid resume | Five bursts separated by 300 ms pauses, each judged complete: one request carries every burst, and the turn ends once at 1,000 ms after the last | `rapid_resumes_never_end_the_turn_early`, `complete_verdict_ends_early_but_resumed_speech_keeps_the_turn` (session) |
| Privacy shutdown and reopen | Two mute cycles: each worker is confirmed reaped and gone before the next cycle's worker exists | `mute_then_reopen_never_overlaps_endpoint_workers`, `confirmed_endpoint_exit_keeps_the_restartable_mute` |
| Unreaped worker | Cleanup fails; the mute is not restartable; no service cycle reopens | `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service` |
| Idle longevity | The scripted worker idles 600 s (300 hang limits) and the real worker 600 s, on the VM: same process, no kill, then correct answers | `an_idle_worker_stays_ready_across_a_long_quiet`, `worker_process_stays_ready_and_bounded_across_idle_and_repeated_checks` with `LAMP_ENDPOINT_IDLE_SECS=600` |
| Normal path | A complete verdict ends input at 1.0 s of quiet and its answer plays after it | `a_complete_verdict_ends_at_one_second_and_its_answer_plays`, `real_worker_session_records_verdict_provenance` (real worker and model) |

A fallback test shows that the missing, loading and failed paths end every
turn on exactly the baseline's block (`tests/endpoint_confirmation.rs`, 40
seeds). Baseline defaults are untouched: `--endpoint` defaults to `silence`,
and `smart-turn` is refused without `--turn-taking v1-baseline`.

## Review of the six added held-out splits

The parity profile was frozen on development shards 0 and 1. These cases come
from the fresh held-out shards 4 and 5 (1,167 clips). They were reviewed after
the fact and **nothing was re-tuned on them**. Matched pause by pause against
the baseline, parity adds 6 splits and removes none.

| Clip | Source | Clip label | Speech before the pause | Pause the person left | Verdict | Parity ended at | Person resumed after the endpoint | Speech still to come |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `6d46ae83` | `mundo_1` (Spanish) | finished | 4.5 s | 1,020 ms | 0.976 | 1,000 ms | 30 ms | 1.1 s |
| `93d2e60a` | `liva_1` | finished | 4.2 s | 1,000 ms | 0.964 | 1,000 ms | 10 ms | 11.4 s |
| `7a81fec2` | `liva_1` | finished | 10.4 s | 1,110 ms | 0.984 | 1,000 ms | 120 ms | 5.0 s |
| `74a01472` | `liva_1` | unfinished | 9.9 s | 1,100 ms | 0.971 | 1,000 ms | 110 ms | 5.4 s |
| `89e462c6` | `liva_1` | unfinished | 6.5 s | 1,090 ms | 0.990 | 1,000 ms | 100 ms | 6.7 s |
| `9027a54c` | `liva_1` | unfinished | 2.0 s | 1,130 ms | 0.990 | 1,000 ms | 140 ms | 9.3 s |

All six cases share one signature:

- **The pause.** The person paused for 1.00–1.13 s after audio the model
  heard as finished (0.96–0.99), then went on with 1–11 s more speech.
- **Why the baseline kept the turn.** It waits 1,200 ms, and every one of
  these pauses was shorter than that.
- **Where they occur.**
  - Five are in `liva_1`, long-form English recordings of 12–16 s with
    several sentences.
  - One is Spanish.
  - None is in the 300 short-form English clips.

There are no transcripts, so whether the first part was a complete sentence
cannot be checked.

What the pilot would do in each case:

1. Input ends at 1.0 s. The person resumes 10–140 ms later, long before a reply
   is audible.
2. The existing path cancels the pending reply (`user_interrupted`). It then
   admits the continuation as an interruption, behind the provider barrier.
3. Main's probe and one real Lamp run show `BarrierTimeout` on this
   before-output path. The 2 s settlement bound expires while the successor
   is still speaking. Every one of these continuations lasts over 1 s, and
   five last 5 s or more. Main's one successful trial continued during
   output, which does not cover these cases.
4. So in the pilot, each case like these may cost the provider connection,
   not only the person's whole thought.

### Rationale for keeping the profile conservative, without re-tuning

- **The development set.** It showed the same balance: +4 and −3 splits on
  1,180 clips. The held-out rate (0.5% of clips) confirms that estimate; it
  is not a new finding to optimise against.
- **Other frozen profiles.**
  - The `strict` profile (≥ 0.99, ending at 800 ms) would have avoided four
    of these six. On development data it ended only 16% of finished turns
    early, which is why parity was frozen instead.
  - Choosing `strict` now because of these six cases would be selection on
    the held-out set, so it is not proposed.
  - If Main wants fewer barrier exposures, the baseline is the honest
    choice.
- **Why not a lower commit.** Every step below 1,000 ms adds splits
  quickly: the 400 ms profile adds 338 on the same held-out set.
- **Parity's other choices protect the baseline.**
  - Clearly unfinished verdicts wait 1,500 ms.
  - Every missing or late verdict keeps 1,200 ms.
  - Any speech-scored block keeps the turn.
- **Unfinished clip ends.** Parity ended 19 of 569 unfinished clip ends
  before 1,200 ms; the baseline ended none. A clip ends with no continuation,
  so these are not counted as splits. In a live conversation the person
  would continue after more than 2.5 s, past either endpoint.

## Pilot acceptance and rollback rule

Applies to the candidate (`--endpoint smart-turn`) only.

### Before the first candidate session

1. **Device self-check.** `lamp-endpoint-worker --check-model` on the device,
   run as the service user and under its limits, exits 0.
2. **Bounds.** Its `warm_max_ms` is below 400. A slower device falls back on
   most pauses and cannot show a benefit, so keep the baseline.
3. **Paired baseline.** A baseline session (no `--endpoint`) on the same
   build, device and placement has been recorded.

### Roll back at once

Restart without `--endpoint smart-turn` on any one of these:

1. **Barrier failure after a model endpoint.** A `local_endpoint` with
   `endpoint.basis` `model_complete` is followed, for the same turn, by
   `turn_finished` with outcome `user_interrupted` before any
   `speaker_first_write` for that turn. That turn, or the next, then ends in a
   provider failure (`BarrierTimeout`, `provider_turn_failed`, or a
   disconnection).
2. **Wrong build or configuration.** Any of:
   - `endpoint_policy` is not the parity profile;
   - a `local_endpoint` has `includes_silence_wait_ms` below 1,000;
   - any `model_*` decision has no verdict provenance;
   - the model hash is not `ab8dc64b…539be`.
3. **Unreaped worker.** Any of:
   - `endpoint_worker_final` with `reaped` false;
   - a cleanup failure naming the endpoint worker;
   - a `lamp-endpoint-worker` process alive after `lamp-live` has exited.
4. **Audio disturbed.** More `playback_gap` events or capture faults per turn
   than in the paired baseline session.
5. **Continuations after model endpoints.** Three or more interrupted
   `model_complete` turns (as in trigger 1, even without a provider failure)
   in the first 100 candidate turns. In the held-out turn model, parity
   interrupts at most about 1.4 turns per 100 clips on any basis, 0.6 of them
   added by parity. Three or more `model_complete` cases point to a higher
   rate than the evidence.
6. **Cut-off reports.** Any cutoff the person or the facilitator reports on a
   turn whose `endpoint.basis` is `model_complete`.

### Accept for further supervised use

After at least 100 candidate turns over at least three sessions, all of these
must hold:

- no rollback trigger fired;
- in every session the worker reached `ready` before the person's first
  pause, with no `failed` and at most one `recovering`;
- at least 50% of finished turns ended `model_complete` at 1,000 ms
  (77% held-out);
- `silence_fallback` with `timeout` on fewer than 5% of pauses;
- in the facilitator's feedback, no more cut-off reports than in the
  baseline sessions.

**No go.** If the benefit criteria fail without a rollback trigger (for
example, frequent `timeout` fallbacks on the A733), keep the baseline. That is
a missing benefit, not a safety finding.

Keep every attempt, as [supervised-pilot.md](supervised-pilot.md) requires.
Record for each session:

- `endpoint_policy`;
- every `endpoint_worker` change;
- each `local_endpoint.endpoint`;
- `endpoint_worker_final`;
- the device's `--check-model` line.

## Appendix: added lock entries

Against `892ec0a1`, `Cargo.lock` gains these 63 third-party entries and the two
workspace crates `lamp-endpoint` and `lamp-endpoint-ipc`. No entry is removed
or changed. "Compiled" means the package is in
`cargo tree --locked -p lamp-endpoint -e normal --target aarch64-unknown-linux-gnu`.

| Crate | Version | License | Compiled | Cargo.lock checksum (SHA256) |
| --- | --- | --- | --- | --- |
| `anymap3` | 1.1.0 | BlueOak-1.0.0 OR MIT OR Apache-2.0 | yes | `fb5dfbc6d8d2675589ccbe4d0fd61df2419075625f8c1a62325e718e2b0049f9` |
| `bit-set` | 0.11.1 | Apache-2.0 OR MIT | yes | `56d87354e4229f54a44f7bf2435906a4656dba36026ab6eaca629a2c436a691c` |
| `bit-vec` | 0.10.1 | Apache-2.0 OR MIT | yes | `5727b15fa97d4f4fee0a3b7c3d550ed0269f54329207b86388de918604e31269` |
| `borsh` | 1.8.1 | MIT OR Apache-2.0 | no | `553c5d846a6ba5150c65e3b1b8ec073bcf1abc20f9b7220de384a4443ea4e20a` |
| `borsh-derive` | 1.8.1 | Apache-2.0 | no | `12cdfe656708a01f89b451a7d36466e6fe6c414de0aa18fc54f864f6f9ca9f56` |
| `byteorder` | 1.5.0 | Unlicense OR MIT | yes | `1fd0f2584146f6f2ef48085050886acf353beff7305ebd1ae69500e27c67f64b` |
| `crunchy` | 0.2.4 | MIT | no | `460fbee9c2c2f33933d720630a6a0bac33ba7053db5344fac858d4b8952d77d5` |
| `derive-new` | 0.7.0 | MIT | yes | `2cdc8d50f426189eef89dac62fabfa0abb27d5cc008f25bf4156a0203325becc` |
| `downcast-rs` | 2.0.2 | MIT OR Apache-2.0 | yes | `117240f60069e65410b3ae1bb213295bd828f707b5bec6596a1afc8793ce0cbc` |
| `dyn-clone` | 1.0.20 | MIT OR Apache-2.0 | yes | `d0881ea181b1df73ff77ffaaf9c7544ecc11e82fba9b5f27b262a3c73a332555` |
| `dyn-hash` | 1.0.0 | MIT OR Apache-2.0 | yes | `5fdab65db9274e0168143841eb8f864a0a21f8b1b8d2ba6812bbe6024346e99e` |
| `erased-serde` | 0.4.10 | MIT OR Apache-2.0 | yes | `d2add8a07dd6a8d93ff627029c51de145e12686fbc36ecb298ac22e74cf02dec` |
| `filetime` | 0.2.29 | MIT/Apache-2.0 | yes | `5c287a33c7f0a620c38e641e7f60827713987b3c0f26e8ddc9462cc69cf75759` |
| `foldhash` | 0.2.0 | Zlib | yes | `77ce24cb58228fbb8aa041425bb1050850ac19177686ea6e0f41a70416f56fdb` |
| `half` | 2.7.1 | MIT OR Apache-2.0 | yes | `6ea2d84b969582b4b1864a92dc5d27cd2b77b622a8d79306834f1be5ba20d84b` |
| `hashbrown` | 0.16.1 | MIT OR Apache-2.0 | yes | `841d1cc9bed7f9236f321df977030373f4a4163ae1a7dbfe1a51a2c1a51d9100` |
| `inventory` | 0.3.25 | MIT OR Apache-2.0 | yes | `6928282826c822ad91bf1c9a1cb90a30ba1c26770749929b4656cd6be829cd7c` |
| `itertools` | 0.15.0 | MIT OR Apache-2.0 | yes | `8b4baf93f58d4425749ca49a51c50ebab072c5df6994d08fed93541c331481dc` |
| `lazy_static` | 1.5.1 | MIT OR Apache-2.0 | yes | `20870f649af7073d53e38067b2a84312175d56ea15217e1b15bc83506ec50afb` |
| `libm` | 0.2.16 | MIT | yes | `b6d2cec3eae94f9f509c767b45932f1ada8350c4bdb85af2fcab4a3c14807981` |
| `lock_api` | 0.4.14 | MIT OR Apache-2.0 | yes | `224399e74b87b5f3557511d98dff8b14089b3dadafcab6bb93eab67d3aace965` |
| `maplit` | 1.0.2 | MIT/Apache-2.0 | yes | `3e2e65a1a2e43cfcb47a895c4c8b10d1f4a61097f9f254f183aee60cad9c651d` |
| `matrixmultiply` | 0.3.11 | MIT/Apache-2.0 | yes | `3f607c237553f086e7043417a51df26b2eb899d3caff94e6a67592ff992fedc7` |
| `memmap2` | 0.9.11 | MIT OR Apache-2.0 | yes | `d1219ed1b7f229ee7104d281dd01d6802fe28bb6e95d292942c4daacdeb798c0` |
| `memo-map` | 0.3.4 | Apache-2.0 | no | `5449c8c750f1a07ea702bbd212bd999fceece9b3d1508b17023b3e174583124b` |
| `minijinja` | 2.24.0 | Apache-2.0 | no | `86886cf6dbf4e614b19c9a1eec9775f021869d7eadde0fc73921a81b90c9b4c9` |
| `ndarray` | 0.17.2 | MIT OR Apache-2.0 | yes | `520080814a7a6b4a6e9070823bb24b4531daac8c4627e08ba5de8c5ef2f2752d` |
| `nom` | 8.0.0 | MIT | yes | `df9761775871bdef83bee530e60050f7e54b1105350d6884eb0fb4f46c2f9405` |
| `nom-language` | 0.1.0 | MIT | yes | `2de2bc5b451bfedaef92c90b8939a8fff5770bdcc1fafd6239d086aab8fa6b29` |
| `num-bigint` | 0.4.8 | MIT OR Apache-2.0 | yes | `c89e69e7e0f03bea5ef08013795c25018e101932225a656383bd384495ecc367` |
| `num-complex` | 0.4.6 | MIT OR Apache-2.0 | yes | `73f88a1307638156682bada9d7604135552957b7818057dcef22705b4d509495` |
| `num-integer` | 0.1.47 | MIT OR Apache-2.0 | yes | `7ce2d95d4b3734dc35aa2f45e1aa22cd416814592a4f9d9205e11affd5b8e10b` |
| `num-rational` | 0.4.2 | MIT OR Apache-2.0 | yes | `f83d14da390562dca69fc84082e73e548e1ad308d24accdedd2720017cb37824` |
| `parking_lot` | 0.12.5 | MIT OR Apache-2.0 | yes | `93857453250e3077bd71ff98b6a65ea6621a19bb0f559a85248955ac12c45a1a` |
| `parking_lot_core` | 0.9.12 | MIT OR Apache-2.0 | yes | `2621685985a2ebf1c516881c026032ac7deafcda1a2c9b7850dc81e3dfcb64c1` |
| `portable-atomic` | 1.15.0 | Apache-2.0 OR MIT | no | `05c8b63e8d9609db387f0324918f81d68fe27748f084ef092fb35954d0539a85` |
| `portable-atomic-util` | 0.2.8 | Apache-2.0 OR MIT | no | `10ab3eb7f3becc3a1cbc4f2c6f20267996cfc1a6467a873763411b136a122715` |
| `primal-check` | 0.3.4 | MIT OR Apache-2.0 | yes | `dc0d895b311e3af9902528fbb8f928688abbd95872819320517cc24ca6b2bd08` |
| `prost` | 0.14.4 | Apache-2.0 | yes | `528ac67416ff8646872a3c02cad9cc4ee5dc9f9540c9b10771855c95cb2e5ae1` |
| `prost-derive` | 0.14.4 | Apache-2.0 | yes | `b570b25f7617e43d59005d0990ccb79e950a423952cea19671b7a876da390adf` |
| `rand_distr` | 0.6.0 | MIT OR Apache-2.0 | yes | `4d431c2703ccf129de4d45253c03f49ebb22b97d6ad79ee3ecfc7e3f4862c1d8` |
| `rawpointer` | 0.2.1 | MIT/Apache-2.0 | yes | `60a357793950651c4ed0f3f52338f53b2f809f32d83a07f72909fa13e4c6c1e3` |
| `redox_syscall` | 0.5.18 | MIT | no | `ed2bf2547551a7053d6fdfafda3f938979645c44812fbfcda098faae3f1a362d` |
| `rustfft` | 6.4.1 | MIT OR Apache-2.0 | yes | `21db5f9893e91f41798c88680037dba611ca6674703c1a18601b01a72c8adb89` |
| `scopeguard` | 1.2.0 | MIT OR Apache-2.0 | yes | `94143f37725109f92c262ed2cf5e59bce7498c01bcc1502d7b9afe439a4e9f49` |
| `simd-adler32` | 0.3.10 | MIT | yes | `3a219298ac11a56ea9a6d2120044824d6f01aeb034955e7af7bc16858527deea` |
| `smallvec` | 1.16.3 | MIT OR Apache-2.0 | yes | `5b3dc8af474f516a851ff4bd12db780f948b9250ad37211e4eec0bccea54e01b` |
| `strength_reduce` | 0.2.4 | MIT OR Apache-2.0 | yes | `fe895eb47f22e2ddd4dabc02bce419d2e643c8e3b585c78158b349195bc24d82` |
| `string-interner` | 0.20.0 | MIT/Apache-2.0 | yes | `ad3df9b59e2eded8d825c7c4363ad339a20fb6bc0b9a4778560f518f59910b15` |
| `tar` | 0.4.46 | MIT OR Apache-2.0 | yes | `3f6221d9a6003c78398e3b239969f352578258df48c8eb051caadae0015bc840` |
| `tract-core` | 0.23.8 | MIT OR Apache-2.0 | yes | `0a5a9e07e863fc28355436c5a48fe859784832094ba1ff410357396ae87ff1e3` |
| `tract-data` | 0.23.8 | MIT OR Apache-2.0 | yes | `a33bafcf1d3dfa404c3271dd08304b162deb8fcfe9536290fb97e578c32d5dd5` |
| `tract-extra` | 0.23.8 | MIT OR Apache-2.0 | yes | `2a05c86ece0c47880c7726e08dabb72d68e9cba0cd36d44bc0aa90e2291a05db` |
| `tract-hir` | 0.23.8 | MIT OR Apache-2.0 | yes | `a52e3ec1f3beb16d552806386e123cf5632d4206154695a80aaecdb6ee2c5128` |
| `tract-linalg` | 0.23.8 | MIT OR Apache-2.0 | yes | `1cc37ab328b95cef8eef98324122e9f956fd3ca197f0892e09ff90fe495fa222` |
| `tract-nnef` | 0.23.8 | MIT OR Apache-2.0 | yes | `5d95e30d7e6c2f73516212367a5b49354c105b52285e8919ec439c84556f54b3` |
| `tract-onnx` | 0.23.8 | MIT OR Apache-2.0 | yes | `5a316674683d04c6070146badf57a7b8b9d99d4e38f586cbe3214e553d70bc2d` |
| `tract-onnx-opl` | 0.23.8 | MIT OR Apache-2.0 | yes | `091e126c52d608214f8784f468c7d0be44459e7a369fb9a26f92aa593a78f4a5` |
| `transpose` | 0.2.3 | MIT OR Apache-2.0 | yes | `1ad61aed86bc3faea4300c7aee358b4c6d0c8d6ccc36524c96e4c92ccf26e77e` |
| `typeid` | 1.0.3 | MIT OR Apache-2.0 | yes | `bc7d623258602320d5c55d1bc22793b57daff0ec7efc270ea7d55ce1d5f5471c` |
| `xattr` | 1.6.1 | MIT OR Apache-2.0 | yes | `32e45ad4206f6d2479085147f02bc2ef834ac85886624a23575ae137c8aa8156` |
| `zerocopy` | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | yes | `86502bf56ac7c77571a32e2647bb2a15894565e981fb2a48d7bde2d91c965a9d` |
| `zerocopy-derive` | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | yes | `5457206954b06561e2608c7e19cf58b1926586d999c246eebe4502f7e2039d1a` |
