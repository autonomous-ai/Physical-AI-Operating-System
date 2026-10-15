# Inline inventory count correction

Recorded 2026-10-16 06:15:45 +0700, from shell `date`.

The frozen worker contains **105 original inline test functions, including two ignored cases**, not the preliminary count of 103. The source-name regex skipped `sixty_second_answer_follows_output_capacity` and `three_minute_answer_follows_output_capacity` because their ignore attributes intervene before the function declaration. The exact whole-module inverse already preserved both functions and their ignore flags; all 544 original assertion macros and all protected files remain exact. No source or test was edited.

`inline-inventory-correction.json` records the corrected names. The first executable gate compares every compiled identity and ignore flag against the independent clean-e7d inventory, without using this source regex. Preserve the preliminary evidence alongside this correction.
