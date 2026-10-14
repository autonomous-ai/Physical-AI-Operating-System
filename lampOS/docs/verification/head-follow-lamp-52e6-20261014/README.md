# How lamp-52e6's head follows, 2026-10-14: raw readings

One servo's goal, position and load (`t_ms,goal,pos,load`, sampled about
every 16 ms from the first goal write) with the body stopped and the lamp
standing, at the body's own servo profile (goal speed 1000; acceleration 60
unless the name says otherwise), by a measuring script outside this
repository. `st-*`: the goal stepped and held 1.2 s, then back. `nod-aA-dD-hH`:
head pitch asked D counts down (toward the base) for H seconds at
acceleration A, then back. Pitch is servo 5, roll 4, yaw 1.
