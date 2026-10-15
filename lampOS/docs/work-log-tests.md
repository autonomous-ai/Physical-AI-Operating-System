# Tests work log

## 2026-10-15 22:30 +0700 — explicit clocks ready; capacity timing and fixture failures retained

Ready for lamp-voice: yes (#861)

- Reviewer merged #850's Bus live-line barrier and #852's relay clock.
  #783 remains open for the sweep and separate fault qualifications.
- #861, head `b34d181d933868966ea5219b21e548dc9cee0572`, is ready.
  ARM64 `38756122123`: 4,662 passed, 126 existing profile skips; all five
  new clock tests ran. Local evaluator: 289 passed, no failures or ignores;
  fmt, strict clippy and workspace build passed. Native raw result rows:
  [backend-clock-arm64-20261015](verification/backend-clock-arm64-20261015/README.md).
- #867, head `18a4560c9f84729c00c0eab8682e38a47365c976`, is draft while
  ARM64 `38759844287` runs. It adds a pure clock and completed-poll seam
  for the finite fake runtime, retaining default timing and existing tests.
  Local evaluator: 288 passed, no failures or ignores; four focused tests
  passed in 0.16 s, plus fmt, strict clippy and workspace build.
- #868, head `1e38b543c9e0f58688ddd089e508b100f4e8d70c`, is draft while
  ARM64 `38760383411` runs. Its paired capacity fixture is 24.049 s to
  0.516 s, with all 500 packets, 480,000 samples and original assertions.
  The earlier source passed 1,733 affected/dependent tests; the final
  rebased gate records 1,765 passed, one failed, 13 existing ignores.
  The changed test passed. Fmt, strict clippy and workspace build passed.
- The rebased gate's unchanged host-clock capture-stall fixture expected
  physical mute but received invalid original microphone read interval.
  It ran before the changed capacity test. This is #866, with full raw
  evidence and source audit in #868; no retry. The suspected unsent-frame
  sequence hole remains an inference, coordinated with #739.
- #860's endpoint cleanup is uncommitted, with full gates running. Both
  formerly leaking cases now report PASS locally: 4.579 s and 1.652 s.
  The new panic-before/after-handoff cleanup regression passes in 3.168 s.
  All 18 original identities and 111 assertions are preserved. Native
  qualification remains pending; these are local results, not an ARM64 fix claim.
- Finite loopback coordination is being integrated separately. Runtime,
  relay, backend and immediate player retain real processes/sockets while
  sharing an explicitly stepped clock and completed-work acknowledgements.
  Review found an output-queue deadlock for a large final trace; that is
  being fixed and tested before any existing scenario is migrated.

No workflow, runner, profile, setting or device changes. The 123-test serial
under-two-minute target remains open; #832 stays draft on #838.

## 2026-10-15 21:44 +0700 — #783 follow-up ready; #823 clock work continues

Ready for lamp-voice: yes (#850, #852)

- #839's Health/Memory log barriers and #843's dispatch transport cleanup
  are merged by the reviewer. Original assertions are retained.
- #843 exact head `a25450bd742cca4108920d1dff5bc5fc9e7a6c4e` also passed
  the existing full-profile ARM64 run `38737583302`: 4,633 unique executed
  identities, comprising 4,631 PASS and two LEAK statuses. All 123 frozen
  identities are present, totaling 848.449 s; this is a sum across four
  shards, not serial suite wall time or a controlled aggregate speedup.
  All 13 final-dispatch cases passed, totaling 5.226 s. The paired local
  release comparison remains 29.938 s to 6.487 s.
- Full native evidence and all raw nextest status rows are now in
  [dispatch-arm64-full-20261015](verification/dispatch-arm64-full-20261015/README.md).
- The full run's two leak warnings are endpoint-session loading and
  unconfirmed-endpoint-exit cases; root is checking existing reports and
  recording the original warnings separately. Green does not erase them.
- #850, head `21c9b55b8f742e5b46f250584eaa4d3f65ca5fe2`, is ready:
  ARM64 `38741600267` passed 4,585 tests with 118 existing profile skips;
  all 11 Bus CLI cases ran. The original failed run compiled no tests
  because of three upstream Start fields; rebasing onto merged #851 fixed
  that source error. Bus source/evidence did not change during rebase.
  Fmt, strict clippy, 66 Bus tests, Voice test compilation and workspace
  build passed on the rebased source. #846 and #783 have the full report.
- #852, head `fef1377efc3f22573e379f5024f5b06aad875411`, is ready:
  ARM64 `38739080219` passed 4,586 tests with 118 existing profile skips.
  All three new relay clock tests ran. Local fmt, strict clippy, all 287
  evaluator tests and workspace build passed. This preserves the default
  system clock and adds an explicit clock API; no existing speedup claimed.
- Capacity fixture: unchanged 20-second/500-packet/5-second-buffer/4-second
  hold assertions pass with paused Tokio time: 24.049 s to 0.516 s in the
  paired release run. All three affected packages and workspace build
  passed locally. It is uncommitted, rebased onto `71a15e75d`; final gates
  are now running on that newer integration code.
- The backend/player explicit clock API is uncommitted. Independent source
  review found no blocker, existing 27 clock-related unit identities remain,
  and all 32 focused tests passed. Full evaluator/workspace gates are now
  running after rebasing onto `a583fda28`. No stepped scenario or speedup is
  claimed for this seam.
- #832 remains draft on #838's separate policy name-context failure. #837's
  normal-exit stderr loss is still open; #839 does not fix it. The eighth
  retains policy loaded qualification and #783's close decision.
- The approval service briefly refused a rebase-cleanup command because of
  its usage limit; it executed no action. Review succeeded after the user
  resumed work. An inactive owned dispatch build cache is being cleaned to
  make room for the two current gates; source and raw evidence stay intact.

No lamp, workflow, runner, profile or repository setting was changed.
The under-two-minute complete-group target remains open.

## 2026-10-15 19:15 +0700 — #783 sweep ready

Ready for lamp-voice: yes (#839)

- PR [#839](https://github.com/autonomous-ai/lampOS/pull/839), exact head
  `5494331c413a19771f7332759df0cf2a1ac5251b`, is ready for the reviewer.
- Native ARM64 run `38731261120`: 4,499 passed, 116 existing profile skips;
  doc/own-harness checks and artifacts also passed. All six active health
  program tests and six memory binary tests passed. The PR profile's
  skipped five-second health lease passed locally.
- The two local normal-exit stderr failures remain documented as #837;
  green ARM64 does not erase them or establish that product race fixed.
  All original test assertions remain. No retry was requested.
- The eighth retains loaded policy qualification and #783 closure.
  #832 remains draft on the separate #838 policy timing failure.
- Continue #823: dispatch fixture before/after measurement and the relay
  clock seam behind the unchanged system-clock entrypoint. The two-minute
  group target remains open.

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
