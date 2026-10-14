# Tough dependency snapshot

Tough 0.24.0 from crates.io, under its MIT OR Apache-2.0 license. Runtime
source files are unchanged; `PROVENANCE.json` records their SHA-256 hashes
and the upstream crate archive checksum. Both upstream licenses are included.

The normalized Cargo manifest has one runtime change: the unconditional
`rustls` dependency has `default-features = false`. Upstream enables the
AWS-LC TLS provider even when Tough HTTP is disabled. That unifies with the
existing Voice Ring provider and makes implicit Rustls configuration panic.
Surround supplies its bounded HTTPS transport with an explicit Ring provider.
Tough still uses its unchanged AWS-LC implementation for TUF signatures.

This library-only snapshot omits upstream integration fixtures, tests, lockfile
and their manifest targets/dev-dependencies. No cryptographic or verification
source is changed. The original authored manifest is kept for comparison.
The package is excluded from workspace membership. Surround's signed-fixture
integration tests exercise this dependency, and its TLS regression test checks
that adding it leaves the existing Voice default provider usable.

Replace this snapshot when a reviewed upstream release makes TLS provider
selection optional. Do not restore upstream Rustls defaults without running
that regression against the whole workspace feature graph.

Not shown: the upstream project's own complete test suite is not run here.
