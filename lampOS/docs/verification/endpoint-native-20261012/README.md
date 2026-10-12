# Composed MIC1 and endpoint native checks — 2026-10-12

The frozen composition built for ARM64 and its standalone endpoint worker ran
on lamp-4ace/A733. Three model self-checks passed; the exact IPC test ELF that
failed one QEMU test subsequently passed all 12 tests natively. The QEMU failure
is retained. This is **not** an endpoint-enabled physical conversation or
release acceptance. No live runtime was started by the installation/self-check,
and the observed service states/PIDs were unchanged across those checks.

## Exact source and artifacts

The source is canonical `7e28dc7ace66eb26cb3e3332f3dcadf02f1f5b9a` plus the
frozen MIC1 and endpoint overlays: 45 changed paths, 518 source files, no shared
runtime paths or manual source resolutions. Source manifest SHA-256:
`204c1de3053b202308f8d9767f27b16604ce2fe58411e6bfc1ac579ee07bed0d`.
Combined source patch SHA-256:
`2a16a0e5cf57f4b45bca499b3b1457448d3d38ec29b9f67493fc4789eebf8287`.
This documentation follow-up changes no executable source.

| Installed/check artifact | SHA-256 |
|---|---|
| Composed `lamp-live` | `7e5a7e015f9f0e682f6577e290d8375849421a167dd4bf0ad0b5ef69464f7ea7` |
| `lamp-endpoint-worker` | `1c4de3b310685296c7feba862a2d10ad86814ebf5ddfb6539812f3cc24d360d6` |
| Smart Turn v3.2 ONNX, 32,411,198 bytes | `ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be` |
| Endpoint IPC process-test ELF | `4e58de44a46e35c51301d14c92af98c6347948fdbf619acede53bd6d369c66d1` |

Defaults remain MIC2 and silence endpointing. SmartTurn is an explicit opt-in
for v1-baseline or duplex, with 1,000/1,200/1,500 ms parity quiet windows; the
speculative early-endpoint path is absent. The four recent MIC1 recordings used
the earlier `14fca…`/`33f2…` artifacts and silence endpointing, not this combined
`7e5a…` artifact. See [their outcomes](../mic1-reference-20261012/four-runs.md).

## Composed-source ARM/QEMU gates

Formatting, strict all-target Clippy for live/endpoint/endpoint-ipc/voice-eval,
and release compilation of live and endpoint passed. The Linux/ARM64 test
binaries were executed under QEMU for the following checks:

| Focused check | Recorded result |
|---|---|
| Production capture-hold worker | 3 passed |
| Reference link / prime | 6 / 8 passed |
| Endpoint IPC framing | 4 passed |
| Endpoint IPC processes | **11 passed, 1 failed** |
| Filtered live endpoint/session checks | 18 passed, 1 ignored |
| Endpoint local unit/parity checks | 10 passed, 6 model-dependent tests ignored |

`missing_executable_fails_at_once` first failed with `worker closed its output`.
A separate QEMU syscall-traced rerun failed the test's 20 ms immediate-failure
assertion instead. Neither failure is discarded, and the first failure's root
cause has not been established. The original gate stopped at the IPC failure;
separate follow-through commands produced the live/local results above. The
planned model suite with `--include-ignored` did not complete, and no full
workspace pass is claimed. The ignored live test requires an actual worker and
pinned model; the native checks below do not substitute for that integrated
session test. Exact commands and log hashes are in [results.json](results.json).

The same process-test ELF then ran natively on 4ace: **12 passed, 0 failed**,
exit 0, no timeout, 4,818.775 ms total. The missing-executable case took 2 ms;
worst full-window submission was 3.467 ms. These are scripted worker lifecycle
tests, with no microphone, speaker, motion, or model inference. The native pass
does not retrospectively prove a specific QEMU root cause.

## Actual A733 model cost

Three separate `lamp-endpoint-worker --check-model` processes ran as root on
4ace (aarch64, glibc 2.36), each with one cold and nine warm classifications of
the built-in reference signal. Each reported probability 0.96853244 against
reference 0.9685323 and exited 0. Classification time is feature extraction plus
inference; it excludes model load, IPC submission, endpoint quiet wait, provider
response, and output. Warm median/max columns describe nine samples per process,
not a pooled conversation distribution.

| Process | Model load (ms) | Cold classification (ms) | Warm median (ms) | Warm maximum (ms) |
|---|---:|---:|---:|---:|
| 1 | 373.4 | 561.0 | 371.8 | 373.7 |
| 2 | 431.6 | 435.6 | 370.3 | 372.7 |
| 3 | 422.1 | 432.8 | 367.7 | 368.8 |

Warm medians were **367.7–371.8 ms** and maxima **368.8–373.7 ms**, all below
the pilot's 400 ms self-check threshold. The worker reported nice 10 and a
262,144 KiB address-space limit; peak virtual/resident usage was
116,168/102,348 KiB. These replace neither the separate M2-hosted VM figures
in the original proposal nor real concurrent-voice measurements. Startup load,
cold cost, contention, fallback frequency, and intended-request handling still
need the bounded endpoint-enabled physical trial. Passing this self-check does
not enable the candidate or demonstrate an audible latency improvement.

## Evidence and limits

Private inputs are `/private/tmp/lamp-mic1-endpoint-composed-v4y0o344`,
`/private/tmp/lamp-endpoint-arm-gate-m7cg15uu`, and
`/private/tmp/lamp-endpoint-native-check-lylkne7s`. [results.json](results.json)
pins their relevant receipts/logs and separates native checks from emulation.
No model weights, binaries, raw audio, or incidental office transcripts are
added to source. `human_listened=false`; acoustic scores remain unassigned.

The [MIC1 verification](../mic1-reference-20261012/README.md) retains the failed
lower-speaker trial, verified restoration, Gemini host-build overlap, and missed
coffee interruption. Its endpoint-to-output timings **exclude the preceding
1,200 ms silence wait**; they are not last-phoneme-to-first-audible-word results.
Natural interaction and owner acceptance remain open.
