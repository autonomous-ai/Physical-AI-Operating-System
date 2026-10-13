# Continuous chat core

`lamp-live serve PRIVATE_CONFIG.json NEW_OUTPUT [session options]` uses the same
coordinator, provider recovery, capture, speaker/reference, privacy and output
ownership as `directed`. It has no elapsed conversation deadline and does not
restart the provider or cancel an answer at 600 seconds. Startup still requires
real retained microphone data and the current provider-ready session within the
existing 25-second readiness budget. Time alone cannot create readiness.

`NEW_OUTPUT` must be a new leaf under an existing parent. The coordinator creates
it with mode 0700. Provision the provider configuration separately; no credentials
belong in command output or the source tree. This mode introduces no Python,
legacy service or shell launcher dependency. Deployment and native qualification remain separate work; this core alone is
not an all-day device release. Physical mute is supervised as described in
[service privacy](service-privacy.md).

The ordinary `SessionOptions` flags remain explicit. No camera or ring is opened
by default; no motor is opened by this coordinator. Existing noise suppression,
turn-taking, endpoint and playout defaults are unchanged. For example:

```text
lamp-live serve /etc/lampOS/gemini.json /var/lib/lampOS/sessions/NEW_ID \
  --turn-taking v1-baseline --playout-start-ms 200
```

These explicit policies require their own physical qualification. Adding
`--camera-config ABS_PATH` or `--ring-channel-ceiling 0..120` retains the same
camera/privacy and phase-bound ring authority. The supervisor selects a private
continuous ring-worker lifetime instead of its finite 630-second ceiling.
Initial authority, parent transport, installed permission expiry and 100 ms
frame age still apply. The lifetime option supplies no output permission.

`serve` rejects `--diagnostics` before startup. It does not silently keep
pre/post-AEC or render PCM forever. Use a separate finite `directed` experiment
for raw diagnostics. Finite `directed` and `directed-fixture` retain their 1..600
second duration, strict 20,000-row evidence cap, complete trace file and existing
failed-on-signal behavior.

## Metadata never supplies interaction authority

Cancellation and accepted-output receipt lineage now lives in a typed bounded
ledger. A dispatched command retains its original owner/sequence through partial
acceptance or rejection. Accepted cursors remain unsettled until an actual drain,
retirement or reset/discard receipt. The original successor admission is retained
inside a cancelled owner's entry. Completing a successor cannot relabel the old
tail or erase that relationship. At most 64 owner entries exist; only settled
terminal entries can be evicted. Exhausting the bound with genuinely unsettled
commands is a protocol fault, not a logging fault.

The cancelled-tail and expected-discard handlers consult this ledger, never JSON
rows. Metadata deletion, rotation or forged rows cannot grant authority or erase
a pending obligation. Normal controller permission checks, privacy priority,
request/session matching and the accepted-tail deadline remain unchanged.

Service ring logs are append-only request/feedback events. They do not keep
mutable indexes into old log batches. The real choreographer still retains and
validates its one outstanding frame independently. Cue emission consumes each
bounded metadata batch once while retaining its boot, capture/reference context
and datagram sequence. Losing a local batch invalidates further scheduling cues;
a slow or absent cue receiver does not stop the conversation. A requested cue
endpoint must still validate and connect at startup. No new strict-cue mode is
introduced.

## Bounded best-effort storage

The coordinator retains at most 256 metadata rows per loop batch, each with a
32 KiB serialized ceiling. It performs bounded serialization and a nonblocking
push into a 64-slot SPSC queue. Only the metadata thread performs file or stderr
I/O after construction. This targets sub-millisecond metadata submission on the
host; it is not a target-hardware latency guarantee.

The writer exclusively opens four private files before activation and keeps the
descriptors pinned. `metadata/events-0.jsonl` through `events-3.jsonl` rotate at
4 MiB each; their total event storage is at most 16 MiB, plus the small health
file. Rotation truncates an already-owned descriptor rather than following a
replaced pathname. A segment header identifies its generation; every retained
service row has a monotonically increasing `service_sequence`. Rows may include
private conversation transcripts; no raw PCM is written by this logger.

Queue saturation, oversized rows and disk failures latch health/counters and drop
metadata. They do not return an audio-loop error, revoke an owner, create readiness
or restart Gemini. A writer fault stops logging for this invocation; it does not
retry or grow a replacement queue. `metadata/health.json` reports counts and
rotation state when the writer makes progress. The writer emits at most one
bounded dropped-event warning and one fault warning to stderr. An indefinitely
stalled native filesystem call can delay that thread's health/stderr publication;
the in-memory counters remain latched. No syscall preemption is claimed.

