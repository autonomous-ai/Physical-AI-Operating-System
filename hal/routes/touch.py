"""Touch route handlers -- /touch/* tuning for the MPR121 touch strip.

The web UI's Touch pad page and the bench (factory) tool use these. Settings are
layered per key (see hal.board.mpr121): package < factory < user. These routes only
ever write the user layer; the bench tool writes the factory layer as root and then
calls POST /touch/reload.
"""

import asyncio
import json
import time
from typing import Optional

from fastapi import APIRouter, HTTPException, Request
from fastapi.concurrency import run_in_threadpool
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, Field

import hal.app_state as state
from hal.board import mpr121 as board

router = APIRouter(tags=["Touch"])

SENSITIVITY_DEFAULT = 5
SENSITIVITY_MIN, SENSITIVITY_MAX = 1, 9
# Each step left (Light) lowers every threshold ~13%, each step right (Firm) raises it.
SENSITIVITY_STEP = 1.15
# Auto-calibration: touch = max(MIN_TOUCH, 2 x idle noise + 2), release = about half.
MIN_TOUCH = 6
# Idle noise this high means someone touched the strip while it was measuring.
TOUCHED_NOISE = 30


class TouchConfigRequest(BaseModel):
    sensitivity: Optional[int] = Field(None, ge=SENSITIVITY_MIN, le=SENSITIVITY_MAX,
                                       description="1 = Light (most sensitive) .. 9 = Firm; 5 = calibrated value")
    touch_thresholds: Optional[list[int]] = Field(None, min_length=12, max_length=12)
    release_thresholds: Optional[list[int]] = Field(None, min_length=12, max_length=12)


class TouchCalibrateRequest(BaseModel):
    seconds: float = Field(10, ge=3, le=30, description="Idle measurement time; nobody may touch the strip")
    save: bool = Field(True, description="false = only measure and suggest (bench tool)")


def _handler():
    handler = state.mpr121_handler
    if handler is None or state.mpr121_source is None:
        raise HTTPException(status_code=404, detail="This device has no MPR121 touch strip running")
    return handler


def _layers():
    device_dir, board_id = state.mpr121_source
    loaded = board.load_mpr121_layers(device_dir, board_id)
    if loaded is None:
        raise HTTPException(status_code=404, detail="MPR121 is not declared for this board")
    return loaded


def _per_pad(config):
    return ([config.touch_for(i) for i in range(12)], [config.release_for(i) for i in range(12)])


def _base(loaded):
    """What sensitivity 5 means: the user's own calibration, else factory/package."""
    calibrated = loaded["user"].get("calibrated")
    if (isinstance(calibrated, dict) and isinstance(calibrated.get("touch"), list)
            and isinstance(calibrated.get("release"), list)
            and len(calibrated["touch"]) == len(calibrated["release"]) == 12):
        return [int(v) for v in calibrated["touch"]], [int(v) for v in calibrated["release"]]
    config, _ = board.merge_layers(loaded["package"], [("factory", loaded["factory"])])
    return _per_pad(config)


def scale(touch, release, sensitivity):
    factor = SENSITIVITY_STEP ** (sensitivity - SENSITIVITY_DEFAULT)
    out_touch, out_release = [], []
    for t, r in zip(touch, release):
        new_t = min(255, max(2, round(t * factor)))
        out_touch.append(new_t)
        out_release.append(min(new_t - 1, max(1, round(r * factor))))
    return out_touch, out_release


def suggest(noise):
    touch = [min(255, max(MIN_TOUCH, 2 * n + 2)) for n in noise]
    return touch, [max(2, round(t / 2)) for t in touch]


def _noise(loaded):
    for name in ("user", "factory"):
        noise = loaded[name].get("noise")
        if isinstance(noise, list) and len(noise) == 12:
            return noise
    return None


def _view(loaded):
    config = loaded["config"]
    touch, release = _per_pad(config)
    noise = _noise(loaded)
    handler = state.mpr121_handler
    sensitivity = loaded["user"].get("sensitivity", SENSITIVITY_DEFAULT)
    return {
        "running": bool(handler and handler._poll_thread and handler._poll_thread.is_alive()),
        "touch": touch,
        "release": release,
        "sensitivity": sensitivity if isinstance(sensitivity, int) else SENSITIVITY_DEFAULT,
        "sensitivity_range": [SENSITIVITY_MIN, SENSITIVITY_MAX],
        "swipe_axis": list(config.swipe_axis or range(12)),
        "stuck_reset_s": config.stuck_reset_s,
        "noise": noise,
        # Pads whose touch threshold sits at/under their measured idle noise: phantom touches likely.
        "warnings": [i for i in range(12) if noise and touch[i] <= noise[i] + 1],
        "sources": loaded["sources"],
        "layers": {
            "factory": loaded["factory"] or None,
            "user": loaded["user"] or None,
        },
        "user_adjusted": bool(loaded["user"]),
    }


def _apply(loaded):
    """Push the merged config into the running driver (chip rewrite + baseline reload)."""
    handler = _handler()
    handler.reconfigure(loaded["config"])


