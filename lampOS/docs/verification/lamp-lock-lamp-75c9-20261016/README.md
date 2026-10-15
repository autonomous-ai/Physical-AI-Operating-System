# The lamp lock on lamp-75c9, 2026-10-16

`lampOS/tools/lamp-lock.sh` at `91a75ad64`, run by the eighth builder from a
Linux laptop (x86_64) to lamp-75c9 (aarch64, `/usr/bin/flock` present) over
ssh by key. Only `~orangepi/lamp.lock`, `~orangepi/lamp.lock.log` and
`~orangepi/.lamp.lock.mutex` were touched; nothing on the lamp was moved,
stopped or installed.

## Run 1, 00:18:05 to 00:18:22 +07 (output not kept)

One shell loop on a free lamp, its output seen in the terminal and not saved
to a file. Every step came out as in the stand-in test
(`lampOS/tools/lamp-lock-test.sh`): `try` took it (exit 0); `status` held
(exit 1); another holder's `try` refused (1); `release` with session
`f3397175.` refused (1); `renew` (0); `run` under our own lock ran its command
and left the lock; `renewed_epoch` set to 1 by hand over ssh, then `status`
said expired, another holder's `try` replaced it and logged it, eighth's
`renew` failed (1); `run` with `LAMP_LOCK_RENEW=2` around a 5 s command
renewed at 00:18:20 and released; the lamp was free.

## Run 2, 00:18:44 to 00:20:00 +07: [session.txt](session.txt), unedited

The same loop again, to keep its output; the one change is the laptop's
host name, shown as `<laptop>`, since it names a person. Each step prints
`$ lamp-lock <args>`, the script's output, then `rc=`.

Between the runs, at 00:18:33, the builder nine took the lock for #830. So
this run shows the lock working for a real second builder: eighth's `try`
refused (1), `renew` said our lock was gone (1), the release with a near
session refused (1), and the `run` waited, polling every 30 s, until its
60 s `timeout` (rc 124).

It also shows the fault in the test, not the script: the loop's next step
set `renewed_epoch` to 1 by hand over ssh without checking whose lock it
was. That made nine's live lock look expired; a stand-in holder (`other`)
replaced it, as the script should for a truly expired lease, and released
it at 00:19:50; eighth then took and released the lamp for its renewal step
(00:19:58 to 00:20:00). From 00:19:50 the lamp was free while nine believed
it held it. nine was told at once through its local session; restoring
nine's lock from eighth's side was not permitted. A test on a real lamp must
not write to the lock file by hand; the expiry is shown only by the stand-in.

## Run 3, 00:44:54 to 00:45:12 +07: [run3.txt](run3.txt), unedited

`run` on the script with the reviewer's 00:29 findings mended (the
command's own process group stopped whole; the command stopped once no
renewal has been confirmed for 540 s), from `d82007e8d`'s tree; the lamp
was free first. `LAMP_LOCK_RENEW=2`; the command started a local
`sleep 300` worker and left it running, and read the lock's `renewed:`
line over ssh three times, 3 s apart. The lock renewed at 00:44:58, 00:45:04
and 00:45:07; `run` exited 0, the worker it left was stopped before the
release, and the lamp was free. The outage, hung call and lost-lock cases
are shown only by the stand-in test (71 cases).

## Run 4, 01:40:28 to 01:40:53 +07: [run4.txt](run4.txt), unedited

`run` from `386a96c43`'s tree (the reviewer's 00:57 findings mended: write
and remove failures on the lamp reported, a lost lease stopping the command
from the renewer, TERM then KILL, and a renewal call stopped with its
processes on every exit); the lamp was free first. `LAMP_LOCK_RENEW=2`; the
command ignored TERM, started a `sleep 300` worker (which inherits that) and
left it running, and read `renewed:` over ssh twice, 3 s apart. The lock
renewed at 01:40:32 and 01:40:35; the command ended about 01:40:35, the
worker took TERM without stopping and was killed after the 10 s grace, and
only then was the lock released (`run` exit 0 at 01:40:53, 25 s in all).

## Run 5, 12:45 (status before) to 12:45:51 +07: [run5.txt](run5.txt), unedited

After the reviewer's P2 and P3 of 01:58, with the fixes: `lamp-lock.sh` blob `639de354d696`, `lamp-lock-test.sh` blob `26bc004d3f8b` (as committed in this PR).
lamp-75c9 was free. A `run` with `LAMP_LOCK_RENEW=2` took the lock, ran
`sleep 9` locally, and released it. A second `run` (`sleep 12`) had two
read-only `status` calls during it: `renewed` moved from 12:45:43 to
12:45:47 while `since` stayed 12:45:38. Then it released, and the lamp was
free. Nothing on the lamp moved or was installed; only its lock file was
written. No `lamp-lock.sh` process was left on the laptop.

The two findings have no safe real-lamp case: one needs a lock expired by
hand, which is never done on a shared lamp, and the other is a local race.
They are shown through the stand-in instead:
[p2-p3-new-script.txt](p2-p3-new-script.txt), 108 passed, 0 failed, the
same five more times. The same new tests against `d0743159a`'s script, with only
the test hold point added ([p2-p3-old-script.diff](p2-p3-old-script.diff)),
are in [p2-p3-old-script.txt](p2-p3-old-script.txt): 102 passed, 6 failed.
The fork-window run missed its 20 s bound, blocked by the hung renewal call
until that call's own 60 s ended. The takeover whose replacement failed logged
no attempt or failure, and its log said "taken by bob" over a lock that was
unchanged. Its message said "nothing changed" although the log had changed.

## Run 6, 15:11:16 to 15:11:21 +07: [run6.txt](run6.txt), unedited

After Thirteen's P2 of 14:47 (a `run` from a terminal stopped its command by
SIGTTIN and kept the lamp locked). Scripts: `lamp-lock.sh` blob `71026d5e9ae3`,
`lamp-lock-test.sh` blob `524a7b521c48`. Under `script(1)`, so the run's input was a
terminal, a `run` on lamp-75c9 whose command reads its input read an empty
one (`read:[]`), finished in 5 s and released the lamp. The lamp was free before
and after. Nothing on it moved; only its lock file was written.

Before the fix, the same case with the stand-in under `script(1)` hung until
a 20 s timeout and left the lock held. The harness's new terminal checks
against `ac385435e`'s script are in [tty-old-script.txt](tty-old-script.txt):
108 passed, 4 failed, exactly the four terminal checks. On this head they are
in [tty-new-script.txt](tty-new-script.txt): 112 passed, 0 failed, the same
six times.

## A fault found by nine at 00:18:33

nine's own `try`, with this script, replaced the lock ten had taken at
00:18:31 (`#812 selftest`) and said "replaced an expired lock
(1792084713s without renewal)". ten's lock had been written by the older
script at `32d4c3253`, which wrote no `renewed_epoch`, and the check took
a missing stamp as epoch 0, so every lock in the older format looked
expired. ten was read-only at the time (its #880). Fixed in the same commit:
a lock with no stamp, or a malformed one, is dated by the file's
modification time, which every take and renew rewrites. The stand-in test
has six cases for it; two of them fail with the old line.

Not shown here: a lamp that does not answer (exit 3) was seen only before
the key was installed (publickey refused, `status` exit 3), and a lost lock
stopping a `run` command only through the stand-in test. Both runs' entries
stay in the lamp's `~/lamp.lock.log`.
