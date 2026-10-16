# Root review of Relay native qualification

The root independently parsed the unedited nextest result lines from [the retained package](../relay-native-composite-20261016/README.md), checking exact per-shard ordinals, PASS-only distinct identities, the fixed 123 inventory, all 508 compiled Evaluator identities, and the ten additions. Successful shard logs from the first full attempt match the composite byte-for-byte. Only the previously unexecuted shard contributes new executions.

`result.json` records the shell date, input log hashes, counts and per-binary printed duration sums. Those sums cross concurrent shards and execution attempts; they are not the requested standalone serial measurement.

Two external audit setup checks stopped before this result: the first used an incorrect baseline-list path, and the second omitted the Evaluator filter on the retained four-package baseline inventory. The corrected parser reads `local-e5aa-qualification/clean-e5aa-baseline/baseline-list.log` and selects binary IDs beginning `lamp-voice-eval`. Neither check executed or retried tests or changed any retained input. The external parser is not part of lampOS.

`SHA256SUMS` covers the result and this note.
