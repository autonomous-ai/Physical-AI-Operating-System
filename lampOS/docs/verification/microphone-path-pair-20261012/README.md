# Fixed-source microphone-path pair — 12 October 2026

MIC1 completed this diagnostic. MIC2 remains an overall failed/incomplete run:
the helper stopped after additional admissions. Both runs nevertheless delivered
the entire same fixed reply: **337,680 PCM frames (14.07 s at 24 kHz)**, with an
exact PCM hash match, one original reply owner, final retirement, and zero
reported speech gaps. A completed reply prefix does not make the MIC2 run pass.

| Retained result | MIC1 / C-Media | MIC2 / Jieli |
|---|---:|---:|
| Status | `diagnostic_collected_unscored` | `failed_or_incomplete_diagnostic` |
| Admitted inputs / endpoints | 1 / 1 | 3 / 2 |
| Full fixed reply | 337,680 frames | 337,680 frames |
| Capture diagnostic frames | 1,120,000 | 547,680 |
| Runtime / transport exit | 0 / 0 | 1 / 2 |
| Room observer | 100 s, valid | 100 s, valid |
| Rollback verified | yes | yes |

MIC2's later terminals were `no_audio_answer` and `runtime_failed`; the host
requested Stop and the runtime retained `directed run interrupted by SIGTERM`.
The admission sources are **unclassified**. These records do not establish that
an extra input was echo, replayed coffee speech, a person, or background noise.

## Treatment and boundaries

Both finite 70-second `directed-fixture` runs used the same immutable binary,
Sonora AEC3, noise suppression off, capture-clock reference, recovery off,
V1-baseline turn-taking with its 100 ms tail, a 200 ms startup buffer, and ring
ceiling 12. No cloud provider, motor, or camera context participated. The selected
microphone differed; the physical paths and their existing processing were not
made acoustically equivalent.

The iMac physically replayed the same cached sunset and coffee clips once each.
Sunset followed observed readiness. Coffee followed completion of the first
player and at least five seconds after the host received the original owner's
first-write event, while that occurrence remained active. The local prepared
player's 100 ms first-copy bound was not a fresh Lamp grant or an acoustic timing
measurement. The original scene's conversational preconditions were not recreated.

The Lamp reply was three copies of the cached 4.49-second reply separated by two
300 ms zero intervals. Its accepted speech PCM SHA-256 was
`3d98cfa176290e5e120f6c5350c04a127bdffaace3711e64b7aaa23d0a630ebe`
in both runs. Idle/prime zero frames are recorded separately from speech gaps.

Actual retained command/rollback evidence records **no mixer writes** and exact
final raw mixer strings. The original plan's inherited `mic_comparison` AGC/gain
values are not the executed treatment. No gain, AGC, or board-DSP cause is inferred.

## Preservation and cleanup

The independent audit verified each native archive's 20 files plus manifest,
continuous capture/render record sequences and offsets, completion certificates,
and both complete source-player deliveries. Each continuous room recording has
4,800,000 mono float32 frames at 48 kHz and 9,375 callbacks, with no reported
faults or clipping. This verifies retained data integrity, not intelligibility.

Both rollback receipts record the same boot, owned trial released, no hardware
owners before restoration, stationary legacy restored, software microphone and
speaker muted, empty trial drop-ins, and original writable controls restored.
Neither runtime required a forced kill. These are historical receipts, not a new
inspection of current device state.

[results.json](results.json) contains exact commands, binary/source/ARM pins,
archive, player, diagnostic, room and rollback hashes. Commands are historical;
retained output leaves must not be reused. Original files and provenance remain
under `.cache/physical-audio/20261012/micpath-{mic1,mic2}-lw4abily/` relative to
`lampOS`, including `preservation-manifest.json` and `rollback-receipt.stdout`.
Each cache preserves 57 original files and 44 provenance files. The independent
report SHA-256 is `e4012d72754ea0df5998cab5d24795cb8476d53ab5f96bf0997e4be08a1fc152`.

No human listening, ASR, signal-path diagnosis, or acoustic score is claimed.
Host, Linux monotonic, CoreAudio and WAV sample clocks are not calibrated to one
another. Equal accepted PCM does not establish equal acoustic arrival or input
gain. This pair does not qualify natural duplex, promote a default microphone,
or include the separate later MIC1 Gemini experiment.
