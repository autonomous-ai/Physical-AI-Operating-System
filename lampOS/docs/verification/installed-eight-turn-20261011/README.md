# Eight-turn installed-service audit

**Verified: eight original grants, eight complete owned replies, clean Stop and exact stationary muted restoration.** No material software-evidence inconsistency was found. This is a finite connected scripted conversation, with quality and acoustic latency unscored. `human_listened=false`.

The immutable result is `463a6b863e4eb00ac21a2f5cfc7f676e01196c72612611b0e24f8e61b2215533`. Runtime is `e9bdcea282be7b787abb0568eb46a5c7958a6cce` / binary `b1c61a224ea9958eeaa5f06b87fdc7591128da3daf01c21e2f3c851a7643edbd`; prepared player is `d51eb60f36234f1e9df0bce9f8a660f2a4b6903bec856d12efe1f5ed418890aa`. The frozen runner's 57 files and executed source pins match. The native archive `0fad6d88a5f71aa9b545239a415e3c5a5d562d8921157e23b701156e151e2ab9` has ten verified members. The earlier failed attempts remain separate.

## Conversation and software intervals

Each row is one owner and one retired playback occurrence. The prompts proceed from greeting → ocean fact → its deepest point → correction to average depth → stressful morning → workload → ten-minute constraint → short-answer preference. Output text preserves those references and corrections. Provider input transcripts are session-scoped; the JSON labels their temporal order without claiming independent turn correlation.

All intervals below are milliseconds on the Lamp monotonic clock. `local_endpoint` is after the configured 1,200 ms silence endpoint. Provider-first-audio is the coordinator event; first ALSA write is the owned worker's first accepted samples. Gate age is the separately recorded startup-gate age field. None is an acoustic onset.

| Turn | Provider output transcript | Endpoint → provider audio | Endpoint → first ALSA | Startup gate age |
|---|---|---:|---:|---:|
| 1 | I'm doing well, thank you. How are you today? | 880.225 | 1073.269 | 188.288 |
| 2 | Over eighty percent of the ocean remains unmapped and unexplored by humans. | 947.327 | 1125.834 | 173.768 |
| 3 | The deepest point, the Challenger Deep, is about thirty-six thousand feet deep. | 918.512 | 1314.028 | 392.512 |
| 4 | The average depth of the ocean is around twelve thousand feet. | 861.599 | 1024.696 | 161.932 |
| 5 | Of course. Take your time, I'm here to listen. | 993.694 | 1174.160 | 174.860 |
| 6 | Try picking just one very small task to finish next to get a little momentum going. | 1046.681 | 1140.776 | 90.517 |
| 7 | Clear one immediate email or organize your immediate workspace for a quick sense of completion. | 940.148 | 1120.411 | 173.734 |
| 8 | Understood. I will keep responses brief. | 843.444 | 1027.283 | 175.708 |

Across eight samples: endpoint→provider audio median **929.330 ms**, range **843.444–1,046.681**; endpoint→first ALSA median **1,123.123 ms**, range **1,024.696–1,314.028**. Provider-audio→ALSA median **180.365 ms**, range **94.095–395.516**. Gate age median **174.314 ms**, range **90.517–392.512**; gate-open→ALSA median **4.753 ms**, range **1.168–8.136**. All startup gates opened on `covered`, with configured horizon 200 ms; this is not a measured 200 ms latency addition. Exact event times and all eight samples are in [results.json](results.json).

Initial service-start→capture-prepared was **223.902 ms**, →capture-started **232.123 ms**, →provider-ready **2,071.877 ms**, →listening-ready **2,075.478 ms**. This readiness requires the actual capture/reference/provider state; it does not establish addressee perception.

## Delivery and continuity

Eleven read-only scheduling checks produced eight fresh grants and three retained expired checks. All eight questions were dispatched once; no expired check played audio. Every first-copy callback remained inside its original 100 ms conservative CLOCK_MONOTONIC grant, with **9.038–34.172 ms** remaining. The player samples the clock again at the first-copy boundary, not merely at callback entry. All eight exact source WAVs were fully submitted, all player exits were zero, and there were no duplicate grants, partial stimuli, or player faults.

There are eight admissions, endpoints, provider starts/completions, first writes, final retirements, and successful `audio_written_unscored` outcomes. No extra inputs, cancellations, reported playback gaps, reference resets, or runtime faults appear. One provider session and capture/DSP/playback epoch continued throughout. The 8,652 metadata events are contiguous; the final coordinator ACK confirms all were written without dropped/unconfirmed events. This acknowledges rolling service metadata, not raw target PCM certification.

The 180-second room WAV (`b36d6ed5d668a633d181ae7335d966d503277b2f6963905dfb009a8dc5027275`) has 8,640,000 continuous mono 48 kHz float32 frames across 16,875 callbacks. No dropped, clipped, nonfinite, xrun, or timestamp-discontinuity evidence was found. Its host callback ledger covers the full target run without cross-domain clock subtraction.

One offline, unseeded `small.en` pass reviewed only exact source frames 201216–4374528 (86.944 seconds), with two CPU threads and a 180-second bound; it completed in **84.944 seconds**, exit zero. Python 3.12.9 / NumPy 1.26.4 / Torch 2.2.2 were used; the cached model digest was verified and network operations prohibited. ASR corroborates all eight prompt/reply contents; “Lamp” became “Lam” and numbers were normalized. These are model hypotheses, not listened/accepted acoustic timing or factual verification.

## Ring, Stop, and restoration

Ring receipts account for **3,762 requested / 3,762 presented**, zero rejected/pending/faults, and 25 blanks including the final shutdown write. Request→SPI return ranged **1.491–4.662 ms** (maximum write 1.749 ms). No optical assertion or new photograph is made.

Stop request→verified inactive took **295.209 ms**; SIGTERM observation→`run_end` took **24.307 ms**. The helper and unit exited zero; all six observed process/start identities were gone, cgroup and hardware-owner lists empty, without forced kill. Same-boot stationary serial inhibition, actual mixer readbacks, both software mutes, unchanged physical switch state, and unchanged installed template were verified; no instance override remained.

## Evidence limits and audit receipt

No overlap/background-addressing, physical mute/reopen, motor, camera-context, failure-notice, optical, or long-duration claim follows from this run. Zero software gaps and ASR content do not prove artifact-free acoustics. The response intervals exclude the endpoint wait and are not human-perceived latency.

The private audit retains full checks and command receipts. [results.json](results.json) omits credentials, network addresses, private paths, boot IDs and PIDs. All 133 original inputs were rehashed unchanged. Main independently recomputed the eight endpoint-to-speaker intervals and their median from the recorded event times. The private report SHA-256 is `67f573cd6f144cb0021ed28e684b62cca78af3a42eb7f997ee5a8267c1c2217b`; its audit receipt is `8726da0d28eb260ce6f5c4e75eaebb8dae09e4982fd007ee1cb645281c74981e`. An initial audit comparison failed only because the host adds `_received_` envelope timestamps absent from the raw relay; the preserved diagnostic and corrected comparison neither changed native evidence nor excused a runtime failure.

The [earlier incomplete eight-slot attempt](../installed-connected-20261011/README.md) remains a separate failure. The host-side prepared player fixed its stale process-launch boundary; the Lamp runtime was unchanged. The new result does not pool that failed attempt into its eight completions. See the [pilot runbook](../../attended-4ace-pilot.md) for the current stopped installation and remaining attended checks.
