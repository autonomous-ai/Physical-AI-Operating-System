# early_end on one logical clock: before and after (Intel Mac, 2026-10-16)

For #823. Not a lamp result: `lamp-voice`'s unit-test binary on the tests
builder's Mac (16 logical CPUs, other builders' work running), built from
`origin/lamp-voice` at `42c4dec84`, once as it is (`base`) and once with
this change (`stepped`). Nothing in these files is edited.

- `base.txt`, `stepped.txt`: `lamp_voice-<hash> coordinator::tests::early_end:: --test-threads=1 --nocapture`,
  one after the other, the first line and the last the times from `date`.
  19 passed in 93.17 s and in 16.26 s. Each conversation prints one summary
  line (`<case>: connections …; … blocks played …; last sound to first block
  handed to the speaker …`). They are the same in both but for times within
  6 ms (602 ms on the logical clock's 2 ms step) and, in the cut-in case
  (`with the switch: Hey Lamp, 0.7 s, the question, stop`), 94 blocks played
  against 91. The provider's stderr diagnostics (`barrier Settled`'s
  `waited_ms`, `retained_samples`, withheld frames) differ by under one
  40 ms packet; their order among the tests' lines differs too. None of
  these is asserted.
- `cut-in-repeats.txt`: that case alone, five times stepped (94 every time)
  and three times base (92, 91, 92).
- `loaded.txt`: the stepped module in six processes at once, three rounds
  without `--nocapture` and one with: 24 of 24 passed, 23 to 32 s each. The
  MD5 of each run's sorted summary lines equals the unloaded run's.

The gate on the change: `cargo fmt --all --check` clean; `cargo clippy -p
lamp-voice --all-targets -- -D warnings` clean; `cargo build --workspace`
ok; `cargo test -p lamp-voice --no-fail-fast`: lib 1,348 passed, 31 failed,
10 ignored, and `fixture_provider` 26 passed, 1 failed. The 31 are
`coordinator::motor::tests` (21), `process::ring_startup_tests` (6),
`coordinator::optional_ring*` (2), `coordinator::tests::termination` and
`coordinator::tests::reminder_roundtrip`. Run again together (eight
threads) they failed 8 of 31 on the base binary and 9 on the stepped one;
one at a time, 31 of 31 passed on both. The `fixture_provider` failure
(`stale_cue_and_peer_loss_latch_invalid_without_blocking_or_retrying`, an
integration test this change does not compile into) passed 27 of 27 when
its binary ran again.
