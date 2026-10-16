The triage report and raw evidence are copied byte for byte from the external
failure audit. The original 34-entry checksum set was independently verified.

Its complete first-failure log is stored once at
[external/corrected-gate/full.log](../external/corrected-gate/full.log).
[The original-to-canonical mapping](gate/full.log.provenance.json) records the
matching SHA256. Every other original triage file is retained.

The original manifest and checksum list are named `original-file-manifest.json`
and `original-SHA256SUMS`: they describe the original audit folder before this
single deduplication. The verification root's manifest/checksums describe this
repository payload. Historical reports and raw results were not rewritten.
