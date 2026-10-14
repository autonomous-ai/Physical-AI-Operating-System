"""Device-owned MPR121 wiring; absent declarations preserve existing inputs.

Tuning is layered per key: the device package (`<device>/mpr121.json`, replaced by
OTA) < the factory layer (`/var/lib/hal/mpr121.factory.json`, written per unit at the
bench) < the user layer (`/var/lib/hal/mpr121.user.json`, written from the web UI).
Both /var/lib/hal layers survive HAL and device updates; an unreadable or invalid
layer is logged and skipped, never fatal.
"""

import json
import logging
import math
import os
from dataclasses import dataclass, field, fields
from pathlib import Path

from hal.board.config_fields import known_fields
from typing import Optional

# Allowed MPR121 filter values mapped to their register codes (datasheet CONFIG1/CONFIG2).
FFI_CODES = {6: 0, 10: 1, 18: 2, 34: 3}
SFI_CODES = {4: 0, 6: 1, 10: 2, 18: 3}
ESI_CODES = {1: 0, 2: 1, 4: 2, 8: 3, 16: 4, 32: 5, 64: 6, 128: 7}

logger = logging.getLogger(__name__)

FACTORY_PATH = os.environ.get("HAL_MPR121_FACTORY_PATH", "/var/lib/hal/mpr121.factory.json")
USER_PATH = os.environ.get("HAL_MPR121_USER_PATH", "/var/lib/hal/mpr121.user.json")
# Keys a factory/user layer may set. Everything else (wiring, gestures) stays with
# the device package.
TUNING_FIELDS = frozenset({
    "touch_threshold", "release_threshold", "touch_thresholds", "release_thresholds",
    "chip_debounce", "nhdf", "nclf", "fdlf", "nhdt", "nclt", "fdlt", "stuck_reset_s",
})
# Bookkeeping a layer carries for people and the UI; never applied to the chip.
LAYER_META_FIELDS = frozenset({"updated_at", "source", "sensitivity", "noise", "calibrated", "note"})


