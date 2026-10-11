# Connected conversation on lamp-4ace, 2026-10-11

Two real-device runs reused the same eight cached iMac prompts: greeting, ocean
fact, follow-up, correction, topic change, work advice, a tighter time constraint,
and a preference for brief replies. Each next prompt required the actual prior
whole-turn completion and fresh scheduling evidence. The iMac was locked and its
approved microphone recorded both voices. This is synthetic loudspeaker input,
not a direct-human cohort or an adaptive agent-to-agent dialogue.

Both runs used source `6e6c58b42`, native binary
`0aa2387a4c382ea25568f5099e11427f5a05b8caa93ea01beff23836aaccb822`,
Kore, V1-baseline turn taking, a 1,200 ms local silence window, AEC on / NS off,
and a 200 ms playout horizon. Only the provider configuration intentionally
changed: Extended Thinking/LOW versus plain Live with no thinking field.

## Results and limits

| Measurement | Extended/LOW | Plain Live |
|---|---:|---:|
| Attempted / software-completed turns | 8 / 8 | 8 / 8 |
| Extra input admissions / cancellations | 0 / 0 | 0 / 0 |
| Endpoint to provider first audio, median | 1.273 s | 1.137 s |
| Endpoint to first ALSA acceptance, median | 1.367 s | 1.292 s |
| First ALSA interval range | 1.173–1.984 s | 1.216–1.524 s |
| Turns without accepted-zero gaps | 6 / 8 | 7 / 8 |
| Accepted-zero gaps | 130 ms and 80 ms | 120 ms |

These software intervals begin **after** the 1.2-second endpoint wait. They are
not speech-end-to-first-audible-word measurements. The 75 ms median difference
is descriptive: replies, conversation histories and cloud conditions differ.
No causal model speedup, acoustic p50/p95 or conversational acceptance is claimed.

All sixteen provider replies are English and broadly relevant. Local offline ASR
of the declared test intervals independently recovers all prompts and complete
replies. Follow-up topic references and the depth correction work in these cases.
Both models give multiple actions when asked for one smaller next step, which
remains a response-quality defect.

Extended adds introductions to turns six and seven, then generates the actual
advice separately. Their substantive first speaker writes occur 5.342 s and
5.128 s after endpoint; the introductions do not satisfy the user's request.
Plain Live avoids those introductions in this run and is the next comparison
configuration, not a proven release winner.

The three gaps occur when the coordinator has only its intentional held final
240 samples and no pending speaker command. The next worker-observed audio
arrives after the gap begins. This identifies late supply at that boundary,
not whether generation, network or worker polling caused the delay. Test the
opt-in continuity start separately on the same compiled runtime.

V1-baseline suppresses prospective input during Lamp playback and the tail.
Zero self-interruptions here does not establish continuous listening, natural
barge-in, addressedness, multiple-speaker handling or visual choreography.

## Same-runtime playback comparison

A second pair used the same eight cached questions, plain Live configuration,
source `8feaf64b9` and binary
`7acfabcd6f38e4cf1d9a8f145c8b3c1ab479118a8bc1edc97897d0e767d9871c`.
Only the playout option intentionally changed: the 200 ms coverage horizon
versus `--playout-continuity`. Both completed all eight replies with no extra
admission, cancellation, capture/reference reset or accepted-zero speech gap.
Every reply has one generation; no separate introduction delays the actual answer.
Local offline room ASR corroborates all sixteen prompt/reply pairs.

| Measurement | 200 ms coverage | Continuity |
|---|---:|---:|
| Attempted / software-completed turns | 8 / 8 | 8 / 8 |
| Endpoint to provider first audio, median | 1.321 s | 1.335 s |
| Endpoint to first ALSA acceptance, median | 1.475 s | 1.647 s |
| First ALSA interval range | 1.395–1.789 s | 1.555–1.807 s |
| Accepted-zero speech gaps | 0 | 0 |

