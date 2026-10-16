2026-10-16 09:02:08 +0700

The first external reconstruction used macOS patch without -E. Patch returned 0 and restored the four existing files exactly, but left both deleted new files empty; the stricter absence assertion exited 1. The first directory, log and runner are preserved. The corrected external inverse invocation adds -E, with a separate directory/log. No repository source changed, and no compilation or tests ran.
