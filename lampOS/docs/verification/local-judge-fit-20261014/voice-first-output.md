# Proposed Voice first-output capture for task 17

Documentation proposal for follow-up #700, checked against source at
`ce7abae79c06c53c8ea1dfb03c42622c41f8f135` after rebasing onto `de69bac33`.
The existing comparison in
[`policy/baseline.rs`](../../../crates/policy/src/policy/baseline.rs) identifies
its mirrored Voice rule as `e07449e13:crates/voice/src/attention.rs`.
The seven-field snapshot already exists. The capture events, provenance
sidecar and transport mapping proposed below do not. This document adds no
Voice code, switch, command, bus topic or device measurement.

## Why the existing results need different denominators

First's [played-scene record](../policy-scenes-lamp-4ace-20261014/README.md)
reports 21 permissions on 28 held-out Policy rows, including 14 not-for-lamp
rows, under `same_snapshot`. It separately reports no audible intrusion into
63 talk lines in seven two-people scenes, mostly because the model gave no
answer. It explicitly records no `voice_first_output` snapshots and no ASR
for the nine talk lines in two rounds. These are First's reported lamp
results, not measurements repeated for this document.

In [`provider_worker.rs`](../../../crates/voice/src/provider_worker.rs), the
attention gate is evaluated only when a provider event proposes Audio,
OutputTranscript or an actionable ToolCall. The reporting calls
`heard_my_name` and `not_for_me` do not trigger it. An Unanswered request
with no candidate output has no first-output gate decision. Even Play is
only permission: model decline or later cancellation, ownership, transport
and speaker checks can prevent audio. A transcript or tool call need not
produce audio at all.

The counts therefore do not show attention holding 63 proposals or causing
14 audible intrusions. The source explains how those counts can coexist;
the public records cannot establish the per-turn contribution of provider
silence, the gate and later output handling.

## Existing snapshot: capture the arguments actually used

The fitting row's optional `baseline` has exactly these seven required
members. This is an authored schema example, not a captured request:

```json
{"kind":"voice_first_output","room":"busy","own_turn":false,"going_on":false,"named_sure":false,"text":"And tomorrow?","facing":null}
```

Capture once, for the call to
[`Attention::first_output_busy`](../../../crates/voice/src/attention.rs)
that returns `Some(verdict)`, before forwarding or executing that proposed
output. A repeated call or a request Attention is not following returns
`None`; neither is a new evaluation. Copy the actual values used by that
evaluation, including its own-turn atomic load, rather than sampling them
again after it returns.

| Member | Required value at that evaluation |
|---|---|
| `kind` | `voice_first_output`; never relabel a Policy snapshot this way. |
| `room` | `quiet`, `busy` or `unknown`, exactly as returned by Voice's local `attention_now`, not a later `room/state` message. |
| `own_turn` | The boolean result of `OWN_REQUEST.load(...) == request` inside this evaluation. |
| `going_on` | The boolean actually returned by `Thread::going_on` to `attention_now`. |
| `named_sure` | Attention's retained per-request boolean, not a score reconstructed from the newest name message. |
| `text` | Attention's actual retained transcript prefix, valid UTF-8 and at most 2,048 bytes. Preserve its concatenation, including spaces as received. |
| `facing` | The actual `turned: Option<bool>` argument: JSON true, false or null. |

The comparison represents the binary `strict = true` rule. Record effective
attention-enabled and strict settings separately; an environment string
alone does not prove the worker's effective settings. Ordinary attention
mode also depends on request freshness and the ordinary name latch, which
this schema does not contain. Attention disabled means no gate evaluation,
not a captured Play verdict. Only enabled strict evaluations are eligible
for this baseline comparison; other modes belong in coverage accounting
with no comparable baseline.

The actual `facing` argument has these meanings, from the callsite and
[`bus_out.rs`](../../../crates/voice/src/bus_out.rs):

- Voice queries eyes only for a busy room under the strict rule. Quiet and
  unknown rooms pass None without querying eyes.
- A missing bus, absent result, read/parse error or expired message can also
  produce None. Record null only if that argument was observed; an
  unrecorded argument makes the whole baseline unavailable.
