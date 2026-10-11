# Discarding an accepted, unsubmitted utterance

`InputAdmission::discard_stream_until_end(at_us)` is a local sequencing helper
for a coordinator that intentionally withholds an already accepted candidate
from the provider. It does not change cloud availability, controller ownership,
privacy, output permits, or the detector. This helper alone does not enable
production outage recovery or provide audible failure delivery.

Call it after `decide` returns `Decision::Accepted` when that utterance will not
be submitted. Drop the returned `Accepted.audio` in the caller: acceptance has
already transferred that bounded prefix out of `InputAdmission`. Do not reset
`TurnDetector` or `InputAdmission` merely because the connection recovers.

| Existing state | Result |
|---|---|
| Streaming accepted input | Discard subsequent blocks through the detector's actual `Activity::End`. |
| Draining | Idempotent; preserve the same utterance and its checks. |
| Idle, including an already ended accepted input | No-op. |
| Pending classifier decision | Error; retain the candidate and its original deadline so the caller can still explicitly accept or reject it. |

The accepted-stream drain retains only fixed capture lineage, sequence and read
timestamp metadata. It retains no PCM, grows no queue, performs no I/O, and does
not extend a lease or decision deadline. Continued audio and the endpoint
produce `Step::None`; after that endpoint, a fresh detector start may create a
new candidate with a new identity. A delayed decision for the discarded
candidate cannot revive it or decide the next candidate.

Hard local readiness, microphone privacy generation, capture worker/epoch,
monotonic DSP epoch, consecutive sequence, capture age and read-time continuity
are checked exactly as for accepted streaming input. Planned forward DSP-epoch
changes remain allowed. Invalid continuity or authority resets admission and
returns an error; a clock regression also returns an error and resets admission
without reusing candidate identities. The coordinator must apply the existing
hard-fault/privacy shutdown or reset policy to the detector as well; an error
does not authorize resuming the same discarded audio under new authority.

Existing policy-rejection draining remains unchanged. The detector's existing
120-second utterance bound remains in force; this helper introduces no timer
that can rearm midway through a long utterance.

The transition is fixed work with no allocation or blocking operation, targeting
less than 1 ms on the local interaction path. Deterministic tests exercise the
real detector with synthetic probabilities across an outage/recovery boundary,
then its actual endpoint and the next fresh opening. They also cover the prior
stream/reset behavior, pending and already-ended input, idempotence, privacy and
capture changes, discontinuity, clock regression and stale decisions. These are
software contract tests, not measured target-hardware latency, speech quality,
interruption accuracy, or end-to-end recovery qualification.
