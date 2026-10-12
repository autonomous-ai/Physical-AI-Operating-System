# Voice handoff to Agent 1 — 12 October 2026

## Ownership and immediate state

The owner stopped Main's broad goal and then clarified the split: **Agent 1
owns voice and works directly with the owner; Main owns servos and the V1-to-Rust
motor work.** The owner will assign Agent 2 another component directly (Harness recipient `brain`). This
handoff assigns no new work to Agent 2. The current Harness recipient for Agent 1
is **`voice`** (formerly `Gemini voice-session reliability A1`, office,
`merry-zebra`). Use Harness Agent Messages, not tmux.

Main is making no more voice edits or running voice experiments as part of this
handoff. No Main build, acoustic run, container or motor command is pending.
Coordinate exclusive physical test windows and shared office VM use with Main;
do not wait for Main to relay ordinary voice decisions through the owner.

**Voice works for ordinary turn-by-turn chat, but reliable natural interruption,
background-speaker rejection and a real-human pilot are not qualified.** Do not
interpret installed binaries, passed tests or zero playback gaps as that result.

## Start from these exact sources

Repository: `autonomous-ai/autonomous-os`.
Integration worktree on office:
`/Users/autonomous/harnesses/worktrees/autonomous-os/lamp-v2-chat`.
Runtime base: `7e28dc7ace66eb26cb3e3332f3dcadf02f1f5b9a` on `lamp-v2-chat`;
it was clean when this handoff began. This handoff changes documentation only.
Do not use the unrelated dirty `calm-pebble` worktree as the integration source.

The reviewed, composed voice candidate is **not committed to the integration
branch**. It is reproducible from that base and two patches, in this order:

1. `/private/tmp/lamp-mic1-endpoint-composed-v4y0o344/combined.patch`
   - SHA-256: `2a16a0e5cf57f4b45bca499b3b1457448d3d38ec29b9f67493fc4789eebf8287`.
   - 45 changed paths: 8 MIC1 paths plus 37 endpoint paths, no overlapping
     runtime edits and no manual resolutions.
   - `receipt.json`, `changed-files.json` and `source-manifest.json` beside it
     describe the exact composition. The 518-file source manifest hashes to
     `204c1de3053b202308f8d9767f27b16604ce2fe58411e6bfc1ac579ee07bed0d`.
2. `/private/tmp/lamp-mic1-endpoint-docs-s_181fec/documentation.patch`
   - SHA-256: `9e4a330ec0517d7482d40bc06a0c07a6c71081dc4a4c7778ecb7c337931bf109`.
   - Seven English documentation files only; no executable source changes.
     It replaces stale pending-gate wording with the actual native results and
     retained failures. `manifest.json` records a strict reconstruction check.

Apply from the **parent repository root** in an isolated checkout of the named
base, checking each patch first. The final ready-to-read standalone source is
`/private/tmp/lamp-mic1-endpoint-docs-s_181fec/source/lampOS`.
The first source copy remains frozen at
`/private/tmp/lamp-mic1-endpoint-composed-v4y0o344/source/lampOS`.

For retention beyond `/private/tmp`, exact copies of both patches, their
manifests, the two detailed verification folders and native receipts are in
the integration worktree's ignored `.cache/handoffs/voice-20261012/`, relative
to `lampOS/`. `manifest.json` there pins every copied file. This is a local
backup, not a Git-published candidate. It contains no model, binary or recording.

Candidate behavior:

- Only `--microphone mic1 --reference-clock capture` holds four original 10 ms
  frames before AEC/VAD. This repairs reference timing at the cost of about
  40 ms capture age. Original timestamps, cancellation and the 100 ms input
  age bound remain. MIC2 and fixed-reference paths do not gain the hold.
- Smart Turn runs in a separate bounded Rust endpoint process. `--endpoint
  smart-turn` is opt-in: 1,000 / 1,200 / 1,500 ms parity quiet windows, with
  failure falling back to silence. Defaults remain MIC2 and silence endpointing.
- A2's **separate** fast proposal is `lamp-a2-endpoint-fast` at `5a92b37c4`,
  reported tested code `57ed58110`. It is not in either patch. It holds early
  answers until the baseline confirmation boundary and requires kept-start
  plus v1-baseline/duplex. Its reported held-out synthetic-provider result was
  1.6 s versus 2.1 s median, with 9 versus 3 false cuts. Those are candidate
  results, not device latency. Use `docs/endpoint-fast.md` on that branch;
  no new assignment to A2 is implied.
- A1's speech-floor branch is separate. The recorded MIC1 replay found no
  detector-start changes; it is not an established solution to missed speech
  or background admissions.

## What has actually worked and what remains broken

