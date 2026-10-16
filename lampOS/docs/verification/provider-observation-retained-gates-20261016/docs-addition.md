## Provider body observations

The explicit test-only `provider_worker::observation::coordinated` factory
binds an owned observation before the original provider body is first polled.
Ordinary constructors stay unattached. Both entrypoints use the same original
body, raw clocks, 2 ms Skip interval, control/input/output budgets and channels.
No existing real-provider scenario selects this observation yet.

The intake records an explicit Stop or authority revocation only after its
original handler has stopped intake. A generic error that clears intake is
neither cause. A sent or accepted datagram is not a handled command: the new
real-channel cases cover a queued Stop before polling, the original full
16-command control slice, actual permission/generation changes, and error
cleanup. Initial denied authority is not relabeled as revocation.

A separate wrapper records the original body future's actual successful or
failed return and preserves its Result object. Never-polled Drop and Drop after
entry remain separate from a return; neither identifies cancellation or panic.
The actual task owner must await its JoinHandle and keep its real result.
Component tests distinguish handler receipt from future return, join a
requested cancellation, and verify an unchanged panic payload. Held connection
attempts expose their own future-Drop receipt; that receipt is not a private
task join. New asynchronous waits share one immutable 10-second host deadline
per test, on an unpaused runtime. Failure cleanup requests cancellation without
claiming that request as a join.

Each worker identity retains its Arc allocation, and each snapshot reads one
bounded atomic state. No observation reads a clock, waits, performs I/O or pumps
work. Invalid publication latches an observation failure without changing the
real worker Result. There is no growing counter, event log or retry. An earlier
snapshot is historical evidence, not a lease on later health.

These receipts establish neither a completed bounded worker poll nor downstream
Actor/service progress. They do not establish Actor, runtime, thread or child
exit. Successful IPC, handled Stop, body return, task join and actual child reap
remain distinct boundaries. Clock admission, controlled scheduling and the
original early-end scenario migration remain separate work.

