# Opt-in V1-style turn taking

`lamp-live directed` and `directed-fixture` accept
`--turn-taking v1-baseline|vad-only`. The default remains `vad-only`, the existing
unqualified interruption experiment. The baseline is an opt-in candidate for
physical qualification, not a selected release policy or a full-duplex fix.
It changes admission before turn replacement, not the audio pipeline.

```sh
lamp-live directed /absolute/private/provider.json 40 /new/private/run \
  --diagnostics --noise-suppression off --turn-taking v1-baseline
```

Noise suppression remains an independent explicit setting. This example does
not authorize hardware handover. Use the existing exclusive-device, privacy,
motor-inhibition and rollback procedure before a physical run.

## V1 behavior preserved and boundaries changed

Pinned V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94` uses
`HAL_LIVE_MODE=false` in the Lamp profile. Its warm-microphone path discards
capture while TTS speaks, discards a short post-playback tail, and clears VAD and
lookback state before a fresh question (`hal/drivers/voice/voice_service.py`,
lines 1063–1115 at that ref). That avoids admitting many of its own replies by
withholding overlapping user speech too. Optional live-mode interruption is a
different configuration. This source comparison does not prove the flags on any
installed Lamp or that V1 never self-interrupts.

The Rust baseline preserves the conservative turn-taking behavior while keeping
continuous microphone capture, accepted render reference, AEC adaptation, neural
VAD computation, and the PCM clock. It does not port V1's DSP or claim sample or
acoustic parity. The tail is a **declared, unmeasured 100,000 µs**, inspired by
V1's minimum post-playback wait; Rust does not reproduce V1's RMS-tail condition.

## Admission and actual output evidence

The coordinator retains a prospective VAD candidate before it can call
`cancel_reply`, admit a replacement owner, or queue provider `Start`. In baseline
mode it discards a prospective block that overlaps confirmed own output or its
tail. It clears prospective onset/prefix state while preserving sequence and
clock checks. This operation refuses already admitted input and the owned
outage-drain phase. Those streams continue to their actual local endpoint.

A dispatched chunk is not proof of sound. Its original owner, sequence, dispatch
time and frame count remain unresolved until a matched positive speaker-write
receipt or explicit rejection. The receipt must preserve the original owner,
positive accepted count, exact first/end cursor difference and ordered source
timestamps. Partial writes are contiguous; separate speech chunks may have real
accepted zero frames between their cursors.

Positive acceptance opens the conservative mask at the **original dispatch
lower bound**, not receipt arrival. A candidate whose six triggering host-read
intervals precede all dispatched writes can still be admitted. Potentially
overlapping input waits for bounded write evidence. Speaker control is checked
at tick start and again before destructive admission, sharing one 32-event tick
budget. Budget exhaustion defers the pending decision; it does not assume the
socket is empty. Unresolved or late write evidence fails at the existing 200 ms
bound. There is no network wait or automatic retry.

After a delayed receipt, the decision uses the latest original retained block
for the existing 100 ms evidence-age check. It does not renew the candidate's
original deadline or manufacture a new capture time. The existing maximum
50-block/600 ms candidate retention still applies.

Before acceptance, every retained pre-roll block is checked again against its
original read interval and the accepted-output history. A delayed write receipt
can reveal masked pre-roll even when all six new onset blocks are after the
tail: a short, permitted read-call gap may skip the last masked interval. In
that case only the contiguous suffix after the last masked block is retained.
The six onset blocks, original candidate identity, deadline and decision-block
budget remain unchanged. Missing original read coverage fails; a masked onset
rejects the prospective candidate. The `input_prefix_sanitized` trace records
the removed block count and cutoff without recording PCM.

`SpeechDrained` reports that the latest actual accepted speech cursor has
retired according to the existing speaker queue observation. It closes the mask
at that original observation plus the 100 ms tail. It is **not** provider,
generation or conversational completion, does not call `playback_ended`, and
does not erase unfinished reply debt. Final `SpeechRetired` and whole-turn
completion retain their separate meanings. A long nonfinal output gap can
therefore reopen admission without falsely completing the old answer.

A newer actual accepted write reopens masking. A stale earlier drain cannot
close it, including a partial earlier cursor within the same chunk sequence.
Old accepted-write receipts settle their original dispatch identity even after
a legal pre-dispatch candidate replaced the reply; they cannot start playback
for the successor. Cancelled-tail receipts retain the existing cancelled outcome.

An actual successful physical drop reports `PlaybackClockReset` with the prior
`QueuedRange`, exact next epoch and original worker observation time, including
an empty old speech queue. This closes discarded output as discarded, never as
played, and allows only that evidenced next epoch. A changing owner alone cannot
invent a reset. Pending partial-command rejection obligations survive it. A
checked already-retired cursor is reported before logical reset or a direct
stop can erase it. Privacy and other hard faults keep their existing stop path.

## Bounded state and readiness

The mask keeps at most 16 recent intervals, 80 original read-interval entries,
and eight unsettled dispatch identities. Only the existing one live speaker
chunk is in flight; extra identities retain old-owner settlement obligations,
not PCM or permission for eight concurrent writes. All dispatch obligations
expire within 200 ms. Closed intervals remain for the existing 600 ms retention
window. Capacity exhaustion, missing coverage, malformed/future metadata,
sequence overflow, duplicate reset and clock regression fail explicitly.

The initial speaker epoch is 1, matching `PlaybackOwnership::new`; only an
explicit reset receipt can advance it. Original first-read and read-completed
times are retained separately. A following first read cannot precede the prior
block's read completion. No PCM is spliced within an ambiguous read block.

This is soft admission policy. It never sets hard capture/admission leases to
Unavailable, because that would revoke the reply it is preserving. The trace
records `turn_taking_availability` and `turn_taking_summary` separately from
interaction authority. Startup `listening_ready` retains its existing meaning;
there is no new per-turn cue contract. `speech_retired` or `turn_completed`
alone does not certify that this experimental 100 ms tail has elapsed. Ring
listening still requires an actually admitted input; a suppressed candidate
cannot create a listening cue.

## Verification and qualification limits

Production-path tests exercise the actual observation, decision, commit,
speaker-control and speaker-dispatch functions with a real Controller. Private
Unix channel tests force an initially empty control check followed by a delayed
write receipt, shared-budget exhaustion and a following fault. Other regressions
cover preserved replies, delayed latest-block evidence, a fresh prefix after
the tail, nonfinal gaps, accepted-stream/outage protection, privacy/lineage
changes, old-owner receipts and explicit next-epoch reset. Pure bookkeeping
tests validate partial cursors, stale drain, original read intervals and bounds.
Root checkpoint `7dde135af` passed standalone workspace fmt, strict all-target
Clippy and 1,065 host tests (zero failures, four ignored). The identical-source
rerun permitted local sockets after the sandbox blocked IPC tests. Linux ARM64
release build and strict audio/Gemini/live Clippy passed, plus release QEMU
executions of five paired-worker, 26 turn-taking and seven prefix-filter tests
(the filters overlap). The staged binary passed native CLI/loader startup.
Its first fixture attempt stopped in source-recorder setup before the Rust
target launched or any prompt played; rollback restored the settings. After a
bounded recorder startup correction, physical fixed-reply run `20261011T060157Z`
retained two inputs and one intentional fixture reply without self-cancellation
or playback gaps. All 107,760 accepted reply samples matched the fixture.

Real Gemini run `20261011T062138Z` then completed two replies with zero extra
admissions, cancellations or provider failures. It still failed conversational
quality: the second reply was Spanish despite identical English stimuli, and
had one 1,920-frame accepted-zero gap (80 ms at 24 kHz). Local endpoint to first
ALSA acceptance was 1,212,722 us and 1,146,240 us; these exclude the 600 ms endpoint
silence wait and are not acoustic measurements. Diagnostic/recording identities
and restoration verified independently. See the [current evidence](../coordination/main.md).

The host tests use synthetic PCM and software events. These two physical trials
establish only their observed bounded lifecycle behavior. No result establishes
human listening quality, direct-human overlap, TV restraint, acoustic latency
or general physical parity. Host-read completion is not an ADC timestamp; dispatch,
ALSA acceptance and nominal queue retirement are not DAC/acoustic boundaries.
The local policy target is under 1 ms per block/decision on Lamp, unmeasured.

The baseline deliberately rejects overlap with a reply and may discard the
beginning of a follow-up that starts before the tail expires. It requires a
fresh six-block onset after suppression. Pending cloud work without dispatched
output remains interruptible. Qualify fixed-reply self-echo, immediate and
post-tail follow-ups, long provider pauses and local privacy on the target
before choosing this policy or claiming a conversational improvement.
