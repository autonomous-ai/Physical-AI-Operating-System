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


class FakeAgent:
    def __init__(self):
        self.sent = []

    def send(self, inputs):
        self.sent.extend(inputs)


def test_look_without_recent_speech_does_not_capture(camera, monkeypatch):
    from types import SimpleNamespace

    monkeypatch.setattr(state, "_camera_disabled", False)
    monkeypatch.setattr(state, "_camera_manual_override", False)
    monkeypatch.setattr(state, "voice_service", SimpleNamespace(last_transcript_ts=0.0))
    captures = []
    monkeypatch.setattr(RealtimeOrchestrator, "_capture_frame",
                        staticmethod(lambda *a, **k: captures.append(1)))
    orch = RealtimeOrchestrator.__new__(RealtimeOrchestrator)
    orch._agent = FakeAgent()

    assert orch._handle_look_call(SimpleNamespace(call_id="c1")) is False
    assert captures == []
    assert "nobody asked" in orch._agent.sent[0].output


def test_someone_spoke_within(monkeypatch):
    import time
    from types import SimpleNamespace

    voice = SimpleNamespace(last_transcript_ts=0.0)
    monkeypatch.setattr(state, "voice_service", voice)
    assert state.someone_spoke_within(30) is False  # never heard anyone
    voice.last_transcript_ts = time.time() - 5
    assert state.someone_spoke_within(30) is True
    voice.last_transcript_ts = time.time() - 31
    assert state.someone_spoke_within(30) is False
    monkeypatch.setattr(state, "voice_service", None)
    assert state.someone_spoke_within(30) is False
