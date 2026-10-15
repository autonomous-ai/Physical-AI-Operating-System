# Provider Moment preparation: local gate remains red

Recorded 2026-10-16 02:57 +0700, from `date`. The exact local gate finishes **1,741 passed / four failed / eight existing ignored**, exit 101. Formatting, strict Voice/Evaluator all-target Clippy and the separately completed workspace build pass. All **13 new Moment tests and seven original clock tests** pass. `root-gate-handoff.md` gives the exact commands, original preservation and limits; raw logs and their recorded SHA-256 are retained unchanged.

The four failing identities are three `coordinator::motor::tests` cases (two missing `child-position-writes` files and a denied privacy-release assertion), plus `second_route::without_a_second_route_the_packets_and_records_are_as_before` with stale service privacy. These source files are unchanged; no baseline run was made, so this alone does not establish causality. #890 already tracks the second-route failure. Motor owner routing follows the source audit. The complete test gate is not declared green and this preparation is not published as ready.

The earlier sandbox invocation could not bind local sockets and was interrupted. Its raw output and explicit invalid-qualification record remain separately retained. The twelve ambiguous failures from that denied run all pass in the authorized local-socket invocation. That environment correction is distinct from a flaky retry; the four actual failures above remain.

This is x86_64 macOS local debug qualification, not native ARM64, a nextest leak check or #823's combined serial target. All seven old clock bodies/43 assertion sites and 160 other Voice Rust files are byte-identical. No provider, Tokio, capture-clock or shutdown-control migration is in this preparation, and no retries or assertion changes were made.
