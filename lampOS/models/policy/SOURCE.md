# Local Policy seed

`local-weights.json` is an authored numerical seed for task 17. It is **not
trained or calibrated on recordings**. No speech, face images, private
dataset, downloaded model, OS voice or external provider output was used to
choose these coefficients. The test fixtures are authored cue/label records
and are not a source of claimed lamp accuracy.

- Source: the Fourth builder's task 17 contribution, 2026-10-14.
- Licence: Apache-2.0, the repository's licence for code/assets outside HAL.
- Size: 3,078 bytes.
- SHA-256: `3de494c07557f99b6da4a52fcd145d32b76cbcf8ebaa230bf7812b4eac6ad075`.
- Schema: 1; eight features, seven question heads, one local choice head;
  fixed local share 0.5. Every head is marked `seeded_unfitted`, with zero
  positive/negative training labels; provenance records zero rows.

The values were hand-authored in the JSON file and then serialized by the
Rust program below to make the checked-in bytes reproducible. From `lampOS/`,
with a nonexistent output path:

```sh
cargo run --release -p lamp-policy --bin lamp-policy-fit --offline -- \
  --seed /tmp/lamp-policy-seed-new.json
shasum -a 256 /tmp/lamp-policy-seed-new.json
```

The command exports the embedded seed; it performs **no training** and
refuses to overwrite a file. The initial serialization here used the same
program's debug build; profile changes do not change these JSON bytes.
Loading and serializing its output is stable. The seed file is the sole
source of the initial numerical weights; there is no second hard-coded
coefficient table in Rust.

The fitting command and exact labelled JSONL schema are in
[Policy](../../docs/policy.md#fitting-from-firsts-labelled-scenes). A future
fitted asset must retain its own source/rights declarations, data partitions,
independent test report and SHA-256. Do not replace this provenance with a
claim of fitting unless that evidence exists. No model over 2 MB or third-party
model is committed by this task.

Not shown: calibrated semantic probabilities, intended-addressee accuracy,
an improvement over Voice's busy-room floor, or behavior on a lamp.
