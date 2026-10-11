# Latest installed runtime: retained qualification attempts

The third finite lab attempt completed **2/2 cached exchanges**, with zero reported
playback gaps/cancellations, 891 matching ring receipts, clean Stop and verified
restoration on the newer V2 candidate. Two earlier incomplete attempts remain
below, including every withheld prompt. This is a scoped native voice/ring check;
physical mute/reopen, real-human interaction and acoustic acceptance remain open.

| Independent attempt | Dispatched | Completed replies | Withheld | Outer result |
|---|---:|---:|---:|---|
| `3j6ge_fg` | 0 | 0 | 2 | Exit 2: host scheduling expiry |
| `j_h258gj` | 1 | 1 | 1 | Exit 2: unavailable tail state rejected |
| `6winqiax` | 2 | 2 | 0 | Exit 0: finite voice/ring/Stop check passed |

Results are not pooled into a success rate or latency percentile. The older
[installed-service](../installed-service-20261011/README.md) and
[installed-ring](../installed-ring-20261011/README.md) evidence remains bound to
its own `55d7af98e` / `738503…5b8d0` runtime.

## Exact candidate

All three attempts ran source `e9bdcea282be7b787abb0568eb46a5c7958a6cce`, binary
`b1c61a224ea9958eeaa5f06b87fdc7591128da3daf01c21e2f3c851a7643edbd`
(8,681,808 bytes), on boot `76a7af25-a1aa-4f3d-bbcd-eddf947227ca`.
Staging, live-process identity and journal executable agree. The instance-only
command retains NS off, capture reference clock/recovery off, V1 baseline,
200 ms start horizon and ring ceiling 12. Camera-context and motors were not
enabled. Failure notices explicitly report `configured=false`,
`missing=no_directory` and no automatic synthesis.

The candidate remains on disk after each attempt, but its override was removed
and the `55d7af98e` / `738503…5b8d0` installed default/template remained unchanged
at trial end. Any later promotion is a separate root action. These finite checks
do not qualify notice playback, physical mute/reopen, natural interruption or
an attended handoff.

## First attempt: both prompts withheld

`lamp-latest-installed-executed-3j6ge_fg`, invocation
`c7b149bcfff443a090f5f828cdfe0d1f`, reached provider/listening readiness and stopped
without a runtime error after 2.410761 seconds. Both prompts were withheld with
`TimeoutError: Scheduling receipt expired`. There are no input candidates,
admissions, endpoints, transcripts or accepted speech writes.

The executed host policy starts its 100 ms dispatch lifetime at request send,
including request/response and local processing. The retained target check grants
its own interval `41396992786..41397092786` microseconds. Target helper Stop
begins 4.325 ms after that check, with 95.675 ms of that separate interval left.
The failed path did not retain host send/receive timestamps. It proves the
conservative host scheduling rejection, without attributing an exact SSH,
remote-check or host scheduler delay or proving runtime readiness expired.
No lease was extended or stimulus replayed.

All 20 service rows and the matching coordinator finish ACK verify. All seven
observed PID/start identities are gone; unit inactive/exit 0, cgroup and
hardware-owner sets empty. Helper Stop → verified clean state was 192.235 ms;
coordinator SIGTERM → run_end was 29.138 ms. Same-boot stationary muted rollback,
original mixer readbacks and exact override removal verify. The observer's valid
180-second metadata and WAV/ledger hashes were checked, but idle audio was not
decoded, transcribed or independently recounted in that focused audit.

## Second attempt: one complete greeting, then withheld

`lamp-latest-ready-executed-j_h258gj`, invocation
`1694dc97978840e1a365124f21964aca`, retained two expired read-only scheduling
checks (106.761878/106.964450 ms), followed by a fresh 60.576716 ms check. The
cached Daniel greeting played **once**. These checks did not retry playback.

| Prompt | Complete original-turn provider reply |
|---|---|
| Lamp, how are you doing today? | I'm doing well, thanks for asking. How about you? How's your day going? |

The provider input hypothesis was “Hi, how are you doing today?” Its session-scoped
correlation remains unreliable; it is not used to establish the physical words.
One cached local Whisper pass over the actual question/reply crop recovered
“Lam, how are you doing today? I'm doing well, thanks for asking. How about you?
How's your day going?” It corroborates the reply, with “Lamp” spelled “Lam”; no
extra words appear in that selected hypothesis. This is not human listening.

| Original turn 1 software boundary | Target monotonic microseconds |
|---|---:|
| Local endpoint | 42964530093 |
| First provider audio | 42965483805 |
| First accepted ALSA write | 42965547287 |
| Final speech retirement | 42969257866 |
| Whole-turn completion | 42969260150 |
| Prospective input available again | 42969358731 |

Endpoint → first provider audio was **953.712 ms**; endpoint → first accepted ALSA
write **1,017.194 ms**; provider-first → ALSA 63.482 ms. These exclude the 1,200 ms
endpoint wait and are not speech-end-to-first-word timings. One occurrence
completed with zero reported gaps, extra admissions, cancellations or reference
resets. Enqueue records total 88,800 samples at 24 kHz (3.7 seconds); the final
receipt retains the original owner/generation 3, playback sequence 1 and
cursor epoch 1/frame 250800. `serve` has no target PCM diagnostics, so byte-level
accepted-output comparison is unavailable.

Slot 2 failed locally with `ValueError: No unique live invocation/idle state`
before sending a remote fresh check or launching its player. The last host-streamed
state was revision 8, available=false, owner/candidate=null, counts 1/1/1 and no
fault. Native metadata shows availability reopening **100.865 ms after final
retirement** and **98.581 ms after completion**. The native helper's final state
is revision 9/available=true. The runner rejected the still-active playback-tail
mask after completion; this is not evidence of a permanently unavailable runtime.
The second prompt remains withheld. No two-turn loop completion is claimed.

