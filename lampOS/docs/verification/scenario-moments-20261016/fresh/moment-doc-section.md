## Explicit snapshots for an observing test thread

The test-only `Scope::moment()` returns an immutable, checked `Moment` from
that active root. It carries the originating epoch and a distinct root identity;
two independent scopes cannot share an identity even if their clock values
coincide. `elapsed()` preserves nanoseconds, `instant()` uses that exact duration,
and `monotonic_us()` truncates only at the existing microsecond boundary. Snapshot
creation checks both clock ranges. Reading or cloning a snapshot does no work.

`earlier.between(&later, elapsed_from_origin)` projects a checked value within
two valid, ordered snapshots from the same active root. The offset is absolute
from the root epoch, not an advance from `earlier`; both endpoints are included.
Projection retains exact nanoseconds and the existing microsecond truncation.
It rejects reversed or outside bounds, different roots, inconsistent endpoints,
overflow and invalidated roots. It does not read a host clock, pump peers, advance
the root or admit the projected value to an observer. Explicit observer admission
still enforces the same root and nonregressing time.

A thread explicitly enters `ObserverScope::enter(moment)` and adopts later
snapshots with `observe(moment)`. It stays at the last admitted moment while its
root advances. Adoption rejects a different root, inconsistent values, regression
(including within one microsecond), and an invalidated root; equal live snapshots
are allowed. The observer is thread-bound and cannot nest with another observer
or root, run a peer pump, advance time, or inject a clock fault. Threads without
an explicit scope retain the host clock.

Removing a root normally or through callback panic invalidates future adoption,
including an equal snapshot and entry into a new observer. An installed observer
keeps its last admitted reading: reads are pure and infallible, and invalidation
never fabricates progress or jumps to the host clock. Its controller must cancel
it; dropping the observer restores host fallback. Root pump order, the 2 ms step,
frozen shutdown, real watchdogs and all existing scenario selections are unchanged.
This preparation adds no provider/Tokio controller and migrates no existing test.