These intervals still exclude the preceding 1.2-second endpoint wait and are not
acoustic word timings. Continuity's observed median is 172 ms later. Different
generated answers and cloud conditions prevent attributing that difference to
the policy. This short-answer pair shows no gap-reduction benefit; continuity
remains optional, and ordinary tests retain the 200 ms coverage setting. Its
recorded-supply replay benefit does not establish a physical default winner.

A first provider packet can contain only one sample. In coverage turn four and
continuity turn three, dispatchable audio follows that packet by 351 ms and
381 ms, respectively. Gate age therefore includes unavailable supply and cannot
be labeled entirely as intentional buffering or model latency. Exact typed
render records distinguish startup/idle zeros, verified terminal padding and
speech starvation; neither new run contains the last category.

These two additional room recordings each contain 165 seconds of mono 48 kHz
float32 audio, with no clipping, dropped/nonfinite samples or reported stream
faults. Source WAVs, player settings, distinct capture/render acknowledgments
and final seals verify. The finite runtime's raw SIGTERM exit remains 1; its
verified completed turns and cleanup do not rewrite that status.

Local evidence, with no raw recording uploaded:

- Coverage: `/private/tmp/lamp-connected-composed-plain200-20261011-01`;
  archive SHA256 `b412032b41b0525ba07902c05d7851d64be1c9a9650a6805ac5f2d56a1d11074`.
- Continuity: `/private/tmp/lamp-connected-composed-continuity-20261011-01`;
  archive SHA256 `42c61071649a5ef227b9dfbda6509ebe73359ebc4a6096109521aec9c65f17d8`.
- Audit: `/private/tmp/lamp-connected-composed-audit-gxln2j9g/report.md`;
  SHA256 `784cd2ae4f545c6c10ec86371fa2d40364b113d455fb7eae298c1b3221d2ede8`.
  All 88 input identities were reverified; input-manifest SHA256
  `490cc930523d6bf71c899c8edd398a29380debc42852a0a75d0fd9cf9124dd1c`.

All four runs total 32 software-completed turns. They remain separate cohorts,
not a pooled latency distribution. `human_listened=false`, accepted acoustic
latency is null, and continuous hearing/addressee handling is still unqualified.

## Original provider-comparison evidence

Both continuous room WAVs contain 7,920,000 frames: 165 seconds, mono float32 at
48 kHz. Neither reports clipping, dropped/nonfinite samples or stream faults.
Capture/render final certificates match actual coordinator acknowledgements;
accepted PCM and final retirement cover every original output occurrence.
Both finite sessions retain their raw cooperative-SIGTERM exit status 1. Verified
cleanup and stationary, muted legacy restoration do not rewrite that as exit 0.

All raw recordings and credentials stay local. Artifact paths on the test iMac:

- Extended: `/private/tmp/lamp-connected-extended-20261011-01`;
  native archive SHA256 `d1c8af0574a3fb8400ca3b00ae77dfb2b8c0e8f886b198af24f666eadc03fa54`;
  room WAV `d2382181f250326708571faf77b4c1915080fdbff5b1d8c10f333b8399582c04`.
- Plain: `/private/tmp/lamp-connected-plain-20261011-01`;
  native archive SHA256 `748261c56b392724f890c22b0a20d54f85f1b3121e6b508b515ff5df002403fa`;
  room WAV `f7d89f32ab042c85753620953ccef59b5baff0cdf8a0e925dfaaef48c52d53b2`.
- Independent audit: `/private/tmp/lamp-connected-audit-pxf_osdi/report.json`;
  SHA256 `ad3366ae9903d8bb7decd6c64ec0a4bedc58a37fa738ba333cfc3bf7cad64ff8`.
  All 88 input identities were reverified; manifest SHA256
  `c97dbf11c60c14fcfe17577ad275524581a8480a3ad7bee5d3943b070321611b`.

