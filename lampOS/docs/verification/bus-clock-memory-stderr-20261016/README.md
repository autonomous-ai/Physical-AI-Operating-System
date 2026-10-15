# Bus clock gate: existing Memory stderr failure

Recorded 2026-10-16 01:15:04 +0700, from `date`. Source base `6b08a70d56e37b061e7a9a6b013072b13639a0b7` plus the explicit Bus publisher cadence-clock preparation in `tests/823-bus-publisher-clock`. All 24 Memory Rust files remain byte-identical to base, as [source preservation](source-preservation.json) records.

The full serial Bus + Memory gate ran once: **249 passed, 1 failed, 6 existing ignored** across 22 summaries including two Bus doctests. All nine new Bus cadence-clock tests passed, as did all seven existing Memory health tests. Formatting and strict Bus/Memory all-target clippy passed. Workspace build did not run after the failed test stage. [Exact commands and date boundaries](gates.json); [unedited test log](tests.log). No retry was made.

The failure is unchanged `lamp-memory::binary a_relative_path_or_an_unknown_option_gets_the_usage_line_and_exit_code_two`, at `crates/memory/tests/binary.rs:217`. The child returned status 2 and stderr began with `lamp-memory: `, but only `lamp-memory: --socket is required\n` was captured; the final exact usage line was missing. The parent used `Command::output` and drained stderr after normal exit. The child emits the error and usage together before returning with its log-relay guard.

This symptom matches open #837/#862, which already name Memory's usage test. The log does not prove the precise relay interleaving. It is additional failure evidence for the relay owner, not a new failure exemption or a complete Bus qualification. The Bus preparation stays local while #881/#886's owner fix qualifies. Original assertions, test membership, intervals, and all Memory product/test source are unchanged.
