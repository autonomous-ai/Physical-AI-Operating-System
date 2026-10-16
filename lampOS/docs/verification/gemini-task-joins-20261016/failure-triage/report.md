Read-only Motor failure triage, recorded 2026-10-16 09:48:50 +0700.

Disposition: **known #739 identity; distinct current timeout symptom, with cause
unproved.** This is not an unclassified new test name, and it is not one of
#906's three identities. Eighth owns the Motor timing work. The retained Gemini
gate stays **2,710 passed / 1 failed / 15 skipped**; its 18 added cases passed.
No original was changed or rerun, and this audit executed no test.

The exact original is
`lamp-voice coordinator::motor::tests::native_head::native_head_six_explicit_actions_prepare_profile_then_new_requests`.
Its current retained failure is `native_head.rs:172:13`, reached through
`run_sequence("profile", true)` at line 110: phase `Inhibited`, failure
`Some("motor worker progress timed out")`. Nextest records 1.491 s for the case.
The complete run records 1,186.068 s, with the original failure and all remaining
no-fail-fast results retained. The raw full-log SHA256 is
`fc7455deb8f8a5dcc746922e9641e2893c336cb56eff078da6d3728c1045abd5`.

Prior classification is explicit:

- [Eighth's #834 comment 6198530173](https://github.com/autonomous-ai/lampOS/pull/834#issuecomment-6198530173)
  names this exact test from ARM64 run 38776871505 and assigns it to #739's
  known timing family and Eighth's `eighth/motor-clock` work.
- [The retained earlier failure, comment 6198372565](https://github.com/autonomous-ai/lampOS/pull/834#issuecomment-6198372565)
  reports a different boundary: shared `tests.rs:601` readiness wait after a
  packet boot/order/freshness violation, ready=false, reaped=true and ControlLost.
  It does not report this gate's progress-timeout assertion.
- [Eighth's loaded-host diagnosis, #739 comment 6186529471](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6186529471)
  identifies native-head real parent/child pauses, permit/segment/freshness
  bounds and preflight as the family being repaired. That historical report
  does not measure the delay that produced this current failure.
- [#739](https://github.com/autonomous-ai/lampOS/issues/739) is open, status:taken,
  builder:eighth; [#900](https://github.com/autonomous-ai/lampOS/pull/900) is the
  separate supervisor/shared-clock preparation. [#906](https://github.com/autonomous-ai/lampOS/issues/906)
  records three different cleanup identities and is not an exact-name match.

The current causal boundary is narrower than a host-load diagnosis:

1. The real shell preflight and native-head attachment reached Ready. The failing
   iteration already acquired a ticket, checked its original owner/stage and
   received a non-error action reply. The log does not identify which of the
   six actions was active, its target stage or how many earlier actions passed.
2. `run_sequence` uses its unchanged 400 ms host Instant deadline, refreshes the
   original 100,000 us capture/admission leases, calls `m.poll` and parks for
   1 ms. The panic is that outer test deadline expiring after the adapter has
   entered Inhibited; it is not a 400 ms product timeout.
3. Body `Supervisor::enforce` has the unchanged 50,000 us progress bound
   (`runtime/supervisor.rs:19,400`). While ready and before a stop/kill/exit,
   host time minus the last accepted event's source `at_us` at or above that
   bound latches this exact failure and attempts to kill the owned child.
   `poll` enforces before receiving a queued event and again after try_wait
   (`:539`). A queued-but-unread receipt therefore does not itself prevent
   the first deadline check. Accepted receipt source time, not its arrival
   or current host time, updates `last_observed` (`:432`).
4. Worker Progress publication is due after 10,000 us during real worker polling
   (`runtime/worker.rs:548`), using Body SystemClock/IPC host monotonic time.
   Both the inert core and worker retain SystemClock. OptionalMotor adopts
   the supervisor's latched reason and disables/shuts down (`motor.rs:670`).
   None of this determines whether the current gap came from the parent,
   child, OS scheduling, delayed I/O, queued receipts or another interaction.
5. The log prints no last-observed/current timestamp pair, action, queue state,
   relevant parent/child scheduling gap, owned-child exit status or terminal
   receipt. Subsequent successful reaping/finish, emitted mask 16 and exact
   final counts [2000;5] assertions at native_head.rs:183 onward were not
   reached. Drop's kill/try_wait path is not proof those assertions passed.

`source-proof.json` independently compares 12 named source/lock/rule files
with exact immutable base `1fb07c6f57ee280a49de7b4ddd74008c2a32a20d` and both frozen
before/after gate manifests. Every comparison is byte-exact, including the
complete native_head.rs and its original function, shared fixture, adapter,
head driver, Body supervisor/worker/channel and SystemClock source. These
Motor paths construct the inert Body worker and channels, without constructing
a Gemini session. Unchanged source and no direct Gemini call path do not rule
out indirect host load or modified-binary timing effects. No baseline rerun,
stress experiment or source change is part of this triage.

The pinned joining rule at lines 322–327 permits a known timed-process local
failure to be disclosed without rerunning the whole gate **only after this
piece's own exact-head ARM64 run is green**. This known identity is eligible
for that conditional rule; the current local failure stays red and its distinct
symptom must be named. This audit does not establish an ARM64 result, declare
readiness or attribute the current failure to host load.

Raw issue JSON, exact prior comments, the complete first gate log and command,
unchanged source snapshots, source comparison and a failure excerpt with
original line ranges are retained beside this report. No Git/GitHub state, source or
device was changed. Only read-only GitHub/Git requests and local evidence
copies were made. Initial sandboxed GitHub capture/search attempts failed to connect; the
same read-only capture then succeeded with authorized network access.
