"""A manual "camera off" can only be undone by the owner, never by the agent alone."""

import time
import types

import pytest
from fastapi import HTTPException

import hal.app_state as state
from hal import privacy
from hal.routes import camera


class _Cam:
    def __init__(self):
        self.starts = 0

    def start(self):
        self.starts += 1


def _req(headers=None):
    return types.SimpleNamespace(headers=headers or {})


@pytest.fixture
def manual_off(monkeypatch, tmp_path):
    cam = _Cam()
    monkeypatch.setattr(state, "camera_capture", cam)
    monkeypatch.setattr(privacy, "camera_muted", False)
    monkeypatch.setattr(state, "_camera_disabled", True)
    monkeypatch.setattr(state, "_camera_manual_override", True)
    monkeypatch.setattr(state, "_persist_camera_state", lambda: None)
    monkeypatch.setattr(state, "voice_service", types.SimpleNamespace(last_transcript_ts=0.0))
    return cam


def test_agent_alone_cannot_undo_manual_off(manual_off):
    with pytest.raises(HTTPException) as e:
        camera.enable_camera(_req())
    assert e.value.status_code == 403
    assert manual_off.starts == 0 and state._camera_disabled


def test_web_owner_session_can_turn_it_back_on(manual_off):
    assert camera.enable_camera(_req({camera.OWNER_ACTION_HEADER: "1"}))["status"] == "ok"
    assert manual_off.starts == 1 and not state._camera_manual_override


def test_someone_speaking_to_the_lamp_can_turn_it_back_on(manual_off, monkeypatch):
    monkeypatch.setattr(state, "voice_service", types.SimpleNamespace(last_transcript_ts=time.time() - 5))
    assert camera.enable_camera(_req())["status"] == "ok"


def test_stale_speech_does_not_count(manual_off, monkeypatch):
    stale = time.time() - camera.OWNER_VOICE_WINDOW_S - 1
    monkeypatch.setattr(state, "voice_service", types.SimpleNamespace(last_transcript_ts=stale))
    with pytest.raises(HTTPException):
        camera.enable_camera(_req())


def test_automatic_pause_is_not_guarded(manual_off, monkeypatch):
    monkeypatch.setattr(state, "_camera_manual_override", False)
    assert camera.enable_camera(_req())["status"] == "ok"
