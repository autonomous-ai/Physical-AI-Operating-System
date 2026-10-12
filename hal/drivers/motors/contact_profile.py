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
from typing import Dict, List, Optional

logger = logging.getLogger(__name__)

# Envelope entries named "lag:<joint>" hold |goal - present| in 0.1 deg: a joint a hand
# slows falls behind its learned lag even when its load is already at the torque cap.
LAG_PREFIX = "lag:"

# Frames either side whose learned load also counts: the monitor samples at 20 Hz
# while playback steps at 30 fps, so the same pose lands a frame or two apart.
WINDOW_FRAMES = 3


class ContactProfile:
    """{recording: {joint: [max load per frame]}}, learned free and saved per unit."""

    def __init__(
        self, path: str, margin: int, data: Optional[Dict] = None,
        lag_margin: Optional[int] = None,
    ) -> None:
        self.path = path
        self.margin = margin
        # 0.1 deg over the learned lag; None ignores the lag entries.
        self.lag_margin = lag_margin
        self._data: Dict[str, Dict[str, List[Optional[int]]]] = data or {}
        self._lock = threading.Lock()

    @classmethod
    def load(
        cls, path: str, margin: int, lag_margin: Optional[int] = None,
    ) -> "ContactProfile":
        """This unit's learned profile, or an empty one; a learn run saves to `path`.

        Only this unit's own: servo load and friction differ unit to unit, and an
        envelope learned on lamp-52e6 tripped lamp-a0ae's idle every 8 s. Missing or
        unreadable = empty, so only the fixed floors apply until the unit learns.
        """
        try:
            with open(path) as f:
                data = json.load(f)
            if not isinstance(data, dict):
                raise ValueError("not an object")
        except FileNotFoundError:
            logger.info("[contact] no learned envelope at %s — fixed floors only until a learn run", path)
            return cls(path, margin, lag_margin=lag_margin)
        except (OSError, ValueError) as e:
            logger.warning("[contact] ignoring unreadable profile %s: %s", path, e)
            return cls(path, margin, lag_margin=lag_margin)
        logger.info("[contact] load envelope from %s (%d recordings)", path, len(data))
        return cls(path, margin, data, lag_margin)

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
