"""Learned holding load per pose: what base_pitch / elbow_pitch release on in a contact pause."""

import json
import types
from unittest import mock

import pytest

from hal.drivers.motors.contact_profile import ContactProfile
from hal.drivers.motors.hold_profile import HOLD_FALL_SHARE, HOLD_SETTLE_S, HoldLearner
from hal.drivers.motors.overload import OverloadGuard
from hal.test.test_servo_overload import IDLE, JOINTS, _animation_service_cls, _Clock, _FakeRobot

# Lamp's release floors before anything is learned: off_playback, elbow load at its cap.
DEFAULTS = {"base_pitch": 570, "elbow_pitch": 600, "lag:base_pitch": 140, "lag:elbow_pitch": 120}
CAPS = {"base_yaw": 600, "base_pitch": 600, "elbow_pitch": 600}
POSE = {joint: 10.0 for joint in JOINTS}


def _learner(tmp_path, clock=None):
    profile = ContactProfile(str(tmp_path / "hold.json"), margin=120, lag_margin=35)
    clock = clock or _Clock()
    return HoldLearner(profile, DEFAULTS, caps=CAPS, window_s=3.0, clock=clock), clock


def _spell(learner, clock, samples, pose=POSE, seconds=3.2, trusted=False):
    """The arm moves to `pose`, then stays still there for `seconds`, sampled at 20 Hz."""
    learner.observe({**pose, "base_yaw": pose["base_yaw"] + 50}, samples)   # still moving
    t = 0.0
    while t <= seconds:
        learner.observe(pose, samples, trusted=trusted)
        clock.advance(0.05)
        t += 0.05


def _held(pitch=250, elbow=334, lag_pitch=10, lag_elbow=21):
    return {**IDLE, "base_pitch": pitch, "elbow_pitch": elbow,
            "lag:base_pitch": lag_pitch, "lag:elbow_pitch": lag_elbow}


def test_one_still_spell_is_not_enough(tmp_path):
    learner, clock = _learner(tmp_path)
    _spell(learner, clock, _held())
    assert learner.floors(POSE) == {}


def test_a_cell_falls_from_the_fixed_floor_half_the_gap(tmp_path):
    learner, clock = _learner(tmp_path)
    _spell(learner, clock, _held())
    _spell(learner, clock, _held())
    # base_pitch starts at 570 - 120 = 450 and falls half the way to 250.
    assert learner.profile.envelope("2,2")["base_pitch"] == [350]
    assert learner.floors(POSE)["base_pitch"] == 470
    # elbow: 600 cap - 120 = 480, toward 334.
    assert learner.profile.envelope("2,2")["elbow_pitch"] == [480 - round(146 * HOLD_FALL_SHARE)]


def test_a_learn_run_sets_the_cell_outright(tmp_path):
    learner, clock = _learner(tmp_path)
    for _ in range(2):
        _spell(learner, clock, _held(), trusted=True)
    assert learner.floors(POSE) == {
        "base_pitch": 250 + 120, "elbow_pitch": 334 + 120,
        "lag:base_pitch": 10 + 35, "lag:elbow_pitch": 21 + 35,
    }


def test_a_heavier_hold_raises_the_cell_at_once(tmp_path):
    learner, clock = _learner(tmp_path)
    for _ in range(2):
        _spell(learner, clock, _held(), trusted=True)
    _spell(learner, clock, _held(pitch=330))
    assert learner.floors(POSE)["base_pitch"] == 330 + 120


def test_never_above_the_fixed_floor(tmp_path):
    """A hand on a still arm can only raise a cell back to where the release was before."""
    learner, clock = _learner(tmp_path)
    for _ in range(2):
        _spell(learner, clock, _held(pitch=900, elbow=600), trusted=True)
    floors = learner.floors(POSE)
    assert "base_pitch" not in floors and "elbow_pitch" not in floors


def test_the_last_move_settling_is_not_learned(tmp_path):
    learner, clock = _learner(tmp_path)
    for _ in range(2):
        learner.observe({**POSE, "base_yaw": 99.0}, _held())
        # Decelerating into the pose: heavy for less than the settle.
        t = 0.0
        while t < HOLD_SETTLE_S - 0.06:
            learner.observe(POSE, _held(pitch=560), trusted=True)
            clock.advance(0.05)
            t += 0.05
        _spell_rest = 3.2 - t
        while _spell_rest > 0:
            learner.observe(POSE, _held(), trusted=True)
            clock.advance(0.05)
            _spell_rest -= 0.05
    assert learner.floors(POSE)["base_pitch"] == 250 + 120


def test_a_spell_cut_short_or_moving_is_not_learned(tmp_path):
    learner, clock = _learner(tmp_path)
    for _ in range(3):
        _spell(learner, clock, _held(), seconds=2.5, trusted=True)
    assert learner.floors(POSE) == {}
    for _ in range(3):
        learner.observe(POSE, _held(), trusted=True)
        clock.advance(1.0)
        learner.discard()                        # stopped, cut or let go meanwhile
        clock.advance(2.5)
    assert learner.floors(POSE) == {}


