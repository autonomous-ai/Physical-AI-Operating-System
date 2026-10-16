Gemini task-join preparation: publication package

Recorded from date: 2026-10-16 09:49:25 +0700.

The corrected source preserves all 2,708 original compiled test identities and
15 ignore flags and adds exactly 18 task-join regressions. All 18 passed in their
first focused execution and in the complete affected suite. The full suite ran
all 2,711 nonignored identities once: 2,710 passed and one existing native-head
identity failed. Strict Clippy, formatting, workspace build and all five doc-test
targets passed. There are zero doctests in these targets. The local full gate is
not green; native qualification has not been run by this agent.

Exact base: `1fb07c6f57ee280a49de7b4ddd74008c2a32a20d`. The six owned files are
below; all other 7,146 repository inputs are byte-exact to the clean base. All
7,152 executed-source inputs remain byte-exact after qualification and packaging.

- `lampOS/crates/gemini/src/session.rs` — `edca2df22c9101fc3e7148c227e82ffde6685772b57baaabb7880e49208ebe2b`
- `lampOS/crates/gemini/src/recovery.rs` — `37ed679cd097da44791a356d7260343ebbe437fb8863b3e22174a1b5da92e98a`
- `lampOS/crates/gemini/src/testing.rs` — `cad490f02196ead1aab9f0402064df84cfe61992fef3abb7a1648cf39e7013ff`
- `lampOS/crates/gemini/src/testing/task_joins.rs` — `21cb7a49c8f1f012b5cf38ee089da42f8a6f2d26a4f3284d2e3926a8125f3934`
- `lampOS/crates/gemini/src/testing/task_joins/tests.rs` — `010fce88be96d93269f2779d1e40764d347283480a39dcf1c5715661c4a285a4`
- `lampOS/docs/gemini-session-reliability.md` — `72445c8a193c41ce1bb4b412c5c6c4b0e57246e1bc93b1f405de0fcd6957eba1`

Rust 1.96.0; jobs 2; incremental 0; locked/offline Cargo; default nextest profile;
one test thread; no fail-fast; retries 0. The package selection was identical for
baseline list, strict all-target Clippy, both changed lists, focused run and full
run: lamp-gemini, lamp-memory, lamp-surround, lamp-voice, lamp-voice-eval. Native
socket/child-process execution used the approved sandbox escalation. Exact
commands, environments, dates, exits and raw log hashes are retained in JSON.

| Corrected stage | Start (+0700) | End (+0700) | Exit |
|---|---|---|---|
| fmt | 09:16:37 | 09:16:42 | 0 |
| clippy | 09:16:42 | 09:16:49 | 0 |
| compiled-list | 09:16:49 | 09:20:23 | 0 |
| focused-list | 09:20:23 | 09:20:28 | 0 |
| focused | 09:20:28 | 09:20:34 | 0 |
| full | 09:20:35 | 09:40:27 | 100 |
| workspace-build | 09:42:57 | 09:44:39 | 0 |
| doc-tests | 09:44:42 | 09:44:47 | 0 |

The first source qualification is preserved without alteration in
`first-compile-failure/`: fmt passed; Clippy exited 101 on E0277 because the added
poison regression captured Arc<Shared> containing AtomicWaker across catch_unwind.
No tests had executed. The reviewed two-line correction captures only the Mutex
reference; it preserves the intended poisoning, real consumer result and every
assertion, and adds no AssertUnwindSafe. Its diff SHA is
`787f274aeb3c9eb116130796dc04a1ef3b3e64571b49ac0a1ce9aa539dc62e53`.
Both first and corrected source snapshots are retained. There were no test retries.

The first full failure is
`lamp-voice / coordinator::motor::tests::native_head::native_head_six_explicit_actions_prepare_profile_then_new_requests`.
At unchanged native_head.rs:172:13 it reported `phase Inhibited, failure
Some("motor worker progress timed out")`. Runtime was 1.491 s. The raw log does not
identify the particular action or a scheduling cause. The original 400 ms bound,
Motor source, child fixture and all manifest inputs are unchanged. The executed
Voice binary's matched dependency fingerprint shows normal Gemini with only
`default`, without `fixture-clock`; the retained TaskJoins adapters are not
compiled into that dependency. This source/configuration separation does not
classify the failure cause or turn it into a pass. Root reports that the same
identity was previously explicitly accepted into #739 on #834 by Eighth; this
run's progress-timeout symptom must be disclosed separately and its own native
result obtained. This package itself performs no GitHub lookup or classification.

The external runtime parser initially counted the post-Summary failure recap as
a second execution and stopped before build. Its original exception, terminal
record and incorrect 2,712-row report remain in `corrected-gate/`. The separate
`completion-supplement/` reads only progress rows before the unique Summary and
proves every ordinal 1 through 2,711 exactly once, with no missing or extra
identity. It preserves and explicitly identifies the excluded recap. No test was
rerun. The already-authorized build and doc tests then ran once; their terminal
records remain separate from the original failed parser. Normal tests were not
re-executed by the doc-test command.

Package navigation:

- `baseline-and-first-source/`: clean actual-base metadata, five-package graph,
  raw before list, 2,708 identity/ignore records, first six-file diff/inverse,
  original-test and unowned-source proofs, first source snapshots.
- `first-compile-failure/`: first fmt/Clippy logs, stopped source hashes and
  failure handoff. Zero test executions.
- `reviewed-correction/`: only the reviewed new-test capture correction and
  all-other-inputs byte proof.
- `corrected-gate/`: changed lists, focused/full raw results and commands,
  source freezes, original parser failure and its provenance.
- `corrected-gate/completion-supplement/`: corrected runtime identity proof,
  exact first Motor failure block, source proof, one workspace build.
- `corrected-gate/doc-test-supplement/`: same five-package doc check, counts and
  before/after source proofs.
- `corrected-gate/motor-feature-evidence/`: exact original source snapshots,
  matched compiled feature fingerprints, unchanged graph/manifest evidence.
- `current-source/`, `current-source.diff`, `current-source.inverse.diff` and
  `current-source-proof.json`: final six files, complete reviewable diff and an
  external-copy inverse that restores the four old files and removes two new ones.
- `current-input-hashes.json`, `handoff.json`, `manifest.json`: final full input
  freeze, structured handoff, and byte hashes for every packaged artifact.

No original 123 scenario has been migrated. Actual Actor/Dialing joins establish
shutdown outcomes only; this preparation makes no live Actor work, service join,
outer worker return, child exit or device latency claim. No repository source
was edited during qualification, no Git/GH/device action occurred, and no target
was cleaned. The two native sessions 49918 and 28914 are terminal. All required
binary/fingerprint inspection is captured; the target is released. Root owns
publication and native qualification.