def _save_user(layer):
    try:
        board.write_layer(board.USER_PATH, layer)
    except OSError as exc:
        raise HTTPException(status_code=500, detail=f"Touch settings not saved: {exc}") from exc


@router.get("/touch/config")
def get_touch_config():
    """Thresholds in effect per pad, where they come from, and the sensitivity setting."""
    _handler()
    return _view(_layers())


@router.put("/touch/config")
def set_touch_config(req: TouchConfigRequest):
    """Set sensitivity (scales the calibrated per-pad thresholds) or explicit per-pad
    thresholds. Applied live and saved as the user layer."""
    _handler()
    loaded = _layers()
    layer = {k: v for k, v in loaded["user"].items() if k in ("calibrated", "noise")}
    if req.touch_thresholds is not None or req.release_thresholds is not None:
        if req.touch_thresholds is None or req.release_thresholds is None:
            raise HTTPException(status_code=422, detail="Send touch_thresholds and release_thresholds together")
        touch, release = req.touch_thresholds, req.release_thresholds
        layer["sensitivity"] = SENSITIVITY_DEFAULT
        layer["calibrated"] = {"touch": touch, "release": release}
    else:
        sensitivity = SENSITIVITY_DEFAULT if req.sensitivity is None else req.sensitivity
        touch, release = scale(*_base(loaded), sensitivity)
        layer["sensitivity"] = sensitivity
    try:
        board.MPR121Config(**{**loaded["package"], "touch_thresholds": touch, "release_thresholds": release})
    except (ValueError, TypeError) as exc:
        raise HTTPException(status_code=422, detail=str(exc)) from exc
    layer.update(touch_thresholds=touch, release_thresholds=release, source="web",
                 updated_at=time.strftime("%Y-%m-%dT%H:%M:%S"))
    _save_user(layer)
    loaded = _layers()
    _apply(loaded)
    return _view(loaded)


@router.delete("/touch/config/user")
def reset_touch_config():
    """Drop the user's adjustment: back to the factory (bench) calibration."""
    _handler()
    board.remove_layer(board.USER_PATH)
    loaded = _layers()
    _apply(loaded)
    return _view(loaded)


@router.post("/touch/reload")
def reload_touch_config():
    """Re-read all layers and apply them (used after the bench tool writes the factory layer)."""
    _handler()
    loaded = _layers()
    _apply(loaded)
    return _view(loaded)


@router.post("/touch/calibrate")
async def calibrate_touch(req: TouchCalibrateRequest = TouchCalibrateRequest()):
    """Measure idle noise per pad (nobody touching) and set thresholds from it.

    save=true (web UI) stores the result as the user layer and applies it;
    save=false only returns the measurement and the suggested thresholds.
    """
    handler = _handler()
    peak = [0] * 12
    samples = 0
    deadline = time.monotonic() + req.seconds
    last_t = None
    while time.monotonic() < deadline:
        snap = handler.signal(watch_s=1.0)
        if snap is not None and snap["t"] != last_t:
            last_t = snap["t"]
            samples += 1
            # Only baseline - filtered > 0 becomes a touch. Negative swings (e.g. the baseline
            # still settling after a chip reset, which starts up to ~31 counts low) never
            # trigger one and must not inflate the thresholds.
            peak = [max(p, d) for p, d in zip(peak, snap["delta"])]
        await asyncio.sleep(0.05)
    if samples < 10:
        raise HTTPException(status_code=503, detail="No touch strip readings - is the MPR121 polling?")
    if max(peak) >= TOUCHED_NOISE:
        raise HTTPException(status_code=409, detail=f"Strip was touched during calibration (pad {peak.index(max(peak))}); "
                                                    "keep hands off and try again")
    touch, release = suggest(peak)
    result = {"noise": peak, "samples": samples, "touch": touch, "release": release, "saved": False}
    if req.save:
        loaded = _layers()
        _save_user({"touch_thresholds": touch, "release_thresholds": release,
                    "calibrated": {"touch": touch, "release": release}, "noise": peak,
                    "sensitivity": SENSITIVITY_DEFAULT, "source": "auto",
                    "updated_at": time.strftime("%Y-%m-%dT%H:%M:%S")})
        loaded = _layers()
        await run_in_threadpool(_apply, loaded)
        result.update(saved=True, config=_view(loaded))
    return result


@router.get("/touch/stream")
async def touch_stream(request: Request):
    """Live per-pad signal as Server-Sent Events (~10 Hz): delta, thresholds, touched."""
    handler = _handler()

    async def gen():
        while not await request.is_disconnected():
            snap = handler.signal(watch_s=1.0)
            config = handler.config
            if snap is None:
                payload = {"ready": False}
            else:
                payload = {
                    "ready": True,
                    "delta": snap["delta"],
                    "touched": snap["touched"],
                    "touch": [config.touch_for(i) for i in range(12)],
                    "release": [config.release_for(i) for i in range(12)],
                }
            yield f"data: {json.dumps(payload)}\n\n"
            await asyncio.sleep(0.1)

    return StreamingResponse(
        gen(),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
    )
