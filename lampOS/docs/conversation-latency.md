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

## Current evidence, 2026-10-11

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
