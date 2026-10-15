# The V1 slump under a hand on lamp-52e6, 2026-10-15 (#703, #780)

By hand, Leo at the lamp; numbers only, from the body's journal and
`body/joint_states` (recorded on the lamp in `~/fifth-give-way-hand*`).

| Run | Build (body SHA-256) | What happened |
|---|---|---|
| 6, 13:41 | `22253374f` (`69b8e68c`), per-joint limp | the base went soft and stiff again, the lamp did not slump; after the retake a hand still on the base ended the run (`load`, base 236) and every restart was refused (base 545 counts from home). Not V1. |
| 7, 13:58 | `abed76793` (`8040f64c`), whole-lamp slump | a hard turn of the base slumped the whole lamp (torque off on all five); 120 s later it took hold again, but the head lifting itself from the slump (load 144) tripped the hand rule and it slumped again, twice, with nobody touching it. |
| 8, 14:17 | `e251027ba` (`b9e9892d`) | a hard turn of the base slumped the whole lamp at 14:17:03; it took hold again at 14:19:04 (2 min) and brought the head home (pitch 2041 to 1764 by 14:19:47) with no second slump; `NRestarts` 0. Leo: "thấy lên rồi". |

So `--give-way return --limp on` behaves as V1 does on lamp-52e6: a joint
forced past V1's contact floor for a second slumps the lamp, and two
minutes later it holds itself again and comes back up. The arm stands
again when someone is in front of it, as after any rest.
