"""Learned per-frame load envelope for the contact stop — an elevator door's learn run.

A fixed load floor has to sit above the highest free-motion load of a joint, and the
joints that carry the arm's weight peak near full drive on their own. What a free
run loads at each frame of a recording is repeatable, though, so a learn run records
it and the contact stop then trips on a small excess over that envelope instead.
"""

import json
import logging
import os
import threading
import time
from typing import Callable, Dict, List, Optional, Tuple

logger = logging.getLogger(__name__)

# Envelope entries named "lag:<joint>" hold |goal - present| in 0.1 deg: a joint a hand
# slows falls behind its learned lag even when its load is already at the torque cap.
LAG_PREFIX = "lag:"

# Frames either side whose learned load also counts: the monitor samples at 20 Hz
# while playback steps at 30 fps, so the same pose lands a frame or two apart.
WINDOW_FRAMES = 3

# Passive learning: clean free runs kept per recording, and how many an update needs
# (a learn run plays each recording twice).
PASSIVE_RUNS = 5
PASSIVE_MIN_RUNS = 2
# Share of the gap one update lowers a frame's envelope by; rising is immediate, up to
# PASSIVE_CEILING_LAG for lag and the cut-off's threshold for load.
PASSIVE_FALL_SHARE = 0.25
PASSIVE_CEILING_LAG = 300
# A contact pause nobody held through (no torque release) reads as a false stop: the
# envelope there rises at once to where that sample would not have stopped it, at
# most this many times a day per recording, and does not fall for PASSIVE_HOLD_S
# after. On a capped joint a stop at its cap (within PASSIVE_CAP_SLACK, 0.1 %) means
# its load says nothing in that recording: lamp-c0a4's elbow reaches its 60 % cap at
# varying idle frames, and stopped idle every 5 s on a 41 % floor. Its whole channel
# rises to the cap there, leaving that joint to its lag.
PASSIVE_LOOSEN_PER_DAY = 10
PASSIVE_HOLD_S = 86400.0
PASSIVE_CAP_SLACK = 10
# A passive update is saved at most this often; the runs themselves stay in RAM.
PASSIVE_SAVE_S = 60.0

Series = Dict[str, List[Optional[int]]]


class ContactProfile:
    """{recording: {joint: [max load per frame]}}, learned free and saved per unit."""

    def __init__(
        self, path: str, margin: int, data: Optional[Dict] = None,
        lag_margin: Optional[int] = None, signature: Optional[str] = None,
    ) -> None:
        self.path = path
        self.margin = margin
        # 0.1 deg over the learned lag; None ignores the lag entries.
        self.lag_margin = lag_margin
        # What the envelope was learned under (the torque caps), saved beside it.
        self.signature = signature
        self._data: Dict[str, Dict[str, List[Optional[int]]]] = data or {}
        self._lock = threading.Lock()

    @property
    def signature_path(self) -> str:
        return self.path + ".signature"

    @classmethod
    def load(
        cls, path: str, margin: int, lag_margin: Optional[int] = None,
        signature: Optional[str] = None,
    ) -> "ContactProfile":
        """This unit's learned profile, or an empty one; a learn run saves to `path`.

        Only this unit's own: servo load and friction differ unit to unit, and an
        envelope learned on lamp-52e6 tripped lamp-a0ae's idle every 8 s. Missing or
        unreadable = empty, so only the fixed floors apply until the unit learns.
        An envelope saved under another `signature` (torque caps) is dropped too: a
        joint's load and lag change with its cap, and on lamp-c0a4 an envelope learned
        at 60 % tripped base_pitch's idle about 300 times in 25 min at 70 %.
        """
        try:
            with open(path) as f:
                data = json.load(f)
            if not isinstance(data, dict):
                raise ValueError("not an object")
        except FileNotFoundError:
            logger.info("[contact] no learned envelope at %s — fixed floors only until a learn run", path)
            return cls(path, margin, lag_margin=lag_margin, signature=signature)
        except (OSError, ValueError) as e:
            logger.warning("[contact] ignoring unreadable profile %s: %s", path, e)
            return cls(path, margin, lag_margin=lag_margin, signature=signature)
        profile = cls(path, margin, data, lag_margin, signature)
        saved = profile._saved_signature()
        if signature is not None and saved is not None and saved != signature:
            logger.warning(
                "[contact] envelope at %s was learned under %s, now %s — fixed floors until it is learned again",
                path, saved, signature,
            )
            profile._data = {}
            return profile
        logger.info("[contact] load envelope from %s (%d recordings)", path, len(data))
        return profile

    def _saved_signature(self) -> Optional[str]:
        """The signature saved beside the envelope; None when there is none (older saves)."""
        try:
            with open(self.signature_path) as f:
                return f.read().strip() or None
        except OSError:
            return None

    def recordings(self) -> List[str]:
        return sorted(self._data)

    def learn(self, recording: str, frame: int, loads: Dict[str, int]) -> None:
        """Fold one free-run sample into the envelope (max per frame)."""
        with self._lock:
            joints = self._data.setdefault(recording, {})
            for joint, load in loads.items():
                series = joints.setdefault(joint, [])
                if len(series) <= frame:
                    series.extend([None] * (frame + 1 - len(series)))
                if series[frame] is None or load > series[frame]:
                    series[frame] = load

    def forget(self, recording: str) -> None:
        with self._lock:
            self._data.pop(recording, None)

    def envelope(self, recording: str) -> Series:
        """A copy of one recording's learned series, {} when it has none."""
        with self._lock:
            return {name: list(series) for name, series in self._data.get(recording, {}).items()}

    def replace(self, recording: str, series: Series) -> None:
        with self._lock:
            self._data[recording] = series

    def floors(self, recording: str, frame: int) -> Optional[Dict[str, int]]:
        """Per-joint floors at this frame, or None when nothing was learned near it."""
        joints = self._data.get(recording)
        if not joints:
            return None
        out: Dict[str, int] = {}
        lo, hi = max(0, frame - WINDOW_FRAMES), frame + WINDOW_FRAMES + 1
        for name, series in joints.items():
            margin = self.lag_margin if name.startswith(LAG_PREFIX) else self.margin
            if margin is None:
                continue
            learned = [v for v in series[lo:hi] if v is not None]
            if learned:
                out[name] = max(learned) + margin
        return out or None

    def save(self) -> None:
        with self._lock:
            text = json.dumps(self._data, separators=(",", ":"))
        directory = os.path.dirname(self.path)
        if directory:
            os.makedirs(directory, exist_ok=True)
        tmp = self.path + ".tmp"
        with open(tmp, "w") as f:
            f.write(text)
        os.replace(tmp, self.path)
        if self.signature is not None:
            with open(tmp, "w") as f:
                f.write(self.signature)
            os.replace(tmp, self.signature_path)