@dataclass(frozen=True)
class MPR121Config:
    bus: int
    address: int = 0x5A
    electrodes: tuple[int, ...] = tuple(range(12))
    touch_threshold: int = 2
    release_threshold: int = 1
    autoconfig: bool = True
    poll_ms: int = 10
    debounce_ms: int = 30
    # Chip debounce encoding (reg 0x5B), 0..7: N requires N+1 consecutive
    # touch/release detections, independently for both directions.
    chip_debounce: int = 0
    ffi: int = 6
    sfi: int = 10
    esi_ms: int = 1
    swipe_axis: tuple[int, ...] | None = None
    tap_min_electrodes: int = 1

    gesture_actions: dict[str, str] = field(default_factory=dict)
    hold_action_s: float = 2.0
    # Per-electrode thresholds (12 entries); when set they win over the global pair.
    touch_thresholds: tuple[int, ...] | None = None
    release_thresholds: tuple[int, ...] | None = None
    # Baseline tracking (AN3893): falling = signal above baseline, touched = while a
    # pad is touched. NHD 1..63 counts per step, NCL samples per step, FDL delay.
    nhdf: int = 1
    nclf: int = 0xFF
    fdlf: int = 2
    nhdt: int = 0
    nclt: int = 0
    fdlt: int = 0
    # A pad reported touched longer than this re-initializes the chip (baseline
    # reload), so drift cannot hold a phantom touch forever. 0 = off.
    stuck_reset_s: float = 30.0
    # Re-open + re-initialize attempts after an I2C fault before polling stops.
    i2c_recover_attempts: int = 5

    def touch_for(self, electrode: int) -> int:
        return self.touch_thresholds[electrode] if self.touch_thresholds else self.touch_threshold

    def release_for(self, electrode: int) -> int:
        return self.release_thresholds[electrode] if self.release_thresholds else self.release_threshold

    def __post_init__(self):
        if (type(self.hold_action_s) not in (int, float)
                or not math.isfinite(self.hold_action_s) or not 0.5 <= self.hold_action_s <= 10):
            raise ValueError("hold_action_s must be a finite number in 0.5..10")
        if (not isinstance(self.gesture_actions, dict)
                or any(key not in {"hold", "swipe_left", "swipe_right"}
                       or value != "toggle_voice_input_mode"
                       for key, value in self.gesture_actions.items())):
            raise ValueError("gesture_actions supports hold/swipe_left/swipe_right -> toggle_voice_input_mode")
        object.__setattr__(self, "gesture_actions", dict(self.gesture_actions))
        for name in ("bus", "address", "touch_threshold", "release_threshold", "poll_ms", "debounce_ms",
                     "chip_debounce"):
            if type(getattr(self, name)) is not int:
                raise ValueError(f"{name} must be an integer")
        if self.bus < 0:
            raise ValueError("bus must be non-negative")
        if not 0x5A <= self.address <= 0x5D:
            raise ValueError("address must be between 90 (0x5A) and 93 (0x5D)")
        if not 0 <= self.release_threshold < self.touch_threshold <= 255:
            raise ValueError("thresholds must satisfy 0 <= release < touch <= 255")
        if type(self.autoconfig) is not bool:
            raise ValueError("autoconfig must be a boolean")
        if not 1 <= self.poll_ms <= 1000 or not 0 <= self.debounce_ms <= 1000:
            raise ValueError("poll_ms must be 1..1000; debounce_ms must be 0..1000")
        if not 0 <= self.chip_debounce <= 7:
            raise ValueError("chip_debounce must be 0..7")
        for name, codes in (("ffi", FFI_CODES), ("sfi", SFI_CODES), ("esi_ms", ESI_CODES)):
            if type(getattr(self, name)) is not int or getattr(self, name) not in codes:
                raise ValueError(f"{name} must be one of {sorted(codes)}")
        if not isinstance(self.electrodes, (list, tuple)) or not self.electrodes:
            raise ValueError("electrodes must be a non-empty list")
        if any(type(i) is not int or not 0 <= i < 12 for i in self.electrodes):
            raise ValueError("electrodes must contain integers 0..11")
        if len(set(self.electrodes)) != len(self.electrodes):
            raise ValueError("electrodes must not contain duplicates")
        object.__setattr__(self, "electrodes", tuple(self.electrodes))
        if (type(self.tap_min_electrodes) is not int
                or not 1 <= self.tap_min_electrodes <= len(self.electrodes)):
            raise ValueError("tap_min_electrodes must be 1..number of selected electrodes")
        if self.swipe_axis is not None:
            axis = self.swipe_axis
            if (not isinstance(axis, (list, tuple)) or not 2 <= len(axis) <= 12
                    or any(type(i) is not int or i not in self.electrodes for i in axis)
                    or len(set(axis)) != len(axis)):
                raise ValueError("swipe_axis must contain 2..12 distinct selected electrodes in physical right-to-left order")
            object.__setattr__(self, "swipe_axis", tuple(axis))
        for name in ("touch_thresholds", "release_thresholds"):
            values = getattr(self, name)
            if values is None:
                continue
            if (not isinstance(values, (list, tuple)) or len(values) != 12
                    or any(type(v) is not int for v in values)):
                raise ValueError(f"{name} must list 12 integers, one per electrode")
            object.__setattr__(self, name, tuple(values))
        for electrode in range(12):
            if not 0 <= self.release_for(electrode) < self.touch_for(electrode) <= 255:
                raise ValueError(f"electrode {electrode}: thresholds must satisfy 0 <= release < touch <= 255")
        for name, high in (("nhdf", 63), ("nhdt", 63), ("nclf", 255), ("nclt", 255),
                           ("fdlf", 255), ("fdlt", 255)):
            if type(getattr(self, name)) is not int or not 0 <= getattr(self, name) <= high:
                raise ValueError(f"{name} must be an integer 0..{high}")
        if (type(self.stuck_reset_s) not in (int, float) or not math.isfinite(self.stuck_reset_s)
                or not (self.stuck_reset_s == 0 or 5 <= self.stuck_reset_s <= 3600)):
            raise ValueError("stuck_reset_s must be 0 (off) or 5..3600")
        if type(self.i2c_recover_attempts) is not int or not 0 <= self.i2c_recover_attempts <= 20:
            raise ValueError("i2c_recover_attempts must be 0..20")


