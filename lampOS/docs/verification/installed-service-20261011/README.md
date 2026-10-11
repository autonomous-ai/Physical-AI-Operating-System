# Installed-service conversation and shutdown on 4ace

The unchanged installed V2 service completed **2/2 real iMac-to-Lamp exchanges**
and a clean systemd Stop. Both replies have zero reported playback gaps. The
continuous 180-second iMac recording passed integrity checks and covers the
whole service window. Stationary muted legacy restoration and actual mixer
readbacks verified afterward. The service is stopped and is not boot-enabled.

This qualifies one installed-service lifecycle. It supplements the earlier
[16-turn, 21.6-minute run](../native05-20261011/README.md); it is not another V1
benchmark or proof of natural interruptions, physical mute/reopen, optical
choreography, all-day reliability, or human-level response speed.

## Conversation evidence

The existing Daniel fixtures ran once each at 190 and 200 words per minute.
The next prompt required the previous owner's completion and final playback
retirement. Provider output was:

- Greeting: “I'm doing well, thanks for asking! How are things with you?”
- Ocean fact: “More than eighty percent of the ocean is unmapped and unexplored by humans.”

An independent offline, two-thread Whisper `small.en` pass corroborated both
prompts and complete replies in 9.248- and 14.709-second room clips. It spelled
the address “Lamp” as “Lam.” ASR reported no additional speech in those selected clips;
the rest of the recording was not transcribed. These are machine hypotheses,
not a human listening signoff or accepted acoustic word boundaries. Recordings
remain local and were not uploaded.

The ocean sentence is retained as spoken, not as a factual acceptance oracle.
NOAA's [broad exploration description](https://oceanservice.noaa.gov/facts/ocean-species.html)
and its [2025 seafloor mapping coverage](https://www.iocm.noaa.gov/seabed-2030.html)
describe different measures. This lifecycle check does not certify factual accuracy.

| Software boundary | Greeting | Ocean fact |
|---|---:|---:|
| Local endpoint → provider's first audio | 999.880 ms | 968.870 ms |
| Local endpoint → first speaker write | 1,356.126 ms | 1,165.030 ms |
| Fixed endpoint silence wait, excluded above | 1,200 ms | 1,200 ms |

These are two individual observations, not a distribution or causal speedup.
First audible word and actual speech-end timing remain unscored. `serve` did
not save target PCM diagnostics; audio retirement is receipt-verified, not
byte-verified. `complete_evidence=false` remains truthful.

## Service and restoration

- Exact invocation: `95b66bb3cc94418bbbd5702f311480d6`.
- 272 contiguous metadata events, no writer loss, healthy finish ACK.
- Five observed PID/start identities gone after Stop; exact cgroup and hardware
  owner checks empty; service inactive with success and main status zero.
- Administrative Stop request → verified clean state: **185.413 ms**.
- Coordinator's SIGTERM event → `run_end`: **25.786 ms**.
- One 180-second, 48 kHz float32 mono recording: 8,640,000 frames and 16,875
  contiguous callbacks; zero dropped frames, clipping, or capture faults.
- Both actual mixer snapshots matched; no gain, motor, camera-context or ring
  changes were part of this run. The stationary serial guard remained in place.

Runtime source is `55d7af98e149591eed7882b9ace7741486daa842`; binary SHA-256 is
`738503a803d3718e52ca9e8886a669c0e41c007f7fe35873642afd4a83f5b8d0`.
The newer optional failure-notice code was not enabled or deployed in this run.
[Numerical evidence and hashes](results.json) identify the exact source, unit,
helper, output, archive and recording without publishing credentials or media.

The helper's original review found a false-success path after a failed mixer
read. Both first- and second-read failures were reproduced; the reviewed R2
fails closed and requires an explicit matching-mixers result. Its 28 focused
checks passed before the single native execution. Earlier helper receipts remain
retained; no failed native attempt was replaced by this result.

Private original evidence: `/private/tmp/lamp-installed-service-executed-s777g6ec`.
This test does not remove the remaining physical privacy and facilitator-handoff
requirements in the [attended runbook](../../attended-4ace-pilot.md).
