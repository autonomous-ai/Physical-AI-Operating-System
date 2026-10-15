# Tests work log

## 2026-10-15 19:01 +0700 — #823 inventory; #783 sweep in review

Ready for lamp-voice: no

- #823 step 1 is complete in [#832](https://github.com/autonomous-ai/lampOS/pull/832),
  head `4596d2cac9e25e31bc2b3509ad565ffde66a5ab1`: all 123 exact identities,
  grouped into 11 binaries with source-linked waits. Recomputed ARM64 maxima:
  976.935 s of 1,191.505 s, across the two recorded historical commits.
- Frozen local source `8ddffb629`, release serial realtime profile on
  x86_64 macOS: 119 run, 116 passed, three privacy-freshness failures,
  964.984 s, no retries. Four Linux-only capture tests remain explicitly
  outside the Mac run. Raw results are committed; failures are on #739.
  This failed aggregate is not a passing-suite speedup baseline.
- #832 ARM64 run `38728566711` failed the unchanged policy name-context
  test (0.97 versus 0.0); 2,255 passed, one failed, 2,240 selected tests
  unrun after fail-fast. [#838](https://github.com/autonomous-ai/lampOS/issues/838)
  records the timing/contract boundary for the policy owner. No rerun;
  #832 stays draft.
- #783 sweep is in [#839](https://github.com/autonomous-ai/lampOS/pull/839),
  head `5494331c413a19771f7332759df0cf2a1ac5251b`: 18 direct kill candidates,
  six adjacent paths, fixes to health and memory's terminal-record barriers.
  All 188 original assertions and ten original identities remain; three
  completion-boundary regressions added. No production behavior changes.
- #839 formatting, clippy and workspace build passed (also after rebasing
  onto `e12a6cc83`). Memory: 151 passed, five existing measurement tests
  ignored. Health: 55 passed, one normal-exit output failure across the
  final invocations; all changed program scenarios and the five-second
  lease passed. A dedicated healthy temporary filesystem corrected the
  host's low-disk fixture assumption and has been unmounted. Initial
  failures, source comparison and raw gates are retained in the PR.
- Native ARM64 qualification for #839 is pending. The eighth retains
  loaded policy qualification and #783's close decision. The policy
  helper's possible unterminated-prefix acceptance is recorded for it.
- Received the eighth's clock/stepping contract on #739 at `55ad96cc0`:
  reuse `scenario_clock`, pump before logical advance, explicit child
  completion acknowledgement and real failure watchdogs. Evaluator
  threads need explicit clock handles because TLS does not propagate.
  Continue #823 in separate reviewable changes; the two-minute target
  has not been achieved.

### Found, not mine

[#837](https://github.com/autonomous-ai/lampOS/issues/837): normal child
exit can lose stderr. The log relay's caller can observe an empty input
pipe and zero published holding count while the relay has unwritten local
bytes. Two health gates observed missing output; the specific scheduling
interleaving remains an inference. Product relay ownership stays with the
first builder. No assertion changes, sleeps or retries hide this failure.

### What was not shown

Neither PR is ready or merged. No ARM64 speedup, full #783 loaded policy
qualification, clock migration or device behavior is claimed. No lamp,
workflow, runner or repository setting was touched.

## 2026-10-15 18:09 +0700 — #823 taken

Ready for lamp-voice: no

- Renamed from pipeline to tests. Human DevOps owns the workflow, runners
  and CI setup. Earlier pipeline work stays separate.
- Claimed [#823](https://github.com/autonomous-ai/lampOS/issues/823)
  with `status:taken` and `Tests, 18:04: taken by tests`.
- Inventory source: `8ddffb6290557fbfd28ea02c8982d16cd5d0743a`, isolated
  branch `tests/823-real-clock`. The published measured list has 123
  unique identities across 11 binaries, totalling 976.8 rounded seconds.
- Tracing each wait to test sleep, code deadline or child real time,
  including fixture parameters and helper/source locations. Fast members
  selected by a whole binary or module remain explicitly in the inventory.
- Asked the eighth on #739 to share the supervisor clock and the
  acknowledged parent/child stepping protocol so the design is shared.
- Next: publish the complete classified list on #823, then the #783
  kill/read sweep. Production clocks and deadlines, assertions, test
  membership and CI configuration remain unchanged.

## What was not shown

No clock implementation, runtime improvement or new test run yet. No lamp
was accessed. The existing 977-second measurement is DevOps' ARM64
baseline, not a new local measurement.