`human_listened=false`; accepted acoustic timing remains null. ASR corroborates
words, not prosody, emotional quality or exact acoustic word boundaries. Numeric
analysis used supported Python 3.12 / NumPy environments. No recordings were
uploaded or used to tune an echo gate in this audit.


## Continuous service qualification, attempt 02

The first attempt stopped before any prompt because the private test harness
treated pending asynchronous metadata as failure. A narrowly corrected harness
retained the original readiness cue and added a bounded wait for metadata. The
second attempt used a fresh namespace and the same `7acfabcd...` Rust binary,
plain Live provider, V1-baseline turn taking and 1.2-second endpoint wait.

Eight scheduled prompts each reached one endpoint and one completed reply.
The qualification then failed on an unexpected input during the idle interval:
264.509 s after capture start, 169.607 s after reply eight retired. The harness
requested Stop 7.694 ms later. No endpoint, provider start, transcript or audio
for that ninth input was observed. Planned prompts 9–16 were withheld. The
165-second room recording had already ended, so the source is unknown; this
is not established self-echo or an incorrect audible response.

The runtime exited 0 after requested Stop, with healthy final metadata closure
and verified stationary, muted restoration. Controller, provider, capture, DSP
and reference identities remained unchanged through the retained observations.
Clean Stop does not turn this into a successful 16-turn qualification.

Endpoint to first ALSA acceptance ranged from 1.126 to 1.573 s for these eight
replies, excluding the preceding endpoint wait. Turn four reported 2,880 accepted
zero frames during speech, or 120 ms at 24 kHz; the other seven reported none.
This service mode retains metadata rather than raw runtime PCM, so the gap
report has no independent accepted-PCM verification. No acoustic timing or
human-listening score was accepted, and no room ASR was performed for this run.

Across five distinct conversation cohorts, 40 scheduled replies have completed
in software. That count excludes withheld prompts and does not mean all five
qualifications passed. Do not pool their latency distributions. The next
continuous-session check requires room recording through the entire idle period.

Local evidence: `/private/tmp/lamp-serve-native-20261011-02`. The independent
audit is `/private/tmp/lamp-serve02-audit-81e2mntu/report.json`, SHA-256
`632e862f51d0de25f478d7c21cc6a49d6267a5fa76fafbcb9a8c5dd3a512dcd4`.
All 70 audit inputs were reverified; input-manifest SHA-256
`ce13592261e068d8deeb4ab497001768d8baaaa2409c0881ed5b0a9ed51b38e4`.


## Third continuous qualification: reference backlog

The next run used the same `7acfabcd...` runtime and fixed-margin reference,
with a whole-interval observer. It completed the first eight scheduled replies,
then failed during idle at 378.626563 seconds from the continuity start. No
second block ran: all eight later prompts remain withheld. This is a failed
continuous-service qualification, not a 16-turn success.

Capture reported `reference_backlog` at `24532161659` us, expected sequence
37862, playback epoch 1, accepted/analysed-through 9,087,120 frames, and 37,855
capture blocks processed. The last accepted and analysed timestamps were
`24532153260` and `24532153421` us; the observed speaker queue held 432 frames.
Shutdown then reported a refused speaker connection and abnormal worker exit.
This identifies the failure boundary; it does not yet distinguish clock drift,
scheduler delay, transport loss or a queue-sizing error as the root cause.

The Mac observer completed 600 seconds of mono 48 kHz float32 audio with no
reported clipping, dropped frames or stream faults. Its measured host/CPAL
coverage spans the entire target interval, with no stitching. The target ended
before a second recording was needed. Neither a source classification nor an
acoustic latency distribution is inferred from that recording. Runtime exit is
1; the separate rollback receipt confirms the stationary prior runtime restored.

Local evidence: `/private/tmp/lamp-serve-native-20261011-03/`.
Room WAV SHA-256:
`a3c4231b4654a1b5c749c2b0d55d2472ebc7f5123f573a10746604e0634ef15b`.
This run is separate from the second qualification's unexplained idle input.
