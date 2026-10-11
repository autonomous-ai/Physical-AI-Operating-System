# Native kept-start ordinary-conversation control

Eight original cached questions received eight complete owned replies on real
Lamp-4ace, with `--provider-continuation kept-start` explicitly enabled. There
were no extra admissions, cancellations, reported playback gaps, reference
resets or runtime faults. All questions followed complete replies; this is an
ordinary-conversation control, **not an interruption or continuation test**.

The source is `2021c1bb9a01bf207c8ab11a05ab2f7ee91a7ad1`; the AArch64 binary is
`96d5a03ad4931ac1fedc9a3ac8ff76e33c160ccc348dd7256f97f490fa4fc44b`.
The original result SHA-256 is
`c2f09411b3bf0009b8421a1ea33cfac9fd933c6cab8582b6954f4007c851d59a`.
The independent audit rechecked all 568 pinned original inputs/source files,
including the 65-file frozen controller and 426-file runtime source export.
The older candidate label in the build manifest is retained; actual source
matches this canonical commit apart from three documentation files.

## Response intervals

All response intervals use the Lamp's monotonic clock, begin at local endpoint,
and exclude the configured 1,200 ms silence wait. First ALSA acceptance is not
the first audible word. Eight samples are retained separately in each cohort.

| Software boundary | Prior ordinary run median | New ordinary run median | New range |
|---|---:|---:|---:|
| Endpoint to provider audio | 929.330 ms | 1,036.712 ms | 896.910–1,192.123 ms |
| Endpoint to first ALSA write | 1,123.123 ms | 1,214.119 ms | 1,086.195–1,560.462 ms |

Adding the configured endpoint wait gives about 2.414 s for this run; this is an
estimate, not an acoustic latency score. Identical cached prompts, provider
configuration, mixers and player were used, but response content, runtime and
cloud conditions differ. No causal speedup or kept-start cost is established.
The prior run is [retained separately](../installed-eight-turn-20261011/README.md).
Initial service to listening readiness was 2.619693 s.

## Delivery, presentation and cleanup

Eleven read-only scheduling checks yielded eight original fresh grants and
three expired receipts that played nothing. All eight exact WAVs were fully
submitted once. Every callback's first source copy preceded its original
100 ms grant expiry; remaining headroom was 7.789–34.276 ms. No prompt replay,
renewed grant, duplicated source or player failure occurred.

The one provider session returned context-linked ocean answers, an average-depth
correction, supportive discussion, a small pre-meeting action and a final brief
reply retaining that meeting context. This is transcript relevance evidence;
there is no independent fact check or acoustic content score.

All 9,121 service events were contiguous and acknowledged, with no lost or
unconfirmed events. All 3,950 requested ring frames have matching presented
receipts. Request to SPI return was 1.419–4.241 ms; optical timing is unmeasured.
The 180-second mono 48 kHz room WAV and callback ledger are complete, with no
reported dropped, clipped, nonfinite or discontinuous samples. Recordings stay
local. No ASR or human listening was performed: `human_listened=false` and
acoustic scores remain null.

Stop request to verified inactive was 238.308 ms; SIGTERM observation to
`run_end` was 22.039 ms. All six observed process identities exited, with no
forced kill, empty hardware-owner lists and clean unit/helper exits. The exact
instance override was removed after confirmed release. The installed stopped
e9/b1 template, actual mixers, stationary serial inhibition and software mutes
were restored unchanged. The candidate artifact remains staged; it was not
promoted to the default service.

## Evidence boundary

[Sanitized results](results.json) preserve every turn, source identity, comparison,
receipt and limitation. The native archive SHA-256 is
`d21fe546af8890fa46d91937af8afcf7af6305a17cbc979e87e34000990e0a5e`.
The private audit receipt SHA-256 is
`1023452c1e5cb5a2f63493dd57829e96107268c41c859371e61a87576d529391`.
The audit generated six private Python bytecode caches while importing helper
readers; all frozen source and original evidence bytes were rechecked unchanged.

No held successor, abandonment, natural interruption, background addressee,
physical privacy toggle, camera, motor or failure notice was exercised. This
finite run does not establish real-human acceptance or all-day reliability.
