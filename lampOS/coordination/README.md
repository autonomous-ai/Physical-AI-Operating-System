# Lamp agent coordination

Direct session messages carry routine assignments, questions and review feedback.
Git holds code, verified checkpoints and durable decisions for all three agents.
The owner does not relay reports. The owner selected this combined workflow on
2026-10-11 after the Git-only setup; revision `direct-and-git-v2`.

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

## Direct route

The existing sessions are connected through native Harness team **Lamp Rust
implementation**, ID `7ab909c63bf648319c8535737bce2a94`, owned by office.
Use member names `main`, `agent1`, and `agent2`; Agent 2 runs on M2. Team
introductions provide each session's local member identity and credentials.
Never put member keys, provider credentials or private recordings in Git.

Use `harness team ... ask`, `reply`, `inbox`, and bounded `wait`. Preserve the
exchange ID when retrying an uncertain request. A queued message is not an
acknowledgment; wait for the intended agent's reply. Read pending questions at
safe work boundaries and answer interface blockers before continuing dependent
work. Do not interrupt another agent's tools or approve its permission prompts.
If native delivery is delayed or unavailable, publish the request/answer in the
agent's Git status without requiring the owner to relay it.

## Working loop

1. Use a verified active session for direct messages. Check recipient identity
   and task before submitting ordinary prompt text; queue messages at a safe
   boundary and require acknowledgment. Do not interrupt a test, approve a
   permission prompt, inject control characters or change another agent's files.
   A message submitted to a terminal is not proof it was read or acted on.
   Fetch branches at startup, on checkpoint notification, before shared-interface
   changes and after publishing work. When direct delivery is unavailable,
   leave the request in your Git status and check at safe work boundaries.
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
   self-referential SHA. Push the status together with its code history and send
   the main agent a short direct notice with branch, SHA and evidence boundary.
5. Give each request a stable ID such as `A1-003`; the recipient answers that
   ID in its own status file and records the source commit it read. Keep open
   requests and unintegrated checkpoints until explicitly acknowledged.
   Send routine questions directly when possible. Record interface agreements,
   unresolved blockers and integration decisions in these files so they survive
   a closed session. Acknowledge received requests with their IDs.
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
