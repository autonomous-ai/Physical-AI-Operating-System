
## Transport exit deadline fixture

The original Eval transport-expiry audit keeps its live `/bin/sleep 30` child,
the full five-second deadline, and every original certification assertion. Its
explicit unit-test clock advances only at the original 20 ms wait points.
Clock reads do not advance time. Each loop still asks the actual child whether
it exited before checking the deadline; expiry still calls actual kill and
wait, and returns no successful transport status.

The fixture verifies 250 waits, the complete logical interval and an actually
reaped SIGKILL status. A separate immutable five-second host watchdog is
checked before each logical advance. Process startup and actual kill/wait
retain their original blocking calls; this is not an end-to-end host bound.
An unwind guard kills and reaps the child if a fixture check panics.
Boundary cases cover live expiry, zero budget, actual successful and
failed exits, pure clock reads and watchdog cleanup. A stepped deadline does
not certify a synthetic exit or establish real-device shutdown latency.

The clock installation is compiled only for unit tests. Ordinary backend
construction, all integration tests and the runtime continue to use the same
system monotonic samples, 20 ms sleeps, exit checks and cleanup order.
