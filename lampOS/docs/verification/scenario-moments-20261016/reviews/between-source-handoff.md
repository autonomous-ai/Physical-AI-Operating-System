# Pure Moment interval projection handoff

Prepared 2026-10-16 04:30:05+0700, from shell `date`. Source is held in `/private/tmp/lampos-tests-provider-moment`. No Cargo, builds, tests, Git, network, comments to GitHub, or device access were performed.

## API and behavior

`Moment::between(&self, later: &Moment, elapsed_from_origin: Duration) -> io::Result<Self>` validates both endpoint values, live Arc origin identity, endpoint order and inclusive bounds, then constructs through the existing checked epoch conversion. The argument is the absolute elapsed offset from the originating root, not a delta from `self`. Exact nanoseconds remain in `elapsed` and `Instant`; only the existing monotonic-microsecond value truncates.

Projection is an immutable value operation. It does not read host time, access TLS, pump, advance a root, admit an observer, or change root lifetime. Root invalidation can race after any successful validation, as with the existing snapshot API; later observer admission/projection validates again. A returned Moment is never a progress receipt.

## Preservation and focused boundaries

`source-proof.json` and the repeatable `verify-source.py` prove:

- Removing only the added method/module recreates the existing uncommitted preparation byte-for-byte. Composing the recorded previous reversal recreates its original pre-Moment source byte-for-byte.
- All seven original root tests (43 assertion macros) and all thirteen existing Moment tests (78 macros) remain exact. The original root test module SHA-256 is `ef98675980c838fc13ad4632aa2ecdcb0aa04f2df9dd509c499063ed5f1add20`.
- All 162 other existing Voice files remain exact. No dependency, transport, worker, Gemini, pump, guard, Shutdown or default-path source changed. Documentation reverses independently to both the prior preparation and its original source.
- All 81 files in the old preparation evidence directory are unchanged, including prior interrupted/local-socket failure logs and triage. Their hashes were captured before any edit. No earlier failure is reclassified as a pass.

The one new module has twelve cases and 67 assertion macros: inclusive endpoints and actual 1 ms interior; absolute offsets above zero; equal endpoints; outside/reversed bounds; genuine submicrosecond snapshots and observer ordering; same-valued actual independent TLS roots; invalid endpoint values; last microsecond, microsecond overflow and platform Instant overflow; projection on an external thread without implicit admission; normal root Drop, callback panic and TLS teardown invalidation. Read-order/identity/boundary instrumentation is explicitly distinguished from a scheduler clock. Threads that wait on a handshake use bounded channels, are joined, and use scoped ownership/release-on-disconnect where a live foreign root is held. There are no sleeps, retries, child processes or timing guesses.

## Verification and remaining work

Direct pinned Rust 1.96 rustfmt and `--check` passed for only the two owned Rust paths with `skip_children=true`. The new cases have not been compiled or executed. Source preservation is not runtime qualification, and this preparation claims no migration, speedup, full-suite result or completion of #823. Root owns target allocation and gates.

Recommended focused command from the worktree’s `lampOS` directory, using root’s allocated target:

```sh
cargo test --locked --offline -p lamp-voice --lib scenario_clock::moment_between_tests:: -- --test-threads=1
```

Then run the `scenario_clock::` filter to include the preserved seven root and thirteen Moment cases, plus root-owned required lint/build/affected-suite gates. Preserve fresh command/exit evidence separately from the old failure directory.

## Final source hashes

| Path | SHA-256 |
| --- | --- |
| `lampOS/crates/voice/src/scenario_clock.rs` | `7ebed538d61d20442e4fe3cc0e2a0b0987cf18a599b1283e511954f54de482d5` |
| `lampOS/crates/voice/src/scenario_moment_between_tests.rs` | `ab7d7f015c077c6ee8bc43cc48bceaa8c723e85f922547f8ec2be4745134b0b4` |
| `lampOS/docs/voice-test-fixtures.md` | `63fbd6fc9d456251f1151a4e65bc6ac96fcd78e2458503e03679897d0a2366fc` |
| `lampOS/crates/voice/src/scenario_moment_tests.rs` | `c372eac927457059471d7a6bc3b56c8855f530aab1b7b81e11305fe2dddfe210` |

## Separate worker plan revision

`/private/tmp/tests-823-provider-worker-next-plan-revised.md` incorporates the four required corrections: independent exact elapsed admission versus asymmetric LinkClock checks; owner/setup ordering and failure closure; one-way frozen-shutdown phase/deadline/permit ownership; causal poll/notice/Stop/outer-result/probe receipts. Its 21-row matrix explicitly separates Moment, first worker preparation and later Actor/service/controller/integration obligations. It introduces no global completion claim and retains original deadline origins.

Revised plan SHA-256: `28c8b181def88a3bb495e94c96671ac9655a76611384ad893dbaf4463bbdb51f`. The raw original remains `667e02178e78f3e41a24e197e75c0d2ca90c7059c6d1fb1dce0bcdfdc85b6ad1`; `worker-plan-revision-proof.json` records exact reversal and no raw-plan edit.
