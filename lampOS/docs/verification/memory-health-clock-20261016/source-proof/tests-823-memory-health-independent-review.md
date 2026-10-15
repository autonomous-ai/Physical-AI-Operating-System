# Independent Memory health migration review

Recorded 2026-10-16 03:12:39+0700 from `date`. Reviewed held `/private/tmp/lampos-tests-memory-progress` source only after the author's stable-source handshake. No repository edits, Cargo/build/test execution, Git, network or devices. Root reports the unchanged original seven-case baseline completed in 39.498 seconds and all 35 new helper cases passed after the separately recorded new-test park-gate fix. This review did not execute or reclassify those gates and makes no after-migration runtime or speedup claim.

No concrete source blocker was found. The migration keeps the intended five controlled live children and four ordinary host-timed children, and its scheduler receipts remain separate from actual reports, file evidence and child status.

## Independent preservation proof

The original source was copied before implementation as `health.original.rs` with SHA-256 `31add261e22dda578ca5f5c0b9016a1382f7a1e3d87f6968a1d083ec722e394d`. The reviewer wrote and ran a separate exact-reversal script, `audit_migration.py`; it did not depend on the author's reversal result.

- All seven test identities remain exact. All 33 assertion macros in the scenarios/shared retained-count body are byte-identical, in order. The complete scenario tail, including request literals, obstruction operations, backend/fault loops, attributes and comments, reconstructs the original byte-for-byte by reversing only four constructor changes plus the sustained-loop Window binding, log call, two cadence reads and one wait call.
- Both host-only bodies are entirely byte-identical: the environment-only two-backend test retains its full 2200 ms window and 100 ms sleeps; the failed-open two-backend test retains actual status1/reap, no ready file, the full 250 ms subscriber window, allowed absence, received-message checks, and corrupt-byte preservation.
- `now`, `ask_raw`, `ask`, `ok`, `value`, `number` and `private_file` are byte-identical. Request JSON/line framing, real UNIX timestamps and the real three-second socket read/write bounds therefore remain unchanged. The original argv/environment/stdio builder block is also byte-identical; only explicit executable selection and controlled bootstrap follow it.
- There were 42 assertion sites in the full original file and 43 now. The extra helper assertion rejects an attached controller slot. All original assertion macro bytes reconstruct exactly after **only two** documented diagnostic substitutions, `self.log()` to `self.raw_log()` in readiness and exit timeout messages. Their predicates, expected values and bounds are unchanged. This is not an unqualified all-macros-byte-identical claim.

Proof files are `/private/tmp/tests-823-memory-health-independent-evidence/migration-proof.json`, `helper-proof.json`, `health.diff`, and `scenario-tail.reversed.rs`.

## Child selection and ownership

`health.rs:114–166` routes ordinary `start` to the original executable and explicit `start_controlled` to the dedicated shared-body fixture. The four controlled call sites comprise retained-count Legacy + Days (two children), failed Legacy writes (one), initially unusable Bus (one), and sustained Bus failure/recovery (one): five controlled children. The two original constructor sites each retain their backend loop: environment-only two plus failed-open two, four host children. No runtime policy chooses based on environment, store state or report contents.

Scratch, Reports and Process explicitly share one local Rc/RefCell slot (`:32–96`). A Reports created before startup sees later attachment; dropping/recreating a subscriber leaves the Process/Scratch controller alive. The returned actual Child is placed in Process before controller publication (`:159–166`). Controller errors stay as Some(failed Controller), propagate to a test failure, and never become a None/host fallback. Slot clearing occurs only during actual Process teardown, after the existing kill/wait (`:298–304`).

RefCell borrows are confined to `control_window`, `with_control` or a pure cadence read. Every formatting/error path runs after that helper returns, using `raw_log`; no synchronizing log call recurses through an outstanding control borrow. Cleanup retains real owned-child kill/reap, with its accepted non-preemptible kernel limitation. Scratch remains the last directory owner in every unchanged scenario scope.

## Real progress and report ordering

