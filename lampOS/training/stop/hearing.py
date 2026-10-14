"""When a run of scores is one hearing: the rule `lamp-name` uses (crates/name/src/said.rs), with a run length.

A hearing begins at the step where the score has been at least `least` for
`run` steps in a row. It is over when the score falls under `least`, or 0.8 s
after it began; for 1 s after that, scores are passed over. With `run` 1
this is `said.rs` exactly, except that the moment counted is when the
hearing begins, not when it is said: for a stop that is the moment to act.
"""

from __future__ import annotations

import numpy as np

STEP_S = 0.08
LONGEST_S = 0.8
HOLD_OFF_S = 1.0


def hearings(scores: np.ndarray, least: float, run: int = 1) -> list[int]:
    """The steps at which hearings begin (the step the run is complete)."""
    out = []
    i, n = 0, len(scores)
    above = 0
    while i < n:
        if scores[i] >= least:
            above += 1
            if above >= run:
                out.append(i)
                # Open until the score falls or 0.8 s from the run's first step.
                opened = i - run + 1
                j = i + 1
                while j < n and scores[j] >= least and (j - opened) * STEP_S < LONGEST_S:
                    j += 1
                # Quiet for 1 s from the close.
                i = j + int(round(HOLD_OFF_S / STEP_S))
                above = 0
                continue
        else:
            above = 0
        i += 1
    return out
