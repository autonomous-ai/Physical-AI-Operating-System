"""Phonon install state for the admin UI.

Read-only: the engine venv and the model are installed by hand (the weights are not
redistributable), so there is nothing to download from here.
"""

from fastapi import APIRouter

from hal.drivers.voice.tts.phonon import (
    PHONON_DIR,
    PHONON_MODEL_DIR,
    engine_installed,
    list_voices,
    model_installed,
)

router = APIRouter()


@router.get("/voice/phonon/status")
def phonon_status():
    """Whether the engine and model are on the device, and the model's voices."""
    return {
        "engine_installed": engine_installed(),
        "model_installed": model_installed(),
        "phonon_dir": PHONON_DIR,
        "model_dir": PHONON_MODEL_DIR,
        "voices": list_voices(),
    }
