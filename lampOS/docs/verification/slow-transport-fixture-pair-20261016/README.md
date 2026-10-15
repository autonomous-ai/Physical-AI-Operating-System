# Delayed transport joins the Serve fixture clock

Recorded 2026-10-16 02:30:53 +0700, from `date`, on a shared x86_64 macOS host. The combined source remains local and unqualified for merge while its complete evaluator gate runs. This is not a lamp, native ARM64 or complete-original-123 result.

The controlled transport keeps the original 400 ms delay, actual exec, literal argv and queued input. A terminal wait acknowledgement is followed by a fresh real relay registration before startup advances. The 2,300 ms first-preparer fault, original 100 ms lateness bound, actual Recorder process boundaries and every scenario assertion remain. Transport and Recorder terminal acknowledgements never stand in for actual exec/registration/exit.

Constructor-only paired measurement, same compiled helper SHA-256 on both sides, 14 original compiled identities and all 22 attempts / 114 assertion sites retained:

| Scope | Before | After |
|---|---:|---:|
| All 14 serial Serve-session tests | 86.271 s | 73.874 s |
| Delayed transport / slow first preparer case | 22.713 s | 3.608 s |

Both invocations passed all 14 with zero retries/skips. The before side already clocked 13 cases; this slice clocks the remaining delayed-transport case. Independent Moment and Bus gates overlapped parts of the pair, so this does not claim a quiet-host statistical speedup. Raw nextest lists, outcomes, date boundaries, helper hashes and exact before/after integration source are adjacent. Original test/statement bodies are byte-identical; only the constructor branch, unused import and explanatory comment change.

Before migration, combined formatting and strict evaluator all-target clippy passed, plus **296 library tests / zero failed or ignored**. This includes 12 wire, 9 command and 14 driver boundary cases added for the delayed path, as well as inherited preparations. The complete evaluator gate after migration is still running when this record is written and is not represented by that preliminary count. Source remains in `tests/823-slow-transport-progress` locally; dependency preparation is #894. Scratch Python only orchestrated commands and exact list/hash/source comparisons outside the repository.
