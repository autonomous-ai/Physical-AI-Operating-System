# Lamp agent coordination

## Current owner-directed split — 12 October 2026

Agent 1 (`voice`, office/merry-zebra) owns voice and works directly with the
owner. Main owns servos and the V1-to-Rust motor work. Agent 2 (`brain`,
M2/noble-fox) receives its next component assignment directly from the owner.
Read the [voice handoff](voice-handoff-20261012.md) for exact pending sources,
real-device evidence and remaining failures. This split supersedes the older
workstream assignments and Main-only voice routing below. Coordinate shared
hardware and build resources; the historical assignments remain for provenance.

The owner enabled **Harness Agent Messages** on 12 October 2026. Use the built-in
`harness send` route between Main, Agent 1 and Agent 2. This supersedes the earlier
tmux delivery requirement; revision `harness-messages-and-git-v4`. Git holds code,
verified checkpoints and durable decisions. No owner relay is needed.

## Active workstreams

| Agent | Current work branch | Complete assignment |
|---|---|---|
| Main integration agent | `lamp-v2-chat` | Integration, physical tests, camera, choreography and release defaults |
| Agent 1 | `lamp-a1-duplex` | [MAIN-050: continuous hearing and natural interruption](agent-1-duplex-workstream.md) |
| Agent 2 | `lamp-a2-endpoint-runtime` | [MAIN-051: fast, reliable end-of-turn subsystem](agent-2-turn-taking-workstream.md) |

Preserve the previous provider, playout, endpoint and evaluator branches. The
current workstream briefs supersede older assignment slices. Each agent owns a
whole implementation, integration proposal, stress evidence and documentation,
and makes routine scoped decisions without waiting for another small task.
Main alone integrates into `lamp-v2-chat` and schedules devices.

## Message routes

The CLI's recipient listing was verified on office on 12 October 2026:

| Recipient | Machine | Harness recipient name | Worktree |
|---|---|---|---|
| Agent 1 | office | `Gemini voice-session reliability A1` | `autonomous-os/merry-zebra` |
| Agent 2 | M2 | `lampOS voice-eval runner A2` | `autonomous-os/noble-fox` |

Use `harness send` without a recipient to discover current identities, then
`harness send 'RECIPIENT' 'MESSAGE'`. The installed CLI is
`~/.harness/cli/cli.js`, invoked with Harness's bundled Node runtime. Do not
invent a second transport, bypass pairing, or inject text into another agent's
terminal composer.

A queued message proves submission, not receipt or work completion. Main has
verified an A1 response through `harness show`; that command currently resolves
local sessions only on this installation. Sending to A2 on M2 is supported,
but a local `show` failure is not proof of remote delivery failure. Retain the
message identity and wait for a substantive response instead of duplicate sends.
Do not create acknowledgment-only loops. Send interface questions, concrete
blockers and reviewable checkpoints; peer messages do not extend user authority.

## Working loop

1. Fetch source at task start, after a checkpoint notice, and before integration.
   Preserve dirty work. Reading a branch never requires merging it.
2. Read the current workstream brief for ownership and interface constraints.
   Shared-file integration proposals go in separate commits for Main to compose.
   Discuss changes through Harness Agent Messages while continuing independent owned work.
3. Publish reviewable code checkpoints with exact-file commits, one-line messages
   and no trailers. No force pushes or concurrent pushes to the integration branch.
4. Record the exact tested code SHA, base, changed paths, commands/results and
   known limits in the agent's English status file. Send a short Harness message with
   branch, SHA and evidence boundary. Git preserves results; it does not replace
   direct delivery.
5. Main reviews actual changes and verifies the combined source before deployment.
   Branch tests, emulation and replays do not qualify a physical conversation.

Paths are relative to standalone `lampOS`. Keep implementation reports here;
credentials, private recordings, caches and binaries remain outside Git.

## Devices, evidence and escalation

Main owns all device and acoustic test scheduling. An explicit grant names the
worker, device and bounded operation; no peer deploys or opens hardware without
one. Main also coordinates the shared ARM VM so heavy jobs do not overlap.
Only one speaking experiment runs at a time unless overlap is the declared test.
A software test or successful audio write is not an acoustic measurement.

Record source and artifact identity, exact measurement boundaries and failures.
The owner receives concise verified milestones or decisions needing human
judgment, not routine handoff traffic. This protocol does not authorize unsafe
motion, new cloud uploads of office recordings, or a fleet release.