def load_package_entry(device_dir: str, board_id: str) -> Optional[dict]:
    """The device package's board entry as plain values, or None (absent/disabled)."""
    path = Path(device_dir) / "mpr121.json"
    try:
        text = path.read_text()
    except FileNotFoundError:
        return None
    try:
        data = json.loads(text)
        if not isinstance(data, dict) or set(data) != {"boards"} or not isinstance(data["boards"], dict):
            raise ValueError("expected an object containing a 'boards' map")
        entries = {}
        allowed = {field.name for field in fields(MPR121Config)} | {"enabled"}
        for board, entry in data["boards"].items():
            if not isinstance(entry, dict):
                raise ValueError(f"{board}: invalid MPR121 configuration fields")
            values = dict(entry)
            # Retired setting that older device declarations may still carry.
            values.pop("harness_voice_chord", None)
            values = known_fields(values, allowed, f"{path} {board}")
            enabled = values.pop("enabled", True)
            if type(enabled) is not bool:
                raise ValueError(f"{board}: enabled must be a boolean")
            if enabled:
                MPR121Config(**values)  # fail on the package, before any layer applies
            entries[board] = values if enabled else None
        return entries.get(board_id)
    except (ValueError, TypeError) as exc:
        raise ValueError(f"Invalid MPR121 wiring at {path}: {exc}") from exc


def read_layer(path: str) -> dict:
    """A factory/user tuning layer: {} when absent; unreadable files are logged and skipped."""
    try:
        with open(path) as f:
            data = json.load(f)
    except FileNotFoundError:
        return {}
    except (OSError, ValueError) as exc:
        logger.warning("MPR121 tuning layer %s unreadable, ignored: %s", path, exc)
        return {}
    if not isinstance(data, dict):
        logger.warning("MPR121 tuning layer %s is not an object, ignored", path)
        return {}
    return data


def write_layer(path: str, layer: dict) -> None:
    """Atomically replace a tuning layer (fsync: lamps are unplugged to power off)."""
    directory = os.path.dirname(path)
    os.makedirs(directory, exist_ok=True)
    tmp = f"{path}.tmp"
    with open(tmp, "w") as f:
        json.dump(layer, f, indent=1)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)
    dir_fd = os.open(directory, os.O_RDONLY)
    try:
        os.fsync(dir_fd)
    finally:
        os.close(dir_fd)


def remove_layer(path: str) -> bool:
    try:
        os.remove(path)
        return True
    except FileNotFoundError:
        return False


def _tuning(layer: dict, where: str) -> dict:
    return known_fields({k: v for k, v in layer.items() if k not in LAYER_META_FIELDS},
                        TUNING_FIELDS, where)


def merge_layers(package: dict, layers: list[tuple[str, dict]]) -> tuple[MPR121Config, dict]:
    """Apply tuning layers over the package values, lowest first.

    Returns the config and {field: layer name} for every field a layer set. A layer
    whose result does not validate is skipped as a whole (logged), so a bad user file
    falls back to the factory tuning, and a bad factory file to the package.
    """
    values, sources = dict(package), {}
    for name, layer in layers:
        tuning = _tuning(layer, f"MPR121 {name} layer")
        if not tuning:
            continue
        candidate = dict(values)
        for key in ("touch", "release"):
            # A global threshold in a higher layer replaces lower per-pad lists.
            if f"{key}_threshold" in tuning and f"{key}_thresholds" not in tuning:
                candidate.pop(f"{key}_thresholds", None)
        candidate.update(tuning)
        try:
            MPR121Config(**candidate)
        except (ValueError, TypeError) as exc:
            logger.warning("MPR121 %s layer ignored, invalid: %s", name, exc)
            continue
        values = candidate
        sources.update({key: name for key in tuning})
    return MPR121Config(**values), sources


def load_mpr121_layers(device_dir: str, board_id: str) -> Optional[dict]:
    """Package entry, both tuning layers, the merged config and per-field sources."""
    package = load_package_entry(device_dir, board_id)
    if package is None:
        return None
    factory, user = read_layer(FACTORY_PATH), read_layer(USER_PATH)
    config, sources = merge_layers(package, [("factory", factory), ("user", user)])
    return {"package": package, "factory": factory, "user": user,
            "config": config, "sources": sources}


def load_mpr121_config(device_dir: str, board_id: str, *, layers: bool = True) -> Optional[MPR121Config]:
    """Load the mpr121.json boards map (plus tuning layers); no legacy wiring is assumed."""
    if not layers:
        package = load_package_entry(device_dir, board_id)
        return None if package is None else MPR121Config(**package)
    loaded = load_mpr121_layers(device_dir, board_id)
    if loaded is None:
        return None
    if loaded["sources"]:
        logger.info("MPR121 tuning from layers: %s", loaded["sources"])
    return loaded["config"]
