# Provider failure contract

MAIN-013, resent as MAIN-016. This describes the integrated coordinated
worker/consumer contract. Device deployment remains separate qualification.
Agent 2 must preserve failure evidence without inventing successful submission
or renewed readiness.

A normal failed completion trace has top-level turn and generation:

```json
{"kind":"turn_finished","turn":7,"generation":9,"at_us":12000000,"outcome":"provider_failed","playback_gaps":0,"provider_session":3,"provider_outcome":{"kind":"failed","stage":"input_open","reason":{"category":"transport"},"local_not_submitted":false}}
```

Preserve the actual `provider_outcome` object. The example stage/reason is not
a required value. Stages serialize as `not_delivered`, `input_open`,
`awaiting_response`, or `responding`; the reason is tagged by `category`.
A `turn_finished` with an `owner` object is the existing cancellation path,
not this normal completion shape.

The scheduling cue uses existing `TurnCompleted`:

```json
{"schema":1,"sequence":12,"kind":"turn_completed","boot":[1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"turn":7,"generation":9,"capture_epoch":2,"reference_epoch_context":4,"event_us":12000000,"sent_us":12000001,"expires_us":12100000,"outcome":"provider_failed","playback_gaps":0}
```

Boot uses the existing 16-byte nonzero `BootId`; generation is positive and
`CUE_FRESH_US` is 100000. Outcome and gap count are required. These timestamps
only illustrate fields. A failed terminal never enters the successful-answer
numerator, including after partial audio. Preserve both latency evidence and
the provider failure cause; a late response must not erase its failure cause.

Abandoned input has this **unowned trace only**, with no scheduling cue:

```json
{"kind":"input_discarded_endpoint","at_us":11900000,"last_block_host_read_us":11899990}
```

Do not invent a turn or generation from this trace. This path does not produce
an ordinary `local_endpoint`. The coordinator completes its failed turn after
local input actually ends, provider idle has a known outcome, PCM drains, and
no speaker write/playback remains pending.

Only the initial `listening_ready` cue exists today. There is no authoritative
re-ready cue after an outage. A failed terminal cannot authorize another
stimulus. Latch unavailability for the attempt, checking during a trigger delay
and immediately before dispatch. Withhold the prompt and report why; retain any
partial answer recording and the exact provider metadata.
