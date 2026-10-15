"""Learned holding load of the weight-bearing joints, per pose: when a held arm is a hand.

A contact stop pins the arm where it is. base_pitch and elbow_pitch still carry its
weight there: their load, and how far they sag behind the pinned goal, depend on the
pose and drift for a few seconds (untouched, lamp-a0ae's elbow_pitch went from 22.7 %
to 33.4 %), not on the recording frame the motion envelope learned. So every still
spell (all goals unchanged), learn run or not, is sampled the way a contact pause sees
the arm: the peak over its first pause_s, past a settle that lets the last move's
tracking lag die out. Peaks are kept per pose cell of (base_pitch, elbow_pitch) and
updated like the passive envelope; a pause then releases these joints on that
learned hold plus the profile margin, never above the fixed floor that applied before.
"""

import logging
import threading
import time
from typing import Callable, Dict, List, Optional, Tuple

from hal.drivers.motors.contact_profile import (
    LAG_PREFIX, PASSIVE_CAP_SLACK, PASSIVE_MIN_RUNS, PASSIVE_RUNS,
    PASSIVE_SAVE_S, ContactProfile,
)

logger = logging.getLogger(__name__)

# The joints that carry the arm's weight; their hold is what this learns.
WEIGHT_BEARING = ("base_pitch", "elbow_pitch")
CHANNELS = WEIGHT_BEARING + tuple(LAG_PREFIX + joint for joint in WEIGHT_BEARING)
# Pose cell size (deg): gravity load changes little within 5 deg of either joint.
HOLD_CELL_DEG = 5.0
# A goal moving more than this (deg) ends a still spell.
HOLD_STILL_DEG = 0.5
# Skipped at the start of a spell: the last move's tracking lag dying out. Any 1 s a
# release needs within the pause still overlaps what is sampled after it.
HOLD_SETTLE_S = 0.5
# Share of the gap one spell lowers a cell by, twice the motion envelope's: the lamp is
# rarely still for a whole pause, and on lamp-75c9 its resting cell still released
# base_pitch at 43 % after 7 min against the 21 % it measured. Never under what was
# measured, so a faster fall only reaches the measured hold sooner.
HOLD_FALL_SHARE = 0.5

Cell = Tuple[int, int]


def _cell(pose: Dict[str, float]) -> Optional[Cell]:
    try:
        return tuple(int(round(pose[joint] / HOLD_CELL_DEG)) for joint in WEIGHT_BEARING)
    except (KeyError, TypeError):
        return None


def _key(cell: Cell) -> str:
    return "%d,%d" % cell


