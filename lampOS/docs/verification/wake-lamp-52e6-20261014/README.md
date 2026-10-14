# The body waking on lamp-52e6, 2026-10-14: raw readings

The body's own JSON reports of two runs with `--wake on`, the lamp folded
and its voice stopped, built from `fifth/wake` (`b60f9281…`): the first with
`--home 1771,2035,2335` (the base's home 300 counts from where it was), the
second with `--home 1771,2035,2035`. Each was stopped with SIGINT after 45 s.
The other options were lamp-52e6's unit's without its standing ones.

`*-<boot>.csv`, one for each of the first two unplugs (the lamp folded, the
fifth builder's person's hand on it, power off about ten seconds): a
read-only script started at boot read every servo about every 0.2 s for
90 s from the moment the servo port appeared (14.2 and 14.6 s after power),
writing nothing (`uptime_s,id,torque,goal,pos,load,temp,status`). Before
the first cut the joints stood at yaw 2038, shoulder 2056, elbow 2073, roll
2042, pitch 1773 with torque on.