- A valid fresh message produces true if any face has `facing: true`, with
  no size threshold. Otherwise it produces false, including an empty face
  list or all unknown poses. False does not identify a turned-away speaker.

Room, Thread, eyes and the own-turn atomic are sampled sequentially. A
snapshot preserves those values, not simultaneous sensor observations.
Record a sampling start/end bracket and the evaluation time. Wall time
when a writer eventually flushes the record is not the decision time.

## Why Policy's snapshot cannot substitute for it

[`policy.rs`](../../../crates/policy/src/policy.rs) freezes speaking and
last-said context at the first ASR receipt; it samples room when the
utterance is assembled. Policy uses a 20-second conversation definition,
completed text, and a face summary restricted to faces at least 15% of the
image height. Its face history is aged by receipt time as the utterance ends.
[`policy/judges.rs`](../../../crates/policy/src/policy/judges.rs) writes these
inputs as `same_snapshot`, sets own_turn false, and thresholds its collected
name score to supply named_sure.

Voice instead uses its own room and the
[`Thread`](../../../crates/voice/src/thread.rs) 45-second conversation
result when output arrives. Thread's exchange age starts at its first
retained lamp transcript piece, not speaker playback. Voice uses the latest
eyes message's expiry, including the bus's 50 ms grace, and any facing face.
Attention keeps up to 2,048 bytes of text accumulated since InputStarted;
a name in the completed transcript may not have arrived before the gate.

Its sure-name bit retains eligible scores >=0.999 across later weaker
hearings. The name window includes a 1.5-second pre-roll and, once known,
excludes names beginning after the input end. Name polling and provider
event order affect which observations have reached that latch. Capture the
actual bit; the last score alone cannot reconstruct it.

## Proposed sidecar: identities, decisions and missing output

Keep the existing snapshot unchanged. Both it and
[`fit::Row`](../../../crates/policy/src/policy/fit.rs) reject unknown fields.
Additional provenance belongs in a separately versioned JSONL sidecar, joined
to the fitting row by its `id`. The following event names and fields are a
proposed contract; no current Voice or Policy command emits them.

Every event would carry `schema: 1`, an `event` name, a machine identifier,
OS boot identifier, a fresh `voice_run` identifier for the worker lifetime,
the exact Voice build revision, and a worker-local increasing `event_seq`.
Request events also carry the provider `session` and `request`. Request IDs
are only scoped within a session; do not join on request alone. Timestamps
are unsigned microseconds from local `CLOCK_MONOTONIC`, explicitly named
with `_us`, with wall-clock milliseconds in `at_unix_ms` for inspection.
Do not silently convert an unavailable observation to zero or false.

| Proposed event | Required evidence |
|---|---|
| `request_started` | The admitted provider session/request and InputStarted observation time; identify lamp-initiated turns separately from heard input. |
| `voice_first_output` | For an enabled strict evaluation: sampling start/end and gate evaluation times; triggering event kind (`audio`, `output_transcript`, `tool_call`); effective attention/strict flags; the exact seven-field `baseline`; actual `permission` (`play`/`hold`) and gate reason. Include the stored input-began and nullable known input-end times, labelled as processing boundaries. |
| `request_outcome` | Reference the first-output event if present; record whether a candidate was seen, whether the gate evaluated it, the provider terminal outcome and whether any PCM was forwarded. Record speaker acknowledgement or an independent audible observation separately, with its source and time; unavailable downstream evidence is unknown. |
| `capture_end` | Capture interval and admitted/written/dropped/failed/pending record counts. A missing end record leaves completeness unknown. |

An outcome distinguishes no candidate, explicit model decline, cancellation,
failure and supersession from a gate Hold. With a candidate but no recorded
gate result, distinguish disabled attention, an untracked request and missing
capture where known. A no-candidate request has no baseline object. Count
it in scene/request coverage, not as Hold in the paired gate table. Never
infer no output merely because a logger has no first-output record.