class PassiveLearner:
    """Learns the envelope from the recordings the lamp plays anyway, no learn run needed.

    Every free run of a recording is sampled like a learn run, into a draft. A run
    counts only when it played from its first frame to its last with nothing in
    between: no contact stop or cut-off, no direct move, no learn run. The last
    PASSIVE_RUNS clean runs of a recording are kept, and from PASSIVE_MIN_RUNS on each
    clean run updates its envelope toward them: their per-frame max while the ring is
    not full (the loosest), then the second highest, so one run a hand brushed does
    not count. A higher value replaces the envelope at once (capped), so a heavy frame
    never stops the arm for long; a lower one takes PASSIVE_FALL_SHARE of the gap per
    update, so a few cool runs do not make it touchy. A recording's first envelope
    falls the same way from the fixed floors (`defaults`, {channel: floor}): it starts
    where the lamp stood before it learned. A stop nobody held through is taken as
    false (loosen()): runs that trip never count, so this is what frees an envelope
    too tight to run clean. Driven from the monitor thread; forget() may come from a
    learn run.
    """

    def __init__(
        self, profile: ContactProfile, defaults: Optional[Dict[str, int]] = None,
        load_ceiling: int = 1000, caps: Optional[Dict[str, int]] = None,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self.profile = profile
        self._defaults = dict(defaults or {})
        # {joint: torque cap}: a load channel's ceiling of its own.
        self._caps = dict(caps or {})
        self._load_ceiling = load_ceiling
        self._clock = clock
        self._lock = threading.Lock()
        self._runs: Dict[str, List[Series]] = {}
        self._loosened: Dict[str, List[float]] = {}
        self._draft: Optional[Tuple[str, int, int, Series]] = None   # rec, total, reached, samples
        self._tainted = False
        self._dirty = False
        self._saved_at = clock()

    def _margin(self, name: str) -> Optional[int]:
        return self.profile.lag_margin if name.startswith(LAG_PREFIX) else self.profile.margin

    def _start(self, name: str) -> Optional[int]:
        """The envelope that puts this channel's floor at its fixed one, if it has one."""
        floor, margin = self._defaults.get(name), self._margin(name)
        if floor is None or margin is None:
            return None
        return max(0, floor - margin)

    def runs(self) -> Dict[str, int]:
        """Clean runs held per recording."""
        with self._lock:
            return {rec: len(runs) for rec, runs in self._runs.items()}

    def observe(self, recording: str, frame: int, total: int, samples: Dict[str, int]) -> None:
        """One sample of `recording` at `frame` (of `total`) while it plays free."""
        with self._lock:
            draft = self._draft
            if draft is not None and (draft[0] != recording or frame < draft[2]):
                self._finish()
                draft = None
            if draft is None:
                # Joined mid-run (startup, after a taint): wait for the next run.
                self._tainted = frame > WINDOW_FRAMES
                draft = (recording, total, frame, {})
            rec, total, reached, series = draft
            if not self._tainted:
                for name, value in samples.items():
                    values = series.setdefault(name, [None] * total)
                    if frame < len(values) and (values[frame] is None or value > values[frame]):
                        values[frame] = value
            self._draft = (rec, total, max(reached, frame), series)
        self._maybe_save()

    def end(self) -> None:
        """No recording plays its own frames now: a finished run counts."""
        with self._lock:
            if self._draft is not None:
                self._finish()

    def discard(self) -> None:
        """Something other than free motion touched this run: it does not count."""
        with self._lock:
            self._tainted = True

    def forget(self, recording: str) -> None:
        """A learn run took over this recording's envelope."""
        with self._lock:
            self._runs.pop(recording, None)
            self._loosened.pop(recording, None)
            self._tainted = True

    def loosen(self, recording: str, frame: int, name: str, value: int) -> bool:
        """A stop at `frame` on `name` (sample `value`) that nobody held through.

        Raises the envelope around that frame to where `value` would not have stopped
        it, or the whole channel to the cap when a capped joint stopped at it. True when
        it changed.
        """
        margin = self._margin(name)
        if margin is None:
            return False
        cap = self._caps.get(name)
        at_cap = cap is not None and value >= cap - PASSIVE_CAP_SLACK
        now = self._clock()
        with self._lock:
            recent = [t for t in self._loosened.get(recording, []) if now - t < 86400.0]
            if len(recent) >= PASSIVE_LOOSEN_PER_DAY:
                self._loosened[recording] = recent
                return False
            series = self.profile.envelope(recording)
            values = series.get(name)
            if not values:
                return False
            needed = max(value, cap) if at_cap else value - margin + 1
            frames = (
                range(len(values)) if at_cap
                else range(max(0, frame - WINDOW_FRAMES), min(len(values), frame + WINDOW_FRAMES + 1))
            )
            changed = False
            for i in frames:
                if values[i] is not None and values[i] < needed:
                    values[i] = needed
                    changed = True
            if not changed:
                return False
            self.profile.replace(recording, series)
            self._loosened[recording] = recent + [now]
            self._dirty = True
        logger.info(
            "[contact] %s frame %d: stop nobody held, %s envelope raised%s",
            recording, frame, name, " to its cap throughout" if at_cap else "",
        )
        return True

    def _finish(self) -> None:
        rec, total, reached, series = self._draft
        clean = not self._tainted and reached >= total - 1 - WINDOW_FRAMES and series
        self._draft = None
        self._tainted = False
        if not clean:
            return
        runs = self._runs.setdefault(rec, [])
        if runs and any(len(values) != total for values in runs[0].values()):
            runs.clear()   # The recording changed length: an update replaced it.
        runs.append(series)
        del runs[:-PASSIVE_RUNS]
        if len(runs) < PASSIVE_MIN_RUNS:
            return
        now = self._clock()
        old = self.profile.envelope(rec)
        holding = any(now - t < PASSIVE_HOLD_S for t in self._loosened.get(rec, []))
        new: Series = {}
        for name in set().union(*runs):
            ceiling = PASSIVE_CEILING_LAG if name.startswith(LAG_PREFIX) else self._load_ceiling
            # A frame with nothing learned yet falls from its learned neighbours (±3
            # frames, the floor window) or, with none, from the fixed floor: sampled
            # at 20 Hz against 30 fps, every run reaches some frames for the first
            # time, and restarting those at the fixed floor would hold their whole
            # window there.
            start = self._start(name)
            before = old.get(name, [])
            values: List[Optional[int]] = []
            for frame in range(total):
                seen = sorted(
                    (run[name][frame] for run in runs if name in run and run[name][frame] is not None),
                    reverse=True,
                )
                prior = before[frame] if frame < len(before) else None
                if prior is None:
                    near = [
                        v for v in before[max(0, frame - WINDOW_FRAMES):frame + WINDOW_FRAMES + 1]
                        if v is not None
                    ]
                    prior = max(near) if near else start
                if not seen:
                    values.append(before[frame] if frame < len(before) else None)
                    continue
                value = min(seen[1] if len(runs) >= PASSIVE_RUNS and len(seen) > 1 else seen[0], ceiling)
                if prior is not None and value < prior:
                    value = prior if holding else prior - max(1, round((prior - value) * PASSIVE_FALL_SHARE))
                values.append(value)
            new[name] = values
        self.profile.replace(rec, new)
        self._dirty = True

    def _maybe_save(self) -> None:
        if not self._dirty or self._clock() - self._saved_at < PASSIVE_SAVE_S:
            return
        self._dirty = False
        self._saved_at = self._clock()
        try:
            self.profile.save()
            logger.info("[contact] passive envelope saved (%s)", self.runs())
        except OSError as e:
            logger.warning("[contact] passive envelope save failed: %s", e)
