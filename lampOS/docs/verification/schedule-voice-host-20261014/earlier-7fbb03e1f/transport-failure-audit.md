# Schedule consumer transport failure evidence

All commands use `/Users/d/code/autonomous-os/lampOS` on macOS 26.6.2
(Darwin 25.6.0, x86_64), rustc 1.96.0 (ac68faa20 2026-05-25).

## Initial shared compile blockers

`cargo check --offline -p lamp-voice --tests` failed because another worker's
declared `tools/schedule_integration_tests.rs` was not yet saved. Raw output:
`/private/tmp/lamp-third-schedule-consumer-voice-check-first.txt`.

`cargo check --offline -p lamp-voice --lib` then passed. Raw output:
`/private/tmp/lamp-third-schedule-consumer-voice-lib-check-first.txt`.

The provider worker's subsequent test compile found this module's Linux-only
`rustix::fs::mknodat` fixture was unavailable on Darwin. Raw output:
`/private/tmp/lamp-third-schedule-voice-reminders-third.txt`.
Correction: common symlink/hardlink/mode/directory/relative-path tests stay
portable; the FIFO trap uses `mkfifoat` only under `cfg(target_os = "linux")`.
That FIFO branch is not exercised by the Mac run.

## Real Darwin datagram backpressure failure

Command:
`cargo test --offline -p lamp-voice --lib reminders::transport::tests -- --test-threads=1`

Raw first result: `/private/tmp/lamp-third-schedule-consumer-transport-first.txt`.
16 passed, 1 failed, 1 ignored (timing probe); the queue-saturation test
`flood_is_backpressured_and_poll_retains_only_one_ordered_packet` failed with
OS error 55 / ENOBUFS from `sendmsg`. This was a real portability gap in the
production adapter, exposed by the deliberately saturated in-process fixture.

Correction: `send_packet` treats ENOBUFS as an unsent transient backpressure
result (`Ok(false)`), alongside EAGAIN and EINTR. It does not loop, discard a
caller message, replace the child, or indicate acknowledgement. The caller
retains the same logical packet for retry. The regression drains every accepted
counter in order, proves no extra packet was accepted, then retries the rejected
counter and proves exactly one delivery.

Primary source consulted 2026-10-14:
https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/sendmsg.2.html
Apple documents ENOBUFS when an internal buffer cannot be allocated and says
the operation may succeed when buffers become available. Its successful return
value is the sent byte count; local errors return -1. The test supplies the
actual observed Darwin queue behavior; this archived manual is supporting
contract evidence, not a lamp measurement.
