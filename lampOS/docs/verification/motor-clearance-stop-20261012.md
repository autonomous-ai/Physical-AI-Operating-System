# Motor clearance and measured-stop verification

Code: `7ca43da4a85f644ee51ded6a52901de323d88fbf`, on
`4e54f83c7` (pure clearance policy), based on integration `10607a439`.
Implementation worktree: `/private/tmp/lamp-servo-v1-port-20261012`.
No device, serial hardware, servo, camera or audio action by Main in this batch.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --offline -p lamp-motor -p lamp-live --all-targets -j 2 -- -D warnings` | Host pass |
| `cargo test --locked --offline -p lamp-motor -j 2 --no-fail-fast -- --test-threads=1` | Host: 187 passed, 0 failed, 0 ignored |
| `cargo test --locked --offline -p lamp-live --lib coordinator::motor::tests -j 2 -- --test-threads=1` | Host: 28 passed, 0 failed, 0 ignored |
| `cargo clippy --locked --offline -p lamp-motor --all-targets --target aarch64-unknown-linux-gnu -- -D warnings` | Pass |
| `cargo test --locked --offline -p lamp-motor --target aarch64-unknown-linux-gnu -- --test-threads=1` | QEMU: 192 passed, 0 failed, 0 ignored |
| `cargo build --locked --offline --release -p lamp-motor --bins --target aarch64-unknown-linux-gnu` | Pass |

ARM used the existing `localhost/lampos-arm64-cross:latest` image, one owned
`main-servo-port-arm` container, a 480 s hard timeout and two build jobs. It exited
0. The source-only archive has no dependency on the parent repository. All 41
checked files (the motor tree and the affected live motor test file) match
that archive byte-for-byte at the code commit above. No runtime binaries were
committed or deployed. The full workspace suite was not rerun.

The initial sandboxed run could not create the owned child-process IPC resources;
the completed host gate ran outside that sandbox. Existing cancellation tests
were updated to distinguish immediate command inhibition from the subsequent
measured hold and to continue polling before final cleanup. They do not redefine
a successful serial write as physical stopping.

Independent read-only review found two defects in the draft: source measurements
could expire during the hold write; direct cleanup could leave the new attempt
pending without a flush or explicit failure. Both were corrected before these
gates. Production-path regressions retain accepted bytes after late writes and
assert failed/abandoned outcomes with exactly one discard. No red/green device
claim is made.

Local retained artifacts (not uploaded):

| Path under `/private/tmp` | SHA-256 |
|---|---|
| `lamp-servo-port-source.tar` | `a28536302414e24b038ac2001443fcaa17b84b856246d8fd684adbb661e336be` |
| `lamp-servo-motor-test-03.log` | `1c5d64f680af44077041df21bec734059a482e3274fb2e2962bcb6619e56797e` |
| `lamp-servo-arm-01.log` | `3b670b03f29be652b50d345aeb1e5294dac2f7d63090694a9665be52d5f1863a` |
| `lamp-servo-composed-clippy.log` | `b6f620595481f6a5c137295a2bcc2190a493916650797ebb67d33cc4d82268bb` |
| `lamp-servo-live-tests.log` | `a7caf5e8e89a8147147fb885da46395d898d51b5889e7bda37a93e8785d4ef11` |

Physical stopping time, force, clearance and the known head/base contact remain
unqualified. Synthetic cells are test fixtures only. Next physical work requires
an observed, clear pose and a per-device verified envelope; do not reuse the
colliding trajectory or infer safe angles from the old per-joint limits.
