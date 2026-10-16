# Actor handling and maintenance receipts: first frozen gate

Date from `date`: 2026-10-16 11:14:31 +0700. Base `8de22aef8d5526f83b48b279e71706d71e7cdf5b`; five owned paths only. Reviewed full diff SHA256 `84736e2e20e51b7732bd95cb2baad0e7d23a688487531db2dede9c9fd543a790`.

The exact frozen first gate passed: fmt, strict all-target Clippy, fresh compiled-list comparison, focused 16, full affected 2,727/2,727, workspace build and the five affected doc targets (zero doc cases). Full summary: 1,332.366 s, 15 original ignored cases. No failures, source fixes or retries. Native gate session 98095 is terminal exit 0.

Same affected graph in every list/Clippy/test stage: lamp-gemini, lamp-memory, lamp-surround, lamp-voice, lamp-voice-eval. Fresh clean 8de baseline compiled 2,726 identities / 15 ignores; candidate compiled 2,742 / 15, exactly 16 added, no removed identities or ignore changes. Focused 16 passed in 1.143 s. Every nonignored runtime identity appeared once in the full execution. All 7,560 candidate inputs remained byte-exact throughout.

Rust 1.96.0, locked/offline, jobs 2, incremental 0, default nextest profile, serial, no-fail-fast, retries 0. Raw commands, date starts/ends, exits, metadata, lists, runtime rows, stdout/stderr and source hashes are retained under baseline/ and gate/. The fresh baseline ran no tests.

The first external candidate and root's entry-frontier semantic correction are retained under first-candidate/ and frontier-corrected/. The correction records commands/discards covered at maintenance entry, not late image replay during the pass. Its complete inverse restores the original base. All original test bodies/bounds remain intact; preparation/ contains source/inverse and unowned-file proofs.

Five changed paths are saved verbatim under source/: session.rs, testing.rs, new testing/actor_work.rs, new session/actor_work_tests.rs, and docs/gemini-session-reliability.md. No Cargo, Voice, production policy/timer, original 123-scenario, or default behavior change. These receipts do not certify live idle, service handling, worker/child joins or clock advancement.

Readiness remains dependent on root review and prerequisite #966's requested direct ordinary-versus-retained TaskJoins comparison. This gate is evidence for the exact frozen 8de candidate; it does not include that later test-only correction or an updated-base native run. No commit/push/PR was made here.

The target `/private/tmp/tests-823-rebase-target` is released at the date above: all commands are terminal and no remaining binary inspection is needed. No target cleanup or artifact deletion was performed.
