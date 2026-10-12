"""Owner-chosen listening light shown when the device wakes (config.json `wake_light`).

Unset fields fall back to the board's `listening` emotion preset, so a device the
owner never customised keeps its existing cue. The owner may turn the light off
only while the wake chime is on: a wake must always give some feedback.
"""
import copy
from typing import Optional

import hal.config as config
from hal.presets import (
    EMO_LISTENING,
    EMOTION_PRESETS,
    FX_BLINK,
    FX_BREATHING_FINE,
    FX_PULSE,
)

# Owner-facing effect names -> LED engine effects (None = steady colour).
# Keep aligned with system/server/config (WakeLightEffects) and the web form.
EFFECTS: dict[str, Optional[str]] = {
    "solid": None,
    "breathing": FX_BREATHING_FINE,
    "spin": FX_PULSE,
    "blink": FX_BLINK,
}
SPEED_MIN = 0.25
SPEED_MAX = 3.0


def _owner() -> dict:
    raw = config._os_cfg_get("wake_light", None)
    return raw if isinstance(raw, dict) else {}


def valid_color(color) -> Optional[list[int]]:
    """[r, g, b] with 0-255 ints and at least one lit channel, else None."""
    if not isinstance(color, (list, tuple)) or len(color) != 3:
        return None
    if any(isinstance(c, bool) or not isinstance(c, int) or not 0 <= c <= 255 for c in color):
        return None
    return list(color) if any(color) else None


def enabled() -> bool:
    """Whether a wake lights the strip; forced on when the chime is off too."""
    return _owner().get("enabled") is not False or not config.wake_chime_enabled()


def look_preset(color=None, effect=None, speed=None) -> dict:
    """The `listening` emotion preset with the given look applied over the board default."""
    preset = copy.deepcopy(EMOTION_PRESETS.get(EMO_LISTENING, {}))
    rgb = valid_color(color)
    if rgb is not None:
        preset["color"] = rgb
    if effect in EFFECTS:
        engine = EFFECTS[effect]
        if engine is None:
            preset.pop("effect", None)
        else:
            preset["effect"] = engine
    if isinstance(speed, (int, float)) and not isinstance(speed, bool):
        preset["speed"] = min(SPEED_MAX, max(SPEED_MIN, float(speed)))
    return preset


def listening_preset() -> dict:
    """The owner's listening look (read live, so an admin edit applies to the next wake)."""
    owner = _owner()
    return look_preset(owner.get("color"), owner.get("effect"), owner.get("speed"))
