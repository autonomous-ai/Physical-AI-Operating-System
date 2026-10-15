# #909 ARM64 Body stop-fixture triage

Recorded from `date`: 2026-10-16 03:46:17 +0700. Read-only source/log review; no execution or repository edits.

The retained failure is a terminal-state rejection during `ready()`, before the failing iteration requests Stop. It is not a failed stop-latency assertion. The underlying lifecycle cause is unproven.

- Run [38801026642](https://github.com/autonomous-ai/lampOS/actions/runs/38801026642), head `188d97b7fcb6a78adaec67a3d3939843f759e1d3`; shard 4 job `116649786983`.
- Exact test: `lamp-body runtime::supervisor::tests::inert_child_stop_timing_samples_are_host_ipc_only` (definition at `lampOS/crates/body/src/runtime/supervisor/tests.rs:498`).
- Panic: that file `151:14`, the `send_control(Control::Authority { snapshot: ... }).unwrap()` inside `ready()`; error `InvalidData: motor worker is stopping`.
- Retained nextest result: 0.076 s for the complete failing test; shard 4 stopped at 102/1327 (101 passed, 1 failed; 1225 not run after failure). Other three test shards passed. This test is outside the original 123-test #823 inventory; do not add it to that denominator.

Observed source path

1. The test runs 12 ordinary inert child attempts (lines 501–519). Each calls `ready()` at 503, then records the real SystemClock time, requests Stop, consumes real receipts/reap, asserts clean exit (517), and finally asserts exactly 12 inhibition samples (520). The log does not identify which attempt failed.
2. `ready()` calls `poll().unwrap()` (146), then sends authority whenever connected and raw `ready` is false (147–151). It does not inspect latched failure/exit before sending. `send_control` rejects if any of exit, killed_at, or stop_at is present (`supervisor.rs:341–342`). Thus the observed error alone does not show which terminal state occurred.
3. `poll()` enforces deadlines both before and after processing at most one actual receipt, and tries the actual child status (`supervisor.rs:539–584`). `enforce` can call `fail` without making the returned receipt result Err; `fail` clears ready and latches a reason (`387–397`). Child exit and some receipt-driven stop transitions can likewise coexist with an Ok poll result.

Hypotheses, not established causes

- A scheduling gap can make a real queued Ready event at least 50 ms old when observed. The event validator sets ready/last_observed from its original timestamp (`432–439`); final enforcement then hits `PROGRESS_DEADLINE_US=50_000` (`428–429`), kills/clears ready, and the helper attempts the now-forbidden authority send. This is a precise source-consistent path to line 151, even though the receipt itself was delivered and the startup bound is 3 seconds. Ready uses the worker iteration-start timestamp (`worker.rs:432,447–449`), while the packet has its own later send stamp, so publication scheduling can contribute too.
- A child that exits early or publishes Inhibited/Fault before Ready can also leave terminal state after an Ok poll (`supervisor.rs:466–476,502–507,561–574`). The inert child uses the real Worker/core and host clock; no output/status/reason in this log selects that path.
- Relevant host boundaries remain: parent/child helper watchdogs 2 s; startup 3 s; progress 50 ms; initial authority 250 ms; authority lease 250 ms; packet freshness 100 ms; child maximum runtime 1 s; worker tick 2 ms/progress interval 10 ms; stop acknowledgement 20 ms. The reported 76 ms whole-test duration does not support blaming the longer ordinary deadlines. Direct receipt validation errors would normally panic at line 146 instead; terminal-state rejection is the observed symptom.

Evidence limits and ownership

The child stdout/stderr are null (`supervisor.rs:232–236`), and this send unwrap prints neither `s.failure()` nor its terminal fields. No receipt timestamps, attempt index or final child status are available. A focused fixture diagnostic that reports the latched lifecycle reason immediately after poll would clarify the next real failure without retries or altered assertions. The current evidence does not justify changing production deadlines, relaxing checks, or claiming a deterministic root cause.

The helper, full test body and relevant supervisor paths blame to `6d81ab49fe5de4aad702bd4f3d35aa4b16c114a6 (original motor path; later renamed to body in 32c279d4c)`. They predate #909. `796969513 → 188d97b7fcb6a78adaec67a3d3939843f759e1d3` has no changes anywhere under Body or IPC. `188d97b7fcb6a78adaec67a3d3939843f759e1d3 → 48b3737d5` changes only `body/src/bus_out.rs` within those crates; all nine selected lifecycle/authority files below remain byte-identical.

#900 is the appropriate adjacent owner to consult: it changes `Supervisor` clock plumbing and `qualification.rs`, overlapping this lifecycle area. However its complete `supervisor/tests.rs` and `worker.rs` are byte-identical to the failed head, and its default test `spawn_command` still installs SystemClock (`#900 supervisor.rs:215–222`). Its reported native-head qualification does not establish a fix for this host-IPC sample fixture. Do not label the failure fixed by #900.

A fixture fix must retain actual private IPC, actual child exit/reap, all 12 clean-exit/sample checks and the truthful host-timing scope. Merely replacing the parent clock would mismatch the child/core/authority clock domains; converting the measured stop path to logical time would also change what this test reports. Any product lifecycle change belongs with the Body owner and requires separate review.

Selected exact-head SHA-256 (complete cross-ref hashes and Git blob IDs are in the adjacent JSON):

| File | SHA-256 at failed head |
| --- | --- |
| `lampOS/crates/body/src/runtime/supervisor/tests.rs` | `9bdc3902f924ea11648acd751aadef5b30e8e4d1a24ed2f3410f608a50c37ce1` |
| `lampOS/crates/body/src/runtime/supervisor.rs` | `103009ea8a3baf449a760cf7f1dc43d0aa0b47d999b585c4084c65ff63ff2a3f` |
| `lampOS/crates/body/src/runtime/worker.rs` | `d126a1b0020f202c22a0a7a9ec64f54dcb6a52d9f6d02e965d61871aa6ab4cca` |
| `lampOS/crates/body/src/runtime/channel.rs` | `471c6cad77547060ec69348ccf08cbb98878463d2038aa34999e4d4f5f3a48ee` |
| `lampOS/crates/body/src/runtime.rs` | `f70ac6cb58b472a85130c8b81eed4258f689afc644de9ffbab98e087f71a0c8b` |
| `lampOS/crates/body/src/control.rs` | `d5502205df3e703a831d013524c2babf6b5398f359130fee2a70c1d4d9efcd17` |
| `lampOS/crates/turn-owner/src/controller.rs` | `b8237eddfd8d17d9c38496aa5b48eac14277bc3ca124da9de8a889440e44788a` |
| `lampOS/crates/turn-owner/src/guard.rs` | `c6273b9e362c6a1ed5c9c33ac0f0a1ff0de96d337390b092dc427324df574985` |
| `lampOS/crates/turn-owner/src/types.rs` | `542f74cb2a13b10d7fab3aae02cdc898e14f285fb83e1fb89a77680d56cc8ed8` |

Raw evidence SHA-256:

- `909-38801026642.json`: `387954af7e1fba08ffd708941831513e7a6d6fe3e367fe9f78129cb05212c0f2` (11746 bytes).
- `909-38801026642.log`: `1b8287532b5741bcf1a45139d5b83d3819ad7528fb4541849d8610977339343e` (1510220 bytes).

Companion files: `909-body-stop-triage.json`, `909-body-stop-failure-excerpt.txt`, and `909-body-stop-line-provenance.txt`. Existing raw run files were read only.