All 895 service rows and the matching healthy finish ACK verify. The runtime
stopped without error after 10.943438 seconds. All seven observed identities
exited, cgroup/hardware owners are empty and the unit is inactive/exit 0.
Helper Stop → verified clean state was **302.415 ms**; coordinator SIGTERM →
run_end **31.181 ms**. All 377 ring requests match original presentation receipts;
three stale-presentation blanks and one shutdown-black receipt are retained.
These are software receipts without optical review. Same-boot stationary muted
rollback, actual mixer readbacks and override removal verify independently.

## Third attempt: two exchanges completed

`lamp-latest-followup-executed-6winqiax`, invocation
`053e1a12f4224e7ea4de2d0b2909cde1`, completed the finite two-prompt loop on the
same candidate. The runner waited for turn 1's original completion and observed
post-tail availability before checking slot 2. Fresh read-only checks took
66.838812/68.124370 ms inside the unchanged 100 ms conservative host budget;
both player launches returned before expiry. No prompt replay occurred.

| Turn | Cached prompt | Complete original-turn provider reply |
|---|---|---|
| 1 | Lamp, how are you doing today? | I'm doing well, ready to help with whatever you need. |
| 2 | Lamp, tell me one interesting fact about the ocean. | The ocean holds over 99% of the living space on Earth, yet we've explored less than ten percent of it. |

| Individual software interval | Turn 1 | Turn 2 |
|---|---:|---:|
| Local endpoint → provider first audio | 925.674 ms | 957.576 ms |
| Local endpoint → first accepted ALSA write | 1,295.211 ms | 1,054.337 ms |
| Provider first audio → first accepted ALSA write | 369.537 ms | 96.761 ms |
| Final retirement → input availability reopens | 101.648 ms | 100.490 ms |

These exclude the 1,200 ms endpoint wait and are not acoustic timings. Both
original owners/generations 3/5 and playback sequences 1/2 match epoch-1 final
cursors 228960/519840. There are no extra admissions, cancellations, reported
playback gaps, reference resets or runtime faults. Source and generated answers
remain separate observations; this is not a causal speedup comparison.

All 2,098 service rows and the healthy coordinator finish ACK verify. All seven
observed identities exited; unit/helper exit 0, cgroup/hardware owners empty.
Runtime duration was 22.019490 seconds. Helper Stop → verified clean state was
221.088 ms; coordinator SIGTERM → run_end was 22.167 ms. Exact override removal,
same-boot stationary muted rollback and actual mixer readbacks verify. Optional
failure notices remained off; the ocean statement is recorded as spoken without
independent factual acceptance.

All **891 original ring request/receipt pairs** match, with zero rejected or
pending frames, six stale-presentation blanks and a checked shutdown-black
receipt. Request → SPI-return spans 1.517–4.737 ms; actual write calls
1.330–1.831 ms. One in-cgroup SPI owner matches its original PID/start identity.
These are software boundaries, not optical latency.

The original result retains `visible_review_pending`; the subsequent independent
**model visual review** is recorded separately. Both original JPEG hashes,
approved-app identities, LaunchServices brackets, speaking/Stop triggers and
reaped wrappers verify. The after-Stop image precedes ordinary legacy repaint.
The active frame shows a brightly illuminated near-white/violet-cast ring;
the after-Stop frame is visibly unilluminated. The base is visibly supported,
but cabinet edge/full clearances are incomplete. Exposure prevents exact color
acceptance; two frames do not establish optical timing, continuous choreography
or absence of intervening motion. Motors stay disabled. This visual review does
not change `human_listened=false` or the null acoustic score.

The full 180-second room WAV/ledger independently verifies 8,640,000 mono 48 kHz
float32 frames, 16,875 contiguous callbacks and zero clipping, nonfinite samples,
loss or recorded fault. Two 8.533/14.709-second local ASR windows corroborate the
complete question/reply text, spelling “Lamp” as “Lam” and expressing percentages
numerically. No extra words appear in those selected hypotheses; idle audio was
not transcribed. Raw audio and photos stay local.

## Recording, provenance and limits

The second attempt's original 180-second WAV and callback ledger were independently
checked: 8,640,000 mono 48 kHz float32 frames, 16,875 contiguous callbacks, no
clipping, nonfinite values, recorded backend fault or missing frames. Original
hashes and host coverage verify. Only room seconds 4.512–16.224 (11.712 seconds)
were cropped using actual player/observer CPAL receipt hints and transcribed
once with the cached `small.en` model, two CPU threads and network blocked.
The supported ASR environment was Python 3.12.9/NumPy 1.26.4/Torch 2.2.2;
numeric validation used Python 3.12.10/NumPy 2.2.3. No known-word prompt or new
model download was used. Mac host, CPAL and target clocks remain distinct;
crop selection is not an accepted acoustic boundary.

All three native archives and their 11 members each were hash-verified before private
extraction. All selected original inputs were rechecked unchanged. A healthy
metadata finish ACK does not turn `serve`'s `complete_evidence=false` into a raw
PCM certificate. The first two host failure paths omit the target-helper exit response;
independent native unit, process/resource and metadata evidence verifies their
cleanup. R3 retains helper exit 0. Seven is the total observed identity count in
each attempt, not a concurrency or worker-role claim.

[Numerical evidence and exact hashes](results.json) bind the three local evidence
roots and independent audits. Raw audio and credentials remain local.
`human_listened=false`; acoustic scores are null. Physical mute/reopen,
real-human addressing, natural duplex and before-output continuation remain
separate gates in the [attended-pilot runbook](../../attended-4ace-pilot.md).
Optional spoken failure feedback remains off without an approved production asset.