def test_an_unlearned_pose_keeps_the_fixed_floor(tmp_path):
    learner, clock = _learner(tmp_path)
    for _ in range(2):
        _spell(learner, clock, _held(), trusted=True)
    far = {**POSE, "base_pitch": 40.0}
    assert learner.floors(far) == {}


def test_a_heavier_neighbour_cell_raises_the_floor(tmp_path):
    learner, clock = _learner(tmp_path)
    near = {**POSE, "elbow_pitch": 15.0}
    for _ in range(2):
        _spell(learner, clock, _held(), trusted=True)
        _spell(learner, clock, _held(elbow=380), pose=near, trusted=True)
    assert learner.floors(POSE)["elbow_pitch"] == 380 + 120


def test_saved_once_a_minute_and_reloaded(tmp_path):
    learner, clock = _learner(tmp_path)
    for _ in range(2):
        _spell(learner, clock, _held(), trusted=True)
    assert not (tmp_path / "hold.json").exists()
    clock.advance(60.0)
    _spell(learner, clock, _held(), trusted=True)
    data = json.loads((tmp_path / "hold.json").read_text())
    assert data["2,2"]["base_pitch"] == [250]
    reloaded = ContactProfile.load(str(tmp_path / "hold.json"), 120, 35)
    again = HoldLearner(reloaded, DEFAULTS, caps=CAPS)
    assert again.floors(POSE)["base_pitch"] == 370


# --- Wired into the contact pause ---------------------------------------------------


@pytest.fixture
def hold_rig(tmp_path):
    """Lamp's shape: fixed floors on base_yaw / wrists, off_playback, 60 % caps, release 0.5 s."""
    clock = _Clock()
    guard = OverloadGuard(800, 1.5, 120.0, clock=clock)
    contact = OverloadGuard({"base_yaw": 550, "wrist_roll": 750, "wrist_pitch": 650}, 0.05, 3.0, clock=clock)
    hold = ContactProfile(str(tmp_path / "hold.json"), margin=120, lag_margin=35)
    svc = _animation_service_cls()(
        port="/dev/null", lamp_id="test",
        overload_guard=guard, contact_guard=contact, torque_limits=CAPS,
        contact_off_playback={"base_pitch": 570, "lag:base_pitch": 140, "lag:elbow_pitch": 120},
        contact_release_s=0.5, contact_hold_profile=hold,
    )
    svc.robot = _FakeRobot()
    svc._running.set()
    svc.resume = mock.Mock()
    svc.dispatch = mock.Mock()
    return types.SimpleNamespace(svc=svc, clock=clock, bus=svc.robot.bus, hold=hold)


def _ticks(rig, seconds):
    for _ in range(int(round(seconds / 0.05))):
        rig.svc._overload_tick()
        rig.clock.advance(0.05)


def _learn_still(rig, loads):
    """Two still spells at the bus pose (10 deg everywhere) during a learn run."""
    rig.svc._learning = {"stretching"}
    for _ in range(2):
        rig.bus.goals = {**rig.bus.goals, "base_yaw": rig.bus.goals["base_yaw"] + 5}
        rig.bus.loads = dict(loads)
        _ticks(rig, 3.3)
    rig.svc._learning = None
    rig.bus.goals = dict(rig.bus.positions)      # back where a stop pins it


def _stop_on_base_yaw(rig):
    rig.bus.loads = {**rig.bus.loads, "base_yaw": 700}
    _ticks(rig, 0.15)
    assert rig.svc.contact_active


def test_still_spells_with_torque_on_teach_the_pose(hold_rig):
    _learn_still(hold_rig, {**IDLE, "base_pitch": 250, "elbow_pitch": 334})
    assert hold_rig.svc._hold.cells() == 1
    assert hold_rig.svc.overload_status()["contact"] is not None


def test_untouched_gravity_hold_at_a_learned_pose_only_pauses(hold_rig):
    """lamp-a0ae: elbow_pitch held 33.4 % untouched after a stop; learned, it stays put."""
    _learn_still(hold_rig, {**IDLE, "base_pitch": 250, "elbow_pitch": 334})
    _stop_on_base_yaw(hold_rig)
    hold_rig.bus.loads = {**IDLE, "base_pitch": 250, "elbow_pitch": 334}
    _ticks(hold_rig, 2.5)
    assert hold_rig.svc.contact_active and not hold_rig.svc.overload_active


def test_a_hand_on_a_weight_bearing_joint_past_its_learned_hold_releases(hold_rig):
    _learn_still(hold_rig, {**IDLE, "base_pitch": 250, "elbow_pitch": 334})
    _stop_on_base_yaw(hold_rig)
    # 45 %: under base_pitch's fixed 57 %, over its learned 25 % + 12 %.
    hold_rig.bus.loads = {**IDLE, "base_pitch": 450, "elbow_pitch": 334}
    _ticks(hold_rig, 0.7)
    assert hold_rig.svc.overload_active


