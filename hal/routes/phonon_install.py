"""Phonon engine install worker, run as its own transient unit so it survives `systemctl restart hal`.

Creates the engine venv and pip-installs `ptts` into it. The model is not touched:
its weights are installed by hand.

Invoked as a script, never imported; must not import `hal`.
Usage: python3 phonon_install.py <job-file> <spec-json>
"""

import json
import os
import subprocess
import sys

_job_path = ""
_state: dict = {}


def _flush(**over) -> None:
    """Write the job file atomically (complete record, never a patch)."""
    _state.update(over)
    rec = dict(_state)
    rec["pid"] = os.getpid()
    tmp = _job_path + ".tmp"
    try:
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(rec, fh)
        os.replace(tmp, _job_path)
    except OSError:
        pass


def _run(argv: list, step: str, percent: int, timeout: int) -> None:
    _flush(target=step, percent=percent)
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    if proc.returncode != 0:
        tail = (proc.stderr or proc.stdout or "").strip().splitlines()[-3:]
        raise RuntimeError(f"{step} failed: {' '.join(tail) or proc.returncode}")


def main() -> int:
    global _job_path
    _job_path = sys.argv[1]
    spec = json.loads(sys.argv[2])
    venv, package = spec["venv"], spec["package"]
    python = os.path.join(venv, "bin", "python")
    _state.update({
        "active": True, "kind": "engine", "target": "", "percent": 0,
        "bytes_done": 0, "bytes_total": 0, "error": "", "done": False,
    })
    try:
        os.makedirs(os.path.dirname(venv), exist_ok=True)
        if not os.path.isfile(python):
            _run([sys.executable, "-m", "venv", venv], "Creating environment", 5, 120)
        _run([python, "-m", "pip", "install", "-q", "--upgrade", "pip"], "Updating pip", 25, 300)
        _run([python, "-m", "pip", "install", "-q", package], "Installing engine", 50, 900)
        _run([python, "-c", "import ptts"], "Checking engine", 95, 60)
    except Exception as e:
        _flush(active=False, done=True, error=str(e)[:300])
        return 1
    _flush(active=False, done=True, percent=100, target="Installed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
