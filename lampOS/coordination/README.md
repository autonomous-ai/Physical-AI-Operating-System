# Lamp Git coordination

Git is the common work and communication record for the main agent, Agent 1,
and Agent 2. The owner does not relay reports. This protocol is authorized by
owner instruction on 2026-10-11; revision `git-coordination-v1`.

## Branches and ownership

| Agent | Work branch | Status and messages |
|---|---|---|
| Main integration agent | `lamp-v2-chat` | `coordination/main.md` |
| Agent 1: Gemini and audio | `gemini-voice` | `coordination/agent-1.md` |
| Agent 2: camera and evaluation | `lamp-v2-agent2` | `coordination/agent-2.md` |

Paths are relative to standalone `lampOS`. The [completion plan](../docs/rust-completion-plan.md)
remains the file-ownership and implementation scope. Each agent writes its own
status file on its own branch. Read the other agents' files from fetched refs;
merging their unfinished code is unnecessary for reading a message.
The main agent alone integrates into `lamp-v2-chat`.

## Working loop

1. At startup, after each verified checkpoint, before changing a shared
   interface, and while waiting on another agent, fetch the three branches.
   During active work, check at least every five minutes at a safe tool boundary.
   A Git fetch does not wake an idle/closed agent; do not claim delivery until
   the recipient acknowledges the message in its status file.
2. Read `main.md` from `origin/lamp-v2-chat` for assignments, interface locks,
   integration decisions and device ownership. Read peers' status files from
   their work branches for requests and answers. Never reset or overwrite dirty
   work to perform this check.
3. Publish small code checkpoints with exact-file commits and one-line messages
   without trailers. Preserve history; no force pushes or concurrent pushes to
   the integration branch. Keep unfinished work clearly marked `draft`.
4. Update your status file with the exact tested code commit, base commit,
   changed files, exact checks/results, known gaps, and requests to named peers.
   A subsequent status-only commit can name the tested code commit without a
   self-referential SHA. Push the status together with its code history.
5. Give each request a stable ID such as `A1-003`; the recipient answers that
   ID in its own status file and records the source commit it read. Keep open
   requests and unintegrated checkpoints until explicitly acknowledged.
   Routine decisions and fixes go directly between agents through these files.
6. The main agent fetches and reviews published checkpoints, verifies the
   combined source, integrates accepted work and records the exact integrated
   SHA or requested correction in `main.md`. Passing a branch's tests does not
   qualify the combined runtime or a physical conversation.

Example reads, from the parent Git worktree:

```sh
git fetch origin lamp-v2-chat gemini-voice lamp-v2-agent2
git show origin/lamp-v2-chat:lampOS/coordination/main.md
git show origin/gemini-voice:lampOS/coordination/agent-1.md
git show origin/lamp-v2-agent2:lampOS/coordination/agent-2.md
```

If a status file is not present yet, record `awaiting acknowledgment`; do not
interpret a missing file as completion. If lampOS later becomes its own Git
root, remove only the `lampOS/` prefix in these Git object paths.

## Devices, evidence and escalation

The main agent owns all device and acoustic test scheduling. Reserve a device
in `main.md` before any agent deploys, restarts services, opens audio hardware,
or changes its configuration. An explicit main-agent grant identifies the
worker, device and bounded operation. Only one speaking experiment runs at a
time unless overlap is the declared test. Both microphones may observe it.
A software test or successful audio write is not an acoustic measurement.

Put English implementation reports in this project. Keep credentials, private
recordings, caches and binaries out of commits. Record artifact identity and
measured boundaries; do not include authentication values in status or messages.
The owner receives concise verified milestones or decisions that need human
judgment, not routine agent handoff traffic. Agents must stay within the existing
permissions; this protocol does not enable unsafe motion or a fleet release.