def test_elbow_pitch_gains_a_load_release_once_learned(hold_rig):
    """No fixed load floor on elbow_pitch (capped at 60 %): learned, a 50 % push releases."""
    _learn_still(hold_rig, {**IDLE, "base_pitch": 250, "elbow_pitch": 334})
    _stop_on_base_yaw(hold_rig)
    hold_rig.bus.loads = {**IDLE, "base_pitch": 250, "elbow_pitch": 500}
    _ticks(hold_rig, 0.7)
    assert hold_rig.svc.overload_active


def test_an_unlearned_pose_releases_on_the_fixed_floor_only(hold_rig):
    _stop_on_base_yaw(hold_rig)
    hold_rig.bus.loads = {**IDLE, "base_pitch": 450, "elbow_pitch": 500}
    _ticks(hold_rig, 2.5)
    assert hold_rig.svc.contact_active and not hold_rig.svc.overload_active


def test_nothing_is_learned_with_the_body_parked(hold_rig):
    hold_rig.svc._running.clear()
    _learn_still(hold_rig, {**IDLE, "base_pitch": 0, "elbow_pitch": 0})
    assert hold_rig.svc._hold.cells() == 0


def test_a_learn_run_holds_each_end_pose_through_a_pause(hold_rig):
    svc = hold_rig.svc
    svc._learning = {"stretching"}
    svc._current_recording = "stretching"
    svc._current_actions = [{"base_yaw.pos": 1.0}]
    svc._current_frame_index = 1
    with mock.patch("time.time", return_value=100.0):
        svc._continue_playback()
    assert svc._hold_until == pytest.approx(104.0)


def test_no_bus_read_for_the_hold_while_the_goals_are_known_to_move(hold_rig):
    """The ramp into a recording and a direct move's settle: no still spell, no lag read."""
    svc = hold_rig.svc
    svc._current_recording = "stretching"
    svc._current_actions = [{}] * 8
    svc._interpolation_frames = 10
    hold_rig.bus.reads.clear()
    _ticks(hold_rig, 0.2)
    assert set(hold_rig.bus.reads) == {"Present_Load"}
    svc._interpolation_frames = 0
    svc._current_frame_index = 8                 # holding its final pose
    svc._direct_move_ended = svc._contact_now()
    hold_rig.bus.reads.clear()
    _ticks(hold_rig, 0.4)
    assert set(hold_rig.bus.reads) == {"Present_Load"}
    _ticks(hold_rig, 0.2)                        # settled: the hold samples it again
    assert "Present_Position" in hold_rig.bus.reads


def test_a_lighter_spell_does_not_lower_a_measured_hold(tmp_path):
    learner, clock = _learner(tmp_path)
    for _ in range(2):
        _spell(learner, clock, _held(), trusted=True)
    for _ in range(8):
        _spell(learner, clock, _held(pitch=150, elbow=200))
    assert learner.floors(POSE)["base_pitch"] == 250 + 120
    assert learner.floors(POSE)["elbow_pitch"] == 334 + 120


def test_a_neighbour_raises_a_floor_by_what_it_measured_not_its_falling_envelope(tmp_path):
    """lamp-75c9: 6,6 learned once still sat at 39 % elbow and held 6,5's release at 51 %."""
    learner, clock = _learner(tmp_path)
    near = {**POSE, "elbow_pitch": 15.0}
    for _ in range(2):
        _spell(learner, clock, _held(pitch=176, elbow=86), trusted=True)
        _spell(learner, clock, _held(pitch=150, elbow=120), pose=near)   # passive: still falling
    assert learner.profile.envelope("2,3")["elbow_pitch"][0] >= 300   # far over its 120
    assert learner.floors(POSE)["elbow_pitch"] == 120 + 120
    assert learner.floors(POSE)["base_pitch"] == 176 + 120


def test_every_fixed_floor_joint_releases_on_the_floor_it_was_stopped_against(hold_rig, monkeypatch):
    """lamp-75c9: wrist_roll stopped the arm; a hand held wrist_pitch at 32 % over its 17 % floor."""
    svc = hold_rig.svc
    learned = {"base_yaw": 210, "wrist_roll": 216, "wrist_pitch": 171}
    monkeypatch.setattr(svc, "_contact_samples", lambda loads: (loads, learned))
    hold_rig.bus.loads = {**IDLE, "wrist_roll": 224}
    _ticks(hold_rig, 0.15)
    assert svc.contact_active
    assert svc._contact_hold_floors["wrist_pitch"] == 171
    assert svc._contact_hold_floors["base_yaw"] == 210
    hold_rig.bus.loads = {**IDLE, "wrist_pitch": 321}
    _ticks(hold_rig, 0.7)
    assert svc.overload_active


def test_a_learned_floor_never_lifts_a_release_over_the_fixed_one(hold_rig, monkeypatch):
    svc = hold_rig.svc
    monkeypatch.setattr(svc, "_contact_samples", lambda loads: (loads, {"wrist_roll": 216, "wrist_pitch": 720}))
    hold_rig.bus.loads = {**IDLE, "wrist_roll": 224}
    _ticks(hold_rig, 0.15)
    assert svc._contact_hold_floors["wrist_pitch"] == 650
