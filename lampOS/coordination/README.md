# Lamp agent coordination

The owner requires **all communication between Main, Agent 1 and Agent 2 through
their tmux terminals**. Git holds code, verified checkpoints and durable decisions;
it is not a message queue. No owner relay and no Harness team/channel messages.
This owner update supersedes `direct-and-git-v2`; revision `tmux-and-git-v3`.

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

## Terminal routes

These identities were observed on 2026-10-11; recheck them before each delivery.
Pane numbers are local to their machine and may change after a restart.

| Recipient | Machine | Observed pane | Session/worktree identity |
|---|---|---|---|
| Main | office | `%206` | `List network lamps`; `autonomous-os/calm-pebble` |
| Agent 1 | office | `%216` | `Gemini voice-session reliability A1`; `autonomous-os/merry-zebra` |
| Agent 2 | M2 | `%37` | `lampOS voice-eval runner A2`; `autonomous-os/noble-fox` |

On office, tmux is `/usr/local/bin/tmux`. Inspect the recipient's current path,
title and composer before loading a text file into a named tmux buffer, pasting
with `paste-buffer -p`, and submitting ordinary prompt text with Enter. Attribute
messages to the sending agent. Never use Ctrl-C, approve a permission dialog,
change permission settings, or overwrite a draft to deliver a message.

M2 is remote; its local hostname did not resolve from office. The existing trusted
machine link can carry ordinary input to its tmux terminal. Main verified Agent
2's exact session, Claude engine, worktree and `%37` runtime before submitting
MAIN-051 through that terminal route, and observed its `turn_started` event.
This uses the normal guarded terminal input path, not the team/channel API or a
new credential. Preserve pairing and permission checks. If a message has uncertain
delivery, inspect the existing terminal before retrying; do not send duplicates.

A successful paste or queued input is not proof that the assignment was read.
Verify an actual reply or work starting. Do not create acknowledgement-only loops.
Send substantive interface questions, blockers and checkpoint notices directly.
If a terminal is temporarily unavailable, retain the unsent message locally and
continue independent work; do not switch to Git polling or require owner relay.

## Working loop

1. Fetch source at task start, after a checkpoint notice, and before integration.
   Preserve dirty work. Reading a branch never requires merging it.
2. Read the current workstream brief for ownership and interface constraints.
   Shared-file integration proposals go in separate commits for Main to compose.
   Discuss changes through tmux while continuing independent owned work.
3. Publish reviewable code checkpoints with exact-file commits, one-line messages
   and no trailers. No force pushes or concurrent pushes to the integration branch.
4. Record the exact tested code SHA, base, changed paths, commands/results and
   known limits in the agent's English status file. Send a short tmux notice with
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
