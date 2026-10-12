# MIC1 diagnostic verification — 2026-10-12

These four runs preserve useful capture, playout, and failure evidence; they do not establish release readiness or successful natural conversation. All planned inputs were cached synthetic speech played through the iMac speakers. Every run records `human_listened=false` and `acoustic_score=null`. No live human interaction, acoustic quality acceptance, or source identity is inferred from these synthetic tests or their transcripts. The lower-speaker run remains failed/incomplete.

This summary reuses the completed private integrity and trace checks. It checks the durable cache manifest and result identities without repeating media verification. No device operation, build, playback, ASR, or cloud call was performed for this summary. Incidental office transcript text is omitted; the original private evidence remains preserved.

## Workloads and outcomes

| Run | Delivered cached sources | Candidates / owners / completed replies | Outcome |
|---|---:|---:|---|
| Coffee alone | 1 | 3 / 1 / 1 | Gemini reply on the coffee topic; two later candidates rejected after yield/resume. Opening-word retention unresolved. |
| Fixed-source timing | 2 | 1 / 1 / 1 | Exact local fixture retired; no second admission. This is v1-baseline, not Gemini or duplex. |
| Gemini timing | 2 | 1 / 1 / 1 | Sunset reply completed; delivered coffee source had no second candidate, admission, input transcript, or answer. Interruption failed. |
| Lower speaker | 2 | 3 / 3 / 2 | Failed/incomplete: unplanned activity and a third owner triggered helper abort/SIGTERM. Two completed replies do not establish the intended coffee request was answered. |

Coffee alone used runtime SHA-256 `14fca820910eb725eece1b082f98fa3d63aaf1eef0301319d8d909a3f4c8941e`. The three timing-treatment runs used `33f2eb2149bebebde7319c4ecd59a3b3c7470c43f88c2a9fbe56a70aa1589800`, built from source `f18362fa947a8f90e4139a3536f04de8449ce2e6`. MIC1 and capture reference clock were selected, recovery and noise suppression were off, with 200 ms playout start and ring ceiling 12. Coffee, Gemini timing, and lower speaker used Gemini/duplex with kept-start provider continuation. Fixed-source timing used a local reply fixture with v1-baseline admission; its timing cannot measure Gemini performance.

The coffee-alone input transcription omitted the scripted opening correction and name, although the remainder and reply were on topic. That does not establish that the opening words reached or were retained by the provider. Two independently counted accepted-zero groups each contain 11,760 frames / 490 ms. They align with coordinator yield-to-resume intervals of 497.717 and 498.190 ms while reply PCM was queued. Triggering source and acoustic gap duration remain unclassified; the trace's `starvation_excluded` field remains false. Neither candidate committed an interruption.

The fixed fixture's 337,680 accepted frames in 1,407 writes match the pinned PCM byte-for-byte (SHA-256 `3d98cfa176290e5e120f6c5350c04a127bdffaace3711e64b7aaa23d0a630ebe`). It had no reported playback gaps. This validates that fixed output path, not recognition of the second delivered source. Gemini timing had zero reported gaps and zero yield/resume events, but did not admit or answer the coffee source. Removing reported gaps is not evidence that interruption succeeded.

## Software timings and capture chronology

Each latency begins at `local_endpoint`, emitted after the configured 1,200 ms silence wait. The reported endpoint-to-provider/output intervals **exclude that preceding 1,200 ms wait**. End boundaries are `provider_first_audio` and the first ALSA acceptance. They do not measure the user's last phoneme to the first audible word. There is one completed observation per first three runs and two in the failed lower-speaker run. No conversation p50/p95 or acoustic latency is claimed, and failed or unadmitted requests remain in the outcomes above.

| Run / reply | Endpoint → provider audio (ms) | Endpoint → first ALSA acceptance (ms) | Queued / accepted frames at 24 kHz |
|---|---:|---:|---:|
| Coffee alone / turn 1 | 1,060.320 | 1,242.368 | 186,241 / 186,480 |
| Fixed-source timing / turn 1 | 4.272 | 21.858 | 337,680 / 337,680 |
| Gemini timing / turn 1 | 1,310.716 | 1,633.635 | 181,681 / 181,920 |
| Lower speaker / turn 1 | 1,857.266 | 1,870.167 | 124,320 / 124,320 |
| Lower speaker / turn 2 | 1,045.821 | 1,163.594 | 73,201 / 73,440 |

All five completed replies reached recorded final ALSA sample retirement. Coffee, Gemini timing, and the second lower-speaker reply each have 239 final padding frames; these account for their queued/accepted difference. The first lower-speaker reply and the fixture have no such padding difference. Zero reported playback gaps were observed in the fixed, Gemini-timing, and lower-speaker replies. Acoustic end and audibility were not measured.

Capture age here means **processing completion minus original read completion**, not ADC acquisition or coordinator receipt age. Quantiles use linear interpolation over the retained 10 ms capture blocks.

