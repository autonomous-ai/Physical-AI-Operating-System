# Native ARM64 result for the preceding c0 checkpoint

[Run 38620759412](https://github.com/autonomous-ai/lampOS/actions/runs/38620759412)
completed successfully at 2026-10-14 19:36:46 UTC for
`c0bd596ac15ca64d7dc09534a65165e2be1eab03` on `third/schedule-voice`.
It used a GitHub Ubuntu 22.04 ARM64 runner with Rust 1.96.0, not a lamp.

The release workspace build and serial workspace tests passed:

```sh
cargo build --locked --release --workspace --bins --examples
cargo test --locked --release --workspace -- --test-threads=1
```

The raw log contains **4,334 passed / 0 failed / 33 ignored** across
279 completed test harnesses. Build step: 19:06:23–19:12:04 UTC;
test step: 19:12:04–19:36:31 UTC. Those GitHub step boundaries are not
interaction latency. Binary checksums and artifact upload also succeeded.

The [terminal result](result.json), [unedited log](run.txt) and
[retention index](retention.json) retain exact identities and output. The
workflow keeps its artifacts for seven days; no binary is committed here.

## Not shown

This run precedes the revised room policy and added claim-cleanup regressions.
It does not qualify their source. Native CI must pass on the offered task
head before the task is ready. No install, acoustic timing, real provider,
motor motion, physical privacy or lamp observation was performed by Third.
