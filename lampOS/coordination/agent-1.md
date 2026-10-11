# Agent 1 status

Owner: Agent 1 (audio and Gemini). This file on `lamp-a1-continuation` is
the status of `MAIN-056`. Reference recovery (`MAIN-053`, `054`, `058`) is
on `lamp-a1-reference` at `f167b94a1` with its own status there. Duplex
(`MAIN-050`) is frozen on `lamp-a1-duplex` at `e2aa5df1c`.

- Updated: 2026-10-11
- **`MAIN-056` is started: one finding and a proposed contract. No code
  yet.** Branch from your `235f50297`.
- No cloud, no device. I read your probe's published result and your
  source audit; I ran nothing against the service.

## The assignment as it reached me

Through my terminal, not in Git: make committed-input continuation
reliable in `crates/gemini`, provider worker changes as a separate
proposal; keep `Require`; do not port V1's force-commit, do not treat
voice-activity notices as settlement, do not switch to
`AssumeAfterQuiet`; say so if the protocol cannot safely support it.

## Finding: moving the timer does not make this case succeed

From `docs/verification/strict-continuation-20261011/result.json`, times
since the first start was queued:

| At | What |
|---|---|
| 5.027 s | First `End` queued: request 1 committed |
| 5.330 s | Second `Start` queued. `waiting_for_barrier: true` |
| 5.602 s | The service's own `voice_activity` End, at the first input's last sample |
| 5.691 s | Its `voice_activity` Start |
| 7.331 s | `BarrierTimeout`. Nothing else arrived |

The two activity notices say the service took the first `activityEnd` and,
89 ms later, the second `activityStart`. It had 89 ms in which request 1
was committed and not yet interrupted, which is far less than it takes to
start an answer. In the 1.64 s that followed it sent no `interrupted`, no
completion and no output for request 1.

So in this run, **interrupting a response before it has produced anything
gets no terminal at all.** The barrier is waiting for something the
service gave no sign of sending.

Your audit's correction, which I agree with as far as it goes, is to stop
the two-second quiet bound running while the next person is still
speaking, and start the strict wait at their `End`. That stops the
session ending in the middle of an utterance. It does not make this case
succeed: at the second `End` the barrier would still be waiting for a
terminal, and two seconds later it would be `BarrierTimeout` all the
same, with the question lost. Unless a terminal arrives later than
1.64 s, which this run cannot say because it ended.

## What can be strict and still succeed

Only two things do not guess:

1. **Fail, at the right time and with evidence.** The timer change above,
   an absolute bound kept, and a bounded count of what was received and
   dropped while the barrier stood, so that an absent terminal can be told
   from a filtered one.
2. **Do not interrupt a response that has not started.** When the next
   request starts while the committed one has produced nothing, hold the
   next request's `activityStart` and keep its audio in a bounded buffer.
   When the earlier response shows its first output, send the
   `activityStart` then: an interruption of a response that is producing.
   When it shows an idle completion instead, it has settled by itself.
   Either way the buffered audio follows at once, in order, and the next
   `End` commits once the earlier response's terminal is in. If the
   earlier response shows nothing at all, that is the response timeout it
   would have met anyway.

The second rests on one premise I cannot test: **that the service does
send `interrupted` or an idle completion for a response interrupted while
it is producing.** Every ordinary barge-in so far that was then answered
suggests it does. No probe has recorded it.

It also has a cost to say plainly: the service does begin answering the
first, partial request, and that partial answer is in the model's context
when it answers the whole thought. Its audio is never played: the request
is retired locally the moment the next one starts, as now.

## For you

| Id | |
|---|---|
| `A1-021` | **One probe before I build the second option**: `interrupt-during-output` on the real service, which the probe tool already labels. Does a response interrupted after its first audio get `interrupted` and an idle completion, and how long after? If it does not, the second option is dead too and the protocol cannot support strict continuation at all |
| `A1-022` | **In the failed run, did anything arrive for request 1 after 7.331 s?** It could not: the session ended. A rerun with the quiet bound lifted for the probe only, watching for 8 s, would say whether a terminal comes late or never |

## Next, without waiting

The first option: the timer placement behind an explicit setting, the
absolute bound, the bounded evidence, and its regressions against the
in-memory service. Defaults unchanged.
