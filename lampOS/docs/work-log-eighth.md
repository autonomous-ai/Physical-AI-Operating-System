# Work log: eighth

The eighth builder's log, newest first. Role file:
[builders/eighth.md](../builders/eighth.md). Lamp: lamp-75c9.

## Proposed tasks

| Id | Proposal | Answer |
|---|---|---|
| eighth-P1 | A lock kept on each lamp (`~orangepi/lamp.lock`), taken with `lampOS/tools/lamp-lock.sh`, so builders on any machine take turns; tests on a lamp run only under it (#833, pull request #834) | yes, adopted for shared lamps, as a 10-minute lease (first builder, #833, 18:36 and 18:54) |

## Found, not mine

- 2026-10-15 13:30: `cargo fmt --all --check` failed on `lamp-voice` at
  `a125eb3da` in `crates/voice/src/agent_tasks.rs:396` (from `5798f73d3`).
  Clean again on `686a49c09`.
- 2026-10-15 14:40: `.github/workflows/lampos-arm64.yml` triggers on
  `lamp-*` and the branches of second to seventh, not `eighth/**`; the
  eighth dispatches its runs by hand (`gh workflow run "lampOS on ARM64"
  --ref <branch>`).

## Questions

None yet.

## 2026-10-16 03:20 — #783 policy lines — ready; #739 part 2 — draft #900

Ready for lamp-voice: yes (#905); #900 no (draft, waiting on the first builder)

- #905, Part of #783, `eighth/policy-lines` at `780320202`, ARM64 run
  38797666662 green. Kind: tests only. `Running::end` counts only whole
  lines (the tests builder's boundary of 19:00). Gate: fmt, clippy
  `-p lamp-policy`, 162 passed. Loaded, 20-core Linux laptop: the test
  alone 60/60 before and 60/60 after, then 300/300 each 10 at a time; ten
  full serial `lamp-policy` runs 10/10. #783's failure not reproduced here,
  before or after: the runs show only that nothing breaks. Second asked to read.
- #900, Part of #739, `eighth/motor-clock` at `a98fcef66`, ARM64 run
  38794846152 green. Draft: asked the first builder on #739 whether
  `native_head` 120/120 alone and 88/88 in eight full runs is enough, since
  the full runs fail 3 to 22 other host-clock tests (#823, #893). Fifth
  asked to read the body change.
- #834 still waiting for the reviewer.

## 2026-10-16 02:05 — #833 the lamp lock — ready again; #739 part 2 — qualifying

Ready for lamp-voice: yes (#834)

- #834 at `d0743159a`, ARM64 run 38787716982 green. Kind: changes nothing a
  lamp runs. Since 00:35, the reviewer's findings of 00:29 and 00:57: `run`
  stops its command's whole process group (TERM, KILL after 10 s) before
  releasing, on a lost lock, on 540 s without a confirmed renewal (an outage
  or a hung call) and when the command ends; a hung renewal call is stopped
  with its processes; failed writes or removes on the lamp exit 5 and are
  never announced as done; `-J <host>`. Found while testing: 1 in 100
  `run -- true` exited 143 without releasing (`set -e` in the EXIT trap),
  fixed, 0 in 300. Stand-in test 87/87, six passes in a row. lamp-75c9 runs
  3 and 4 (lock files only) in the verification folder.
- #739 part 2, `eighth/motor-clock` at `8816b6e66` (pushed, no pull request):
  `wait_child_exited` probes before its deadline; the preflight self-tests
  warm their script first; the preflight retries `ETXTBSY` (new on Linux,
  2 of 15 `native_head` failed before). Under load 24 to 28 on a 20-core
  Linux x86_64 laptop: `native_head` 60 of 60; five full serial runs, no
  `native_head` failure, but 3 to 15 other timed tests fail in each; those
  not in #823's inventory filed as #893. Ten full runs not achievable at this
  load until #823/#893; running to ten for the `native_head` count.
- The reminders test the seventh gave #739: not reproduced (100 runs loaded,
  200 in parallel under fsync load, 5 full runs).

## 2026-10-16 00:35 — #833 the lamp lock, as a lease — offered again

Ready for lamp-voice: no (#834, until ARM64 run 38776871505 is green)

- Session started on a Linux laptop (x86_64) by the same person; `gh` signed
  in at 00:05; the laptop's key put on lamp-75c9 by the person at 00:17.
- Kind: changes nothing a lamp runs (a tool, its test, joining.md).
- `eighth/lamp-lock` at `1ccfa74b6`, on `lamp-voice` at `416efbc43`, one
  commit: the lease (renew 5 min, expiry 10 min by the lamp's clock, the
  replaced lock logged on the lamp), every step under `flock` in one ssh call,
  exact holder/session comparison, transport failure exit 3, `run` releasing
  only what it took and renewing while it runs, mode 100755. The reviewer's six
  findings answered on #834.
- `lampOS/tools/lamp-lock-test.sh` (stand-in ssh, no lamp): 57 passed.
- lamp-75c9, lock files only, `docs/verification/lamp-lock-lamp-75c9-20261016/`:
  run 1 on a free lamp as the stand-in says; run 2 met nine's real lock and
  refused and waited correctly.
- Two faults, both written in the README. nine found that a lock in the older
  format (no `renewed_epoch`) counted as expired: its `try` replaced ten's live
  lock at 00:18:33 (ten read-only then, its #880); fixed, such a lock is dated
  by its file. And my run 2 set `renewed_epoch` to 1 by hand without looking
  whose lock it was: it was nine's, and a stand-in holder took and released it
  at 00:19:50. nine and ten told at once through their sessions; restoring
  nine's lock from my side was not permitted. A test on a real lamp no longer
  writes the lock file by hand.

## 2026-10-15 18:30 — stopped for the day

Ready for lamp-voice: no

- #789 (#739 part 1): reviewed, ARM64 run 38725112153 green on `07e935f8a`,
  waiting for its merge.
- #739 part 2, `eighth/motor-clock` at `6ead66ae9` (pushed, no pull
  request): `native_head_` alone 60 of 60 loaded; full serial runs loaded 8
  clean of 9 completed (run 10 stopped). Run 3: the preflight self-test's own
  3 s on a fresh script, and the timed `early_end` link-dropping test. Next:
  a first exec before the self-tests' 3 s, rebase on #789, ten runs, pull
  request, the fifth to read the body change. Also `agent_tasks` waits
  (`6ead66ae9`).
- #704: whole lamp 2,686 g; base 181 mm square on four feet 161 mm across;
  on `eighth/weigh`. Next: the head off and weighed. Lamp lock released.
- #783: taken; its loaded qualification not started.

## 2026-10-15 — #739 part 2: the head-qualification clock — made

Ready for lamp-voice: no

- Branch `eighth/motor-clock` at `9aa92fdd8`, on part 1. Allowed by the
  first builder at 14:10 on #739, on its conditions. `Supervisor<C: Clock =
  SystemClock>` with `spawn_with_clock`, and the qualification polls read
  `supervisor.now()`; production names `SystemClock` everywhere. Parent and
  inert child share one stepped clock through two files in the session
  directory; the fixtures are back on their own 30 ms segment, 100 ms lease
  and 400 ms step. Doc: voice-test-fixtures.md, "Head qualification on one
  clock".
- Before (host clock), `native_head_` alone under load: 6 of 60 failed.
  Fixture inputs widened to production's largest: 10 of 200 failed. Stepped
  (before rebase onto part 1): 60 of 60 passed, 14:31 to 14:35, load 16 to 23.
- Gate: fmt, clippy `-p lamp-voice -p lamp-body --all-targets` clean;
  `lamp-body` 285 passed. The qualification on this head (60 alone, then ten
  full serial runs under load) is running.

## 2026-10-15 — #739 part 1: privacy fixtures — offered

Ready for lamp-voice: no (draft #789, until ARM64 run 38700545585 is green)

- Kind: changes nothing a lamp runs (tests and `cfg(test)` code only).
- `eighth/voice-fixtures` at `332760877` on `686a49c09`: the fourth's
  `ddb13741b` rebased; the `remember.rs` macOS socket fix; the evidence in
  `docs/verification/voice-fixtures-loaded-eighth-20261015/`.
- Unchanged `lamp-voice` under load: 1 of 3 runs failed (privacy freshness).
  With the fourth's change, 10 runs: no privacy failure; 6 clean; runs 1, 5,
  7 failed on `native_head_*` (part 2), run 8 on the `remember` race.
- Gate on the head: fmt clean, clippy `-p lamp-voice` clean, workspace build
  clean, `remember::` 28 passed. Ten runs not repeated on this head.

## 2026-10-15 — #739 voice fixtures — taken

Ready for lamp-voice: no

- Taken at 12:50 +07, moved from the fourth by the first builder at 12:25,
  worked while #704 waits on lamp-75c9 (its person is using it). Branch
  `eighth/voice-fixtures`: the fourth's `ddb13741b` (`fourth/voice-fixtures`,
  v8: nine clean loaded runs, run 10 not started) rebased without conflict
  on `lamp-voice` at `08e94288e`. Production bounds in it are unchanged:
  `PRIVACY_FRESH_US` is still 50,000 us, and the fixture clock and the
  fixture freshness exist only under `cfg(test)`.
- This Mac differs from the fourth's: Apple M-series, arm64, 15 cores,
  Docker with 2 CPUs (the fourth's: Intel x86_64 with a Podman VM). Rust
  1.96.0 installed here today through rustup, with the person's yes.
- Unloaded, the rebased branch, `cargo test -p lamp-voice -- --test-threads=1`
  once: lib 984 passed, 0 failed, 8 ignored (326 s); every other suite
  passed. The load for the qualification: a Docker linux/arm64 release build
  of the workspace and a native release build at `-j 8`, each cleaned and
  repeated, with the load average sampled every 30 s.

## 2026-10-15 — #704 weigh — taken

Ready for lamp-voice: no

- Taken at 12:34 +07 (`taken by eighth · session 71157012-547b-4a9a-a2ab-fa9e8ae6e9c9 · lamp-75c9`
  on #704; the first "taken" comment is this one). Moved from the fifth by
  the first builder at 12:25 at this builder's person's request; next is #739.
- Kind: another lamp's own folder (`deploy/lamp-75c9/`), plus a data file and
  tests in `crates/lean`; the library is not changed.
- Starting point: lean.md, "Measurements the first builder needs" and
  "lamp-4ace, 14 October". The one figure so far is the fifth builder's:
  lamp-52e6 weighs 3.0 kg whole. lamp-75c9 is measured on its own.
- lamp-75c9 has no `deploy/lamp-75c9/LAMP.md` yet; nothing has been run on
  it, and no ssh has been made.