The earlier [installed ordinary conversation](../docs/verification/installed-eight-turn-20261011/README.md)
completed 8/8 context-linked replies with no reported playback gaps. Its median
local-endpoint-to-first-ALSA-write time was **1.123 s**, excluding the preceding
1.2 s silence wait. The separate [kept-start ordinary control](../docs/verification/provider-control-20261011/README.md)
also completed 8/8, median **1.214 s** on that same software boundary. Adding
the configured wait gives approximately 2.32 / 2.41 s respectively; neither
is an accepted acoustic last-phoneme-to-first-word measurement or a causal
comparison. Reuse existing V1 evidence; the owner deferred new V1 benchmarking.

Four later MIC1 runs are fully described in the documentation overlay's
`docs/verification/mic1-reference-20261012/{README.md,four-runs.md,four-runs.json}`:

| Run | Actual result |
|---|---|
| Coffee alone | One on-topic answer. Opening-word retention unresolved. Two fruitless yields paused playback by about 490 ms each. |
| Fixed-source timing | Exact fixture output retired; no second admission. A local fixture, not a Gemini interaction. |
| Gemini with timing correction | Sunset answered; the delivered overlapping coffee request generated no candidate, admission or answer. Interruption failed. |
| Lower speaker | Three owners from two delivered sources; two replies, then helper abort. Other room speech was present. Not a successful interruption test. |

All four are cached loudspeaker speech, `human_listened=false`, unscored
acoustically. Recording/diagnostic integrity and rollback checks passed, which
does not change the failed interaction outcomes. The Gemini timing run overlapped
an office VM build; preserve that confounder. The lower-speaker trial changed
only C-Media playback 9,9 to 3,3 and restored it to 9,9.

A1's fixed-source comparison reports roughly **2.8 to 24.7 dB** whole-reply
level reduction with the timing correction. The quieter overlapping source
still was not admitted. Cancellation improvement is not speech-preservation
proof. Reply onset leakage, marginal quiet-source VAD/history, background
admission and microphone selection remain separate problems.

Useful existing analysis, local only:

- `/private/tmp/lamp-a1-mic1-timing-result-20261012T0704Z-rrcXJ4/`
- `/private/tmp/lamp-a1-mic1-speaker-low-result-20261012T0737Z-Zbo2Pf/README.md`
- `/private/tmp/lamp-mic1-hold-integration-M3bwNoQf/private-provenance/`

Kyle's hardware note: MIC1 is the C-Media/Unitek ambient input; MIC2 is the
Jieli dual-microphone board, reported to have onboard echo/noise/beamforming DSP.
He reports direct human speech is clearer than replayed loudspeaker speech on
MIC2. That is useful evidence to test, not a verified DSP specification or a
reason to pool human and loudspeaker cohorts. Do not promote MIC1 by default on
the current synthetic evidence.

## Exact gates and staged artifact

The composed candidate passed format, strict all-target Clippy for the affected
crates, and Linux ARM64 release builds. Focused emulated results include capture
hold 3/3, reference link 6/6, reference prime 8/8, IPC framing 4/4, live endpoint
18 passed / 1 ignored and endpoint local 10 passed / 6 model-dependent ignored.
**IPC processes were 11/12 under QEMU**, with two distinct failure observations
retained. The same process-test ELF then passed **12/12 on 4ace**. No full
combined workspace or endpoint-enabled physical conversation pass is claimed.
At 09:56 UTC, Agent 1 reported starting the composed-source host formatting,
Clippy and full workspace gate, two jobs at nice 10; its result is pending.
Do not duplicate that gate or infer success from its start notice.

Exact commands and receipts:

- `/private/tmp/lamp-endpoint-arm-gate-m7cg15uu/`
- `/private/tmp/lamp-endpoint-native-check-lylkne7s/`
- Final source: `docs/verification/endpoint-native-20261012/{README.md,results.json}`.

Three real A733 self-check processes passed. Warm median inference including
feature extraction was **367.7–371.8 ms**, warm maximum **368.8–373.7 ms**;
the first cold classification was 561.0 ms. These ran as root with nice 10 and
the 256 MiB address-space limit, without concurrent voice. They do not measure
end-to-end response time or loaded runtime performance.

The following are staged together on 4ace, but were **not started as a live
conversation** by the installation/self-check:

`/opt/lampOS/releases/7e5a7e015f9f0e682f6577e290d8375849421a167dd4bf0ad0b5ef69464f7ea7/`

| File | SHA-256 |
|---|---|
| `lamp-live` | `7e5a7e015f9f0e682f6577e290d8375849421a167dd4bf0ad0b5ef69464f7ea7` |
| `lamp-endpoint-worker` | `1c4de3b310685296c7feba862a2d10ad86814ebf5ddfb6539812f3cc24d360d6` |
| `smart-turn-v3.2-gpu.onnx` | `ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be` |

