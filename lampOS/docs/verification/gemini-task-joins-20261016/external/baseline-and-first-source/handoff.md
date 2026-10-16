Gemini task-join preparation — source review handoff

Recorded from date: 2026-10-16 09:05:13 +0700.

Worktree: /private/tmp/lampos-tests-gemini-task-joins. Root-supplied branch:
tests/823-gemini-task-joins. Authoritative base:
1fb07c6f57ee280a49de7b4ddd74008c2a32a20d.

Status: source-only preparation, frozen for independent review. The clean base
compiled successfully. Changed-source compilation, lint, test listing, test
execution and workspace build have NOT run; this is not PR-ready evidence.
Only pinned Rust 1.96.0 source formatting ran after edits (both exits 0).
No Git/GH/network/device action, Voice/Cargo edit or target cleanup by this agent.
The reserved target remains /private/tmp/tests-823-rebase-target; no process is
currently using it on this agent's behalf.

Review proposed.diff (SHA 4f79ebfd96a8f56e5cc93e4e03c9caeb24d5b27f5d11175150c4349633bf624f) and source-freeze.json.
The six changed paths are exactly the authorized four existing Gemini/doc files
and two new task_joins files. All 7,146 unowned source inputs remain byte-exact.
The 652 existing Rust files containing test attributes are byte-exact; no
owned existing Rust file contains an inline test body. External original test
modules, assertions, thresholds and ignored flags have not been edited.
The 18 additions are enumerated in added-source-identities.json; expected compiled
delta is 2,708 to 2,726 with the same 15 ignores, pending actual compilation.

Implementation and default reversal

- testing.rs:311 adds the explicit feature-gated factory, returning ordinary
  Supervisor<FakeConnector>, dial receiver and one TaskJoins collector. Existing
  factories select JoinSource::Plain, including feature-enabled builds.
- session.rs:703 keeps the actual setup/Actor body; registration is reserved at
  line 835, before the original tokio::spawn(actor.run()). Ordinary builds retain
  JoinHandle<()>; feature/test builds delegate through UnitTask. Connection Drop
  at 636 remains byte-exact: input.shutdown(), then task.abort().
- recovery.rs:384 adds the opt-in Dialing adapter. The existing direct start body
  retains its original attempt.await, completion stamp, failure flag, result and
  spawn order. The opt-in task executes that same sequence, then synchronously
  sends the original AttemptResult to one bounded oneshot, with no later await.
  Its actual output is unit. No second task, reaper or scheduling layer exists.
- AttemptTask at 431 first polls the real JoinHandle; the original consumer then
  receives the same AttemptResult, or the same owning JoinError. No observer fault
  is converted into a protocol result. Missing result after a successful real
  join is a private structural invariant, not a fabricated transport error.
- Dialing Drop at 423 is byte-exact (abort). AttemptTask Drop at 475 releases the
  result receiver before its UnitTask field releases the retained handle. This
  synchronously drops any successful unharvested Connection, preserving shutdown
  and Actor abort even though the fixture keeps the completed unit task for join.
  If send races receiver drop, Tokio oneshot send fails with the original value,
  which drops within the actual Dialing task before its Ready result.
- Supervisor's fixture_owner is its last field (678), dropped after all original
  fields. Constructor/redial pass the installed source, preserving all original
  connect, timestamp, poll/select, shutdown and field-drop ordering. No original
  progress/select/drain/Actor serve body or policy is changed.
- The collector uses immutable task/session/kind identities and at most 64 lifetime
  registrations, never reused. It retains actual handles/results only. Consumer
  joins record a borrowed classification; abandoned handles yield actual owned
  Result<(), JoinError> to the fixture. Abort requests are separate data.
- finish borrows the collector, so canceling a wait retains its handles. It waits
  for owner release AND every registered task's actual join (or an explicit fault).
  A running Dialing task can still admit an Actor after owner release; the join
  fence covers that admission. It neither shuts down owners, drains output, polls
  the Supervisor nor advances time. Report creation retains slot identities so a
  consumed registration can later drop safely. Collector Drop is best effort only.
- Overflow, closed tracking, missing reservation or poisoned registry latch an
  incomplete fault while preserving the real consumer result. No panic/cancel
  category is inferred as success. Task success is distinct from protocol success.

