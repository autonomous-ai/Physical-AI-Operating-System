"""Camera perceptions that upload to the DL backend are opt-in and toggle live."""

import json
import threading

import pytest

from hal import config
from hal.drivers.sensing.perceptions import orchestrator as orch_module
from hal.drivers.sensing.perceptions.models import PerceptionConfig
from hal.drivers.sensing.perceptions.orchestrator import PerceptionOrchestrator
from hal.drivers.sensing.perceptions.processors.base import Perception


@pytest.mark.parametrize(
    ("saved", "want"),
    [
        (None, False),  # old config without the field: off
        ({}, False),
        ({"motion": True}, False),  # another feature's opt-in does not cover this one
        ({"pose": "true"}, False),  # only a real boolean counts as consent
        ({"pose": False}, False),
        ({"pose": True}, True),
    ],
)
def test_only_saved_opt_in_enables(tmp_path, monkeypatch, saved, want):
    cfg = tmp_path / "config.json"
    cfg.write_text(json.dumps({} if saved is None else {"cloud_perception": saved}))
    monkeypatch.setattr(config, "OS_CONFIG_PATH", str(cfg))
    # An old default left in a device .env is not consent.
    monkeypatch.setenv("HAL_POSE_ENABLED", "true")

    assert config._cloud_perception_enabled("pose") is want


class _FakeProcessor:
    def __init__(self, **_):
        self.retired = False

    def check(self, _data):
        pass

    def set_pose_perception(self, pose):
        self.pose = pose

    def retire(self):
        self.retired = True


@pytest.fixture
def orchestrator(monkeypatch):
    for name in ("MotionPerception", "EmotionPerception", "PosePerception", "FireHazardPerception"):
        monkeypatch.setattr(orch_module, name, type(name, (_FakeProcessor,), {}))
    orch = PerceptionOrchestrator(
        poll_interval_ts=1.0, send_event=lambda *a, **k: None,
        perception_config=PerceptionConfig(enable_face=True),
    )
    orch._cloud_ready = True  # as after start()
    return orch


def test_toggle_creates_and_retires_processors_live(orchestrator):
    frame = orchestrator._perception_state.frame
    procs = orchestrator._processors

    orchestrator.set_cloud_perception(motion=True, emotion=False, pose=True, fire_hazard=True)
    pose, fire = procs.pose_processor, procs.fire_hazard_processor
    assert pose is not None and fire is not None and procs.emotion_processor is None
    assert {pose.check, fire.check, procs.motion_processor.check} <= frame._subscriptors
    assert procs.motion_processor.pose is pose

    orchestrator.set_cloud_perception(motion=True, emotion=False, pose=False, fire_hazard=False)
    assert procs.pose_processor is None and procs.fire_hazard_processor is None
    assert pose.retired and fire.retired
    assert pose.check not in frame._subscriptors and fire.check not in frame._subscriptors
    assert procs.motion_processor.pose is None


def test_emotion_listens_to_detected_faces_live(orchestrator):
    faces = orchestrator._perception_state.detected_faces
    procs = orchestrator._processors

    orchestrator.set_cloud_perception(motion=False, emotion=True, pose=False, fire_hazard=False)
    emotion = procs.emotion_processor
    assert emotion is not None and emotion.check in faces._subscriptors
    assert emotion.check not in orchestrator._perception_state.frame._subscriptors

    orchestrator.set_cloud_perception(motion=False, emotion=False, pose=False, fire_hazard=False)
    assert procs.emotion_processor is None and emotion.retired
    assert emotion.check not in faces._subscriptors


def test_per_face_motion_follows_motion_opt_in(orchestrator, monkeypatch):
    monkeypatch.setattr(orch_module, "MotionPerFacePerception", type("MotionPerFacePerception", (_FakeProcessor,), {}))
    monkeypatch.setattr(orch_module.hal_config, "MOTION_PER_FACE_ENABLED", True)
    procs = orchestrator._processors
    faces = orchestrator._perception_state.detected_faces

    orchestrator.set_cloud_perception(motion=True, emotion=False, pose=False, fire_hazard=False)
    per_face = procs.motion_per_face_processor
    assert per_face is not None and per_face.check in faces._subscriptors

    orchestrator.set_cloud_perception(motion=False, emotion=False, pose=False, fire_hazard=False)
    assert procs.motion_per_face_processor is None and per_face.retired
    assert per_face.check not in faces._subscriptors


