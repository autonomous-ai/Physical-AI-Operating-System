"""Realtime `look` must honour the user's manual "camera off", not only the privacy switch."""

import numpy as np
import pytest

import hal.app_state as state
from hal import privacy
from hal.drivers.camera import video_capture_device
from hal.realtime.orchestrator import RealtimeOrchestrator


class FakeCamera:
    def __init__(self):
        self.starts = 0
        self.stops = 0

    def start(self):
        self.starts += 1

    def stop(self):
        self.stops += 1


@pytest.fixture
def camera(monkeypatch):
    cam = FakeCamera()
    monkeypatch.setattr(state, "camera_capture", cam)
    monkeypatch.setattr(privacy, "camera_muted", False)
    monkeypatch.setattr(
        video_capture_device, "capture_still",
        lambda *a, **k: np.zeros((48, 64, 3), dtype=np.uint8),
    )
    return cam


def test_manual_camera_off_blocks_capture_and_never_wakes_the_sensor(camera, monkeypatch):
    monkeypatch.setattr(state, "_camera_disabled", True)
    monkeypatch.setattr(state, "_camera_manual_override", True)
    assert RealtimeOrchestrator._capture_frame() is None
    assert camera.starts == 0


def test_automatic_pause_may_still_be_woken_for_one_frame(camera, monkeypatch):
    monkeypatch.setattr(state, "_camera_disabled", True)
    monkeypatch.setattr(state, "_camera_manual_override", False)
    assert RealtimeOrchestrator._capture_frame() is not None
    assert (camera.starts, camera.stops) == (1, 1)


def test_privacy_switch_blocks_capture(camera, monkeypatch):
    monkeypatch.setattr(state, "_camera_disabled", False)
    monkeypatch.setattr(state, "_camera_manual_override", False)
    monkeypatch.setattr(privacy, "camera_muted", True)
    assert RealtimeOrchestrator._capture_frame() is None


def test_predicate_matches_the_snapshot_route_rule(monkeypatch):
    for muted, disabled, manual, blocked in [
        (False, False, False, False),
        (False, True, False, False),  # automatic pause
        (False, True, True, True),  # user turned the camera off
        (True, False, False, True),  # privacy switch
    ]:
        monkeypatch.setattr(privacy, "camera_muted", muted)
        monkeypatch.setattr(state, "_camera_disabled", disabled)
        monkeypatch.setattr(state, "_camera_manual_override", manual)
        assert state.camera_blocked_by_user() is blocked
