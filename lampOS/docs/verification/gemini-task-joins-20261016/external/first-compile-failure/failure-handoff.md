First qualification failure — unchanged frozen source

Recorded from date: 2026-10-16 09:13:38 +0700.

Exact reviewed diff: 4f79ebfd96a8f56e5cc93e4e03c9caeb24d5b27f5d11175150c4349633bf624f.
Base: 1fb07c6f57ee280a49de7b4ddd74008c2a32a20d.
Tool session 78308 is terminal, exit 101; no Cargo process remains from this gate.

Fmt started 09:10:54 and passed at 09:10:59. Strict five-package all-target Clippy
started 09:10:59 and failed at 09:12:08, exit 101. Full first log: clippy.log
(SHA 94dd432514ad8986f066d2d5ad16ed1f998da6662c03e0e89fb9b0c56ec7381c). The actual failure is compiler E0277 in the new
poison-registry regression at testing/task_joins/tests.rs:674: catch_unwind captures
Arc<Shared>, which contains AtomicWaker and is not RefUnwindSafe.

No changed-source compiled list, focused list, test execution, full suite or
workspace build ran. The existing pre-edit 2,708/15 baseline remains valid; no
changed-source identity comparison or green test claim exists. All 7,152 source
inputs remain byte-exact to the reviewed source (source-after-stopped.json).
No source edits, retries, Git/GH, device actions or target changes occurred.

Proposed review-only correction: proposed-test-only-correction.diff. Capture the
Mutex reference (&registrar.0.state) rather than cloning the entire Shared; lock
that same reference inside the existing caught deliberate panic. This narrows
the unwind boundary to the Mutex whose poison behavior is under test, without
asserting unwind safety for AtomicWaker. All conditions, panic text, task result
and expected incomplete report remain the same. No production or original test
source changes are proposed. Candidate source and exact hashes are preserved in
proposed-test-only-correction.rs/.json; the inverse restores the first source.
The correction is NOT applied and has NOT been compiled or run. Awaiting root
review before any edit or changed-source re-execution.