These rolling files are deliberately incomplete evidence, even without detected
drops. They cannot qualify acoustic latency or a benchmark denominator.
`unconfirmed_events` distinguishes accepted metadata not yet written, including
pending or abandoned prefixes. The final `service_metadata_finished` stdout row
reports the bounded finish acknowledgement separately from the run outcome.

## Terminal provider shutdown

A recoverable outage keeps the existing provider recovery policy and local
interruption path. A terminal status alone does not end serve: the priority
status can arrive before valid queued audio and that request's final outcome.
Only the ordered provider `Ended` packet enables terminal service shutdown.
Submitted Starts can still receive their local `NotDelivered` outcome afterward.

Serve continues normal privacy, capture and speaker servicing while retained
work settles. Abandoned active input drains to the existing detector's actual
endpoint without replay or detector reset. Already received PCM keeps its
original owner through actual acceptance/retirement. Each turn gets its existing
completion or failure outcome once; a partially generated failed answer remains
failed even if its retained PCM is played. Pending commands and accepted cursors
of cancelled owners must also settle in the bounded live receipt ledger.

After no reply, abandoned input drain, queued input or speaker obligation remains,
serve records `service_provider_ended` and returns an explicit terminal-provider
error. It does not fabricate readiness, restart the provider, replay input or
return to the physical-mute supervisor. Ordinary worker, privacy and protocol
faults retain their existing immediate failure behavior. This adds no grace
period or response timeout; incomplete obligations remain subject to their
existing capture, authority, transport and utterance bounds. Finite experiments
keep their existing lifetime behavior. The tests use synthetic peers and real
local IPC, not a cloud outage or acoustic measurement.

One terminal end is treated differently after all of that: the provider's
quota or its rate limit (`quota_exceeded`, `rate_limited` in the terminal link
state). Serve shuts down and records exactly as above, and then the process
stays for 120 s before it exits with the same failure status, with a line in
the journal saying so; a request to stop ends the wait at once. The reason is
whoever restarts the service. On lamp-4ace on 2026-10-13 the relay refused
every new session with `quota_exceeded` from about 07:05 local, each start
ended within a second, and systemd (`Restart=on-failure`, `RestartSec=4s`)
started it again about 380 times in half an hour before anyone looked. With
the wait that is one request every two minutes. The lamp is still silent and
dark for as long as the refusal lasts; nothing here tells the person in front
of it why. Checked by a unit test of which faults count as such a refusal,
not against a provider that refuses.

## How loud an utterance was

Each `local_endpoint` record carries `speech_level` when the detector scored
any block of the last eight seconds as speech: how many such blocks, and the
middle and the upper tenth of their levels after cancellation, on the face's
scale (0 is -48 dBFS or under, 255 is -14 dBFS or over). It is there to find
out whether someone at the lamp can be told from someone across the room by
level; nothing acts on it yet. Added on 2026-10-13, when an office full of
people talking brought the conversation checks down: other voices were taken
for the person at the lamp going on, and for that person cutting in.
What has been done about that since is in [group-talk.md](group-talk.md).

## Stop and current limits

After entering the coordinator loop, SIGTERM/SIGINT records a service-stop request,
cancels any unfinished reply as `service_stopped`, revokes camera/microphone
authority and requests Stop from all workers before waiting. Worker failure or a
required forced kill remains failure. Only after worker cleanup does the optional
logger receive up to 500 ms for final acknowledgement. A timed-out writer is
detached; its eventual file update cannot revise that returned timeout. Process
exit reclaims the remaining thread. A signal during incomplete startup retains
the existing explicit startup failure path.

Serve waits with only its GPIO privacy worker while physically muted. A fresh
active mute closes the full runtime and returns to that wait only after verified
cleanup; stable unmute then creates new capture/provider identities. Unknown or
stale privacy and worker/provider faults remain errors. There is no claim that
reopening preserves Gemini context. See [the precise restart and bounded cleanup
contract](service-privacy.md).

Host regressions cover simulated progression beyond 600 seconds without a session
expiry, bounded rotation and stalled/failing writers, cancellation/privacy while
metadata is dropped, immutable old-owner settlement after history rotation,
append-only ring observations, batch cue continuity and orderly synthetic worker
shutdown. macOS mock channels are retained through the final reap because peer
socket teardown differs from Linux; actual Linux worker socket shutdown, sustained
on-device operation, resource use and acoustic behavior remain native checks.
