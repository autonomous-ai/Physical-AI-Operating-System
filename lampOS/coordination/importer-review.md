# Importer review regressions

Base: `7edd8d947fbda3d403235f54e7e22ad6f7aad10a` (`lamp-v2-agent2`).
Reviewed importer commits: `02529e7d`, `0a0cfdcf`, `fb096dce`.
This review branch adds only a regression test file and this note; it changes
no importer/runtime implementation and includes no recordings or binaries.

From `lampOS/`:

```sh
cargo +stable test --locked --offline -p lamp-voice-eval --test audit_importer audit_ -- --test-threads=1 --nocapture
```

The exact test file was run in the isolated review copy against byte-identical
`import.rs`, `annotations.rs`, and the same receipt fixture. Result: **1 passed,
2 failed**, exit **101**. The importer modules and regression file were rehashed
against that reviewed copy before publication; no long gates were repeated.

- `audit_malformed_frame_count_is_invalid_evidence_not_a_panic` fails at
  `src/import.rs:498`: a positive but oversized receipt frame count
  (`u64::MAX`) reaches unchecked `frames * 2` and panics in a normal debug
  build. It should produce retained invalid evidence. Release overflow
  behavior was not tested.
- `audit_original_receipt_must_bind_the_wav_reverified_for_scoring` fails at
  `src/import.rs:569–585`: the original receipt is rehashed but its claims are
  not reparsed. Replacing cached WAV/hash/PCM claims selects another recording
  while the original receipt bytes remain unchanged. Review succeeds and a
  synthetic listened annotation scores 1000 ms. Revalidate the selected input
  and derive the canonical WAV/PCM/frame claims from the exact receipt bytes;
  reject contradictory cached claims.
- `audit_missing_receipt_is_retained_as_unmeasured_evidence` passes. A missing
  receipt remains invalid and yields no acoustic measurement.

The synthetic `listened: true` annotation is a **test fixture**, not a claim
that a person listened or a physical timing measurement. The second regression
models modified/misbound cached ledger data, not corruption of existing real
recordings or a cryptographic authenticity attack. Automatic proposals remain
separate from accepted/listened acoustic annotations.

Regression-file SHA-256:
`ce644a56d8451da0c1693d3c49ff946c9de93a76c060c0719f2c4c3adece5050`.
Original red-log SHA-256:
`f15ec8edb5f1cf5d30d7c0f792b50620ac48bc9faa0570fc5507dfed5d3a8e86`.
