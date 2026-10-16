Gemini TaskJoins preparation, recorded 2026-10-16 09:59:33 +0700, on immutable base
`1fb07c6f57ee280a49de7b4ddd74008c2a32a20d`.

Explicit in-memory fixtures retain actual Gemini Actor and Dialing task handles
and observe their joins after normal owner shutdown. Registration is bounded for
the fixture's lifetime; task completion stays separate from protocol success.
This preparation migrates no original clock scenario and claims no service or
worker exit, child reaping, device behavior or speedup.

The local full gate is **failed: 2,710 passed / 1 failed / 15 skipped**. Native
ARM64 qualification is pending; this is draft evidence.

| Check | Retained result |
| --- | --- |
| Clean compiled baseline | 2,708 identities, 15 ignored; no baseline tests executed |
| Corrected compiled candidate | 2,726 identities, same 15 ignored; all originals retained, exactly 18 additions |
| First focused execution | All 18 additions passed |
| First full affected execution | All 2,711 nonignored identities executed once; 2,710 passed, one original failed |
| Formatting and strict all-target Clippy | Passed after the reviewed new-test compile correction |
| Workspace build | Passed once after the full gate |
| Five doc-test targets | Exit zero; zero doctest cases; normal tests were not rerun |

The package selection was `lamp-gemini`, `lamp-memory`, `lamp-surround`,
`lamp-voice` and `lamp-voice-eval`. Rust 1.96.0, jobs two, incremental zero,
locked/offline Cargo and a serial default nextest profile were used. Exact
commands, exits and date-derived boundaries remain in the
[original package handoff](external/handoff.md) and command/result JSON.
The [independent package audit](independent-package-audit.json) reparses the raw
compiled lists and pre-Summary runtime records, verifies all original identities
and ignores, and checks the source against the executed-source freezes.

The failed original is
`lamp-voice / coordinator::motor::tests::native_head::native_head_six_explicit_actions_prepare_profile_then_new_requests`:
`native_head.rs:172:13`, phase Inhibited, `motor worker progress timed out`,
1.491 s. The [complete first log](external/corrected-gate/full.log) is retained
unchanged. [Independent source/history triage](failure-triage/report.md) verifies
the unchanged 1fb Motor path and the exact identity's
[prior acceptance into #739](https://github.com/autonomous-ai/lampOS/pull/834#issuecomment-6198530173).
The prior symptom was packet freshness loss; this occurrence is a distinct
50 ms progress-deadline observation. Neither unchanged source nor the family
classification proves current host-load causation. The pinned joining rule's
known-failure exception applies only after this piece's own exact-head ARM64
run is green; the local failure remains disclosed and is not retried.

Two preparation defects remain visible in the raw evidence:

- The first Clippy attempt stopped on E0277 in the added poison regression,
  with zero tests executed. The [reviewed correction](external/reviewed-correction/correction.diff)
  changes only that new test's captured reference. First diagnostics and both
  source snapshots remain under `external/first-compile-failure/`.
- The first external parser counted the post-Summary failure recap as another
  execution. Its original 2,712-row report and exception remain unchanged.
  [The separate corrected proof](external/corrected-gate/completion-supplement/full-runtime-identities-corrected.json)
  and this independent audit prove 2,711 unique pre-Summary executions. No test
  was rerun to correct that bookkeeping. Later build/doc commands are separate.

Both first and corrected six-file diffs independently apply and reverse in
scratch copies: four original files return byte for byte and both added files
are removed. All 7,152 current input hashes match the corrected executed source;
all 7,146 unowned inputs remain exact to the clean base. The source copies and
[whole-file inverse](external/current-source.inverse.diff) retain this proof.
Historical raw records preserve their then-current wording and counts; the
new audit and triage are supplementary evidence.

[The original external manifest](external/manifest.json), SHA256
`c9e3c8da37671d75ffbc14732b563843433adcc46ad64cd0a30beac41c01203b`,
was checked against all 152 entries. [Nine exclusions](excluded-artifacts.json)
record exact paths, hashes and descriptions of eight Python runners and one
opaque Cargo fingerprint stamp. No script, compiled binary, executable or
symlink is in this payload. Raw logs, JSON feature metadata, diffs and source
snapshots are copied unchanged. The triage's identical full log is stored once
through an [explicit provenance link](failure-triage/gate/full.log.provenance.json).
The [payload map](payload-provenance.json) records every original-to-repository
copy. `file-manifest.json` lists payload hashes and the six owned source hashes;
`SHA256SUMS` covers every evidence file except itself.
