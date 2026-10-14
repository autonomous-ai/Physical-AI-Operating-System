# First's played-scene fit, reproduced on Fourth's Mac

Task 17 / [issue #700](https://github.com/autonomous-ai/lampOS/issues/700),
2026-10-14. This is an offline audit of First's published lamp-record
artifacts, not a new lamp trial. No runtime, weights, labels, splits or
thresholds change. The local judge remains opt-in and shadow only.

## Inputs and reproduction

Source audit at `e822396c3001acfe9c3d53b00a40dc4f7c4f0961`, carried without
Policy source changes onto `ce7abae79c06c53c8ea1dfb03c42622c41f8f135`
(upstream `de69bac33412026475974c0e5ea39a87881816f0`). First's
[dataset record](../policy-scenes-lamp-4ace-20261014/README.md) describes
the collection, script labels and exclusions. Fourth did not repeat the
plays or inspect the withheld raw transcripts. Of 111 recorded Policy
choices, 89 matched the script and were retained; excluded choices and
played lines without transcripts are not measured by this fit.

| Committed input | SHA-256 |
|---|---|
| `scenes.jsonl` | `25db2d1c103bb934b1263ed343d719293915689844ba7af8fcc12cdbdc6a206c` |
| `fit-weights.json` | `d7bde805807b742bf7da19617c17221a1481da5b6ccee16e527dfc4cb6a1b043` |
| `fit-report.json` | `a995f06a5a17eab9cb22776fe4f046bb2b1df8e959516c86f1366e72602e7d4c` |

Machine: macOS 26.6.2 (25G83), x86_64 Intel Core i7-10700K at 3.80 GHz,
16 logical CPUs, 64 GiB RAM; Rust/Cargo 1.96.0. A fresh isolated dev-profile
build of `lamp-policy-fit` passed in 14.43 seconds. That is build time,
not inference latency. From `lampOS/`, with a new output directory:

```sh
trial_dir=$(mktemp -d /var/tmp/lamp-judge-fit.XXXXXX)
cargo run -p lamp-policy --bin lamp-policy-fit --offline --locked -- \
  docs/verification/policy-scenes-lamp-4ace-20261014/scenes.jsonl \
  "$trial_dir/weights.json" "$trial_dir/report.json"
cmp docs/verification/policy-scenes-lamp-4ace-20261014/fit-weights.json "$trial_dir/weights.json"
cmp docs/verification/policy-scenes-lamp-4ace-20261014/fit-report.json "$trial_dir/report.json"
```

The fresh fitter and both `cmp` checks passed: weights and report matched
byte for byte on this host. No numerical repair is supported by that
reproduction. Cross-platform floating-point equality is checked separately
by the dataset regression, with an explicit tolerance rather than assuming
bit-identical system math libraries.

The committed Rust audit uses the production parser, fitter, `Local::ask`
and `Weights::judge`, without reimplementing prediction arithmetic:

```sh
cargo test -p lamp-policy --offline --locked --test fit_lamp_scenes -- --nocapture
```

[audit-mac.txt](audit-mac.txt) is its unedited output;
[audit-mac.jsonl](audit-mac.jsonl) extracts the emitted records, including
all 89 predictions and three split summaries. Its in-memory numeric comparison
against parsed historical JSON differs by at most 2.22e-16 in weights and
1.11e-16 in the report; the exact-JSON flags are therefore false. This is
separate from the byte-identical CLI file reproduction above. Comparisons
allow `1e-10 * max(1, abs(actual), abs(recorded))`; counts and nonnumeric
fields match exactly. The regression identifies this historical fit, not
an accuracy target for future models. Deliberate fitter changes require new
identified fit evidence; retain the old recordings, labels and splits.

## Why every test call is held

`attention::Choice::at` fixes Answer at probability ≥0.8 and LookUp at
≥0.4. The fitter does not optimize those thresholds. It fits semantic
question heads on train, freezes them, and fits a logistic choice head on
calibration. The 0.5 local/Jev mixture is also not fitted. These diagnostics
describe the local model alone, before any runtime guard or timely Jev reply.

| Split | Rows / sessions | Directed / other | Directed score range | Other score range |
|---|---:|---:|---|---|
| Train | 35 / 8 | 10 / 25 | 0.306143–0.612317 | 0.227450–0.615051 |
| Calibration | 26 / 4 | 9 / 17 | 0.252252–0.575287 | 0.227450–0.547531 |
| Test | 28 / 4 | 8 / 20 | 0.303956–0.557913 | 0.245145–0.637737 |

All scores are below 0.8. The test result follows directly:

| Independent test label | Answer | LookUp | StayQuiet |
|---|---:|---:|---:|
| Directed: 3 named calls, 5 follow-ups | 0 | 3 | 5 |
| Not directed | 0 | 9 | 11 |

The report's test Brier score is 0.22542972802462938 and log loss is
0.6406479385272718. Zero false answers accompanies zero answers: it shows
abstention, not demonstrated safe separation. Several test negatives rank
above the highest test positive. No threshold sweep or test-set selection
was used to propose a replacement.

The choice intercept is −0.07920269205799525. Its coefficients, in question
order, are 0.43997401979039785, 0.7593627021350859, −1.0702017403001194,
1.0782815720846266, 0.697047903176853, 0, 0. Inputs are question probabilities
minus each head's neutral prior, rounded through the production float32
boundaries. `listening_noise` and `asks_to_stop` have no positive training
labels, remain prior-only, and contribute exactly zero to this choice.
The positive `names_someone_else` coefficient is learned on the supplied
calibration rows; it is not an inverted label or a question-order bug.

## What more data must resolve

Three positive `calls_lamp` labels are not all directed training examples:
seven follow-ups make ten. Calibration has four named calls and five
follow-ups; test has three and five. Near-name negatives are distributed
4 / 0 / 6 across train / calibration / test. The calibration choice head
therefore sees none of that important negative class. Five training rows
with opposite directed labels have identical effective features and cannot
be separated by this model.

All 89 room cues are busy and all acoustic durations are unknown. Name
score is zero in 75 rows. `text_called` is true in three directed and four
non-directed rows. Name score ≥0.999 occurs in three directed and one
non-directed row. Forcing either cue to Answer would be a new policy rule,
not a fitting correction, and this sample does not justify it. In particular
the raw name score is not Voice's retained per-request sure-name latch.

The retained facing cues contain six true, eight false and 75 unknown
values. First [clarified their provenance on #700](https://github.com/autonomous-ai/lampOS/issues/700#issuecomment-6167233618):
Eyes saw colleagues working in the office, not the played speakers; nobody
sat at the lamp. These are bystander observations, not ground truth for
whether the caller faced the lamp. The upstream README was corrected at
`41d09542f`. No row was relabelled, dropped or refitted; the audit preserves
what the judge actually received.

Keep this inspected test set frozen. Future training/calibration should
include independently staged direct calls, nameless follow-ups, near-name
negatives and varied known/unknown cue conditions, split by encounter.
Any threshold objective must be specified before testing and selected on
calibration only, followed by new held-out qualification. Accuracy on these
played voices would still not establish performance on natural office talk.

## The Voice comparison and hand-back

The current test's `same_snapshot` copy permits 21/28 choices, including
14/20 non-directed rows, and misses 1/8 directed rows. First separately
reports no audible intrusion in 63 two-person talk lines, mostly because
the provider proposed no answer. These are different cohorts and stages.
Voice's gate runs only for proposed output; silence is not a Hold verdict.
Permission also does not imply a speaker accepted or audibly played audio.

First can reproduce the fit with the command above without a provider or
lamp. For the next authorized staged capture, implement the
[first-output receipt contract](voice-first-output.md), retaining actual
gate arguments and verdict, request outcome coverage, source event IDs and
a separately declared exact or heuristic Policy join. Keep the existing
seven-field baseline schema; provenance belongs in a separate sidecar.
Report paired gate-eligible rows separately from whole-scene audible results.

Most likely wrong on the lamp: attribution of delayed/unassociated transcript,
name and face observations to a played line; missing-output denominators;
and fitting data that does not cover the eventual use conditions. A forced
call override would hide these problems rather than measure them.

## Local gate

From `lampOS/`, Rust/Cargo 1.96.0, with an isolated target directory and
four build jobs; tests use one thread:

| Command | Result |
|---|---|
| `cargo fmt --all --check` | Passed |
| `cargo clippy -p lamp-policy --all-targets --offline --locked -- -D warnings` | Passed, no warnings |
| `RUST_TEST_THREADS=1 cargo test -p lamp-policy --offline --locked` | 144 passed, 0 failed, 2 existing manual timing tests ignored |
| `cargo build --workspace --offline --locked` | Passed |
| Audit command above | 1 passed; emitted predictions reproduced the tables |

Policy has no dependent workspace crate. [mac-clippy.txt](mac-clippy.txt),
[mac-tests.txt](mac-tests.txt) and [mac-workspace.txt](mac-workspace.txt)
retain output; [source-sha256.json](source-sha256.json) identifies the
checked source. No runtime algorithm changed: the source edit corrects a
snapshot comment, and the new test audits existing production behavior.
The workspace build spanned a host command stall; its 6m50s elapsed time
is not a performance measurement. The required ARM64 qualification is still
outstanding and this task is not ready for integration yet.

## Task-only branch and upstream gate repairs

The task now sits directly on `1733307d7672ad302154984952a77772f8ebd6e5`,
without Fourth's log-branch ancestry, under the one-task pull-request rule.
No model, fixture row, label or prediction changed. The same Mac gate above
was repeated: formatting, strict Clippy and workspace build passed;
144 tests passed, zero failed, two existing manual tests ignored. Complete
output: [mac-pr-base-gate.txt](mac-pr-base-gate.txt).

Native ARM64 run 38557228565 passed its release build and then failed two
unchanged Records JSON helper tests (87 passed, 2 failed), because an ordinary
user could not use `O_NOATIME` on root-owned ancestors. First repaired that
in upstream `21c24fc0a`, included in the new base. Raw failed-step output is
on `lamp-fourth` in `docs/verification/arm64-nonroot-20261014/`. A green
native ARM64 run on this task's eventual head is still required.

## Not shown

No new lamp measurement, provider call, training-data licence determination,
speaker attribution, live human accuracy, audible latency, physical response,
or deployment qualification. The original raw logs are withheld, so Fourth
cannot independently reconstruct their script join or excluded 22 choices.
No actual `voice_first_output` publisher or exact cross-program join exists
as a result of this documentation. The proposal must still be implemented
and measured by the owner of the Voice path.
