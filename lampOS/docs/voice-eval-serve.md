# Serve-session conversation runner

Status 2026-10-11 (Agent 2): implemented and tested offline only. The runner
has driven a fake `lamp-live serve` through the real `lamp-session` relay on one
Mac and a Linux VM. It has **not** run against a Lamp, a real provider or the
iMac speakers. Nothing below is an acoustic, latency or quality result.

One attempt is one supervised `lamp-live serve` runtime and one provider
session. It carries a whole pre-registered conversation of up to 20
context-linked turns. Every stimulus is cached speech: the runner never renders
audio and never replays a prompt. Every attempt, including a failed or
withheld one, stays in the ledger.

## Running it

Physical (operator-authorized; same readiness checks as `physical-run`): put
`"serve_config": "/private/provider.json"` in the physical config in place of
`provider_config` or the fixture pair. Serve requires loudspeaker replay
through the prepared player and refuses raw audio diagnostics.

```sh
cargo run --release -p lamp-voice-eval -- physical-run --config FILE --plan fixtures/voice-eval-serve-v1.json \
  --cache .cache/acoustic --manifest M... --out NEW_DIR --scenario serve-conversation [--execute]
```

Runtime policy is forwarded typed and validated. The fields are
`turn_taking`, `reference_clock`, `reference_recovery`, `playout_start_ms` and
`provider_continuation`, in the config or as the same `lamp-session` flags.

- Each value is parsed with lamp-live's own types.
- The whole set is checked with lamp-live's own `DirectedOptions` rules, on the
  Mac and again in the relay before any child starts. For example, bounded
  recovery without a masking policy is refused.
- An omitted field passes nothing, so the default child argv is unchanged.
  Fixture runs refuse these fields.

`fixtures/voice-eval-serve-pilot-config.example.json` holds the installed
pilot's settings, the kept-start candidate and an external room recorder:

- noise suppression off;
- `duplex`; the installed template uses `v1-baseline`;
- capture reference clock;
- reference recovery off;
- 200 ms playout start;
- ring ceiling 12;
- kept-start continuation.

It contains no secrets: the SSH target is a placeholder, and the provider path
is the template's path, not its content. The serve child then runs:

```text
lamp-live serve PROVIDER OUT --noise-suppression off --turn-taking duplex --reference-clock capture
  --reference-recovery off --playout-start-ms 200 --provider-continuation kept-start
  --cue-socket WORK/cue.sock --ring-channel-ceiling 12
```

Offline, with no device and no audio, using the fake serve runtime, the real
relay and a fake prepared player whose "first copy" is a room datagram:

```sh
cargo run -p lamp-voice-eval -- serve-offline --out NEW_DIR --cache .cache/acoustic --manifest M... \
  [--plan FILE] [--fake-arg --fake-reopen-ms --fake-arg 700] [--player-fault 2:late:150]
```

`fixtures/voice-eval-serve-v1.json` is the reusable cached-scene example. Its
eight turns are:

1. A question at listening readiness.
2. A follow-up.
3. A second follow-up.
4. An interruption during the third answer.
5. A follow-up.
6. A correction on that follow-up's local endpoint.
7. A change of topic.
8. A last question.

Each follow-up starts on `next_turn_ready`. Existing plans, the catalog and
their hashes are unchanged; the default plan still hashes to `e9dad734…`.

## Readiness and the start grant

- **Before the trigger wait.** The runner arms each stimulus first. It calls the
  observer's `play_prepared` in-process: no process is created at the grant. The
  call loads and verifies the WAV and opens the output in silence. The ready
  receipt's `source_sha256` must equal the cached asset's identity.
  - Process creation, WAV preparation and report persistence are therefore
    never between readiness and the start.
  - A player that fails to prepare, or reports another source, withholds that
    step as `player_failed`.
- **Triggers.** They are events only:
  - `listening_ready`, `speaker_first_write` (for overlap).
  - `next_turn_ready`: availability reopened and the link ready after every new
    turn finished, with no other input since.
  - `local_endpoint`.
- **Final dispatch check.** It drains everything already relayed and rechecks
  every precondition.
- **The grant.** The runner sends exactly one grant, whose expiry is the
  original lateness bound on the player host's CLOCK_MONOTONIC:
  `mapped(Lamp event) − clock uncertainty + configured delay + 100 ms`.
  - The Lamp event time is translated through the ping/pong clock map. The
    runner takes the earliest Mac time the map allows; it never assumes the
    Lamp's monotonic epoch is the Mac's.
  - No fresh budget is added after transport, receipt or any other delay. A
    receipt-time trigger (no Lamp event time) or a missing clock map withholds
    the step.
- **At the first source copy.** The player re-reads the clock and refuses at or
  after expiry (`start_expired`). A refused or failed delivery is polled while
  the runner waits for the next trigger. It becomes a runner `evidence_lost`
  event, and every later turn is withheld.
- **What the start evidence is.** The step's start is replaced by the player's
  first-copy time. That is still not an acoustic onset; room audio stays a
  separate boundary.

In serve mode, listening readiness comes only from the accepted schema-1 cue,
which carries the runtime's own event time. A stdout readiness line has only
the relay's receipt time and may be buffered, so it is kept as a diagnostic
(`stdout_readiness_relay_us`) and never triggers a step. The grant receipt
keeps a signed `margin`: `headroom_at_grant_us` and `expired_at_send`. A send
delayed past expiry shows as negative headroom and is refused by the player at
its first copy; it never wraps around into a positive margin.

`local_endpoint` binds to the one turn admitted since the previous step.

- **Not dispatched:** a second admission, or the turn finishing or speaking
  before its endpoint.