`Reports::recv` is exactly a delegation to the actual Subscriber (`health.rs:70–74`). `report_when:353–388` creates one Window, settles current time without advancement, then invokes actual zero-time receives. Only the `None` branch may obtain next_due and advance. Every Some is type-checked, parsed, name-checked and tested by the original predicate; unmatched messages update the original last-report diagnostic before another actual receive. No Checked value supplies a Diagnostic or delivery count.

The unchanged Bus implementation (`bus/src/subscribe.rs:585–615`) drains the real socket queue first and preserves latest-file fallback; its fallback rechecks the queue before admitting a kept message. Therefore this migration preserves queue order rather than replacing it with receipts. The parent performs every next Step only after its real stderr marker has been seen, and exact-due steps complete actual Reporter work. The final logical-window endpoint still gets an actual receive before a prohibited further due step.

The unchanged request path supplies a real preceding completion boundary: `memory/src/server.rs` performs the handler, State finish/count accounting, original answer logging, and actual socket send in that order. The test's successful response is after the relevant State update and log write. No extra status/probe request is inserted to make a report appear, so answered accounting and the original requests remain meaningful.

## Readiness, logs, and full sustained window

`health.rs:221–245` retains actual Child::try_wait, exact `ready\n` bytes and the original 8-second host guard plus 5-to-50 ms presence polling. Its controlled Window is created at helper entry and reused for the initial settle only after the real ready file is observed. Connected is never readiness. Initial NotAttempted remains valid because server readiness publication precedes State::serving.

`log_in:190–192` settles under its supplied Window, which validates Checked and waits for the actual complete marker in the regular stderr file before `raw_log` reads it. `logged:265–294` retains one Window for the whole positive-log loop, inspects real bytes, checks the actual child/host guard, and advances to the next due only while the expected text is absent. The host branch keeps its original polling. `exited:248–262` continues using actual try_wait/reap and actual ExitStatus; neither EOF nor a control receipt replaces it.

The sustained failure case (`:606–631`) retains one Window across both all log barriers and all 200 ms advances. `wait_cadence_in:207–218` computes a checked target and calls advance_to with **that same Window**, avoiding the independent-wait convenience method that would create fresh budgets. The full `REPORT_EVERY * 3` interval is unchanged. Every due instant is split by the already reviewed Controller, so three additional full Reporter cycles complete while the obstruction remains. At the final zero-left iteration, the original status request, answered increment, obstruction check, same-Window real log barrier and exact FAILED/RECOVERED assertions all occur before break. The bus obstruction is removed only afterward. This source proof preserves the loop body and dynamic answered accounting; it does not claim the old host-driven loop had a fixed iteration count.

## Limits and held hashes

The eight-second control/marker deadlines exclude preemption of actual filesystem and kernel kill/reap calls, as expressly accepted by root and documented in the new section. State handler duration/count-age clocks, UTC, subscriber leases, request timestamps and original ignored timing tests remain real. The 1744-byte log-capacity proof is restricted to the specified successful fixed-size interval; it is not a guarantee for arbitrary errors or flooding. Original seven-case real-process execution, full gates and paired timing remain root-owned and are necessary before runtime qualification.

- `lampOS/crates/memory/tests/health.rs`: `0d829067a29a9e5105326b1e0b50c09436e6aefe5091849a683be282910f9d91`
- `lampOS/docs/memory-fixture-clock.md`: `22fc06ead6ad7e0a8d5a1791adf74a340be755c91ddb3d0ec9d4f7a5a6b60de4`
- `lampOS/crates/memory/tests/support/health_control.rs`: `a06cb3e241ac901f5729eefe89abedf96cf5c8f7eb580bfcc715540ee67f97db`
- `lampOS/crates/memory/src/fixture_protocol.rs`: `21f2d5ca2fc309c8b63989ff6a889f37edd5e3eb8a4fc5de305348d11a0efb0c`
- `lampOS/crates/memory/src/fixture_schedule.rs`: `aa4cd9fb4c59fc82011131a3461870efe8eef43af74186fdb606bb18e58f54f6`
- `lampOS/crates/bus/src/subscribe.rs`: `30f1ef9e2e5ddd732ccb89a746e290b53a52ac7a44ccef488eb43b70c96217c2`
