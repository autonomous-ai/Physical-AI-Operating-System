# Frozen original-123 serial ARM64 measurement

Prepared from `date`: 2026-10-16 06:47:21 +0700. This is a concrete request to the human DevOps owners of #788, in support of #823. It changes no workflow, profile or runner. A qualified integrated source SHA will be supplied after the migrations merge; this package does not claim that current code meets the target.

`original123.filter` names every frozen binary/test pair with exact `=` matching: **123 distinct cases in 11 binaries**, with no helper additions. `expected-identities.json` is the canonical sorted selected set. Both are derived from the inventory at `4596d2cac9e25e31bc2b3509ad565ffde66a5ab1`, whose SHA-256 is `72d671f8b22246898f9142c0216c9fbced55e350e46b83ffec787c81496fb1fc`. The clauses select all 123 once from the observed 5,205 outcomes of full native run 38820115320. That validates identity membership, not serial timing. The installed nextest filter reference defines the exact-match syntax used here. Actual compiled selection must still pass the comparison below on ARM64.

## Requested execution

Use one existing Linux ARM64 worker and the project's pinned Rust toolchain, release configuration and default features. Keep its model inputs, dependencies and fixture behavior the same as normal full CI. No `--partition`, other concurrent test shard, retry, or realtime skip. New helper regressions continue to run in the ordinary complete gate; they must not enlarge this historical 123-case denominator. Record the runner hardware/OS, source SHA, Rust/nextest versions and host load. Compile and enumerate before measuring the test phase. A failure remains a failure and is retained rather than rerun for a green measurement.

The following Bash commands run from `lampOS/` in the qualified source checkout. `proof_dir` is the directory containing this pinned request package; `output_dir` is a new evidence directory outside the source tree. `jq` is used only to compare nextest's actual selected identities. Its `test-count` field alone is not that selection count.

```bash
proof_dir=/absolute/path/to/original123-serial-request-20261016
output_dir=/absolute/path/to/new-serial-evidence
set -euo pipefail
mkdir "$output_dir"
selector="$(cat "$proof_dir/original123.filter")"
git rev-parse HEAD > "$output_dir/source-sha.txt"
uname -a > "$output_dir/host.txt"
nproc >> "$output_dir/host.txt"
uptime >> "$output_dir/host.txt"
rustc -Vv > "$output_dir/rust-version.txt"
cargo nextest --version > "$output_dir/nextest-version.txt"
date '+%Y-%m-%d %H:%M:%S %z' > "$output_dir/list-started.txt"
cargo nextest list --locked --release --workspace \
  --profile realtime --ignore-default-filter -E "$selector" \
  --message-format json > "$output_dir/selected.json" 2> "$output_dir/list.log"
date '+%Y-%m-%d %H:%M:%S %z' > "$output_dir/list-finished.txt"
jq '[.["rust-suites"][] | . as $suite | (.testcases // {}) | to_entries[]
  | select(.value["filter-match"].status == "matches")
  | select(.value.ignored == false)
  | {binary: $suite["binary-id"], test: .key}]
  | sort_by(.binary, .test)' "$output_dir/selected.json" \
  > "$output_dir/actual-identities.json"
cmp "$proof_dir/expected-identities.json" "$output_dir/actual-identities.json"
date '+%Y-%m-%d %H:%M:%S %z' > "$output_dir/run-started.txt"
uptime > "$output_dir/run-load-before.txt"
set +e
cargo nextest run --locked --release --workspace --color never \
  --profile realtime --ignore-default-filter -E "$selector" \
  --test-threads 1 --retries 0 --no-fail-fast --no-tests=fail \
  --status-level all --final-status-level all > "$output_dir/run.log" 2>&1
run_status=$?
set -e
printf '%s\n' "$run_status" > "$output_dir/exit-code.txt"
date '+%Y-%m-%d %H:%M:%S %z' > "$output_dir/run-finished.txt"
uptime > "$output_dir/run-load-after.txt"
```

`--ignore-default-filter` makes the explicit fixed inventory authoritative even if the profile's broader selection later changes; it does not unignore tests. The exact JSON comparison rejects any missing, ignored, renamed or additional selected identity. All 123 were nonignored in the frozen inventory. In particular, the Linux-only capture-worker cases cannot be certified from a Mac list.

## Acceptance and handback

Return the complete output directory and run URL, without editing raw outcomes. Require a zero exit code and exactly one PASS outcome for each of the 123 selected identities, with no duplicate, FAIL, LEAK or retry. Report nextest's **single test-phase elapsed time under 120 seconds** as the acceptance boundary, plus the outer shell-date bounds. Also group the recorded case times by binary for comparison with the frozen historical baseline. The sum of four concurrently running shard durations is not this measurement.

For a comparable new before/after experiment, use the same runner class, release mode, fixture inputs and scheduling conditions, retain both exact source SHAs and selected lists, and say if host load was uncontrolled. The historical 977-second inventory is not an isolated paired baseline. Do not add timing observations from different source heads together to claim a result for an integration that never ran.

## What has not been shown

This package has not executed a standalone serial ARM64 run or proved the two-minute target. It supplies a fixed selector and concrete protocol for DevOps. Code migration, source/identity preservation, ordinary full gates and each PR's own native qualification remain Tests' responsibility. Workflow changes, runners, concurrency and repository settings remain DevOps' responsibility.
