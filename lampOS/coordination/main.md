# Main agent status

- Protocol: `git-coordination-v1`
- Updated: 2026-10-11
- Branch: `lamp-v2-chat`
- Shared checkpoint before this coordination change: `2535fb565537c41ddcf465afadbc63ebda7b9c28`
- Runtime checkpoint: `3a21db25d89532a5aadb7db40bf46505d15cccba`
- State: implementing and beginning alternate-device qualification; release unfinished.

## Open requests

### MAIN-001 — Agent 1 and Agent 2: acknowledge Git coordination

Fetch this branch, read [the protocol](README.md), and acknowledge revision
`git-coordination-v1` in your own status file on your work branch. Publish your
current tested checkpoint and requests there. Check all three branches at safe
work boundaries at least every five minutes during active work. The owner has
asked us to coordinate directly; no owner report relay is required.

### MAIN-002 — Agent 1: publish and prioritize the voice path

Local `gemini-voice` was observed at `155bab4e2`, with `67154e0b6` below it.
The last fetched remote still ends at `fbf9ecc3c`; local work is not a published
checkpoint. Publish your verified commits and exact evidence. Keep the
[completion-plan ownership](../docs/rust-completion-plan.md). Prioritize continuous
audio, echo/near-end evidence, real-provider probes and cached failure audio
before optional environmental acquisition. The main agent can run approved
service probes on the available boards once the artifact and credential setup
are verified; publish bounded commands and expected observations without keys.

### MAIN-003 — Agent 2: publish current validation and interface requests

Fetched `origin/lamp-v2-agent2` at `197fa0b96`; relay framing and camera frame
commits are available for review, not yet integrated. Publish the exact checks,
remaining gaps and interface decisions in your status file. Continue the real
coordinator/process-graph evaluation and camera/perception plan. Coordinate
shared CLI/wire changes explicitly; preserve your existing history.

## Current interface ownership

The main workstream is finishing priority provider-link availability and the
coordinator consumer. It owns `provider_worker.rs`, `provider_link.rs`,
`coordinator.rs`, `coordinator/**`, `wire.rs`, `admission.rs`, and the associated
fixture migrations for this batch. The new contract keeps capture/privacy and
retained playback independent of cloud admission; drains unsent input through
its actual endpoint; and attributes `Started` to its original provider session.
Production remains Legacy/Require until the combined consumer is verified.
Agent 1 retains `crates/gemini/**`; propose producer API changes by request ID.

An independent motor-control candidate has 84 passing host tests, including 30
new cases. It remains private and unintegrated; it has no Linux motion port or
physical qualification. Neither that count nor the source-only provider tests
are claims about the shared runtime.

## Devices and current reservations

| Device | Address | Test owner | State |
|---|---|---|---|
| lamp-a0ae | 172.168.20.197 | Main | Primary native Rust preflight; no motion |
| lamp-52e6 | 172.168.20.190 | Main | Repeat hardware checks; preserve existing STT experiment drop-in |
| lamp-4ace | last known 172.168.20.159 | Main | Not rediscovered in the last authorized scan |

The owner authorized both alternate Lamps for real-device testing and temporary
ambient-microphone recording until iMac microphone consent is available. Both
C-Media ambient inputs recorded their own speaker tests. Temporary capture gain
8/35 with AGC off eliminated sample clipping at requested speaker volume 100;
original gain, AGC, volumes and software mute states were restored and verified.
The iMac's cached question was not recognizably captured. A full acoustic
conversation route and new V1-main/Rust latency cohort remain unqualified.

Exact staged ARM binary: SHA256
`199903b36fd3212df7e6d054adba03c6259fcb6248f60813cc7c27d4e303bf6c`.
It matches runtime source manifest `bc9e6ba2b1905269575e031afab6af5e96033ccb53800f4fbecd7b342f51a3d3`.
Prior qualification: 843 host tests and 174 targeted emulated ARM tests; this is
not native-board qualification. Physical handover must account for the old
HAL's shutdown path potentially parking/releasing servos. Do not stop/restart
HAL or remove motor guards independently.

## Integration ledger

- `d6b813695` and Agent 2's earlier `3eaa0365` history: included before shared
  checkpoint `2535fb565`.
- Agent 1 local `155bab4e2`: publication/evidence requested by MAIN-002.
- Agent 2 published `197fa0b96`: fetched, review pending (MAIN-003).
- New Git coordination acknowledgments: pending from both external agents.
