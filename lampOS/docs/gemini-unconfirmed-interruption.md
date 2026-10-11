# A cut-off request the service never confirms

Scope: `crates/gemini`. This answers `MAIN-024`: what `BarrierPolicy::Require`
did in the physical run of 2026-10-11, why, what recovery is safe, and what
would stop the failure from being asked for. The earlier handoff listed this
as a known gap that only a live probe could close
([reliability](gemini-session-reliability.md#when-the-service-does-not-confirm)).
The device has now supplied the first sample.

No interface changed. One test helper and one probe argument were added.

## What the device recorded

`51bd815e9` on `a0ae`, one cached question, split in two by the endpoint
detector. Times are from the end of the first part.

| Time | Event |
|---|---|
| 0 ms | Request 1 ends. Its `activityEnd` is on the wire: the service owes it an answer |
| +241 ms | The second part is admitted as request 2. Nothing has arrived for request 1 |
| +245 ms | `activityStart` for request 2 is written, `waiting_for_barrier: true` |
| +528 ms | One input transcript, "Hola." It names no request |
| +1,499 ms | Request 2 ends locally |
| +2,265 ms | `BarrierTimeout`; the worker exits |

Between +245 ms and +2,265 ms the service sent no audio, no `interrupted` and
no completion.

## What `Require` did

1. Starting request 2 retired request 1. Request 1 was committed and
   unanswered, so its answer became the earlier response that has to settle.
   `activityStart` is the only interruption request there is with provider
   activity detection off, and it was sent.
2. The second part's audio went out at once. It is never held.
3. Request 2's `activityEnd` was **withheld**. Until it is sent the service
   cannot be answering request 2, so everything that arrives belongs to
   request 1.
4. For 2.0 s after the interruption request nothing of request 1 arrived. The
   connection was given up.

That is the barrier doing what it is for. On `51bd815e9` the worker had no
supervisor, so the process ended. Under the supervisor the same timeline is a
reconnect: the regression below.

## Why only a new connection is safe

A silent service and a withheld `activityEnd` are a deadlock. Neither side
will send anything again. Breaking it on the same connection means sending
the `activityEnd`, and after that:

- If the service dropped request 1 without saying so, it now owes **one**
  answer, and it is request 2's.
- If the service kept request 1 queued, it now owes **two**, and request 1's
  comes first.

Nothing on the wire tells these apart, so the first answer cannot be given to
request 2. Dropping it does not repair the connection either: having dropped
one answer, the transport still does not know whether another is owed, and
the next request would inherit the doubt. The number of answers outstanding
is unknown, and only a new socket sets it to zero.

So the correlation-safe recovery is the one the supervisor already performs:
replace the connection and report the request lost. I found nothing cheaper
that is safe, and I am not proposing `AssumeAfterQuiet`: it would have
answered this run, but only after its own two seconds of silence, and on the
assumption above.

## What it costs, and how often

In this run the person finished at +1,499 ms and the connection was given up
766 ms later. Setup took 1.94 s in the same run, so a replacement would have
been ready about 2.7 s after the person stopped, with the question gone.

This is not a corner. It needs an endpoint and then more speech before the
first audio of the answer, which the first Rust pilot put about one second
after the endpoint. A false endpoint inside a sentence does it, and so does a
person who pauses for 600 ms and goes on. One sample says the service stays
silent. If it always does, every such pause costs a connection.

## Proposed

1. **Keep `Require`, and let the loss be one question and one connection.**
   The supervisor reports `Unavailable(BarrierTimeout)`, then
   `TurnLost { stage: AwaitingResponse }` once for request 2, then `Ready`
   with the resumed context. The coordinator answers that loss with its
   turn-lost cue. Request 2 is reported at `AwaitingResponse` although its
   `activityEnd` never reached the service.
2. **Stop submitting a continuation as a new question.** In the run the
   second part was not a new question. A request that has produced no output
   has given the person nothing to react to, so more speech then is the same
   question going on. If admission says so, there is no second owner and
   nothing to attribute. I can add this to the transport on your word, and
   not before, because it is an interface:

   ```rust
   supervisor.try_continue_for_session(expected, request) -> Result<(), StartRefusal>
   ```

   - Accepted only if the request before it has ended its input and produced
     no output of any kind. Otherwise a typed refusal, and the caller starts
     an ordinary request with its barrier.
   - No barrier and no withheld `activityEnd`. `Event::BarrierAssumed` naming
     both requests is reported at once, so the trace shows the declaration
     and no new event is needed.
   - Output that arrives before the continued question ends is the answer to
     the truncated part. It is dropped, and from then on a barrier stands, as
     for any response that shows itself.
   - What still depends on the service: if it answers the truncated part
     late, after the continued question has ended, that answer is delivered
     and the real one arrives with no owner, is dropped and is counted. The
     owner is right either way. The answer may not be.
3. **Measure the service before choosing.** One probe, yours to run:

   ```sh
   service_probe /absolute/private/provider.json QUESTION.wav interrupt-before-output 241
   ```

   The number is the gap in milliseconds; the device's was 241. Two fields
   decide the matter. `service_confirmed_the_cut_off_request`: if true, the
   barrier is answered and this run was an exception. `output_after_the_answer`:
   the probe keeps listening for eight seconds after the answer; if true, the
   service answered twice and proposal 2 would deliver the wrong one. The
   probe runs under `AssumeAfterQuiet` with a five-second bound so that it can
   see what follows. It is the only place that policy is selected.

## Considered and not done

- **Not failing while the person is still speaking.** Nothing ambiguous can
  arrive before the `activityEnd`, so the decision could wait for it. That is
  safe, but it only helps a service that confirms more than two seconds late
  and before the person stops. A service that is already producing resets the
  bound today. With a silent service the outcome is the same loss, later.
- **Dropping the next answer and keeping the connection.** Unsafe, for the
  counting reason above.
- **Replaying the lost request on the replacement.** Safe, but it needs the
  transport to keep input audio and to waive the one-second freshness rule,
  and the answer would come four to five seconds after the question.
- **A shorter bound.** The same loss, sooner.

## Regressions

`crates/gemini/src/recovery/tests/barrier.rs`, paused clock, production
bounds, the device's own gaps (241 ms and 1,254 ms):

| Test | What it pins |
|---|---|
| `a_service_that_confirms_nothing_costs_the_resumed_question_and_one_connection` | Request 2 starts with `waiting_for_barrier`; its audio is sent and its `activityEnd` is not; the transcript settles nothing; `Unavailable(BarrierTimeout)` 2.00 s to 2.03 s after the interruption request; a start is refused `NotConnected` meanwhile; `TurnLost` once for request 2, nothing for request 1; `Ready` on a resumed connection; an answer the first connection sends afterwards reaches nobody; request 3 is committed at once and its answer is owned by connection 2, request 3 |
| `the_same_resumed_question_is_answered_when_the_service_confirms_within_the_bound` | The same timeline with `interrupted` and an idle completion 1.9 s in: the withheld `activityEnd` goes out, the answer is request 2's, the connection stays |

At the connection level the silent case was already covered by
`silent_service_after_an_interruption_fails_or_is_assumed_by_policy`,
`silent_service_ends_the_session_under_require_and_is_flagged_under_assume`
and `missing_old_barrier_fails_instead_of_assigning_new_audio`.

## Not established

- **What the service does.** One device run. Whether it ever confirms a
  request it has not begun to answer, and whether it ever answers twice, are
  the probe's two fields.
- **Anything on the cloud or a device by me.** I ran neither.
- **The resumed context.** Whether a resumed connection has heard the two
  parts depends on when the service issued its last handle.