class HoldLearner:
    """{pose cell: {channel: holding peak}}, stored in a ContactProfile (one frame per cell).

    A spell counts once it stayed still for `window_s` (the contact pause) with torque
    on and no contact stop or cut-off in it. Like the passive envelope: the last
    PASSIVE_RUNS spells of a cell are kept, from PASSIVE_MIN_RUNS on their max (second
    highest once the ring is full) updates the cell, a higher value at once, a lower
    one by HOLD_FALL_SHARE of the gap, starting from the fixed floor (`defaults`).
    A learn run's spells (`trusted`) set the cell outright, as its envelope does. A
    hand on a still arm only raises a cell, and a floor never exceeds the fixed one,
    so a bad sample can at worst leave the release where it was before this learned.
    """

    def __init__(
        self, profile: ContactProfile, defaults: Dict[str, int],
        caps: Optional[Dict[str, int]] = None, window_s: float = 3.0,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self.profile = profile
        # {channel: the release floor before this learned}; channels without one are not learned.
        self._defaults = {name: floor for name, floor in defaults.items() if name in CHANNELS}
        self._caps = dict(caps or {})
        self._window_s = window_s
        self._clock = clock
        self._lock = threading.Lock()
        self._runs: Dict[str, List[Dict[str, int]]] = {}
        # (anchor goals, since, peaks, committed) of the spell in progress.
        self._spell: Optional[Tuple[Dict[str, float], float, Dict[str, int], bool]] = None
        self._dirty = False
        self._saved_at = clock()

    def _margin(self, name: str) -> Optional[int]:
        return self.profile.lag_margin if name.startswith(LAG_PREFIX) else self.profile.margin

    def _start(self, name: str) -> Optional[int]:
        floor, margin = self._defaults.get(name), self._margin(name)
        if floor is None or margin is None:
            return None
        return max(0, floor - margin)

    def cells(self) -> int:
        return len(self.profile.recordings())

    def discard(self) -> None:
        """The arm was stopped, cut or let go: the spell in progress does not count."""
        with self._lock:
            self._spell = None

    def observe(
        self, goals: Dict[str, float], samples: Dict[str, int], trusted: bool = False,
    ) -> None:
        """One tick with torque on: every joint's goal (deg), loads and lags (0.1 % / 0.1 deg)."""
        now = self._clock()
        with self._lock:
            spell = self._spell
            if spell is None or any(
                abs(goals.get(joint, anchor) - anchor) > HOLD_STILL_DEG
                for joint, anchor in spell[0].items()
            ):
                self._spell = (dict(goals), now, {}, False)
                return
            anchor, since, peaks, committed = spell
            if committed or now - since < HOLD_SETTLE_S:
                return
            for name in self._defaults:
                value = samples.get(name)
                if value is not None and value > peaks.get(name, -1):
                    peaks[name] = value
            if now - since >= self._window_s and peaks:
                self._spell = (anchor, since, peaks, True)
                self._commit(anchor, peaks, trusted)
        self._maybe_save()

    def _commit(self, anchor: Dict[str, float], peaks: Dict[str, int], trusted: bool) -> None:
        cell = _cell(anchor)
        if cell is None:
            return
        key = _key(cell)
        runs = self._runs.setdefault(key, [])
        runs.append(dict(peaks))
        del runs[:-PASSIVE_RUNS]
        if len(runs) < PASSIVE_MIN_RUNS:
            return
        old = self.profile.envelope(key)
        measured = self.profile.measured(key)
        new: Dict[str, List[Optional[int]]] = {}
        real: Dict[str, List[Optional[int]]] = {}
        for name in self._defaults:
            seen = sorted((run[name] for run in runs if name in run), reverse=True)
            prior = (old.get(name) or [None])[0]
            was = (measured.get(name) or [None])[0]
            if not seen:
                if prior is not None:
                    new[name] = [prior]
                if was is not None:
                    real[name] = [was]
                continue
            start = self._start(name)
            value = seen[1] if len(runs) >= PASSIVE_RUNS and len(seen) > 1 else seen[0]
            if start is not None:
                value = min(value, start)
            # Measured once, a hold stays: a lighter spell does not lower it.
            if was is not None and was > value:
                value = was
            real[name] = [value]
            if prior is None and not trusted:
                prior = start
            # Only what is above the measured (the fixed floor it starts from) falls.
            if prior is not None and value < prior and not trusted:
                value = prior - max(1, round((prior - value) * HOLD_FALL_SHARE))
            new[name] = [value]
        self.profile.replace(key, new)
        self.profile.replace_measured(key, real)
        self._dirty = True

    def floors(self, pose: Dict[str, float]) -> Dict[str, int]:
        """Release floors learned for this pose, each under its fixed one; {} when none.

        A channel counts only where its own cell learned it; its neighbours (±1 cell)
        then raise it by what they actually measured, the load changing across a
        cell's edge. Not by their envelope: a neighbour learned once still sits near the
        fixed floor it falls from, and on lamp-75c9 one such cell held its learned
        neighbour's elbow release at 51 % against the 21 % it had measured.
        """
        cell = _cell(pose)
        if cell is None:
            return {}
        own = self.profile.envelope(_key(cell))
        out: Dict[str, int] = {}
        for name, series in own.items():
            if name not in self._defaults or not series or series[0] is None:
                continue
            margin = self._margin(name)
            if margin is None:
                continue
            peak = series[0]
            for dp in (-1, 0, 1):
                for de in (-1, 0, 1):
                    near = self.profile.measured(_key((cell[0] + dp, cell[1] + de))).get(name)
                    if near and near[0] is not None:
                        peak = max(peak, near[0])
            floor = peak + margin
            # Not below what applied before, and not at a cap the load stops at anyway.
            cap = self._caps.get(name)
            if floor >= self._defaults[name] or (cap is not None and floor >= cap - PASSIVE_CAP_SLACK):
                continue
            out[name] = floor
        return out

    def _maybe_save(self) -> None:
        if not self._dirty or self._clock() - self._saved_at < PASSIVE_SAVE_S:
            return
        self._dirty = False
        self._saved_at = self._clock()
        try:
            self.profile.save()
            logger.info("[contact] hold profile saved (%d pose cells)", self.cells())
        except OSError as e:
            logger.warning("[contact] hold profile save failed: %s", e)

    def flush(self) -> None:
        """Save now if anything changed (end of a learn run)."""
        with self._lock:
            dirty, self._dirty = self._dirty, False
        if dirty:
            self._saved_at = self._clock()
            try:
                self.profile.save()
            except OSError as e:
                logger.warning("[contact] hold profile save failed: %s", e)
