# Current-base clock migrations and native outcomes

Recorded from shell `date`: 2026-10-16 03:52:17 +0700. No retries, original assertion changes, workflow/profile changes, device use or cloud-provider calls.

## Native qualification

Reporter scheduling [#910](https://github.com/autonomous-ai/lampOS/pull/910), exact head `fd3a2764cb6624ac825fa7a044317d9080ff1e3d`, passes [38801802028](https://github.com/autonomous-ai/lampOS/actions/runs/38801802028), attempt one: **4,902 unique passes, 130 profile exclusions**, all eight new schedule cases once. The doc/own-harness and artifact stages succeed. The PR is ready for the reviewer; it migrates no original scenario. See [`native/910-38801802028-audit.json`](native/910-38801802028-audit.json) and unedited log/metadata beside it.

Serve [#909](https://github.com/autonomous-ai/lampOS/pull/909), first head `188d97b7fcb6a78adaec67a3d3939843f759e1d3`, has two retained failures:

- Normal [38800988605](https://github.com/autonomous-ai/lampOS/actions/runs/38800988605): 305 pass / one fail / 130 profile exclusions, with 4,633 left unrun. The unchanged Body queue-age test failed on a checkout preceding the actual owner fix #902. The new local integration is rebased onto that fix.
- Full [38801026642](https://github.com/autonomous-ai/lampOS/actions/runs/38801026642): **3,845 unique passes / one failure**, three complete passing shards and one shard stopped after 101 pass / one fail, leaving 1,225 unrun. Newly observed Body readiness failure is [#914](https://github.com/autonomous-ai/lampOS/issues/914), outside the original 123. Its underlying cause remains unproven; [source triage](native/909-body-stop-triage.md) retains the exact observed authority rejection and hypothesis limits. #900 is not established as a fix.

The full run observed **93/123 original identities** and **13/14 Serve identities**. [`native/909-38801026642-audit.json`](native/909-38801026642-audit.json) lists every missing original identity. The 13 Serve timings sum to 71.114 s across concurrent separate hosts; this is not a serial benchmark or complete coverage. Serve remains draft pending complete native qualification. No failed head was rerun during this triage.

## Fresh local integration gates

Each complete sequence used Rust 1.96, locked/offline Cargo, two build jobs, no incremental compilation, its dedicated target directory and real authorized local IPC/processes. Raw commands and shell-date boundaries are in each `root-gates.json`; raw logs, counted results and source hashes are retained beside them.

| Integration | Base | Complete result | Original identity comparison |
| --- | --- | --- | --- |
| Serve only | `48b3737d5e83efa00278172cb602d3525d368a43` | **422 pass / 0 fail / 0 ignore**, finished 03:44:54 | all 14 Serve names retained |
| Serve + later Recorder | same | **463 / 0 / 0**, finished 03:37:40 | all 14 retained |
| Reporter + later Memory health | same | **296 / 0 / 6 existing ignores**, finished 03:38:09 | all seven health names retained |

All three pass `cargo fmt --all --check`, strict all-target Clippy for their touched/dependent crates, the full serial Cargo tests and `cargo build --locked --offline --workspace`. Exact commands are recorded rather than inferred from this summary. Serve source is unchanged across its rebase; its six extra cases over the earlier 416-case gate come from merged #894. Recorder and Memory remain local separate continuations until their respective preparation PRs merge. None of these local gates is an ARM64 result.

Original checks remain: Serve has 14 identities / 114 assertion macro sites / 22 expanded attempts; Recorder controls 13 identities / 21 attempts; the remaining delayed-transport case retains host time in that continuation. Memory retains seven identities, 33 scenario assertions and nine actual children (five controlled, four ordinary). The historical same-helper timing pairs stay separately identified in earlier verification folders; no new timing claim is derived from these full-gate durations.

The scratch Python collector lived outside the Rust-only repository, copied files byte-for-byte, counted standard libtest/nextest output and wrote hash inventories. No binary artifacts are included. The combined original-123 serial under-two-minute target remains unproved.

Unedited runner logs retain their original trailing whitespace and final blank lines. Whitespace checks apply to authored documents and metadata; raw logs are verified by byte equality and SHA-256 instead.