inverse.diff SHA 7364240494660bdc53a2739aa0ac7379c82dafe57397043563720cc76e8e0843 was applied only to the copied
external after/ tree using patch --batch -E -p1. It restores all four existing
files exactly to the clean baseline and removes both new files. This is the exact
mechanical reversal. The first inverse checker used macOS patch without -E;
patch exited 0 but left both deletion paths as empty files, causing the checker's
absence assertion to exit 1. Its unedited runner/log/tree and a dated explanation
are retained. The separate corrected inverse check exits 0. No source or test
failure was concealed or rerun.

Clean baseline evidence and commands

Both pre-edit captures are preserved:
- 69bae: /private/tmp/tests-823-gemini-task-joins-evidence; list completed 08:42:46,
  2,688 identities / 15 ignores / 88 binaries; 7,148 inputs unchanged.
- Current 1fb: this directory; metadata 08:49:52 exit 0, list 08:53:29 exit 0,
  source proof 08:53:31; 2,708 identities / 15 ignores / 88 binaries; 7,150 inputs
  unchanged. Raw list SHA 96e79093b582c6c1c83cb0f126de7adf2a0fa287376bd35ac7df830845b3584d.
  No baseline tests executed.

Cargo metadata derived the full workspace reverse dependency closure, including
dev/build edges: lamp-gemini, lamp-memory, lamp-surround, lamp-voice,
lamp-voice-eval. affected-graph.json preserves exact package IDs, edges and args.
Both captures used Rust 1.96.0, jobs 2, incremental 0, locked/offline and the reserved
rebase target. Exact environment/commands/times/exits are in clean-source.json,
baseline-gates.json and baseline-completed.json.

Baseline command, cwd worktree/lampOS:
cargo nextest list --locked --offline -p lamp-gemini -p lamp-memory
-p lamp-surround -p lamp-voice -p lamp-voice-eval --message-format json

After source review, proposed commands (NOT RUN): retain this identical package
selection for Clippy, lists and execution so feature unification stays comparable.
First cargo fmt --all -- --check; cargo clippy --locked --offline --all-targets
with all five -p selectors -- -D warnings; nextest list as above and prove all
2,708 old identities/ignore flags plus exactly the 18 declared additions. Verify a
filtered list selects exactly 18 before running once, same graph, filter
-E 'test(testing::task_joins::tests::)', --profile default --test-threads1
--no-fail-fast --retries0. The root decides subsequent complete affected gate and
workspace build. No authorization for those changed-source commands is assumed.

New causal coverage

Actual in-memory protocol/owner cases cover pending setup cancellation, ready
shutdown, unharvested successful Connection drop before collector polling, protocol
failure with a successful task join, consumed and abandoned Dialing panics, actual
Actor panic with original payload, ordered buffered PCM after terminal state,
reconnection preserving the earlier answer and explicit capture clock, capacity
fault, closed collector, independent collectors. Owned actual task cases cover
identical panic payload/JoinError task ID, original consumer cancellation, canceled
finish with retained handles, Actor admission after owner release but before the
Dialing join, poisoned observation preserving the real result, lifetime bound and
no slot reuse. Each async case has one fixed five-second deadline on unpaused
Tokio (host monotonic time); no sleeps, retries or clock advances. These are source
coverage claims only, awaiting execution.

Planning-source correction and remaining blockers

The e5aa read-only plan was historical. Fresh 69bae additionally contained Gemini
withdrawn-call tracking, so the earlier assumption that only Records differed was
incorrect. Both that tracking and the later 1fb has_room/input_has_room additions
are preserved. The 1fb update also includes Voice/provider changes; every current
byte outside these six files was frozen anew, not copied from an old prototype.
No prior execution evidence is relabeled as current-base evidence.

This slice supplies actual Actor/Dialing completion after original shutdown. It
does not prove live Actor progress at a proposed clock frontier, provider outer
worker return, fixture service task join, or child-process exit. EarlyEnd still
needs the reviewed provider/transport progress fences plus an actual Actor live
work barrier and separately owned service/worker/child completion. No original
frozen 123 scenario, assertion, bound, clock domain or requirement changed here.
