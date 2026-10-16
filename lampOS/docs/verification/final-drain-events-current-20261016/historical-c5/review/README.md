External source-only follow-up prepared at 2026-10-16 09:35:06 +0700, from immutable prerequisite
`c5bcc45b905458c1a8adcd8bcf06885f35dbfdf7`. This is a proposed two-file patch,
held locally until #963 merges. No active worktree was edited. No Cargo,
formatter, test execution, Git mutation, GitHub write or device action was run.
No new runtime result or speedup is claimed.

`followup.patch` changes exactly three `let mut backend` initializers in
`lampOS/crates/voice-eval/tests/final_dispatch.rs` to the existing
`final_drain_clock::backend(&rig, ...)`, then appends English fixture
documentation. It adds no helper, product, pacing, cleanup, test identity or
assertion change. `candidate/` contains only those two proposed files.

| Original identity in lamp-voice-eval::final_dispatch | Original authoring retained | Dispatch retained |
| --- | --- | --- |
| an_invalid_cue_first_seen_in_the_final_drain_withholds | One start line, `"0.3"`, `lines(&[cue_invalid()])`, StayOpen | +800,000 us and protocol_failed |
| a_relay_framing_error_first_seen_in_the_final_drain_withholds | One start line, `"0.3"`, exact `"not a relay line\n"`, StayOpen | +800,000 us and protocol_failed |
| an_unobserved_event_in_the_final_drain_requires_revalidation | One start line, `"0.3"`, original ready JSON and `lines(&[ready])`, StayOpen | +800,000 us; revalidate, one pending event, then original same-time second dispatch |

`source-proof.json` proves all 13 frozen original identities remain. Ten
whole original test functions are byte-identical to c5; the other three
become byte-identical after reversing only their backend initializers.
Those three unmodified c5 function bodies also match the original
8ddff inventory source exactly. The existing six helper cases remain in the
unchanged helper, so the source identity set in these two files is still 19. This is a source proof, not a substitute for compiled enumeration.
The complete test file and document both reverse to their exact c5 bytes.
`patch-application-proof.json` records successful real `patch` forward and
reverse application in a disposable directory, with exact-byte comparisons.

The source audit found no selector-specific blocker:

- All three inputs satisfy the unchanged helper's explicit one-start-line,
  one-LF-terminated-later-line, `"0.3"` and StayOpen requirements
  (`base/.../tests/common/final_drain_clock.rs:201`). The authoring helpers and
  `Rig::config`, including finite Fixture mode and no room recorder, are
  unchanged (`base/.../tests/final_dispatch.rs:53`, `:73`, `:143`).
- The helper requires the first actual cat completion marker and the reader's
  actual enqueue count of one before any logical progress. At +300,000 us it
  sends the existing nonblocking FIFO release, then requires the second cat
  marker and actual enqueue count of two before further progress
  (`base/.../tests/common/final_drain_clock.rs:137`, `:157`). The original
  +800,000 us injection advances through `pump_fixture(until, false)` and
  consumes neither line during that wait (`base/.../src/physical/mac.rs:2943`).
- A malformed JSON LF frame remains a complete frame: reader framing decides
  completeness before JSON parsing, sends the resulting Err through the real
  channel, and increments the counter only after that send succeeds
  (`base/.../src/physical/mac.rs:2477`). The helper cannot certify semantic
  validity and does not consume the line. The unchanged final drain calls
  `take`, which turns its queued Err into the original protocol failure
  (`base/.../src/physical/mac.rs:2072`, `:2958`).
- The readiness line is accepted by the original finite-fixture Stdout path,
  after the start line has established the session. Its ListeningReady is a
  nonterminal event, so it stays pending and makes the first injection
  Withheld::Revalidate (`base/.../src/physical/mac.rs:2213`, `:2025`, `:2987`).
  The original `next_event(now)` pops that pending event and returns before
  any clock, ping or wait work (`base/.../src/physical/mac.rs:2777`). Thus the
  original next `now_us()` still reads the same Cell value. The second
  injection's `while now < at` is false; it performs its own nonblocking final
  drain, sees no additional authored line or held cue, and reaches the real
  Recording Player::start once (`base/.../src/physical/mac.rs:2943`, `:3041`).
  This second dispatch neither invokes the helper again nor requires a third
  frame, repeated release, timer advance, synthetic event, or report join.
  The unchanged `rig.started() == 1` assertion observes that actual start call.
- The same StayOpen shell silently consumes backend input and remains owned
  until backend stdin closes. It produces no spontaneous third record. The
  helper's Drop only attempts abort and closes its control peer; backend Drop
  closes stdin and retains the original actual child wait/kill/reap path
  (`base/.../tests/common/final_drain_clock.rs:83`, `:193`;
  `base/.../src/physical/mac.rs:2390`, `:2512`). No enqueue count, marker or
  pending-queue state is treated as EOF, task exit or reaping evidence.

The inherited immutable ten-second watchdog checks driver progress and
logical advancement; it does not interrupt blocking filesystem/OS calls.
The original two-second child cleanup fallback remains a host-clock path.
The unchanged pre-initial-marker fallback has no new direct regression here.
The closed-transport original remains on its original helper because reader
closure requires different evidence; this patch does not migrate it.

After the clean three-case baseline and prerequisite merge, root can review
and apply this exact patch to an independently verified matching base.
Qualification remains pending: pinned-format check, strict Eval Clippy,
compiled identity comparison (same complete baseline list; no additions or
removals), the three original cases with raw per-case timing, full affected
suite and workspace build, followed by the exact-head native ARM64 run.
Root separately owns the clean 504-case prerequisite baseline and three-case
baseline run. Those raw results are outside this source-only handoff and are
not presented as qualification of this candidate. A later timing claim
must use comparable baseline/candidate boundaries and preserve any first
failure without retries.

Contents: `base-capture.json` records each exact `git show` source and hash;
`base/` contains four immutable source files for this audit; `inputs/` holds
the frozen 123 inventory and original final-dispatch source; `candidate/`
holds the two proposed files; `inverse/` holds exact restored baseline files.
`initializer-edits.json` pins the three before/after blocks and doc append.
`file-manifest.json` enumerates every payload file and hash, excluding itself
and `SHA256SUMS`; `SHA256SUMS` covers every file except itself.
