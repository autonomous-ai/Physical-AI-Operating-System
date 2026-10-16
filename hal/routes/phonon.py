"""Phonon engine install + status for the admin UI.

The engine (`ptts` in its own venv) installs on demand from a button, via
`phonon_install.py` as a transient systemd unit (survives HAL restart); state via a
job file. The model is never downloaded: its weights are installed by hand.
"""

import json
import logging
import os
import subprocess
import sys
import threading
import time

from fastapi import APIRouter

from hal.drivers.voice.tts.phonon import (
    PHONON_DIR,
    PHONON_MODEL_DIR,
    PHONON_PYTHON,
    engine_installed,
    list_voices,
    model_installed,
)

router = APIRouter()
logger = logging.getLogger("hal.voice.phonon")

PTTS_PACKAGE = os.environ.get("HAL_PHONON_PACKAGE", "ptts==0.4.0rc1")
VENV_DIR = os.path.dirname(os.path.dirname(PHONON_PYTHON))

# On the SD card: /tmp and /var/log are RAM on this board.
JOB_FILE = os.environ.get("HAL_PHONON_JOB", "/var/lib/autonomous/phonon-job.json")
WORKER = os.path.join(os.path.dirname(os.path.abspath(__file__)), "phonon_install.py")
# Prefix: each run gets a unique unit name (systemd-run refuses a name still reaping).
SYSTEMD_UNIT_PREFIX = "autonomous-phonon-install"

# Max time a claim may sit without a worker write before it counts as a failed start.
CLAIM_GRACE_S = 45.0

_IDLE = {
    "active": False, "kind": "", "target": "", "percent": 0,
    "bytes_done": 0, "bytes_total": 0, "error": "", "done": False,
}

# Guards the read-then-write in _claim so two clicks start one install.
_claim_lock = threading.Lock()


def _alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except (OSError, ValueError):
        return False
    return True


def _read_job() -> dict:
    """The job as the worker last left it; an "active" record with a dead pid is reported failed."""
    try:
        with open(JOB_FILE, "r", encoding="utf-8") as fh:
            raw = json.load(fh)
    except (OSError, ValueError):
        return dict(_IDLE)
    job = {k: raw.get(k, v) for k, v in _IDLE.items()}
    if not job["active"]:
        return job
    pid = int(raw.get("pid") or 0)
    if pid:
        if _alive(pid):
            return job
        reason = "install stopped unexpectedly"
    else:
        if time.time() - float(raw.get("claimed_at") or 0) < CLAIM_GRACE_S:
            return job
        reason = "install failed to start"
    job.update(active=False, done=True, error=job["error"] or reason)
    return job


def _write_job(rec: dict) -> None:
    try:
        os.makedirs(os.path.dirname(JOB_FILE), exist_ok=True)
        tmp = JOB_FILE + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(rec, fh)
        os.replace(tmp, JOB_FILE)
    except OSError as e:
        logger.warning("Phonon: cannot write job file %s: %s", JOB_FILE, e)


def _claim() -> bool:
    """Record the install as running before its worker exists; False if one already is."""
    with _claim_lock:
        if _read_job()["active"]:
            return False
        _write_job({
            **_IDLE, "active": True, "kind": "engine", "target": "Starting",
            "pid": 0, "claimed_at": time.time(),
        })
        return True


def _spawn(spec: dict) -> None:
    """Start the worker outside hal.service's cgroup via systemd-run (plain child as dev fallback)."""
    # System python3: the venv module must not come from HAL's own venv.
    python = "/usr/bin/python3" if os.path.isfile("/usr/bin/python3") else sys.executable
    argv = [python, WORKER, JOB_FILE, json.dumps(spec)]
    unit = f"{SYSTEMD_UNIT_PREFIX}-{time.time_ns()}"
    try:
        subprocess.run(
            ["systemd-run", "--collect", "--quiet", f"--unit={unit}",
             "--property=Description=Phonon engine install"] + argv,
            check=True, capture_output=True, timeout=20,
        )
        logger.info("Phonon: engine install started as %s", unit)
        return
    except subprocess.CalledProcessError as e:
        detail = (e.stderr or b"").decode("utf-8", "replace").strip() or e
        logger.warning("Phonon: systemd-run failed (%s), running in-process "
                       "— this install will not survive a HAL restart", detail)
    except Exception as e:
        logger.warning("Phonon: systemd-run unavailable (%s), running in-process "
                       "— this install will not survive a HAL restart", e)
    try:
        subprocess.Popen(argv, start_new_session=True)
    except Exception as e:
        logger.warning("Phonon: install worker failed to start: %s", e)
        _write_job({**_IDLE, "kind": "engine", "done": True, "error": str(e)})


@router.get("/voice/phonon/status")
def phonon_status():
    """Whether the engine and model are on the device, the model's voices, and any install in flight."""
    return {
        "engine_installed": engine_installed(),
        "model_installed": model_installed(),
        "phonon_dir": PHONON_DIR,
        "model_dir": PHONON_MODEL_DIR,
        "voices": list_voices(),
        "job": _read_job(),
    }


@router.post("/voice/phonon/install")
def phonon_install():
    """Install the engine; already-installed returns ok, not an error."""
    if engine_installed():
        return {"status": "ok", "message": "already installed"}
    if not _claim():
        return {"status": "error", "message": "an install is already running"}
    _spawn({"venv": VENV_DIR, "package": PTTS_PACKAGE})
    return {"status": "ok", "job": _read_job()}
