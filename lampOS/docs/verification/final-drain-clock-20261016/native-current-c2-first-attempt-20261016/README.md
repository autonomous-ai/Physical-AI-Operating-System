# Final-drain current head: first native failures

Recorded 2026-10-16 10:54:13 +0700. This partial first-attempt record is pinned to PR #963 head `c2eacfa4e29a5c9db21d4d919a77d33ce8112238`. It preserves two terminal failed jobs before any test execution. Full shards 1, 2 and 3 remain **pending terminal capture and audit** in this package; their current status is not inferred or polled here.

| Job | Actual checkout | Observed failure | Tests executed |
| --- | --- | --- | --- |
| [Normal run 38846187840, job 116797136519](https://github.com/autonomous-ai/lampOS/actions/runs/38846187840/job/116797136519), attempt 1 | synthetic merge `e7b170fb166ba2f16c6d3f6fa7e5ee0b192eb308` | Release workspace build: E0063, missing `Reply.say` at `coordinator.rs:6209:23` and `:6279:19`; exit 101 | 0 |
| [Full run 38846183455, shard 4/job 116797126853](https://github.com/autonomous-ai/lampOS/actions/runs/38846183455/job/116797126853), attempt 1 | exact branch head `c2eacfa4…` | Dependency retrieval: crates.io `ho/un/hound` download, curl `[16] Error in the HTTP2 framing layer`; exit 101 | 0 |

Both job APIs mark nextest and documentation/custom-harness stages skipped. Neither complete log contains a test-run header or test-result row. These are a source compilation failure and a registry transfer failure respectively, not failed test cases. No E0061 is reported in the normal log; this does not predict which diagnostics a later corrected source would produce. The HTTP2 diagnostic does not identify whether a client, intermediary or remote server caused the transfer failure.

Normal merged the branch with newer upstream `8da7d7ee175da8aa2b77717e189f540eb3904569`. The failing complete `coordinator.rs` is byte-exact to that upstream parent and differs from c2. `source-input-proof.json` records 142 head-to-normal differences, each exactly inherited from that parent. All three owned fixture/doc files and all 53 historical evidence files are exact at the normal merge. The complete coordinator sources and their diff are retained, including the `say` field definition and both actual missing-field initializers. This source provenance does not turn the failed native build into a pass.

The complete normal log is `normal/job.log`, SHA-256 `40823d902d2869d25db480701b1914212c3a9f9ebea6638776dc662382cfa1e8`. Full shard 4's complete log is `full-shard4/job.log`, SHA-256 `145700567748d132015d45c863f61d2599344823e0f85f4aa04cc9ede96447dc`. Raw job metadata, merge/parent commit records, capture commands, source hashes and numbered excerpts are retained unchanged. Two complete recursive API trees remain preserved externally with hashes in `provenance.json`; they are omitted from this compact payload.

`expected-full-membership.json` pins the immutable original 123 identities, the locally compiled 521 identities and six fixture additions for later terminal comparison. Expectations are not executions. Complete native coverage of those sets is **not established**, and neither native run is green. The existing current-base local 521/0/0 qualification remains a separate result; it does not replace failed native qualification.

No job was retried and no source was edited by this audit. The owner is integrating a genuinely changed upstream source and owns its later gates. Any later successful run must be a distinct result; these first-attempt failures remain intact. No readiness, complete-123, speedup, device or root-cause claim beyond the observed diagnostics is made.

All payload files are regular UTF-8 text, mode 100644. There are no Python/scripts, binaries, symlinks or executable-mode files. Verify `SHA256SUMS` from this directory; the external destination manifest enumerates the exact publication set.
