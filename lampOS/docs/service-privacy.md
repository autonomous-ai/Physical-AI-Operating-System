# Supervised physical mute in serve

`serve` begins with only the GPIO privacy worker. It sends denied microphone and
camera snapshots to maintain that worker's existing parent heartbeat. No capture,
speaker, camera, ring or provider worker is created during this wait. Missing,
malformed, out-of-order, future or stale switch observations are faults, not user
mute. The existing 50 ms freshness limit and 60 ms stable-unmute requirement are
unchanged. Fresh muted readings may continue indefinitely without creating a
conversation readiness cue.

After stable fresh unmute, the privacy-only worker must cooperatively stop and
exit successfully. Only then does the ordinary coordinator create a fresh full
runtime. That runtime independently checks new privacy observations before its
existing microphone/input readiness path; the outer observation is not a permit
or a promise that the switch stayed open during startup.

During an active serve runtime, a current valid physical mute ends the current
owners. An unfinished reply is cancelled as `privacy_muted`; it is not reported as
completed. Microphone/camera permissions close, all workers receive Stop before
any final wait. Only that exact fresh-mute outcome with successful cleanup may
return to the GPIO-only wait. Optional camera/ring failures disable that capability
during the active conversation without cancelling healthy audio. Final cleanup
still verifies their exit and required local-revocation, teardown or black-write
receipts. A missing receipt, failed exit or forced termination blocks reopening;
contained presentation failure alone is not permission to skip cleanup. Lost
freshness, unknown status, mandatory worker/provider faults and invalid messages
remain service errors. A previous runtime
error cannot hide a shutdown failure. The combined error retains both causes
and does not expose a restartable physical-mute token.

Unmute creates fresh controller/worker/provider identities. Old PCM, prospective
input, request IDs, permits and queued output are not replayed. No preservation
of Gemini conversational memory across physical mute is claimed. There is no
periodic context reset or timer-based reopen. A later stable unmute must satisfy
the new runtime's ordinary readiness checks again.

One service invocation owns one signal registration and one bounded rotating
metadata writer. Each GPIO-only/full runtime uses a fresh private IPC directory;
a successful cycle explicitly removes the now-empty directory after all workers
and local endpoint bindings have ended. Unexpected remaining paths refuse the
next cycle rather than being recursively removed or accumulating indefinitely.
No per-cycle recording/output directory is created. The typed receipt ledger is
cleared only after verified destruction of the old runtime. Cue sequence numbers
continue; each runtime has a new boot and cleared capture/reference context.
A latched cue fault stays latched. Consumers must distinguish boots; old finite
cue consumers do not become multi-cycle service clients by assumption.

SIGTERM/SIGINT during the established mute wait requests cooperative Stop and
reaps the privacy worker. Audio/provider startup is not attempted. The wait loop
polls at 2 ms and publishes its denied heartbeat at most every 25 ms; these are
software scheduling targets, not measured GPIO or acoustic response bounds.
Privacy-worker shutdown retains the existing 300 ms grace; full-runtime cleanup
uses the existing shared shutdown bound. Native kill/wait cannot promise a hard
userspace deadline, and any forced cleanup remains failure. A signal or failure
during incomplete worker startup stays an explicit startup failure rather than
claiming a verified graceful mute lifecycle.

The regressions use actual local Unix datagrams and synthetic child processes,
including repeated verified cycles, stale/unknown input, active owned-reply
cancellation, cleanup failure refusing the next cycle and an actual process
SIGTERM while muted. They do not establish native GPIO behavior, full-duplex
quality, continuous-session resource use or acoustic latency. Device qualification
of the composed optional-camera/ring failure policies remains required. Finite directed/fixture behavior and audio policies are unchanged.
