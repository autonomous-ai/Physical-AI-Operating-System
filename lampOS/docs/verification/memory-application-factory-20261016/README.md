# Shared Memory application factory preparation

Recorded 2026-10-16 02:11:41 +0700, from `date`. Part of #823; no original clocked scenario is migrated in this change. Kind: default behavior unchanged, explicit internal factory seam only; no new executable, CLI option, environment switch or clock mode.

The binary includes one private `application.rs` and calls `entrypoint(Reporter::start)`. The shared application accepts a `FnOnce(PathBuf, CountSource) -> io::Result<Reporter>`. This factory is invoked only at the original optional-reporting site, after the real socket bind. Default argument parsing, private-directory validation, store opening, serving, log guard, errors, return codes and Reporter finish retain their original order. Constructing the ordinary function value does not run the factory or sample time. A later explicit clock fixture can reuse the actual application rather than duplicate startup.

Reversing the extraction and four factory substitutions reconstructs the complete original main file byte-for-byte. The original three parser test bodies remain at the binary root; their exact compiled names are retained in both list logs. All 23 other original Memory Rust files and all 12 integration files are unchanged. Three added boundary tests each exercise both Legacy and Days using real owned file/socket obstructions: no bus root does not invoke the factory; failed socket ownership precedes the factory/store; a selected factory runs once after socket bind with the exact root/source, preserves the original failed-reporter fallback, and drops the owned socket. They cannot enter the serving loop and start no Reporter thread.

## Validation

Original complete Memory gate on `587a361a3`: formatting; strict all-target clippy; **162 passed, zero failed, five existing ignored measurements**, across 16 suite summaries; workspace build. The original binary identities were also listed explicitly.

Rebased to `fb54b25028a25d3767f968fcc354f2682fe34aa2` with all Memory Rust and its owned documentation byte-identical. Fresh fmt, strict all-target clippy, compiled binary listing, **83 library/binary tests passed / zero failed / one existing ignored measurement**, and workspace build passed. These fresh library/binary results are not labelled as a repeated full integration gate. Exact commands, start/finish times from `date`, source hashes and unedited outputs are retained in the two gate records.

Native ARM64 qualification is pending when the PR opens. It must run all three new boundary cases before this head is ready. No speedup, provider-clock or real-device result is claimed. The source author's handoff predates root execution; the root/integration logs establish the gate results above. Scratch orchestration/reconstruction scripts remain outside lampOS.
