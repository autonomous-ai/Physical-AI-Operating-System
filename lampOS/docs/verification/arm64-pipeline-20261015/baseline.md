# #788 baseline audit

Run-list snapshot: 2026-10-15 15:14:01 +0700. Per-run snapshots have separate `*.observed-at.txt` timestamps, also taken from `date`.

Definitions: queue = workflow `createdAt` to each runner’s first `Set up job` step; release build/test/doc columns = step start to finish. `≥` is an observed lower bound, `—` means not executed/available, `†` is time consumed by a cancelled step, not successful completion. A queued job’s nonzero `startedAt` is not proof it received a runner. Run-list `queued` may coexist with already-active shards. Total = createdAt to latest actual job completion; cancelled runs without jobs use terminal `updatedAt` and are labelled. All times are m:ss. A run begins after the push event; exact push delivery latency is not exposed by these workflow records.

## Exact latest ten requested runs (frozen IDs)

| Run / branch | State | Queue by runner | Release build by runner | Test step by runner | Doc/custom by runner | Total / elapsed |
|---|---|---|---|---|---|---|
| [38703533987](https://github.com/autonomous-ai/lampOS/actions/runs/38703533987) `lamp-third` | queued | 1: ≥6:24 (not started) | 1: — | 1: — | 1: — | ≥6:24 |
| [38703521653](https://github.com/autonomous-ai/lampOS/actions/runs/38703521653) `fifth/limp-rest` | queued | 1: ≥6:30 (not started); 2: 1:28; 3: ≥6:30 (not started); 4: ≥6:30 (not started) | 1: —; 2: 2:24; 3: —; 4: — | 1: —; 2: ≥1:33; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | ≥6:30 |
| [38703507705](https://github.com/autonomous-ai/lampOS/actions/runs/38703507705) `fifth/limp-rest` | pending | — | — | — | — | ≥6:37 |
| [38703489997](https://github.com/autonomous-ai/lampOS/actions/runs/38703489997) `third/privacy-switch` | queued | 1: 4:44; 2: ≥6:46 (not started); 3: ≥6:46 (not started); 4: ≥6:46 (not started) | 1: ≥1:12; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | ≥6:46 |
| [38703309129](https://github.com/autonomous-ai/lampOS/actions/runs/38703309129) `second/news-polish` | queued | 1: ≥8:19 (not started); 2: ≥8:19 (not started); 3: ≥8:19 (not started); 4: ≥8:19 (not started) | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | ≥8:19 |
| [38703302892](https://github.com/autonomous-ai/lampOS/actions/runs/38703302892) `second/progress-reads` | queued | 1: 5:15; 2: ≥8:22 (not started); 3: 4:33; 4: ≥8:22 (not started) | 1: ≥1:59; 2: —; 3: 2:31; 4: — | 1: —; 2: —; 3: ≥0:04; 4: — | 1: —; 2: —; 3: —; 4: — | ≥8:22 |
| [38703275522](https://github.com/autonomous-ai/lampOS/actions/runs/38703275522) `fourth/addressed` | queued | 1: ≥8:35 (not started); 2: ≥8:35 (not started); 3: ≥8:35 (not started); 4: ≥8:35 (not started) | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | ≥8:35 |
| [38703256438](https://github.com/autonomous-ai/lampOS/actions/runs/38703256438) `second/policy-speaks` | queued | 1: ≥8:45 (not started); 2: ≥8:45 (not started); 3: ≥8:45 (not started); 4: ≥8:45 (not started) | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | ≥8:45 |
| [38703250546](https://github.com/autonomous-ai/lampOS/actions/runs/38703250546) `second/progress-news` | queued | 1: ≥8:49 (not started); 2: ≥8:49 (not started); 3: ≥8:49 (not started); 4: ≥8:49 (not started) | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | ≥8:49 |
| [38703074291](https://github.com/autonomous-ai/lampOS/actions/runs/38703074291) `third/touch-calibrate` | pending | — | — | — | — | ≥10:18 |

## Latest ten terminal runs at snapshot (nine cancellations)

| Run / branch | State | Queue by runner | Release build by runner | Test step by runner | Doc/custom by runner | Total / elapsed |
|---|---|---|---|---|---|---|
| [38701872452](https://github.com/autonomous-ai/lampOS/actions/runs/38701872452) `lamp-second` | cancelled | 1: ≥8:37 (not started); 2: ≥8:37 (not started); 3: ≥8:37 (not started); 4: 1:28 | 1: —; 2: —; 3: —; 4: 5:25 | 1: —; 2: —; 3: —; 4: 1:25† | 1: —; 2: —; 3: —; 4: — | 8:52 |
| [38701823306](https://github.com/autonomous-ai/lampOS/actions/runs/38701823306) `second/progress-reads` | cancelled | 1: 9:15; 2: 9:24; 3: 9:17; 4: 9:20 | 1: 2:26†; 2: 2:16†; 3: 2:29†; 4: 2:25† | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 12:20 |
| [38701425418](https://github.com/autonomous-ai/lampOS/actions/runs/38701425418) `second/progress-reads` | cancelled | 1: ≥3:27 (not started); 2: ≥3:27 (not started); 3: ≥3:27 (not started); 4: ≥3:27 (not started) | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 3:27 |
| [38701421240](https://github.com/autonomous-ai/lampOS/actions/runs/38701421240) `second/hello-asks` | cancelled | 1: 2:48; 2: ≥15:30 (not started); 3: ≥15:30 (not started); 4: 12:45 | 1: 5:52; 2: —; 3: —; 4: 1:41† | 1: 6:19†; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 15:46 |
| [38701415210](https://github.com/autonomous-ai/lampOS/actions/runs/38701415210) `second/policy-speaks` | cancelled | 1: 10:23; 2: ≥15:35 (not started); 3: ≥15:35 (not started); 4: 2:55 | 1: 4:53†; 2: —; 3: —; 4: 5:58 | 1: —; 2: —; 3: —; 4: 6:29† | 1: —; 2: —; 3: —; 4: — | 15:56 |
| [38701410728](https://github.com/autonomous-ai/lampOS/actions/runs/38701410728) `second/progress-news` | cancelled | 1: ≥15:39 (not started); 2: ≥15:39 (not started); 3: ≥15:39 (not started); 4: ≥15:39 (not started) | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 1: —; 2: —; 3: —; 4: — | 15:39 |
| [38701285530](https://github.com/autonomous-ai/lampOS/actions/runs/38701285530) `second/news-polish` | cancelled | 1: 0:24 | 1: 5:20 | 1: 11:01† | 1: — | 17:13 |
| [38700769552](https://github.com/autonomous-ai/lampOS/actions/runs/38700769552) `lamp-eighth` | cancelled | 1: 3:45; 2: 5:11; 3: 2:23; 4: 2:47 | 1: 5:21; 2: 5:27; 3: 5:36; 4: 5:33 | 1: 8:46†; 2: 8:02†; 3: 9:49†; 4: 9:23† | 1: —; 2: —; 3: —; 4: — | 19:12 |
| [38700685725](https://github.com/autonomous-ai/lampOS/actions/runs/38700685725) `lamp-voice` | cancelled | — | — | — | — | 0:31 (terminal update) |
| [38700545585](https://github.com/autonomous-ai/lampOS/actions/runs/38700545585) `eighth/voice-fixtures` | success | 1: 0:07 | 1: 5:35 | 1: 23:21 | 1: — | 29:49 |

## Supplement: latest ten successful runs at snapshot

| Run / branch | State | Queue by runner | Release build by runner | Test step by runner | Doc/custom by runner | Total / elapsed |
|---|---|---|---|---|---|---|
| [38700545585](https://github.com/autonomous-ai/lampOS/actions/runs/38700545585) `eighth/voice-fixtures` | success | 1: 0:07 | 1: 5:35 | 1: 23:21 | 1: — | 29:49 |
| [38697914780](https://github.com/autonomous-ai/lampOS/actions/runs/38697914780) `fifth/ci-test-shards` | success | 1: 0:07; 2: 0:07; 3: 0:08; 4: 0:07 | 1: 5:40; 2: 5:35; 3: 5:23; 4: 5:33 | 1: 10:23; 2: 9:27; 3: 10:56; 4: 10:01 | 1: 0:17; 2: —; 3: —; 4: — | 17:12 |
| [38697844896](https://github.com/autonomous-ai/lampOS/actions/runs/38697844896) `fourth/addressed` | success | 1: 13:21 | 1: 2:19 | 1: 24:35 | 1: — | 41:17 |
| [38697522833](https://github.com/autonomous-ai/lampOS/actions/runs/38697522833) `lamp-voice` | success | 1: 1:02 | 1: 2:21 | 1: 24:42 | 1: — | 29:18 |
| [38696914090](https://github.com/autonomous-ai/lampOS/actions/runs/38696914090) `lamp-sixth` | success | 1: 1:25 | 1: 5:34 | 1: 24:33 | 1: — | 32:13 |
| [38695601860](https://github.com/autonomous-ai/lampOS/actions/runs/38695601860) `seventh/memory-session-per-person` | success | 1: 0:22 | 1: 2:31 | 1: 24:55 | 1: — | 29:01 |
| [38695339133](https://github.com/autonomous-ai/lampOS/actions/runs/38695339133) `third/privacy-switch` | success | 1: 36:43 | 1: 5:38 | 1: 24:46 | 1: — | 67:50 |
| [38694990477](https://github.com/autonomous-ai/lampOS/actions/runs/38694990477) `fifth/v1-limp` | success | 1: 4:32 | 1: 5:31 | 1: 24:45 | 1: — | 35:32 |
| [38694193893](https://github.com/autonomous-ai/lampOS/actions/runs/38694193893) `seventh/who-gets-the-work` | success | 1: 15:00 | 1: 2:19 | 1: 24:16 | 1: — | 42:42 |
| [38693725124](https://github.com/autonomous-ai/lampOS/actions/runs/38693725124) `lamp-voice` | success | 1: 8:49 | 1: 2:21 | 1: 24:32 | 1: — | 36:45 |

## Four-runner reference 38697914780: split compilation from execution

All four runners had a cache miss. All have 0:07–0:08 actual setup queue. The complete run took 17:12 to its last job completion (17:13 to terminal workflow update).

| Shard | Release build | Test compile (Cargo reported) | Test execution (nextest reported) | Executed nextest tests | Doc/custom step |
|---|---|---|---|---|---|
| 1 | 5:40 | 5m 24s | 298.554 s | 1112 | 0:17 |
| 2 | 5:35 | 5m 20s | 242.209 s | 1141 | — |
| 3 | 5:23 | 5m 03s | 350.974 s | 1041 | — |
| 4 | 5:33 | 5m 20s | 279.839 s | 1168 | — |

The four nextest partitions executed 4,462 tests in total (including two results marked `LEAK`, which nextest counted as passed). Their summaries’ skipped counts are primarily the other hash partitions, plus ignored tests; they are not evidence of dropped tests. The two custom harnesses also ran on shard 1: `lamp-ipc::relay_signals` returned `ok`, and `lamp-turn-end-wire::process` reported 12 passing cases. The doc-test step ran for all 34 library crates (four passing doc-test cases in these logs).

## Log availability caveat

`gh run view --log` returned partial logs then `log not found` for cancelled runs 38701872452, 38701421240, and 38701415210; it returned no logs for 38701425418 and 38701410728 because the referenced jobs never started. Their metadata still reports actual executed step times. Raw stdout and stderr are preserved separately. All ten successful-run logs downloaded successfully.
