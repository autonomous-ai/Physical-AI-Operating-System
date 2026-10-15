## Explicit transport success observations

Test-only `Channel::install_probe()` attaches a bounded observer before any
send or receive attempt, including an attempt that fails or finds no packet.
Ordinary channel, worker-channel and reference-channel constructors remain
unobserved. No probe API or state exists in non-test builds.

A probe records only an actual successful kernel send or a fully decoded and
validated accepted receive, after the original sequence/receipt commit. It adds
no I/O, receive, clock sample, pump, advance or await. Original receive samples,
boot/order/age validation, startup grace, stall handling and gap acceptance stay
unchanged. Failed sends and refused/empty receives do not increment observations.
Probe faults do not change the transport operation's actual success or error.

Snapshots retain an allocation identity for that exact channel, including after
the channel and probe drop. Each direction records its configured boot, accepted
count, actual last sequence, consecutive prefix from sequence 1, and first gap.
An accepted gap stops that prefix permanently; a highest sequence or count alone
cannot prove complete consumption. State has fixed size and stores no event log.

Publication and snapshot reads are nonblocking. A snapshot makes one bounded
consistency attempt; `Busy` supplies no receipt and never implies completion.
Counter/publication overflow, invalid state and duplicate/late installation
latch failure without further successful evidence. Snapshots are historical
transport observations: they prove neither worker handling, Actor/service
progress, downstream completion nor child exit. A retained snapshot does not
validate the probe's later health. A later controller must check the live probe,
retain causal targets and use its own real watchdog; this preparation adds no
controller and migrates no scenario.

