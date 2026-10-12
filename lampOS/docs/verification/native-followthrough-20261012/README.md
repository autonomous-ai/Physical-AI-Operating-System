# Native follow-through: retained failures, 12 October 2026

Two fresh device attempts used the final `b3186e7e` runtime. Neither produced a
completed requested conversation. Both restored stationary muted legacy with
verified administrative receipts. They are failed attempts, not added to the
successful conversation count. See [the exact results](results.json).

## Head and voice attempt

The motor worker refused inherited CAP_SYS_ADMIN before opening the serial
port. The systemd launcher had NoNewPrivileges but did not remove that
capability. No head action or position write occurred. Voice reached actual
readiness after the optional motor failure. The worker supplied no structured
release evidence, so final runtime cleanup remained unconfirmed; process reap
and outer restoration are separate evidence.

Main also selected the permission-approved recorder executable for playback;
that older build implements no `play` command. It refused before source output.
The existing qualified player is a different pinned binary. One unplanned
input was admitted and cancelled at the finite deadline. The retained 90-second
room WAV is valid, but full attempt coverage has not been established.

The next head trial needs an explicit capability restriction and verification,
a fresh namespace/inspection/plan, and the correct cached player. It must not
weaken serial exclusivity or relabel the original cleanup failure.

## Eight-input serve attempt

The first prepared player submitted zero source samples: a startup
`input_availability` cue was refused as stale including clock uncertainty.
The runner stopped the serve child before its listening-ready event, then
retained all eight withheld steps. The actual cue arrived at the Lamp relay
214 microseconds after emission. The final best clock mapping would put the
host receipt about 21.5 ms after the event, but the mapping used at the refusal
was not retained. The final mapping cannot establish what the earlier decision
knew. A2 owns the startup preparation/clock-evidence correction; no freshness
limit is widened.

The complete 330-second room recording is valid: 15,840,000 frames at 48 kHz,
zero dropped frames, zero timestamp discontinuities and zero backend xruns.
It contains no planned spoken stimulus. Raw recordings stay local; the file
hashes and failed test identities are retained for repeatable analysis.

## Test policy

Use the existing cached WAVs without synthesis. Preserve every attempted
launch, delivered stimulus, generated reply, room recording and runtime trace.
Distinguish runner failures from runtime failures and successful conversations.
Use the already-qualified ordinary-conversation controller while the serve
runner is repaired. Loudspeaker tests remain separate from direct-human tests.
No new V1 benchmark is requested or run.

A durable local copy is retained under `.cache/physical-audio/20261012`: three
run archives and fourteen cached stimulus files, with SHA-256 manifests. Audio
uses hard links, so temporary source-name removal will not delete the cache and
no extra audio storage is allocated. This cache is Git-ignored and was not
uploaded. The earlier invalid recording remains invalid.