The four MIC1 recordings used earlier artifacts, not this combined binary.
Inspect the actual unit override, binary hash and argv before calling anything
the installed default. Historical pilot documentation is not a fresh device
inventory.

## Reusable recordings and test setup

Saved originals, cached sources and manifests are under the integration
worktree's `lampOS/.cache/physical-audio/20261012/`:

- `mic1-coffee-0h0e416g/`
- `mic1-timing-fixture-69rmqb_6/`
- `mic1-gemini-timing-_r4egoic/`
- `mic1-speaker-low-lmuglbg8/`

Each includes original receipts and a manifest. Reuse those exact WAVs rather
than regenerating speech. Preserve all attempted, failed and withheld cases;
keep raw room recordings and incidental speech local. Their detailed hashes
are in the four-run report. The older V1/V2 evidence remains in
`lampOS/.cache/benchmarks/` with its original cohort labels.

Use the Rust prepared player and [serve-session runner](../docs/voice-eval-serve.md)
for event-triggered follow-ups, not fixed sleeps or stdout readiness. The player
arms before the trigger and checks the original grant expiry immediately before
copying the first source sample. CLOCK_MONOTONIC must match on both host sides;
Python's `time.monotonic_ns()` used a different epoch on this Mac. The evaluated
native runs also retained private operational controllers; these are not shipped
Rust runtime dependencies and must not be re-executed with old one-shot pins.

The iMac microphone, speakers and camera previously worked while the screen was
locked, after the owner approved permissions. Reuse the existing authorized
observer bundle/player identity from the retained run receipts rather than
creating another app requiring fresh consent. A historical prepared player is
`/private/tmp/lamp-prepared-player-a2r2-hafqqg2l/artifacts/lamp-observer`, SHA-256
`d51eb60f36234f1e9df0bce9f8a660f2a4b6903bec856d12efe1f5ed418890aa`.
Current access and physical placement still need checking for each run.

## Device boundary for voice

Last observed target: `lamp-4ace`, `orangepi@172.168.20.159`, A733/aarch64,
glibc 2.36. The existing administration path is:

```sh
/usr/bin/ssh -S /private/tmp/lamp-4ace-onsite-control \
  -o BatchMode=yes -o StrictHostKeyChecking=yes \
  -o UserKnownHostsFile=/private/tmp/lamp-4ace-known_hosts \
  -o ConnectTimeout=5 orangepi@172.168.20.159
```

This handoff makes no new device connection. Last recorded boot was
`76a7af25-a1aa-4f3d-bbcd-eddf947227ca`; do not reuse same-boot snapshots after
a reboot. Private provider configuration is `/etc/lampOS/pilot-4ace/provider.json`;
read the credential in place, never print it or add it to source.

Main's last observed state had no active Rust conversation and legacy audio
muted. Preserve the stationary serial guard at
`/etc/systemd/system/hal.service.d/99-lampos-4ace-stationary.conf` and
`/var/lib/lampos-4ace-audio-onsite-20261011/stationary.env`.
**Normal legacy HAL shutdown can park/release joints.** Use the reviewed
stationary handover and fresh rollback evidence, not a blind service restart.

There was a head-to-base contact in the separate manual motor test. Main owns
its correction; clearance and physical recovery have not been confirmed.
Voice work must not issue home/park/torque/motor commands or undo the serial
inhibition. No actuator change is part of this voice handoff.

## Suggested continuation, under the owner's direction

1. Read the final candidate and receipts, then preserve it on a voice-owned
   branch. Keep motor files and Main's private motor candidate out of that commit.
2. Re-establish a stationary ordinary-conversation control with saved prompts,
   continuous room recording and exact artifact/flag/mixer identity. Change one
   treatment at a time; no need to rebuild V1 benchmarks.
3. Qualify direct-human hearing and interruption separately from synthetic
   loudspeaker replay. Check false self-interruptions, first-word retention,
   breath/continuation, topic changes and background/unaddressed speech.
4. Trial the parity endpoint only with its failure fallback and real device
   cost visible. Keep the early proposal separate until continuation behavior,
   false cuts and acoustic latency support it. Existing kept-start probes and
   a native correction are limited observations, not blanket qualification.
5. Publish exact code/artifact, settings, all attempts and the distinct timing
   boundaries. Tell the owner what they can reliably try and the remaining
   failures; do not label the voice experience finished from a test count.

This document preserves existing evidence only. No build, cloud call, playback,
recording, deployment or motor operation was performed to write the handoff.
