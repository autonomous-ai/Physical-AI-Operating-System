# Successful-run compile/execution split

Cargo compile times come from each invocation’s `Finished release` log line. Serial execution spans the first `Running ...` through the last test-result line, including doctests and custom harnesses; reference execution uses nextest’s shard summary and separate doc/custom step. All values below come from stored logs.

| Run | Cache | Release compilation (Cargo reported) | Test compilation (Cargo reported) | Serial tests+docs/custom or nextest shard execution |
|---|---|---|---|---|
| 38700545585 | 1 miss; 0 hit | 1: 5m 35s | 1: 5m 23s | 1: 1076.995s |
| 38697914780 | 4 miss; 0 hit | 3: 5m 23s; 2: 5m 35s; 4: 5m 32s; 1: 5m 39s | 3: 5m 03s; 2: 5m 20s; 4: 5m 20s; 1: 5m 24s | 3: 350.974s; 2: 242.209s; 4: 279.839s; 1: 298.554s |
| 38697844896 | 0 miss; 1 hit | 1: 2m 19s | 1: 5m 15s | 1: 1159.349s |
| 38697522833 | 0 miss; 1 hit | 1: 2m 20s | 1: 5m 22s | 1: 1159.841s |
| 38696914090 | 1 miss; 0 hit | 1: 5m 33s | 1: 5m 13s | 1: 1159.072s |
| 38695601860 | 0 miss; 1 hit | 1: 2m 30s | 1: 5m 24s | 1: 1170.509s |
| 38695339133 | 1 miss; 0 hit | 1: 5m 37s | 1: 5m 24s | 1: 1161.348s |
| 38694990477 | 1 miss; 0 hit | 1: 5m 31s | 1: 5m 25s | 1: 1159.248s |
| 38694193893 | 0 miss; 1 hit | 1: 2m 18s | 1: 4m 57s | 1: 1158.005s |
| 38693725124 | 0 miss; 1 hit | 1: 2m 21s | 1: 5m 12s | 1: 1158.296s |
