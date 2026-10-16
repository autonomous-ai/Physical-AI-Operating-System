
## Delayed final-dispatch line fixture

The original final-drain session-end case explicitly selects a test-local
driver for its real shell and stdout pipe. The first `cat` must have returned
and its complete line must be queued by the actual backend reader before
logical time advances. At the original 300 ms delay, a private FIFO releases
the unchanged later bytes. The second `cat` completion marker and the reader's
second complete-frame count are both required before advancing farther.

The driver never consumes a backend line. The original 800 ms dispatch target
and final drain still discover the terminal line, withhold playback and retain
the RunEnd for the runner. Clock reads are pure. Ordinary helper calls keep
their host sleeps; this first migration does not control the closed-transport
case, whose real reader/channel closure needs separate evidence.

Control writes are nonblocking. One immutable ten-second host watchdog bounds
the driver's progress checks; it does not interrupt blocking filesystem or OS
cleanup calls. Dropping the driver attempts an abort token and closes its FIFO.
The backend retains ownership of stdin close, actual exit and its existing
two-second kill/wait fallback, including abandonment before the child opens the
FIFO. Neither a completion marker nor an empty queue certifies child reaping.
