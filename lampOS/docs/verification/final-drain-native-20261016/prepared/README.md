# Final-drain native audit inputs

Prepared 2026-10-16 09:32:06 +0700, from shell `date`, for [PR #963](https://github.com/autonomous-ai/lampOS/pull/963), exact head `c5bcc45b905458c1a8adcd8bcf06885f35dbfdf7`, base `1fb07c6f57ee280a49de7b4ddd74008c2a32a20d`. Normal run **38839828555** and full run **38839828296** are audit targets; this folder makes **no native outcome claim**. Root owns the existing 120-second watcher. No unfinished logs or workflow-status queries were made by preparation.

The local manifest SHA-256 is `7022fc9efcf5c99ffe30642061dac100adcd9920a2fb98d55080a22a120ac086`. All **56 published entries** and **1,023 frozen gate inputs** match immutable local Git bytes and the complete remote Git tree `b4f45bc806c4b34661828d0cb2b25fce1cf37bf7`. The source set includes **77 Evaluator files**. Source reads do not depend on the current working tree. The original immutable compiled comparison preserves all **498** prior identities/ignore flags and adds exactly **six** cases, total **504**, zero ignored.

| Expected native membership | Normal PR profile | Full profile |
|---|---:|---:|
| Evaluator identities | 452 | 504 |
| Six new final_drain_clock cases | 0 | 6 |
| Fixed original inventory | 0 | 123 |

The 52 normal-profile Evaluator exclusions come from the unchanged actual nextest filter, whose three copies are checked equal. All six additions and the selected original are in the excluded final_dispatch binary. A green normal check alone cannot qualify this migration. No total workspace pass count is predicted before inspecting the actual run source and complete results.

All **123 original module-qualified definitions** remain present with zero source ignores. All 123 assertion-macro lists are byte-exact to base 1fb; 122 function bodies are exact and the selected original's helper-initializer inverse restores the remaining one exactly. Against the older original8ddff inventory, **120** assertion-macro lists are exact, with the same three inherited upstream exceptions. This change introduces no further macro change; identity preservation does not mean all historical assertions were unchanged.

The selected original is `lamp-voice-eval::final_dispatch / a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept`. Its real shell releases the later line at the original **300 ms** logical boundary, with both after-cat marker and actual reader-enqueue observation; the unchanged final drain at **800 ms** retains its assertions. The separately retained local pair is **1.272 → 0.444 s**, one uncontrolled shared-host sample per version, not native or standalone-123 timing. The ten-second observation watchdog does not preempt OS calls; the real backend's existing stdin-close and two-second kill/wait cleanup remains. The code PR records the slice's missing dedicated pre-initial-marker panic and closed-transport coverage; preparation does not broaden that claim.

`expected-native-membership.json` lists every expected identity and exclusion. `prepared-source-proof.json`, `prepared-remote-source-proof.json` and `prepared-original123-source-proof.json` retain hashes, definitions and provenance. The exact raw local lists and original inventory are in `inputs/`; relevant source blobs and before versions are in `source-snapshots/`. `prepared-audit-plan.json` and `NATIVE-AUDIT-CHECKLIST.md` define the terminal audit and optional infrastructure-composite checks. Script paths/hashes point outside this package; no Python, source changes, Cargo/test executions, Git/GitHub mutations or device actions belong to preparation.
