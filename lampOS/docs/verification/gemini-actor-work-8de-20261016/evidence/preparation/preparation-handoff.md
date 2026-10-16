Actor work preparation on published 8de — 2026-10-16 10:46:21 +0700

Worktree: /private/tmp/lampos-tests-gemini-actor-work
Branch: tests/823-gemini-actor-work
Base: 8de22aef8d5526f83b48b279e71706d71e7cdf5b
Only the five reviewed paths are changed. Their complete diff is byte-exact to
the approved external candidate, SHA 84736e2e20e51b7732bd95cb2baad0e7d23a688487531db2dede9c9fd543a790.
The maintenance-entry correction remains SHA 2645a9bdec131d005fd97bd03ea2c2017b2065bcc5f3ac031c8a31dc082079db.
Both first candidate and corrected external evidence remain unchanged.

The clean capture covers all 7,558 tracked paths, including the symlink which rg
alone omits; its link target bytes are recorded. All 7,152 previously qualified
inputs match. Additional tracked paths are verification material or files outside
the Rust workspace. The candidate has 7,560 inputs: three existing files changed,
two added, and all 7,555 unowned inputs retained exactly. The original test bodies,
assertions, bounds and default operation order have their earlier inverse proof.

The target was released after the initial no-Cargo preparation. Before the first
executable gate, only our five paths were mechanically restored to the captured
clean base. A fresh full five-package metadata/list build ran in this same
worktree and target: 2,726 identities and 15 ignores, exactly matching the prior
compiled baseline; zero test executions. Then the identical reviewed five files
were reapplied and frozen again. See /private/tmp/tests-823-gemini-actor-work-8de-baseline/terminal.json and its raw logs/source
proofs. Baseline native handle 33334 is terminal exit 0.

The reviewed source is now in its first executable gate under native handle
98095. At the time of this note fmt, metadata equality and strict all-target
Clippy passed; candidate-list compilation is active. This note does not claim
any Actor tests have passed. Complete stage status belongs to
/private/tmp/tests-823-gemini-actor-work-8de-gate, not this preparation manifest.

Prepared gate script: /private/tmp/tests-823-gemini-actor-work-8de-gate-runner.py
SHA: c8910c47ea017acc28c994982878163d23fbc0cdc986b183956bbff1eed648bb
It uses Rust 1.96, locked/offline, jobs 2, incremental 0 and the released
/private/tmp/tests-823-rebase-target. Every list and run retains the same five
packages (Gemini, Memory, Surround, Voice, Evaluator). The compiled candidate
must be the fresh baseline plus exactly 16 identities, with the same 15 ignores.
A second list must select exactly those 16. Their first run is serial, default,
no-fail-fast, retries 0; only then the complete same graph runs with those
settings, followed by workspace build and separate five-package doc tests.
Execution parsing uses only progress rows before the final Summary, preserving
failure recaps separately. Original failures do not trigger reruns; source/lint
or new-test failures stop for review. Source hashes are checked before each
stage and at termination. No source edits are allowed during this gate.

No commit, push, pull request, repository setting, network operation or device
access occurred. The local worktree creation was initially rejected by automatic
review due to the earlier pipeline instruction; the same operation was approved
once the later tests/#823 and explicit CLAUDE builder-exception scope was supplied.
The first inventory-only assertion and its symlink-accounting correction are
retained in preparation-events*.json. They are not test failures or test retries.
