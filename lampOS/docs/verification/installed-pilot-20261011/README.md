# Stopped standalone pilot installation on 4ace

The Native05 binary is installed in its immutable SHA-256 directory under
`/opt/lampOS/releases`, with a private standalone configuration under
`/etc/lampOS/pilot-4ace` and `lamp-live-pilot@.service` in systemd. The package is
installed **stopped and not boot-enabled**. No voice session, hardware open,
legacy cutover, mixer change or motor action was performed by installation.

[Numerical result and identities](results.json) preserve the exact checks.
Native `systemd-analyze verify` passed. The effective service is root, 0077,
`Restart=no`, coordinator-first `KillMode=mixed`, five-second stop timeout and
the serial path inaccessible. It has no drop-ins, ExecStop or ExecStopPost.
The staged binary, private copied credential and migrated configuration matched
their sources; only the credential path changed. All six legacy service states
and PIDs, both microphone/speaker mixer snapshots and the stationary guard
remained unchanged. The session-output directory remained empty.

The installed binary's `provider-check` succeeded in 1,948,866 microseconds,
opening no microphone and sending no audio. This is one setup check, not a
conversation-readiness or response-latency measurement.

Two original administrative checks stopped because this systemd version omits
empty ExecStop/ExecStopPost arrays from `systemctl show`, even with `--all`.
Their failed receipts and partial-install paths remain retained. A separate
read-only reconciliation checked both exact D-Bus arrays as length zero, then
rechecked every file, active-service state and mixer. Nothing was reinstalled,
started or silently treated as a passing original attempt.

The [runbook](../../attended-4ace-pilot.md) separates installation from startup
and facilitator handoff. A subsequent [installed-service run](../installed-service-20261011/README.md)
completed two exchanges and verified Stop, empty cgroup, worker release and
stationary restoration. Physical mute/reopen and facilitator handoff remain
before the attended pilot. The installed mode
uses the fixed silence baseline; it does not qualify natural overlapping
interruption, unrelated-conversation rejection or full physical choreography.
No credential, provider endpoint, recording or binary is in this record.
