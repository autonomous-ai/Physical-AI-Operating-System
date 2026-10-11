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

## Retained evidence

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