- **Withheld at dispatch:** the turn has output or a finish, or any new
  candidate or admission arrived after the endpoint.
- **Declaration only:** the plan marks such a scene as a fresh request, but the
  runner does not reset turn ownership or context. Whether the runtime
  superseded the turn before output is judged from the trace; the dispatch
  qualifies nothing.

## Stop, reaping and failure provenance

Every serve session ends through one relay path. The runner's `{"type":"stop"}`
line, a closed runner transport and the hard deadline (`seconds +
allowance`) all start it.

1. SIGTERM goes only to the runtime the relay started, the leader of the
   process group it created. This matches the pilot unit's coordinator-first
   `KillMode=mixed`; there is no group-wide SIGTERM.
2. After a 5 s grace (the unit's `TimeoutStopSec`), the relay kills that group.
3. The relay then confirms the group is gone.

`session_end.serve` keeps these separately:

- `stop_cause`: `runner_stop`, `transport_closed`, `deadline`, or
  `runtime_exited` when the runtime ended by itself;
- the runner's stop reason;
- request, signal and exit times;
- `forced_kill` and `group_reaped`;
- the metadata summary.

A runtime that sends its own `run_end`, or reports a terminal provider link,
is waited for (10 s) rather than stopped, so its own end keeps its provenance.

The following are relay errors, so the attempt is aborted, never scored as a
clean conversation:

- a nonzero or signalled runtime exit, even after complete, acknowledged
  metadata (the runner checks the exit status as well);
- a forced kill;
- processes left in the group;
- a closed transport;
- the deadline;
- a group still present 2 s after the end.

After exit, the relay streams `metadata/events-{0..3}.jsonl` as trace lines,
oldest generation first:

- Each segment must begin with a valid `service_segment` header.
- Generations must be contiguous.
- `service_sequence` must advance by one.
- `health.json` must report a finished writer.
- The runtime's `service_metadata_finished` line must acknowledge every event,
  with nothing dropped, oversized, unconfirmed or degraded.

Overwritten history (generation 0 gone), gaps or a missing acknowledgment are
missing evidence: they are reported and the attempt is not scored.

## Service cycles

A physical mute ends the runtime cycle. A later cycle has a new cue boot.

- **Finding retained.** A new boot with a contiguous cue sequence is still a
  CueOrder rejection ("context continuity lost"). The conversation's remaining
  turns are withheld at once and the attempt is aborted.
- **Restart verification.** The attempt's `serve_continuity` evidence records:
  - every observed boot change and every service cycle record;
  - whether the runtime's own metadata verifies a restart: cycle 1 muted, and
    cycle 2 started no later than the first new-boot cue.
- **Scoring.** Only the first cycle's records are scored; the rest are counted.

## Cue contract this relies on

In addition to the proposed `input_availability`, `provider_link` and
`input_candidate` kinds (see [directed cues](voice-eval-directed-cues.md)):

- **Startup history.** Before a boot's listening readiness, a state record
  already older than the 100 ms cue bound is startup history. It is not sent and
  latches no fault; it is counted (`historical_state_skipped`).
- **Current link state.** At readiness the coordinator publishes the link state
  it just validated, `provider_link_current`. It is a fresh `provider_link`
  cue, never a re-stamped old record.

Both are separate lamp-live proposal commits.

## Offline controls

These ran through the real relay process, lamp-live's own `CueSink` and the
fake serve runtime and player, one attempt at a time. The 100 ms bound was
unchanged; no lease was widened.

| Control | Result |
| --- | --- |
| Normal 8 turns | 8/8 injected; each grant sent with 92–99 ms of the original bound left; first copy before expiry; correction superseded the follow-up before output (fake); SIGTERM to exit 12.5 ms; 105 metadata records, no gaps; group reaped |
| Delayed readiness (700 ms reopen) | each follow-up dispatched on the reopened availability, ≥ 690 ms after the finish |
| Extra candidate during the wait | `precondition_lost`; rest withheld; nothing granted |
| Stale, duplicate, reordered, foreign-boot, dropped cue | rejected cue retained; conversation ended (`session_ended`); attempt aborted |
| Provider outage 800 ms | dispatched on the recovery's link-ready event |
| Provider terminal | `provider_unavailable`; runtime exited by itself (`runtime_exited`, exit 1) |
| Player first copy 150 ms late | `start_expired`, zero copied; later turns withheld; no second admission |
| Player prepare failure | `player_failed` → `delivery_failed`; rest withheld |
| Runtime stopped externally | `session_ended`; `runtime_exited`; clean metadata |
| Physical mute and new cycle | withheld at the new boot; continuity lost; restart verified from metadata |
| SIGTERM ignored | forced kill after 5 s; attempt aborted, never clean |
| Room recorder ends early | `evidence_lost` (room recorder); rest withheld |
| Relay: stop / transport closed / deadline / SIGTERM ignored | each cause, signal, exit, forced kill and reap recorded separately |

## Limits

- The fake runtime is a policy simulation of serve's shape, not lamp-live. The
  real serve path additionally needs the physical privacy worker, Gemini and
  hardware.
- The prepared observer has not run in this runner against the iMac speakers.
  Main's separate real-iMac check covered the observer itself.
- Every cached fresh-request scene starts its speech 1 s into the clip. With
  about 1.1 s from endpoint to first ALSA write, a correction dispatched on the
  endpoint can be heard after the reply started; the trace decides which
  happened.
- A mute or stop is visible live only as cancellation, lost availability or
  the next boot. Service-cycle records reach the runner only in the final
  metadata.
- The room recorder runs for its configured length; the runner waits for it
  at the end.