| Run | Capture blocks | Age p50 / p95 (ms) | First source sequence / processing-time reference counter |
|---|---:|---:|---:|
| Coffee alone | 7,174 | 1.022 / 3.254 | 1 / 1 |
| Fixed-source timing | 7,001 | 40.511 / 43.004 | 1 / 5 |
| Gemini timing | 7,164 | 40.384 / 43.033 | 1 / 5 |
| Lower speaker | 4,058 | 40.567 / 43.058 | 1 / 5 |

The held-frame timing runs retain original read timestamps, ordered source indices, and contiguous diagnostic PCM offsets. Reference counter 5 describes processing-time state for source frame 1; it is not microphone-frame sequence 5. These processing-age observations alone do not independently prove the coordinator's 100 ms capture lease.

The workloads, admission policies, generated replies, and sample counts do not form a matched causal speed comparison. **A1 reported an office-VM cross-build from 07:12:12 to 07:13:39 UTC during the Gemini-timing recording** (room attempt created about 07:13:05.459 UTC). Its capture-integrity pass does not remove this host-load confounder, and that run is not a quiet-host qualification. No host workload classification is inferred for the other runs.

## Lower-speaker failure and restoration

The lower-speaker trial changed only Cmedia `numid=6`, Speaker Playback Volume, **9,9 → 3,3**. The journal records one apply and one restore to **9,9**. Every other writable control, including both microphones and AGC, stayed unchanged. A temporary readonly PCM Playback Channel Map difference (3,4 → 0,0) retained its readonly/fixed descriptors; after legacy HAL restart, all final normalized mixer values and descriptors exactly matched the originals. No acoustic dB change was measured.

Despite both cached sources being delivered, the trace contains three owners. The third was admitted at 83,542,306,581 µs; the helper requested stop 4.631 ms later. No third scripted prompt was played. That owner had no provider input-start, endpoint, provider audio, or ALSA write, and ended `runtime_failed`; `run_end` explicitly records SIGTERM/failure. The runtime was reaped without SIGKILL (runtime return code 1; outer target return code 2). Unplanned activity remains source-unclassified. These findings do not support a lower-speaker win or interruption success.

## Integrity, rollback, and preservation

Each independent room recording contains **100 seconds / 4,800,000 frames / 9,375 contiguous callbacks**, with zero reported clipping, dropped frames, nonfinite samples, or capture faults. WAV/metadata/ledger identities and host-window coverage were checked in the earlier reports. Both capture and render diagnostic streams have matching hashes, contiguous records, completion markers, and acknowledged completion receipts for every run. These establish observed recording integrity, not speaker identity, naturalness, audibility, or conversational success. Diagnostic completion does not turn the aborted run into a successful trial.

All four rollback receipts verified trial-cgroup release, restored writable mixer settings, unchanged provider configuration and installed unit template, and muted microphone/speaker software privacy; the stationary legacy motor guard was retained. The three completed collection runs exited normally. The lower-speaker runtime's failure and successful cleanup are recorded separately.

The following paths are relative to `lampOS/`. They are private, ignored durable caches containing original run files and cached stimuli; do not add recordings or binary artifacts to Git. Result identities below match the existing verified reports and cached manifest entries. This summary did not rehash all cached media.

- **Coffee alone**: `.cache/physical-audio/20261012/mic1-coffee-0h0e416g`. Result SHA-256 `41a20e65e1b5ef4675889206b6a9f82d61609e8101c4fecddebd1ed18916473f`; cache manifest SHA-256 `b3849ecd1b016b7575ff65cef266164bf837dfafdffbc5c48d7c433534a02504`.
- **Fixed-source timing**: `.cache/physical-audio/20261012/mic1-timing-fixture-69rmqb_6`. Result SHA-256 `2eedfc7eedd8dd7eda897e9315b4ff2b465cfd0a51d2f7b1bb2416aa920d59e2`; cache manifest SHA-256 `fdca6da5ee0fbeca9fd66cc2b42aee781d015f59d5ca43db2af002adb9250116`.
- **Gemini timing**: `.cache/physical-audio/20261012/mic1-gemini-timing-_r4egoic`. Result SHA-256 `f83fafe37069fe5c857c02742dafa54095e1edfd7d500176ee2cb8d17d610392`; cache manifest SHA-256 `bb1a65790aba5da9d3ad5bba1c5da7ccbdf2eacf6aeed79a9b638d450dc0d041`.
- **Lower speaker**: `.cache/physical-audio/20261012/mic1-speaker-low-lmuglbg8`. Result SHA-256 `ef6bb8f2a14eb868925a3c41e41ed217123c30046533a2cf6f7e876a8319b444`; cache manifest SHA-256 `6f9393a2eb601ae61a935c39c0ab0dbe2d72c9a8a8bdfbc1e7d711215b72c8f3`.

The accompanying [four-runs.json](four-runs.json) contains the per-run software timings, frame accounting, final-retirement times, room and diagnostic hashes, capture-age summaries, cache identities, failure outcome, and host confounder. It intentionally contains no incidental transcript text. Product acceptance still requires comparable physical conversations, acoustic last-phoneme-to-first-word measurements, reliable intended-request handling, and the owner's listening judgment.
