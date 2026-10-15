# Tasks

Tasks is an optional process that keeps what the lamp proposed to an agent,
the person's confirmation, the one dispatch attempt, the output observed
later, and what Voice reports as played. Its private records survive process
restart. It starts no work without `--enable` and a fresh trusted confirmation.
The lamp's existing Voice, Actions, deployment and account configuration are
unchanged by this crate.

## What exists and what we take

The chosen route is the installed Harness CLI, as described in
[brain-build](brain-build.md#step-5-in-more-detail-tasks-through-harness-on-the-lamp).
The [contract review](tasks-contract.md) records the inspected owner revision,
V1 behavior, command shapes and unresolved boundaries. The review compared
the CLI, its MCP wrapper, direct local WebSocket RPCs and the older Device
protocol. The CLI retains Harness's caller policy, account, lineage, rate
limits and ownership checks. MCP adds no stronger completion receipt; raw
mutation RPCs would omit part of that policy; Device is the route explicitly
not taken. Tasks implements no pairing, login, credential reader or transport.

Keep V1's write-before-send reservation, explicit uncertainty, identity checks,
durable speech claims and admission-versus-playback distinction. Reuse Rust's
process/socket facilities, `rustix` for bounded polling and process groups,
`serde` for strict local requests, SHA-256 for immutable local fingerprints,
and bundled `rusqlite` for the journal. The private Store follows the reviewed
[Schedule](schedule.md) persistence implementation without a Tasks-to-Schedule
runtime dependency. All source remains inside the standalone `lampOS/` tree.

The current CLI cannot prove completion of the particular lamp request. Every
successful local response therefore carries `completion: "unverified"`.
`idle`, text saying “done,” and a successful process exit never change that.
There is no automatic completion announcement in this implementation.

## Ownership and switches

`lamp-tasks serve` returns `enabled:false` before opening a store, reading a
target file, constructing an adapter, accessing a clock, or creating workers.
`--enable` starts the private service. It does not confirm a task. There is no
automatic deployment, service unit, startup wiring or Voice tool registration.

The operator supplies an existing Harness executable and a target allowlist.
The model can choose an alias and task text; it cannot choose an executable,
environment, permission mode, machine prefix, transport frame or arbitrary
CLI verb. The first scope starts a new child through `new`; focused-agent
selection and following up an existing child require an owner-supported
contract. Tasks never substitutes a guessed agent or another engine.

The target file must be an absolute, service-user-owned, single-link 0600
regular file, at most 16 KiB, with version 1 and 1–8 unique aliases. For example
(the machine ID below is a fixture, not an actual account destination):

```json
{"version":1,"targets":{"coding":{"machine_id":"0123456789abcdef0123456789abcdef","project":"desk-work","engine":"codex"}}}
```

After the real-account qualification below, First can provision the parent
directories and run the explicitly enabled service:

```sh
./lamp-tasks serve --enable --dir /var/lib/lampOS/tasks \
  --socket /run/lampOS/tasks/request.sock --voice-socket /run/lampOS/tasks/voice.sock \
  --targets /etc/lampOS/tasks.json --harness /absolute/provisioned/harness
```

`--harness-as-person` (off unless set; added 2026-10-14 by the first
builder) calls Harness without `AI_AGENT` and `HARNESS_AGENT_SESSION`, so
Harness takes the call as the person's own. Marked as an agent, `harness new`
is refused until the person turns on "Agents can start other agents", an
account-wide setting (see "First real Harness check" below). The owner chose
the plain CLI on 2026-10-14 ("just use the harness cli to create a new
claude code session"); the person's spoken yes to each exact task is still
what lets a task start.

`--bus-root ABS` selects an isolated bus for qualification. `--run-for-ms N`
bounds an operator run to 1–86,400,000 ms; it does not cancel remote work.
Linux reads its boot identity itself; development hosts require `--boot-id ID`.
Configuration is loaded once. Prepared records retain their exact target and
must be prepared/confirmed again after restart, even if an alias was remapped.

The request socket permits preparation, inspection, cancellation before an
attempt, and an explicit observation request. Only the separate Voice socket
permits `confirm`, `claim_speech` and `ack_speech`. Both sockets and their
parent are private to the service user. This separation is an integration
boundary, not authentication against another process with that same UID.
Actions must expose only the public operations to the model. First must wire
the trusted route exclusively to Voice's real confirmation and output owner.

“Spoken yes” means the **person's** confirmation of the exact prepared task,
not the lamp saying “on it.” The private caller attests that fact; Tasks does
not recognize speech or identify a speaker. It also cannot establish who is
allowed to act on a nearby person's account. Those remain Voice/product rules.

## Local state transitions

1. `prepare` commits the exact text, operator target, unique task ID and label,
   payload digest, current boot/service session, and a 120-second expiry.
   No Harness command runs. Voice obtains that immutable content with
   `get_task` before requesting confirmation.
2. Trusted `confirm` must name that ID, digest and current service session.
   It commits `queued` and returns promptly without waiting for Harness.
   The storage owner commits `attempting` before giving one job to the CLI
   worker. That durable transition is the only launch grant.
3. The worker preflights the full machine ID, invokes `new` at most once, and
   the owner commits its outcome. `accepted` records a validated agent ID;
   `pending` retains an unresolved creation ID; `not_sent` records a proven
   pre-launch failure; `uncertain` records a possible launch without proof.
4. `observe` is explicit and only available for an accepted exact agent ID.
   One bounded `show` supplies an observation. The owner commits its state,
   ask/answer, truncation flag, tool count, fingerprint and local audit time.
   Repeated identical content updates seen metadata without a second record
   or speech claim. Different same-length answers remain different content.
5. Trusted `claim_speech` commits one exact text and digest against one stored
   observation before returning a playback grant. A duplicate claim returns
   `claim_uncertain` without another grant, even if only Voice restarted.
   Only an exact qualified `ack_speech` records `reported_spoken:true`.

The launch label is `lamp-<store-id>-<task-id>`; it is a diagnostic hint, not
a Harness idempotency key. A receipt with an already-bound agent or creation
ID on a different task becomes uncertain. The adapter validates the expected
machine, label, engine and ask-mode receipt before accepting a new binding.

At startup, old prepared/queued tasks become `invalidated`; they are never
automatically dispatched. An interrupted `attempting` record becomes
`uncertain`. An interrupted observation becomes a failed read, without a new
launch. An outstanding speech claim becomes uncertain. Accepted, pending,
uncertain and reported-spoken history remains. Old confirmations and speech
attestations cannot cross the newly generated service session.

`cancel` only cancels a prepared or queued local task before its attempt.
It returns `remote_cancelled:false`. There is no remote cancel, close, resend,
follow-up, permission reply, automatic observation retry, or automatic polling.
A person can explicitly request a new observation of an accepted agent.

## Wire interface

Each Unix connection carries one UTF-8 JSON object followed by a newline and
one JSON response followed by a newline. Requests reject unknown and duplicate
fields. Success includes `ok`, `store_id`, `revision` and `completion`;
failure includes `ok:false`, a stable `error` code and a bounded explanation.
Prompts, transcripts, raw CLI stderr and account data are excluded from logs
and bus diagnostics.

Every mutation carries a compare-and-set guard:

```json
{"store_id":"0123456789abcdef0123456789abcdef","revision":3,"request_id":"voice-turn-42-prepare"}
```

Read `status` to obtain the actual store identity, revision and
`summary.session_id`. Generate a new request ID for each intended mutation.
The exact most recent guarded request has a durable receipt; ordinary exact
retries return that receipt without executing again. Claim retries are the
exception above. Internal dispatch/read transitions can advance the current
revision while the original mutation receipt keeps its original revision.
Inspect state after a conflict; never substitute a new guard just to force an
ambiguous mutation through. `status.last_receipt` strips speech text and is
inspection evidence, not a playback grant.

| Operation | Fields besides `op` | Route / meaning |
|---|---|---|
| `status` | none | Counts, bounds, current identities, last receipt |
| `targets` | none | Each target's `alias` and `engine`, in alias order; never its machine or project (since task 41, for the voice) |
| `list` | optional `after_id` (default 0), `limit` (1–8, default 8) | Metadata page; follow `next_after_id` |
| `get_task` | `id` | Exact task, configured target, dispatch and observation metadata |
| `get_observation` | `id`, `observation_id` | One stored observation plus its speech record |
| `prepare` | `guard`, `text`, `target` alias | Immutable proposal; no CLI call |
| `confirm` | `guard`, `id`, `prepared_digest`, `session_id` | Trusted person's spoken confirmation |
| `cancel` | `guard`, `id` | Only before dispatch attempt |
| `observe` | `guard`, `id` | Explicit `show` of the bound accepted agent |
| `claim_speech` | `guard`, `id`, `observation_id`, `text`, `session_id` | Trusted one-time playback grant |
| `ack_speech` | `guard`, `id`, `observation_id`, `claim_id`, `text_digest`, `session_id`, `played:true` | Trusted completed-playout attestation |

The [protocol](../crates/tasks/src/protocol.rs) and
[engine](../crates/tasks/src/engine.rs) define the serialized fields and error
codes. Query the local service without a separate scripting runtime:

```sh
printf '%s\n' '{"op":"status"}' | ./lamp-tasks request --socket /run/lampOS/tasks/request.sock
```

The reference client has a 1.5-second round-trip deadline. Timeout is uncertain
for a mutation. It is not evidence that no commit or dispatch occurred.

## Harness adapter

Only three argv forms are constructed, without a shell:

```text
harness machines --json
harness new codex --machine <full-machine-id> <project> --name <unique-label> --mode auto --json --task <exact-text>
harness show --machine <full-machine-id> <full-agent-id> --chars 4000 --json
```

`--mode auto` (Harness's default) since 2026-10-15, approved by the owner: the
agent works in its own project folder without asking and still stops for anything
risky. With `--mode ask` Claude Code asked before every file it wrote, with nobody
at the computer to answer: on lamp-4ace a two-line poem waited on "Do you want to
create coffee_poem.txt?", which Harness reports as state `unknown`, while the lamp
said "still working" (#770 makes the lamp say when an agent waits for the person).

A launch counts as started only when Harness's reply names the agent with
`permissionMode: "auto"` and a `bypassPermission` of true, false or null;
Harness 0.4.6 reports auto mode as `"auto"` with `true`. Any other mode, `full`
or `ask` among them, leaves the task `uncertain` with `invalid_reply`. Before
this fix (2026-10-15) the check still demanded `ask`, so every auto-mode launch on
lamp-4ace was refused after Harness had started it, and the lamp said it had
not gone through.

The machine is named with `--machine` since Harness 0.4.2 (openharness #1333,
2026-10-14), on `new` and on `show`, and `show` reads the agent on the machine
the task was started on. The `@<id>` form before it is refused ("Name the
machine with --machine <id>."), so a `lamp-tasks` build from before that
change fails every launch with `new_failed`.

The engine allowlist is `codex`, `claude` or `opencode` (OpenCode since task
41: Harness 0.4.2's `harness new --help` names it, and its ask mode passes
OpenCode no flag; see "Who gets the work"). A project is an explicit name
(at most 128 UTF-8 bytes) or an absolute/`~/` folder (at most 512 bytes).
Leading option/target syntax, ambiguous relative folders, control characters
and whitespace around a project are refused. Text is at most 1,000 UTF-8
bytes, nonempty, without NUL and not beginning with `--`, which the inspected
CLI parser treats as another option. The confirmed text is never rewritten.

The executable is resolved once to an absolute canonical regular file, owned
by the service user or root, executable, without group/other write or set-ID
bits. Its identity, size, modification time, mode and owner are checked again
before each invocation. Replacing it requires a service restart. This is not
a signature check or a sandbox for an untrusted executable.

The child gets `AI_AGENT=lampos` and `HARNESS_AGENT_SESSION=<persistent store
ID>`. It receives only explicitly allowed ordinary provisioning variables:
`HOME`, `PATH`, `LANG`, `LC_ALL`, `LC_CTYPE`, `TMPDIR`, `TMP`, `TEMP`, `USER`,
`LOGNAME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`,
`XDG_RUNTIME_DIR`, `ADAPTER_DATA_DIR`, `HARNESS_AUTH_DIR`, `PORT`. `HOME` is
absolute and every PATH entry is absolute. No inherited pane, Claude or Codex
identity, provider key or shell startup environment is forwarded. Tasks never
opens Harness credential/ledger files; Harness itself owns their use.
Each forwarded value is at most 4 KiB, the environment totals at most 32 KiB,
and missing HOME/PATH is refused rather than filled from a different account.
Child stdin is null. Machine preflight accepts at most 256 unique full IDs
with typed `current` values. Oversized observed ask/answer fields are refused;
accepted text retains Harness's `truncated` flag without silent local trimming.

Machine preflight is NDJSON and must contain the exact configured full ID.
The current CLI still has a strict-target gap: if that ID disappears before
`new` re-fetches machines, its resolver can fall back to a matching label.
Returned-ID checking detects the mistake after possible launch. This client
cannot claim atomic exact-machine selection; see the owner qualification list.

Once a `new` process was spawned, timeout, nonzero exit, malformed output,
wrong identity, same-name `existing` heuristic or incomplete cleanup is
uncertain. Human error text is never parsed as proof of non-dispatch. A pending
creation is retained without guessing an agent from its name. The CLI's
creation-ID lookup gap prevents reliable automatic reconciliation.

## Speech and questions

An observation can be read at any later time; it can receive a new speech
claim only within ten minutes of its original observation and within the same
Tasks service session. Re-observing identical old content does not manufacture
freshness or reset its once-only speech history. A claim is not a statement
that audio was played. `ack_speech` records the trusted caller's report of
qualified completed playout, not independent acoustic proof or human hearing.

First must bind a claim to Voice's own durable output identity and check
privacy, active turn, current output authority, interruption, neither listening
nor speaking, and at least 1.5 seconds of quiet at the final output boundary.
No socket receipt, transcript, model admission, generated text or queued PCM
can stand in for that boundary. A lost or interrupted claim remains uncertain;
the component deliberately permits omission rather than automatic repeat.

`needs-input` is only an observed state. The CLI does not supply a stable
permission-dialog ID or type, so this component cannot implement V1's
once-per-dialog permission announcement. If requested, Voice can truthfully
say the agent needs attention in Harness. It must not label every such state
a permission request or approve it by voice. Task output is untrusted data,
not authority to call another tool or override these rules.

Probed 2026-10-15 (#770) on a Harness machine (CLI and daemon 0.4.6,
`--mode auto`, Claude Code): risky actions are not prompted but denied
(Claude Code's auto-mode classifier, e.g. writing in `~/.ssh`) or refused by
the model, so a permission prompt no longer stops a task. What still stops is
a question: `show --json` then says `state: "needs-input"` (about 14 s, until
answered) with no question text (`answer` empty or the previous answer), and
`wait --timeout 1s` exited 124 throughout, never 3. So `needs-input` is taken
from `show`, and the voice says the agent is waiting on the person's computer
rather than voicing a question it does not have.

## Bounds, durability and health

| Resource | Bound / behavior |
|---|---|
| Retained tasks | 16. When full, the oldest task that is over (never sent, expired, cancelled, or its agent finished, stopped or failed) gives way to the new one; only when all 16 are still going (waiting for a yes, being handed over, working, or asking the person) is preparation refused. Until 2026-10-15 a full store refused every new task for good: lamp-4ace stopped handing work over at its sixteenth |
| Observations | 4 distinct records per task; no silent eviction of delivery evidence |
| Text | Task and selected speech: 1,000 bytes each; observed ask: 4 KiB; answer: 16 KiB |
| Local transport | 8 KiB request; 128 KiB response, enough for escaped bounded observation text |
| Clients | 4 workers, 4 queued sockets, 16 queued owner requests |
| Client deadlines | 300 ms complete request, 1,100 ms owner receipt, 1,200 ms total server service |
| CLI | One worker, one job/result slot; preflight/read 10 s each, new 60 s |
| CLI capture | 64 KiB stdout, 16 KiB stderr; individual poll waits capped at 10 ms |
| Private persistence | 8 MiB snapshot, 32 MiB database, 40 MiB journal; 4,096-byte pages |
| SQLite | DELETE journal, FULL sync, fullfsync requested, 2 MiB cache, 50 ms busy wait, no mmap/WAL |
| Diagnostics | Once a second, with early readiness changes; owner stale after 1,500 ms |

There are seven service threads including main: four clients, the storage
owner and the CLI worker. Clients never run Harness. The CLI worker never
holds the ledger. Main publishes the standard `diagnostics` type as `tasks`,
with readiness, CLI-busy, counts, queue refusals and publication failures.
It publishes no task text and no automatic result delivery topic. A stalled
owner loses readiness even while its process still exists. Missing Tasks
does not inhibit healthy Voice; First owns the “cannot reach agents” fallback.

The Store locks its private 0700 directory for the owner's lifetime and uses
0600 single-link regular files owned by the effective UID. It rejects linked
final directories/files, unexpected schema/versions, malformed snapshots,
oversized files and WAL sidecars. Initialization publishes without replacing
an existing database and synchronizes the directory. A commit installs memory
only after SQLite commit and directory sync succeed. Any uncertain write
poisons the owner until reopen; unreadable data is never replaced with empty
history. Cooperating owners are excluded; another process with the same UID
is outside this protection boundary.

Linux confirmation/speech ages use BOOTTIME, including suspend. The host
fallback uses MONOTONIC and an explicit test boot identity. Every stored audit
includes boot, service session, milliseconds and revision; these are not UTC
timestamps. Clock reversal or a boot change inside one owner fails closed.
Physical power-loss durability and the lamp's storage latency remain unproven.

Shutdown cancels the CLI, attempts to kill its local process group and reap
the direct child, and waits a bounded interval for workers. Killing the CLI
does not cancel work already held by a daemon or remote machine. Detached or
privileged descendants are not contained by this wrapper. A blocked storage
system call cannot be forcibly ended by a Rust thread. Executable metadata
validation and synchronous process spawning can also block in the kernel:
the subprocess deadline starts after metadata validation, before spawn, and
is checked once capture can run. It is not an interruptible OS-call deadline.
Post-exit pipe draining has a 100 ms allowance within capture; cleanup has a
separate 200 ms budget. Incomplete cleanup poisons that adapter so subsequent
calls refuse without spawning until the process is replaced. If shutdown reports an
unfinished worker, the supervisor must exit that process and verify its death
and child cleanup before replacement. Do not restart an owner inside the same
surviving process. Terminal error reporting waits at most 10 ms for its one
writer before process exit; a blocked log sink cannot hold that error path.

No history garbage collector or remote close/archive operation is implemented.
Capacity stops admission rather than forgetting ambiguous work or resetting
speech history. Account, local Harness identity, operator target, store reset
and personal-data erasure need First's explicit reconciliation/reset wiring.
The private database contains task and answer text; it belongs in that reset
manifest. Deleting the database is not an ordinary retry strategy.

## Host qualification and first lamp check

Build the process and the standalone check from the `lampOS/` root:

```sh
cargo build --release --locked -p lamp-tasks --bin lamp-tasks --example tasks_check
```

Copy the resulting executables through First's authorized workflow. First's
first check on a persistent, short, private scratch parent is:

```sh
findmnt -T "$HOME"
./tasks_check --scratch-parent "$HOME" --samples 8
```

`tasks_check` launches its **own executable** as the scripted Harness. It
uses a fresh private store per cycle, no installed Harness, account, network,
credentials, Voice, microphones, light or motors. Each cycle checks public
confirmation refusal, one creation, exact observation, one speech claim,
duplicate-claim refusal, an authored playout receipt, restart preservation,
stable caller identity and no resend. Extra scenarios retain uncertainty after
malformed scripted JSON and unresolved pending creation across restart. They
do not simulate a real network losing a remote receipt. The final JSON
must say `all_assertions_passed:true` and `scratch_removed:true`; failures
retain their private scratch evidence.

Reported milliseconds measure the executing machine's real local SQLite,
socket and scripted-process work. Each boundary includes exactly the steps
named in the output. The first sample and subsequent-sample median/maximum
are separate, with every raw value retained. Stores are fresh per cycle;
“warm” means subsequent process/OS samples, not a claim about a warm production
database. Confirmations, boot identity and played receipts are authored.
These timings do not measure remote execution, speech or physical restart.

The [host evidence ledger](verification/tasks-host-20261014/README.md) retains
every failed run and the final gate: formatting, strict all-target Clippy,
**101 tests passed / 0 failed / 0 ignored**, full workspace build, and a fresh
source-only workspace build plus the same 101 passing tests. One release run
on the Intel i7-10700K Mac (macOS 26.6.2, Rust 1.96.0, APFS) completed eight
cycles and the two uncertainty scenarios in 13.998 seconds. Confirmation
receipts were 132.785 ms first, 82.716 ms median of the next seven; status
beside a silent client was at most 11.671 ms across eight samples. Raw values,
exact boundaries, source hashes and all nine timing rows are in that ledger.

The first likely lamp failures are installed CLI JSON/agent-policy
compatibility, its ordinary environment and executable wrapper, storage sync
latency relative to the 1.1-second receipt deadline, and Linux process-group
cleanup. No host result substitutes for those checks.

## First real Harness check (lamp-4ace, 2026-10-14, 21:36)

By hand, not through `lamp-tasks`, with the owner's leave to use their account
and their office computer. Harness 0.3.99 on the lamp (`~/.local/bin/harness`)
and on the office machine (`office`, `m0.local`); `harness machines --json` on
the lamp listed ten machines, the lamp itself among them ("current").

- **Starting works.** On the lamp, `harness new claude @<office's full id>
  lamp-first-task --mode ask --name lamp-test-1 --json --task "<a text-only
  request>"` returned `ok` with the agent's id, `permissionMode: ask`, and a
  new project `~/harnesses/lamp-first-task` on the office machine, 6.2 s after
  it was run. Claude Code answered with one sentence and used no tools.
- **Reading it back from the lamp does not.** `harness show` and `harness wait`
  read only the agents on the machine they run on: on the lamp, `show` by id,
  by name and with the machine named in every form tried said
  `UNKNOWN_HARNESS` ("on this machine"); on the office machine `show
  lamp-test-1` gave the asked text, the answer, `state: idle` and `tools: 0`.
  So `lamp-tasks` can start work on another machine and cannot read its
  answer there. Asked of the owner, for the Harness team: `show`/`wait` for an
  agent on another machine of the same account. The alternative is to run the
  agents on the lamp itself, which needs Claude Code installed and signed in
  there.
- **Reading it back works since Harness 0.4.2** (installed on the lamp by
  its own update, 2026-10-14 evening). On the lamp, `harness show --machine
  office lamp-8b18c0ad6a1fe15fc84148fb81446b16 --json` gave the 22:39 task's
  ask, its answer (three names, 331 characters), `state: idle`, `tools: 0`
  and `startedBy: person`: 2.8 s once, 4.4 s at 23:51:39. The machine's full
  id and the agent's id in place of the two names gave the same. `harness ps --machine office` lists
  that machine's sessions. The old `@<id>` form is refused.
- An agent may close only what it started: `harness close lamp-test-1` from the
  office machine was refused (`NOT_YOURS`); it stays for the person to close.
- **Through `lamp-tasks`, 21:40, a trial by hand** (the CI build `377d975b…`
  in a private folder on the lamp, `serve --enable --run-for-ms 900000`, not a
  service; target `office`, engine `claude`, project `lamp-tasks`): `prepare`
  and the voice route's `confirm` both answered `ok`; 8 ms after the confirm
  it ran `harness new`, which failed in 3 s, and the task was kept
  `uncertain` with `new_failed`, as designed. The same call by hand with the
  adapter's environment says why: `"Agents may not start harnesses: the
  person turns on \"Agents can start other agents\" in Harness Settings ▸
  Experimental, or runs harness settings agents-start-agents on."` The
  adapter marks itself as an agent (`AI_AGENT=lampos`), so that setting is
  the owner's to turn on; the earlier call worked because it carried no such
  mark. Not worked around.

## The voice's side (first builder, 2026-10-14 evening)

`crates/voice/src/agent_tasks.rs`, off unless the voice's unit sets
`LAMP_TASKS_DIR` (the directory holding `request.sock` and `voice.sock`;
`LAMP_TASKS_TARGET` names the alias, `office` unless set). The model is told
of two functions:

- `start_agent_task(task)`: for computer work the person asks to have done
  on the office computer. The voice reads `status` and sends `prepare` on the
  request socket with the target alias. Nothing runs. The model is told to
  say the task back in one short sentence and ask whether to start it.
- `confirm_agent_task()`: the voice, not the model, decides whether it may.
  Only when a task is waiting, in a request after the one that prepared it,
  which the lamp heard rather than wrote itself (`OWN_REQUEST`), within two
  minutes of preparing it. Then `status` again and `confirm` on the voice
  socket with that store's guard and session. The model is told it has
  started and that the answer will be on that computer.

A request whose answer is held back (the busy-room rule) has neither run, as
for the other functions. Harness can now be read on the other machine
(above, since 0.4.2); the voice does not yet ask `lamp-tasks` to read the
answer and say it.

**On lamp-4ace since 2026-10-14, 22:24** (voice `c2ac775f5`,
`LAMP_TASKS_DIR=/run/lampos-tasks`; `lamp-tasks.service` with the build of
`b73dfe6fa`, `--harness-as-person`, target `office`, Claude Code, project
`lamp-tasks`; its target list is in `/var/lib/lampos-tasks/targets.json`, as
the service user cannot enter `/etc/lampOS`, which at first left it
restarting with "Permission denied"). Played at 22:39: "Hey Lamp, can you
ask Claude Code on my office computer to suggest three friendly names for a
desk lamp?" got "I'll ask the code to find three friendly names for a desk
lamp. Shall I start that?" (`prepare` answered in 35 ms); "Yes, go ahead."
got "I've started that task on your office computer." (`confirm` in 38 ms),
and the tasks part started a Claude Code session on the office machine,
which answered there with three names. The answer was not heard on the lamp.

One try before it failed: "Yes, please start it." was ended after "Yes." and
its rest heard as a new utterance; the model's `confirm_agent_task` for the
first arrived after the second had taken over and was dropped
(`UnownedToolCall`), and the model then said it could not start the task. A
yes said in one breath worked.

## The lamp as chief of staff: watching and telling (2026-10-15)

The owner, just after midnight: the lamp is the person's chief of staff at
work and their best friend. It hands work to Claude Code, Codex and the
others like an assistant hands work to staff ("I gave it to Claude Code to
work on it"), and later, unasked and at a good moment, says how it went ("oh
Claude Code just finished the task on xyz. the result is xyz"), "but must be
natural". Watching and deciding cost no model tokens; the realtime model is
used only to say it.

- **Handing over.** After the yes, the model is told to say in a few natural
  words that Claude Code has it and that the lamp will say when it is done.
- **Watching** (`Engine::watch`; on unless `serve --no-watch`). Each accepted task is read
  on its own, with the same `show --machine` as an asked read: every 30 s for
  its first 10 minutes, then every 2 minutes, for at most a day in one boot.
  A read made by the watch keeps an observation only when the agent has
  finished (idle with an answer), stopped, failed, or stopped to ask
  (`needs-input`); otherwise it ends as a failed read with code `working`,
  so the four observations a task may keep are not spent on work in
  progress. Once one such observation is kept, the watch leaves that task.
  An asked read's receipt that a watch read replaces is dropped: a retry of
  that request finds the revision moved on and is refused, never run twice.
  Nothing is read while a launch or another read is in hand.
- **Telling** (`crates/voice/src/task_news.rs`). A thread of the voice reads
  `list`, `get_task` and `get_observation` every 10 s and offers the newest
  settled observation of each task, if it was seen in this boot within the
  last 12 hours and has not been told. The coordinator tells it as a turn the
  lamp begins itself (basis `lamp_tells_news`, record `task_news`) when the
  eyes say someone is at the lamp, or something was said to or by it within
  two minutes; nothing has been said for 15 s; nothing is owed or being
  heard; and the room is not busy. The note gives the model the task's words
  and up to 1,200 characters of the answer, and asks for one or two natural
  sentences starting like "Oh, Claude Code just finished…" ("earlier" after
  ten minutes), the gist in plain words, no code, lists, paths or links, and
  the rest left on the computer. An agent waiting for the person
  (`needs-input`) is told once, by name, as waiting for them on their
  computer, to be answered there; the model is never asked to voice a
  question the observation does not contain. With no answer text it is told
  not to ask or guess one; with text (which may be the previous answer) it
  may pass it on only if it is a question to the person. The same words
  (`task_news::waiting`) answer `check_agent_task`. A stop is said plainly. Each piece is told once: its key
  (store, task, observation) goes into `/var/lib/lampOS/voice/tasks-told`
  (`LAMP_TASKS_TOLD`), read again on each pass.
- **Asked** (`check_agent_task`). "How's it going?" or "what did Claude Code
  say?" is answered at once from what was last seen, with no new read; a
  finished answer heard this way is kept as told, so it is not told again
  unasked.
- **Progress** is #732: `show` is empty while an agent works (measured
  2026-10-15, 00:05), so the progress signal will be `ps --machine`, put on
  the bus as typed state, with Jev deciding whether a step is worth telling
  now, and the model used only to say it.

The voice's side is on wherever the tasks part is (`LAMP_TASKS_DIR`). The
watch was `--watch` for its first run on lamp-4ace (00:49 on 2026-10-15) and
is the default since; `--no-watch` turns it off (`docs/joining.md`).

### The first run end to end on lamp-4ace (2026-10-15, 00:49 to 00:56)

`lamp-tasks` `45a1e4d2c` with `--watch` and the voice `55e92e3d8`, both
installed at 00:49. Played at 00:50:48: "Hey Lamp, can you ask Claude Code on
my office computer to suggest three names for a cat?" got "Shall I ask Claude
Code to suggest three names for a cat?" (1.75 s after the end of speech);
"Yes, go ahead." got "Done, Claude Code's on it. I'll let you know when it's
finished." (1.82 s). Three faults, all mended the same night:

- **The hand-over failed and the lamp said it had not.** `harness new` ended
  `new_failed`. By hand: since 0.4.2 a project *name* that already exists
  is refused ("A project with that name already exists. Name its folder
  instead."), and the 22:39 task had made `lamp-tasks` on the office Mac.
  lamp-4ace's target is now the folder, `~/harnesses/lamp-tasks` (started by
  hand from the lamp with it at 00:55: `ok` in about 2 s). The voice is told
  that it is handing the work over now, not that it is done, and a hand-over
  the person said yes to that fails is told as news within ten minutes, in
  plain words, with an offer to try again (`task_news::failed_hand_over`).
- **News told to an empty office.** At 00:49:48, 11 s after the restart, the
  lamp told the 22:39 task's names with nobody there: the presence check took
  the start of the service for someone speaking. Now only something a person
  said (an utterance of at least the gate's 30 blocks) in the last two
  minutes, or the eyes, count as someone there.
- The model, given that news, called `check_agent_task` by itself to read
  the answer (the tasks part answered in 19 ms); both kept it as told.

### The whole loop in a played day (2026-10-15, 01:25:42)

With the three faults mended (voice `fd1a273ee` from 01:24:30; the target a
folder), in `~/lamp-voice-data/tools/day.sh`, ten lines played in one voice
at the lamp, nobody in view, the empty office at night:

- "Hey Lamp, can you ask Claude Code on my office computer to write a
  two-line poem about a desk lamp?" → "I can ask Claude to write a two-line
  poem about a desk lamp. Should I go ahead and start?"
- "Yes, go ahead." → "I'm passing that over to Claude Code and will let you
  know when it's finished." (1.84 s after the end of speech)
- 39 s later, unasked: "Oh, Claude Code just finished that poem for you. It
  describes a small bent sun leaning over the page, pouring gold where worries
  disappear." (Claude Code on the office Mac; the watch saw it finished; the
  news was told at a quiet moment because a person had spoken within two
  minutes.)
- "How is that task going?" → "It's finished!", and the gist again.

Every other reply in the day began 1.17 to 1.96 s after the end of speech.
Two things to improve after the MVP: a short piece of writing (a two-line
poem) is described instead of read, and asking right after the news repeats
it.

### The second played day, and a page too long (2026-10-15, 02:54)

The same ten lines on the `lamp-voice` head of 02:53 (`f60d6d6f9`, with
memory across days on). The hand-over went as before ("I'm passing that over
to Claude Code now and will let you know when it's ready."), and Claude Code
answered and the watch kept it, but nothing was told in the 100 s after, and
"How is that task going?" got "I'm sorry, I can't check on that right now."
(`check_agent_task`: "the tasks part did not answer in 3 ms"). The night's
tests had left ten tasks in the store, and a page of eight with their
records is 9,333 bytes, over the voice's 8 KiB bound on a socket answer
(`tools::exchange`, set for the executive); both the news reader and the
check read that page first. Now the tasks part is read with a 64 KiB bound
(`tools::MAX_TASKS_ANSWER_BYTES`), which also covers an agent's longest
answer (16 KiB) with what it was asked (4 KiB). The test of "how is it
going?" now serves a page of over 9,000 bytes.

### News too long for one written turn (2026-10-15, 02:31)

During a two-breath test on lamp-4ace (#740's first try), the voice ended
three times in three, 4 ms after committing news (`telling news from the
work handed over`), with `Gemini Live InvalidConfiguration`, and systemd
restarted it. The likely cause, which the first builder took for the
switch under test: the news is one written turn (`clientContent`), and the
transport refuses a written turn over 1,024 bytes (`wire::MAX_NOTE_BYTES`)
as `InvalidConfiguration`, a refusal the provider worker ends on. The
note's own words are 545 to 585 bytes, and it quoted up to 300
characters of the task and 1,200 of the answer, so any answer of more than
about 400 bytes was over. The poem told at 01:27 was short enough.

Now the note is cut to fit (`task_news::fitted`, the second builder):
the answer first, then the task's words, by whole characters, the answer
then marked "…(it goes on)". A note that fits is as it was, byte for byte
(a test); every note, for long answers and long tasks in English and
Vietnamese, in every state, fits and is taken by the transport (a test).
The same bound holds for any written turn: a note made from other text
(progress news, #732) has to fit in it too.

### A lead-in before a call (#799, off unless `LAMP_TOOL_LEAD_IN=on`)

A turn in which the model calls `start_agent_task` or `confirm_agent_task`
began 1.8 to 2.9 s after the end of speech on lamp-4ace on 2026-10-15, against
0.87 to 1.45 s for a plain answer: the model makes the call in one generation
and speaks in another. With `LAMP_TOOL_LEAD_IN=on` the read-back and its
question come before `start_agent_task`, "giving that to …" before
`confirm_agent_task` and "let me see" before `check_agent_task`, in the same
turn, and each answer's `next` says the person has heard it
([tools.md](tools.md#where-a-turn-with-a-function-call-spends-its-time-799)).
What a prepared or handed-over task is, and what reaches the tasks part, is
unchanged.

## Who gets the work (task 41, #733, 2026-10-15)

The owner: "you are a boss with many employees (claude code, codex, etc) and
lamp assists you in delegating tasks to them". So the person names who gets
the work ("ask Codex to …", "have Claude Code …"), and nobody named is Claude
Code. Off unless the voice's unit sets `LAMP_TASKS_WHO=on` (`--tasks-who`
for the provider worker), for its first minute on lamp-4ace; once that has
passed, it is to become the default, as `docs/joining.md` asks and the issue
says.

**What exists and what was taken.** `lamp-tasks` already took one target per
alias, each with its engine (`claude` or `codex`), 1 to 8 of them; the voice
sent every task to one alias (`office`, or `LAMP_TASKS_TARGET`), and
`task_news` already named Codex from a task's engine. Harness 0.4.2 (the
lamp's version, also on the second builder's Mac, read there with `harness
new --help`, nothing started) takes `claude (default), codex, opencode, …`;
its own launch table (read in the installed `cli.js`, not run) gives
OpenCode `--auto` for auto and no flag for ask, and `--prompt` for the first
message. So OpenCode is now a third engine `lamp-tasks` takes. Whether a real
OpenCode receipt says `permissionMode: auto`, which `lamp-tasks` requires
before it binds a task, is not seen: a receipt that does not leaves the task
`uncertain` (`invalid_reply`) and is never sent again.

**What was built.**

- `lamp-tasks` answers `{"op":"targets"}` on the request socket with each
  target's alias and engine, in alias order, and nothing of its machine or
  project. A read: it runs nothing and writes nothing.
- The voice's provider worker, with the switch, reads that once as it starts
  (waiting at most 300 ms, as for the memory) and again every 30 s on a
  thread of its own, so a tasks part started after the voice, or restarted
  with other targets, is seen by the next session, and opening a session
  waits for nobody. A failed read keeps what was known. The journal says
  `lamp-voice provider: work can go to Claude Code (office-claude), Codex
  (office-codex)` when it changes.
- The staff are the first target of each engine, in alias order (one per
  engine: a second machine with the same engine is not offered yet). With
  two or more, `start_agent_task` takes `who`, an enum of their names, and
  `check_agent_task` takes the same to ask about one of them. The model is
  told the names from the targets, not from a list in the prompt:

  ```text
  start_agent_task: Prepare a piece of computer work for one of the person's agents on their office
    computer (Claude Code and Codex): … then say the task back in one short sentence, with who will
    do it, and ask whether to start it.
  who (enum "Claude Code", "Codex"): Who does it, if the person named one: Claude Code (they may say
    "Claude" or "Claude Code", which can sound like "cloud" or "cloud code"); Codex (they may say
    "Codex", which can sound like "codecs" or "code X"). Leave it out if they named nobody: then
    Claude Code does it.
  ```

- Nobody named is the target the voice was given (`LAMP_TASKS_TARGET`) if
  it is one of the staff, else Claude Code, else the first.
- **"Codex" heard as "codecs", "Claude" as "cloud"** (the issue's most likely
  failure). The enum and its description are the first guard. The second is
  in the voice: a name the model gives is taken by its letters and digits in
  lower case (so "Code X" is "codex"), and only when it is one member's
  name, a close form of it, or its alias, and nobody else's: `claudecode`,
  `claude`, `cloudcode`, `cloud` for Claude Code; `codex`, `codecs`, `codec`
  for Codex; `opencode` for OpenCode. None of these is close to another
  engine's, and a form of an engine that is not among the staff names
  nobody. Anything else ("code", "Cursor") prepares nothing and sends
  nothing to the tasks part; the model is told "Nothing was done: nobody by
  that name works here. Ask the person who should do it: Claude Code or
  Codex."
- The hand-off names the right one: after `prepare` the model is told
  `"where": "Codex on the office computer"` and to say the task back
  "saying Codex will do it"; after the yes, `"who": "Codex"` and "Sure, I'm
  giving that to Codex". The news, a failed hand-over and
  `check_agent_task` name it from the task's engine (`task_news::who`, now
  with OpenCode).

**Without the switch, nothing changes.** Tests show that with it off, or on
with the staff not yet read, or on with one target `office` for Claude Code,
the three functions declared are exactly the ones before (compared with a
copy of the old declaration), and the lines sent to the tasks part (`status`,
`prepare` with target `office`, `status`, `confirm`) and what the model is
told back are exactly the same, even if a `who` is given. The voice sends no
`targets` request without the switch.

**Tests** (host only, the second builder's Mac, Apple M2 Max, macOS 26.6.2,
Rust 1.96.0): in the voice, `agent_tasks` has 14 tests, 9 of them new: the
old declaration without the switch, the same lines and answers without it,
naming Codex, "codecs", "cloud code", nobody and a blank name, names that
are none of the staff, close forms never crossing engines, the staff read
from `targets` (one per engine, unknown engines left out, an older tasks
part that refuses `targets`), the switch's flags, and asking about one of
them. In `lamp-tasks`: `targets` in the protocol, the engine (alias and
engine only, nothing written) and over the socket, and an OpenCode launch's
argv and receipt checks. The gate, from `lampOS/`: `cargo fmt --all
--check`, `cargo clippy -p lamp-tasks -p lamp-voice -p lamp-voice-eval
-p lamp-memory --all-targets -- -D warnings` (`lamp-memory` now tests
against the voice), `cargo test -p <crate> -- --test-threads=1` for each,
and `cargo build --workspace`, on this branch rebased on `lamp-voice`
`1337e7e1d`: `lamp-tasks` 107 passed; `lamp-voice` 1,065 passed, 6 ignored;
`lamp-voice-eval` 284 passed; `lamp-memory` 147 passed, 5 ignored. Three
ring start-up tests far from this change failed in the whole run with the
Mac loaded and passed run alone three times out of three; they are counted
above as passed. Earlier runs failed other ring start-up tests, one
reply-buffer test and one head-motor test the same way, each passing alone.
The copy of the old declaration in the tests is token for token the one
`lamp-voice` declares there.

**lamp-4ace's `targets.json`** (`/var/lib/lampos-tasks/targets.json`, owned
by `orangepi`, mode 0600). `OFFICE_ID` is the office machine's full id, the
one today's `office` target already holds; it is an account's identifier,
so it is not written here:

```json
{"version":1,"targets":{"office-claude":{"machine_id":"OFFICE_ID","project":"~/harnesses/lamp-tasks","engine":"claude"},"office-codex":{"machine_id":"OFFICE_ID","project":"~/harnesses/lamp-tasks-codex","engine":"codex"}}}
```

Each is a folder, not a project name: since Harness 0.4.2 a project name
that already exists is refused (above, "The first run end to end"). Claude
Code keeps today's `~/harnesses/lamp-tasks`. Codex gets a folder of its own,
which step 1 below has Harness make for it (`harness new codex …
lamp-tasks-codex`), so that it is a folder Harness made for Codex, which
Codex should not ask to trust, and the two agents never work in one folder
at once.

**The played run on lamp-4ace** (first builder), smallest step first:

1. **Codex by hand.** On the lamp, as `orangepi`, one text-only task to Codex
   in auto mode, which also makes Codex's folder `~/harnesses/lamp-tasks-codex`
   on the office computer (a new project name, so Harness makes it):

   ```sh
   harness new codex --machine office lamp-tasks-codex --name lamp-codex-check --mode auto --json \
     --task "Reply with one sentence: what is a good name for a desk lamp?"
   ```

   The receipt must say `"engine":"codex"` and `"permissionMode":"auto"`
   (anything else: `lamp-tasks` would keep the task `uncertain`; stop here and
   write down the receipt). After about 30 s, `harness show --machine office
   lamp-codex-check --json` must say `state: idle` with an answer and not
   `needs-input` (the issue's second most likely failure). Then the same once
   more with the folder, as `lamp-tasks` will start it: `harness new codex
   --machine office ~/harnesses/lamp-tasks-codex --name lamp-codex-check-2
   --mode auto --json --task "…"` must also answer `ok` and not ask to trust
   the folder. (With `--machine`, a `~/` path is read under the office
   computer's home.) Codex must be installed and signed in on the office
   computer for this.
2. **The minute with the switch off.** Install this branch's `lamp-tasks` and
   `lamp-voice` from its green ARM64 run, change nothing else, restart both.
   `printf '%s\n' '{"op":"targets"}' | lamp-tasks request --socket
   /run/lampos-tasks/request.sock` answers `office` / `claude`; a task by
   voice goes to `office` as before.
3. **Room in the store.** `printf '%s\n' '{"op":"status"}' | lamp-tasks
   request --socket /run/lampos-tasks/request.sock`: `summary.retained` must
   be 13 or less. The store keeps 16 tasks, every prepared one included, and
   has no clean-up; this run prepares three.
4. **Switch on.** Keep the old file, write the new one with the id it holds,
   and restart the tasks part (its watch is on unless `--no-watch`, so the
   news comes unasked):

   ```sh
   cd /var/lib/lampos-tasks
   sudo cp -p targets.json targets.office.json
   ID=$(sudo grep -o '[0-9a-f]\{32\}' targets.json | head -1)
   printf '{"version":1,"targets":{"office-claude":{"machine_id":"%s","project":"~/harnesses/lamp-tasks","engine":"claude"},"office-codex":{"machine_id":"%s","project":"~/harnesses/lamp-tasks-codex","engine":"codex"}}}\n' "$ID" "$ID" \
     | sudo -u orangepi tee targets.json.new >/dev/null
   sudo chmod 600 targets.json.new && sudo mv targets.json.new targets.json
   sudo systemctl restart lamp-tasks
   printf '%s\n' '{"op":"targets"}' | lamp-tasks request --socket /run/lampos-tasks/request.sock
   ```

   The last line must list `office-claude` / `claude` and `office-codex` /
   `codex`. Then add to `lamp-voice.service`, beside `LAMP_TASKS_DIR`:

   ```text
   Environment=LAMP_TASKS_WHO=on
   Environment=LAMP_TASKS_TARGET=office-claude
   ```

   (`LAMP_TASKS_TARGET` keeps tasks going to Claude Code in the moment before
   the staff are first read), `sudo systemctl daemon-reload && sudo systemctl
   restart lamp-voice`, and look for `work can go to Claude Code
   (office-claude), Codex (office-codex)` in its journal.
5. **One task to each, said in one breath each:**
   - "Hey Lamp, ask Codex to suggest three friendly names for a desk lamp."
     The read-back names Codex; "Yes, go ahead." gets "I'm giving that to
     Codex".
   - "Hey Lamp, have Claude Code write two short lines on why a desk needs a
     lamp." The read-back names Claude Code; "Yes, go ahead."
   - "Hey Lamp, can you get someone to suggest a name for a plant?" (nobody
     named): the read-back names Claude Code; say "No." Nothing starts.
6. **Read what went where:** `printf '%s\n' '{"op":"list"}' | lamp-tasks
   request --socket /run/lampos-tasks/request.sock` (eight a page; then
   `{"op":"list","after_id":N}` with its `next_after_id`) gives each task's
   `target_alias` and dispatch state: `office-codex` accepted, `office-claude`
   accepted, `office-claude` prepared (`invalidated` once its two minutes
   pass). The voice logs function names only, never arguments, so this is
   where `who` shows.
7. **Both answered, both told back.** Within a few minutes, sitting at the
   lamp and saying nothing, it should say unasked "Oh, Codex just finished…"
   and "Oh, Claude Code just finished…" (`task_news`, record `task_news`).
   Then "How's Codex doing?" should be answered about Codex's task
   (`check_agent_task` with `who`).
8. **Going back:** `sudo cp -p targets.office.json targets.json`, take the two
   lines out of `lamp-voice.service`, daemon-reload, restart both.

Keep the read-back sentences, the `list` output and the two journals
(`journalctl -u lamp-voice -u lamp-tasks --since …`) in the verification
folder, with no task text that is private.

**Where it is most likely wrong on the lamp.** The model choosing `who` from
speech: the enum, its hints and the close forms are tested only as text on
the host, and the model hears sound; a mishearing outside the close forms
("clawed", "code") prepares nothing and has the model ask, which costs a
turn. The read-back may still garble the name (on 2026-10-14 it said "the
code" for Claude Code). Codex in ask mode may stop to ask permission even
for a text-only task, and then the news says Codex needs the person instead
of an answer. A Codex receipt or `show` reply shaped otherwise than the
fixtures leaves the task `uncertain` or unread. Codex may ask to trust its
folder when `lamp-tasks` names it (step 1 checks this by hand). OpenCode is
not in lamp-4ace's file: it needs OpenCode installed and signed in on the
office computer, and its receipt checked by hand first, as step 1 does for
Codex.

**Not shown.** No task was started anywhere for this; no Harness session,
receipt or answer from Codex or OpenCode was seen. Nothing was run on a lamp.
How often the model names the right one from speech is not measured.

## Real Harness and Voice qualification still required

After the account owner authorizes a disposable task and confirms its exact
target, First must check the following against the installed versions:

1. Record CLI and daemon versions on the lamp and target, chosen engine and
   existing permission configuration. Verify generic outside-agent identity,
   agent-start policy and the provisioned absolute executable/environment.
   Preserve a real ask-mode refusal; do not escalate mode to make it pass.
2. Obtain owner support for strict full-machine-ID dispatch, or explicitly
   resolve the documented resolver race before enabling real task delivery.
   An exact preflight alone is not that proof.
3. Use a harmless, reversible task in an explicitly selected project. Confirm
   the person's exact spoken yes precedes the attempt, the immediate local
   feedback leaves Voice available, and the returned child/target match.
4. Interrupt local transport/processes before and after launch receipt. Verify
   the remote task may continue while the lamp retains uncertainty and issues
   no second `new`. Exercise installation-pending and the creation-ID lookup
   gap without guessing a name or reading Harness's private ledger.
5. Read an old answer, a changed same-length answer, needs-input, stopped state,
   missing/deleted-agent read failures,
   missing/rotated Harness ownership ledger and engine restart. None may become
   completed work merely from text or activity state. Completion/group replay
   and focused-agent/follow-up support need an owner-supported extension.
6. Wire Voice's private attestations, output cancellation and once-only claim
   handling. Test privacy, busy/listening/speaking state, quiet interval,
   ten-minute staleness, interrupted output, lost acknowledgements and separate
   Voice/Tasks crashes. Qualify actual played speech independently.
7. Measure cold/warm real CLI, durable receipt and output boundaries on the
   lamp. Check suspend, reboot, flash failures and user-performed power loss;
   reconcile reset/account changes and retained private history.

## What has not been shown

No live Harness task, account request, credential access, remote execution,
permission dialog, focused-agent resolution or Voice output is part of Third's
qualification. Source inspection and scripted tests do not establish installed
0.3.94 compatibility, atomic target selection, pending recovery, correlated
completion, a new permission sandbox, exactly-once human hearing, real flash
power-loss safety or lamp latency. Those gaps remain explicit; nothing here
changes the running lamp until First opts into and qualifies the integration.
