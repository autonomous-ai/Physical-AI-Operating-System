# Bounded reply-start experiment

This private integration candidate uses Agent1's unchanged `StartGate` and
`StartNeed` from `d64983e8a` and adapts coordinator proposal `b4782eda1` to the
Coordinated provider contract. It is a proposed queue policy, not a measured
physical improvement. No interruption/barrier, capture, microphone, image,
notice, noise suppression, or output-capacity policy changes with it.

The coordinator holds its first dispatchable reply block until the elapsed
time since the first audio reached its queue plus the duration of queued PCM
is at least 200 ms. A completed generation or terminal failure releases its
remaining PCM immediately. The gate adds no sleep or I/O: each ordinary
coordinator iteration still services privacy, provider control, capture,
admission, and output credits. The 30-second coordinator queue, one outstanding
speaker chunk, and 40 ms physical playback bound remain unchanged.

The existing last-block rule remains authoritative: an unfinished generation
retains at most 240 frames, and a prior final playback cursor must retire before
another occurrence starts. Those conditions are checked before recording a
gate opening. The unmodified proposal could record an opening on one held
sample even though no chunk could be sent; a focused regression reproduced it.
No padding is added to open the gate. Only the existing final partial block
receives padding. Empty completion emits no owned speech; actual all-zero model
PCM stays unscored audio, never proof of a substantive answer.

Matching source-session Started and the real local input endpoint are still
required before dispatch. Fresh output authority is checked after the gate.
Cancellation during the hold drops the original reply and its PCM. A new owner
gets a fresh gate; no old time or samples transfer. Once open, the gate remains
open for later generations of that same reply. Their accepted final cursors
and playback tokens retain the normal retirement protocol. A terminal provider
failure may finish playing prior PCM but remains `provider_failed`.

## Evidence boundaries

This is a **queued-frame heuristic**, not a guarantee of 200 ms of playable
coverage. The queue count includes the existing final 10 ms holdback; dispatch
cadence and actual arrival delays also matter. A 200 ms horizon cannot cover
every provider stall, and the gate never rebuffers a later gap. There is no
extra silent PCM attributed to a turn; the separate speaker clock continues
its already-authorized real zero writes when it has no speech.

Agent1's 15 recorded arrival schedules replay from eight startup-gap cases to
one, with a modeled mean 62 ms later first speech and maximum 160 ms later
start. These are counterfactual scheduling results, not acoustic observations.
The replay assumes completion at the last retained arrival, uses a fixed 3 ms
enqueue allowance for most rows, and does not carry cancellation/completion
timestamps or prove full-answer delivery. The horizon was selected from this
same small cohort; it has no independent holdout qualification. A newer
reported 260 ms startup gap requires its exact schedule and is not silently
included as a success.

A separate held-out schedule from current code `02148ed4d` includes all 94
enqueue events, 89,760 PCM frames, and the actual idle-completion timestamp,
instead of assuming completion at the final audio arrival. Hardware accepted
two zero runs of 80 ms and 140 ms. Across twenty 10 ms clock phases, a finite
scheduling model predicts two gaps totaling 220–230 ms without the gate, and
one gap totaling 120–130 ms with the unchanged 200 ms horizon. First speech
moves 100 ms later; modeled completion does not become earlier. The later gap
remains. This tests a new schedule without choosing a new horizon.

The model is not an exact hardware replay: at the recorded first-write phase,
its ungated gaps are 90 ms and 130 ms, rather than the actual 80 ms and 140 ms.
It uses real queue arrival and completion timestamps with synthetic fixed
speaker slots, and does not simulate IPC, ALSA, clock jitter or changed cloud
feedback. This result supports a bounded experiment, not a physical latency
or smoothness claim.

`speaker_first_write` remains the actual worker's accepted-PCM receipt. Its
`start_gate` object separately records queue-condition opening time, age since
first queue audio, queued frames, included final holdback, and the complete
flag. `added_latency_us` is null: queue age can include time with no dispatchable
PCM, local input still open, or scheduler delay, so it is not a causal delay
measurement. The object is scoped to the first dispatchable chunk of the
reply; later playback occurrences do not pay that hold again.

Normal and cancelled `turn_finished` records include `start_deficit_us` and
`start_horizon_needed_us`, explicitly scoped to all retained reply arrivals,
including later generations and excluding local handoff. Failed/cancelled
streams are marked censored. These statistics diagnose arrival schedules;
they do not prove that a configured horizon eliminates a gap or that a partial
reply is a complete answer.

## Qualification required

Focused tests use the actual coordinator dispatch, real Controller authority,
and private Unix channels for held PCM, short/failing completion, cancellation,
stale ownership/privacy, matching Started/endpoint requirements, priority
provider-control progress, and later-generation retirement. The unchanged
Agent1 library/replay tests cover the arithmetic and stored schedules.

Before selecting the policy, replay complete new schedules with their actual
enqueue and terminal boundaries, then run matched physical trials. Keep the
speech-end to first substantive audible word metric separate from provider
first-audio, gate opening, first accepted PCM, inserted zero duration, and
completion. Count failures and censored replies; compare smoothness and extra
startup delay together. Neither this private candidate nor these host tests
qualify the physical result.

## Physical experiment checkpoint

This branch preserves the reviewed seven-file candidate for repeatable experiments;
it is not the selected policy on `lamp-v2-chat`. Runtime source matches the private
ARM experiment binary `d06e795176e49615b910875bc7a67e0a622dc021fa729fb2b0d858dc20103ad7`.
Only this documentation was extended after the frozen build.

Host formatting, strict audio/live all-target Clippy, and all 484 audio/live tests
passed; two longer tests were ignored. Source-only ARM release/strict Clippy
passed and 11 release QEMU lifecycle tests passed. These are software checks.

The first three planned physical attempts are retained in
`lamp-rust-playout-evidence-20261011T033405Z`. Two prompts played. Their six
resulting answer prefixes/replies had zero recorded speech-gap frames, but four
prefixes were cancelled by extra input admissions and neither conversation was
successful. The third prompt was withheld by its original-readiness deadline.
Both Lamps were restored. Cancelled prefixes remain censored observations; these
results do not prove a gap-free policy, improved acoustic latency, or reliable
interruption. The interruption/echo diagnosis is separate from start buffering.

The prior coordinated baseline (`02148ed4d`, one completed native greeting) had
80 ms and 140 ms accepted-render gaps. Different provider delivery schedules and
the censored candidate replies prevent treating that as a controlled speedup.