For diagnostic reconstruction, attach the exact transcript event identities
that contributed to the prefix; name message identity, receipt time and
derived name-began time; eyes message identity and expiry at read time; and
the conversation value with a qualifying exchange's age when available.
The eyes status should distinguish `not_queried`, `missing`, `read_error`,
`parse_error`, `expired` and `fresh`. Those statuses explain the observed
argument; they are proposed metadata, not additional baseline inputs.
Optional history may be unavailable without invalidating a directly captured
baseline, but missing required join identities prevents an exact pair.

Any implementation needs an off-unless-set measurement switch, bounded
nonblocking admission and a background writer. A full queue, oversized
record or exhausted file limit must record loss without delaying or changing
the gate. Publish the configured byte/count/time bounds and loss counters
with each capture. Never silently trim an identity list and call its join
complete. Keep raw transcript-bearing logs private; use only authorized
staged speech and publish reviewed, sanitized evidence. The existing dataset
withholds raw judge logs because they may contain bystander speech.

## Proposed join and the limits of current records

Each accepted sidecar join would contain the fitting `row_id`, Policy
`run` and `number`, source-record/file SHA-256, the Voice first-output event key,
independent scene/line label identity, and an explicit association method.
It would retain all ASR piece identities consumed by the Policy utterance,
first/last receipt times, gather eligibility time and choice time. Preserve
multiple possible matches as ambiguous instead of selecting one silently.

The existing [`bus header`](../../../crates/bus/src/wire.rs) identifies a
message with `(topic, source, boot, seq)`. Retain publisher `at_us`,
`at_unix_ms`, subscriber receipt time, gap and publisher-restarted markers
separately. Header `boot` is a publisher-instance nonce, not an OS boot ID.
[`lamp_ipc::monotonic_us`](../../../crates/ipc/src/lib.rs) uses host-wide
`CLOCK_MONOTONIC`: local processes using it share a comparable clock within
one OS boot. Times from different machines or boots require explicit
alignment. Record the scene player's clock, alignment samples and uncertainty;
wall-clock changes must not be mistaken for speech or delivery latency.

Today's path lacks the exact join. The provider's
[`UncorrelatedInputTranscript`](../../../crates/gemini/src/lib.rs) has no
reliable service turn ordering. Voice adds it to Attention's current buffer,
then forwards `ProviderOutput::Transcript` with request None and no source
event ID/timestamp. A large event can be split into 3,500-byte IPC pieces.
The coordinator publishes `voice/heard` with request null. Policy's `Ear`
parses Speech and drops its bus header. A choice record retains run and
number but no source ASR envelope IDs or exact receipt times.

The minimal transport extension for an exact processing join is a stable
provider transcript-event ID plus split-piece ordinal, mapped through the
coordinator to each bus publication, and retained with the consumed bus
envelope IDs by Policy. The gate receipt identifies which source events
contributed to its prefix. This establishes which text observations the
programs used; it does not establish who spoke or invent provider-certified
turn ownership. Any request active at transcript receipt must be labelled
as processing context rather than acoustic lineage.

Without that extension, a staged sequence and time/text matching can support
only an explicitly heuristic association, with ambiguous rows excluded.
The current dataset's [`label.py`](../policy-scenes-lamp-4ace-20261014/label.py)
matches choice wall time, converted by one supplied offset, to the most
similar scripted line begun in the preceding 20 seconds (similarity >=0.6).
Its fitting rows omit run, exact times, play timestamp and similarity; their
IDs embed scene and choice number. Keep those discarded association facts
in a future sidecar. They cannot be reconstructed as exact Voice joins from
the existing fitting rows.

## What is not shown

No proposed event, sidecar writer or transport extension has been implemented
or measured by this document. The current public dataset cannot supply its
missing first-output snapshots retrospectively. Source inspection establishes
the gate's inputs and the evidence gaps, not lamp accuracy, an exact paired
comparison, capture overhead or end-to-end response latency.

Future reports must keep `same_snapshot` and `voice_first_output` separate,
state the paired eligible denominator and exclusions, and separately account
for played lines, admitted requests, ASR, Policy choices, provider proposals,
gate evaluations, forwarded audio and audible responses. Those stages need
not have one-to-one counts. Independent addressee labels and held-out scenes
remain necessary; a facing face, a matching timestamp or a Play verdict does
not establish that the lamp was addressed or that it spoke.