def test_people_perception_off_keeps_people_processors_off(monkeypatch):
    for name in ("MotionPerception", "EmotionPerception", "PosePerception", "FireHazardPerception"):
        monkeypatch.setattr(orch_module, name, type(name, (_FakeProcessor,), {}))
    orch = PerceptionOrchestrator(
        poll_interval_ts=1.0, send_event=lambda *a, **k: None,
        perception_config=PerceptionConfig(enable_face=False),
    )
    orch._cloud_ready = True
    orch.set_cloud_perception(motion=True, emotion=True, pose=True, fire_hazard=True)
    procs = orch._processors
    assert procs.motion_processor is None and procs.emotion_processor is None and procs.pose_processor is None
    assert procs.fire_hazard_processor is not None


class _SlowPerception(Perception):
    def __init__(self):
        super().__init__(perception_state=None, send_event=None)
        self.started = threading.Event()
        self.release = threading.Event()
        self.cleanups = 0
        self.runs = 0

    def _check_impl(self, data):
        self.runs += 1
        self.started.set()
        self.release.wait(5)

    def cleanup(self):
        self.cleanups += 1


def test_retire_waits_for_in_flight_check_then_cleans_up_once():
    p = _SlowPerception()
    p.check("frame")
    assert p.started.wait(5)

    p.retire()
    assert p.cleanups == 0  # the in-flight upload owns its connection until it ends
    p.check("frame")  # a retired perception accepts no new frames

    p.release.set()
    for _ in range(100):
        if p.cleanups:
            break
        threading.Event().wait(0.02)
    assert p.cleanups == 1 and p.runs == 1


def test_retire_when_idle_cleans_up_immediately():
    p = _SlowPerception()
    p.retire()
    assert p.cleanups == 1
    p.check("frame")
    assert p.runs == 0


def test_retire_drops_a_check_still_waiting_in_the_queue():
    # Occupy every shared pool worker so the next check waits in the queue.
    gate = threading.Event()
    blockers = [Perception._pool.submit(gate.wait, 5) for _ in range(Perception._pool._max_workers)]
    p = _SlowPerception()
    p.release.set()
    p.check("frame")  # queued, not started
    p.retire()
    gate.set()
    for b in blockers:
        b.result(5)
    for _ in range(100):
        if p.cleanups:
            break
        threading.Event().wait(0.02)
    assert p.runs == 0, "a queued check uploaded after the opt-in was turned off"
    assert p.cleanups == 1


class _CountingEstimator:
    def __init__(self, *a, **k):
        self.uploads = 0

    def update(self, frame):
        self.uploads += 1
        return None  # e.g. no usable body in view

    def close(self):
        pass


def test_pose_uploads_only_when_present_and_once_per_sample_interval(monkeypatch):
    import numpy as np

    from hal.drivers.sensing.perceptions.processors import pose as pose_module
    from hal.drivers.sensing.presence_service import PresenceState

    monkeypatch.setattr(pose_module, "RemotePoseEstimator", _CountingEstimator)
    clock = [1000.0]
    monkeypatch.setattr(pose_module.time, "time", lambda: clock[0])

    class Presence:
        state = PresenceState.AWAY

    presence = Presence()
    p = pose_module.PosePerception(perception_state=None, send_event=None, presense_service=presence)
    frame = np.zeros((8, 8, 3), dtype=np.uint8)
    est = p._estimator

    for _ in range(30):  # one minute of 2 s ticks with nobody present
        p._check_impl(frame)
        clock[0] += 2
    assert est.uploads == 0, "pose uploaded a frame of an empty room"

    presence.state = PresenceState.PRESENT
    for _ in range(30):  # one minute present, even with no usable body in the frame
        p._check_impl(frame)
        clock[0] += 2
    interval = pose_module.config.POSE_SAMPLE_INTERVAL_S
    assert est.uploads <= 60 / interval + 1, f"{est.uploads} uploads in 60 s"
