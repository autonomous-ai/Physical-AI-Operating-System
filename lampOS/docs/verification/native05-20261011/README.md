# Native05: sustained conversation on 4ace

**Passed the operational conversation/idle/conversation gate; acoustic quality
remains unscored.** The iMac played eight cached Daniel prompts at 185 words per
minute, remained quiet, then played the same eight after 1,211.650239 seconds in
the same runtime. All 16 replies reached their actual speaker-retirement and
turn-completion receipts. No extra admission, reference fault, DSP reset, provider
reconnection or second service cycle was recorded. Total runtime continuity was
1,295.074642 seconds (21.6 minutes).

Code: `55d7af98e149591eed7882b9ace7741486daa842`. The runtime source manifest and
binary match the pins in [results.json](results.json). This is the prime-handshake
repair and capture-clock integration, with V1-baseline turn-taking, noise
suppression off, fixed 1,200 ms endpoint, plain 200 ms playout start, and reference
recovery off. Experimental duplex, early endpointing, camera, ring and motors
were disabled. This is not a V1 benchmark.

## What was observed

| Observation | Result |
|---|---|
| Scheduled prompts / completed owned replies | 16 / 16 |
| Unexpected admitted turns / dropped accepted reply lifecycles | 0 / 0 |
| Reference fault / reset / recovery | 0 / 0 / 0 |
| Reference samples | 1,288, one capture/DSP/playback epoch |
| Final learned microphone/speaker drift | 168.011 ppm |
| Unread resampler frames | 931–1,317 of 2,400 |
| Native metadata | 3,406 contiguous events, final ACK with no loss |
| Cooperative SIGTERM to `run_end` | 23.687 ms, software boundary |
| Runtime exit / stationary rollback | 0 / verified |
| Mac recordings | Three valid 600 s files, zero dropped or clipped frames |
| Adjacent recordings' overlap | Approximately 120 s, separate clocks retained |

The previous fixed-reference run failed at 378.6 s. This run exceeded that point
and completed the later conversation with no clock reset. It demonstrates this
run's continuity; it does not establish all-day fault tolerance. Native04's
startup failure and invalid recorder attempt remain retained separately.

## Reply timing and remaining playback gaps

Across these 16 replies, median **local endpoint to provider first audio** was
**1.045969 s**; median **local endpoint to first speaker write** was **1.145450 s**.
Both exclude the 1.2 s endpoint wait. Adding that wait gives an estimated
**2.345450 s**, not speech-end to first audible word. Different generated replies
and service conditions prevent attributing changes from earlier runs solely to
the audio-clock repair. No speedup or causal A/B claim is made.

Four replies contain accepted-zero playout gaps: turn 3 **30 ms**, turn 4
**110 ms**, turn 7 **20 ms**, and turn 8 **10 ms**. The remaining twelve have none
reported. All four gaps close and all 16 replies retire, but completion does not
mean seamless speech. Room waveform/content review remains separate; do not
silently promote continuity playout from this run.

## Evidence boundary

The private archive is `/private/tmp/lamp-serve-native-20261011-05` on office.
All attempts and source/player/observer receipts are retained. `results.json`
contains sanitized numerical evidence and hashes, not room audio or credentials.
The independently generated metadata summary is
`/private/tmp/lamp-serve-native05-summary-20261011`; it checks native transfer
hashes and receipt relations, but does not rehash or decode room WAVs itself.
The recorder runner verified each finalized WAV and its ledger before claiming
coverage. An independent raw-file/content audit then verified all three WAVs,
their contiguous callback ledgers, native archive/transfer identities and exact
source files. Local ASR corroborated all 16 questions and corresponding English
replies in 194.432 seconds of selected room windows, including the later eight.
It did not review all idle audio or establish subjective smoothness.

Independent local audit: `/private/tmp/lamp-native05-independent-floj9b`;
report SHA-256 `af679e1d4174f113d9a4c8c31a91d3d993a624e31bd5a353873f19494dd9cab6`,
receipt `655920f0118e4f27fb33decc2ea4405f75a409739aa6a5bd106c810c71d21386`,
input manifest `c32af6397efb8eea42441e50cb2a7c9010fe7f923e80ffaaf211b4a2dd2e5834`.
All 89 listed input/tool identities remained unchanged. Recordings stayed local;
ASR is corroboration, not a human listening verdict or an accepted word boundary.

`serve` intentionally records bounded metadata without continuous raw runtime
PCM. Its `complete_evidence=false` and `diagnostics_certified=false` labels remain
true; a healthy metadata finish is not a PCM diagnostic certificate. Observer
host and CPAL clocks remain separate from Lamp's clock. No human-listened flag,
accepted acoustic latency, speech-quality score, natural-interruption claim,
physical-mute/reopen claim, vision claim or motor-safety claim is assigned.
