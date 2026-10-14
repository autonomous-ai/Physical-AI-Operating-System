# Conversation latency target

The goal is connected desk conversation: follow-ups, corrections, short replies,
topic changes, and thoughtful pauses. Fast unrelated question/answer pairs alone
do not establish that experience.

## Human reference and Lamp targets

Human speaker transitions commonly take about 200 ms. This is a conversational
reference, not a maximum for every question: planning a difficult answer can take
longer. The [turn-taking review](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2015.00731/full)
describes typical gaps of 100–300 ms. The
[ten-language study](https://pmc.ncbi.nlm.nih.gov/articles/PMC2705608/)
found a 208 ms overall mean, but its response onset could be vocal or gestural;
do not treat that entire dataset as a speech-only robot benchmark.

Lamp's engineering targets for ordinary live voice replies are:

- Typical audible reply, p50: 300–500 ms after the person finishes speaking.
- Tail, p95: at most 1,000 ms in the declared ordinary-conversation cohort.
- The previous 2,000 ms p50 goal is an intermediate milestone, not the final
  human-like target. These stricter targets are not yet demonstrated feasible
  with the current cloud model, endpoint policy and hardware path.

The boundary is the last relevant input speech in the room to the first relevant
audible reply word in that same recording. A stock acknowledgment, an introduction
before a story, light or head motion cannot substitute for the requested answer.
Measure appropriate visual acknowledgment separately. Never cut off a person or
hide failures to obtain a faster distribution.

Keep synthetic loudspeaker, recorded-human loudspeaker, and direct-human cohorts
separate. Retain every attempted turn and report misses, cutoffs, duplicates,
unwanted responses and gaps alongside latency and sample counts. Acoustic boundary
proposals remain unscored until verified. Provider/log/ALSA receipts are separate
software measurements and must not be relabeled as first audible words.

## Where it stands on lamp-4ace, 2026-10-14, and a shorter wait that was tried

Played questions on the always-on voice that night (`--end-silence-ms
600`, `--playout-start-ms 350`, `gemini-3.8-live` through the relay): the
first words of an answer arrive 1.4 to 2.1 s after the played line ends,
which is 0.6 s of waiting to be sure the person has stopped and 0.9 to
1.5 s more. The first sound was written 0.99 to 1.67 s after the
end-of-speech decision in the night's checks (34 answers in five runs).
Loudspeaker lines and the service's own records, not a first audible word.

Over every answer in the lamp's records, counting for each the wait that
was in force for it (600 ms for 900 of them), from the person stopping to
the first sound written to the speaker:

| | Answers | Half within | Nine in ten within |
|---|---|---|---|
| 2026-10-12 | 156 | 2.19 s | 2.90 s |
| 2026-10-13, a working day in the office | 632 | 1.98 s | 2.93 s |
| 2026-10-14, played into a quiet room at night | 246 | 1.83 s | 2.39 s |
| All | 1,034 | 1.98 s | 2.81 s |

The quickest of them took 1.33 s. Against the targets above (a typical
reply 300 to 500 ms after the person finishes), the lamp is four to six
times too slow, and no setting of its own closes that: the service's
first sign of an answer came 1.15 s after the end of speech was decided,
at the median of 1,064 answers.

The 0.6 s is the one part of it that is the lamp's own to shorten, so
400 ms was tried, once, on the pause test (`pauses.sh`: two plain
questions and three thoughts with a silence of 450, 650 and 900 ms in
them, each beginning "Lamp," or "Hey Lamp,").

| | 600 ms, two runs | 400 ms, one run |
|---|---|---|
| The name and the comma after it | Part of the sentence | Ended as an utterance of its own in three of the five lines: thrown out as a sound twice, replaced by what followed once |
| The thought with a 450 ms silence | Heard whole both times (and declined both times, for another reason: below) | Heard in three pieces after the name; the answer had no words in it |
| The thoughts with 650 and 900 ms | Each in two pieces, continued, and answered in three of four | Each in two or three pieces, continued, and answered; one answer played with gaps |
| First sound after the decision | 1.02 to 2.15 s (six answers) | 0.98, 0.98, 0.99 and 2.53 s |

A synthetic voice leaves more than 400 ms after "Lamp," so at 400 ms
three of five called sentences were cut in two after the name. The wait
is back at 600 ms. Two tenths of a second is not worth a lamp that takes the
name and the question apart; what would be worth it is ending the wait
early only when the sentence is plainly finished, which the end-of-turn
model does and is too slow at on this board
([endpoint-pilot.md](endpoint-pilot.md)).

The same test showed something else. Of eight lines that began with a
bare "Lamp," over the two runs at 600 ms, four got no answer: the service
wrote them without the name, the model took "we could go to the beach
this weekend, what do you think?" for other people's talk, and the part
that listens for the name heard it in none of the eight. It hears "Hey
Lamp" and "Lamp ơi" ([name.md](name.md)), and a bare "Lamp" is not one
of them.

## The end sent early (`--early-end-ms`): built, not yet on the lamp

Task 19 of [work-split.md](work-split.md), by the second builder,
2026-10-14 ([faster-answers.md](faster-answers.md), way 2, "Built: the end
sent early"). Off unless given. With `--early-end-ms 200` the request is
ended to the service after 200 ms of quiet, and the answer is held until
the lamp's own end of speech (the 600 ms, or the end-of-turn model's
verdict) confirms that the person has finished. If they speak again first,
the request is given up as a continued utterance is, and nothing of its
answer is played. Since the probe of `second-P1`, what they go on to say
carries a note that it is the rest of the request cut off, to be answered
as one ([early-end-taken-back.md](early-end-taken-back.md)); played on
lamp-4ace without that note, at 13:45, the early end made the pause scenes
worse ([faster-answers.md](faster-answers.md)).

The path from the person's last sound to the first block handed to the
speaker, without and with it:

| | Without (today) | With `--early-end-ms 200` |
|---|---|---|
| The end reaches the service | 600 ms after the last sound | 200 ms after |
| The lamp is sure the person has finished | 600 ms | 600 ms, unchanged |
| First block to the speaker | 600 ms + service + start hold | the later of 600 ms and 200 ms + service + start hold |

Measured where the clocks are injected (`coordinator::early_end::tests`:
the real detector at 600 ms, the real start gate at 350 ms, a service
whose sound comes as fast as it plays, so the start hold is 160 ms): at
the lamp's median service time of 1.15 s, 1,910 ms without and 1,510 ms
with. An answer ready before the lamp is sure begins at the 600 ms, never
before. Through the coordinator's own loop, the real provider worker and
session, and a scripted service answering 900 ms after each end, on the
second builder's laptop, once each: 1,670 ms without and 1,269 ms with;
the end left 604 and 203 ms after the last sound. Neither is a lamp, the
real service or a first audible word.

So with this service the lamp's half-within would go from about 1.98 s to
about 1.58 s, if the service's own time does not change with requests
given up and continued. That is an estimate from medians, not a
measurement.

Not shown: anything on lamp-4ace; the real service's handling of a
request given up a third of a second after its end; how often people
pause 200 to 600 ms and go on, each of which is now a request given up;
what the model makes of a thought sent in two pieces with a stopped
answer between them.

## Latest ordinary conversation, 2026-10-12

The [newest ordinary attempt](verification/pilot-readiness-20261012/README.md)
completed two replies, then admitted an unplanned input before the third prompt.
The remaining plan was stopped/withheld. The two endpoint-to-first-ALSA-write
samples are 1.246 and 1.305 s (median 1.275 s); adding the configured 1.2 s wait
estimates 2.475 s. Room audio is saved, but acoustic latency is unscored. This is
an incomplete cohort, not a causal improvement or a pilot acceptance pass.

The preceding [real-Lamp run](verification/ordinary-b318-20261012/README.md) completed
six replies; the seventh was cancelled by an unplanned detected input and the
eighth stimulus was withheld. Endpoint to first ALSA write was 1.424 s median
(range 1.266–1.505 s). Adding the configured 1.2 s silence wait yields an estimated
2.624 s total; it is not an acoustic measurement. The 180-second room recording
is valid and retained locally. This failed/incomplete run establishes no causal
improvement over earlier cohorts and does not meet the natural-conversation goal.

## Earlier evidence, 2026-10-11

The six-turn 4ace run `lamp-4ace-varied-12io918n` used source `6e6c58b42`,
`v1-baseline`, a 1,200 ms local silence window, and a 200 ms playout horizon.
All six turns completed with no extra admission or cancellation. Four speech
starvation gaps and a separate story transition remain defects.

Automated room estimates are 2.7–3.3 s for five reply beginnings and 5.1 s for
the sixth. The story itself begins around 6.9 s after the question; its earlier
introductory phrase does not satisfy the request. These are unaccepted proposals,
not a qualified acoustic p50/p95. The six questions were varied but independent.

The local endpoint trace includes the fixed 1,200 ms silence wait. Endpoint to
first ALSA acceptance for the six turns is 1.656733, 1.504063, 1.846100, 1.634824,
3.792812 and 1.538589 seconds (median 1.6457785). This decomposition explains why
endpointing alone cannot reach the final target: provider/output startup also
needs substantial improvement. No stage may be omitted from the acoustic total.

The later [connected eight-turn runs](connected-conversation-20261011.md) each
completed all eight prompts, including follow-ups, a correction and a topic
change. Extended/LOW and plain Live had endpoint-to-first-ALSA medians of 1.367 s
and 1.292 s, respectively; both still follow the 1.2-second silence wait. Those
software timings exclude acoustic boundaries and are not a causal model speedup.
Extended added two introductions with substantive reply writes at 5.342 s and
5.128 s after endpoint. Plain avoided these in this run. Three brief supply gaps
remain across the sixteen turns. Keep the physical continuity A/B separate from
this provider comparison; natural interruption is still a different workstream.

The subsequent same-binary playback comparison completed another sixteen turns.
The 200 ms coverage and continuity settings both had zero accepted-zero speech
gaps, with endpoint-to-first-ALSA medians of 1.475 s and 1.647 s (eight turns each).
This pair demonstrates no gap-reduction benefit or causal speedup. Keep it
separate from the older model comparison and from acoustic first-word estimates.
The fixed 1.2-second endpoint wait still precedes both distributions.

Agent 1's opt-in continuity policy eliminates starvation in its 23-reply recorded
supply replay, while adding 137 ms average first-start delay. It is not a latency
win. Agent 2's first endpointer has seven new synthetic false splits and remains
experimental. Human whole-turn policy replay is still required; clip-end
classification accuracy does not establish safe conversational turn-taking.
