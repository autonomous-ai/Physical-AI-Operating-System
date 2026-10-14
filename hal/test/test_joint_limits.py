"""SAFETY.md motion.joint_limits: parsed from the lamp contract, clamped on every goal write."""

import os
from types import SimpleNamespace

import pytest

from hal.safety.policy import parse_safety

LAMP_SAFETY = os.path.join(os.path.dirname(__file__), "..", "..", "robots", "lamp", "SAFETY.md")


def _motion(body: str):
    return parse_safety(f"---\nschema: autonomous.safety.v1\nmotion:\n{body}---\n").motion


def test_the_lamp_declares_a_range_for_every_joint():
    with open(LAMP_SAFETY) as f:
        limits = parse_safety(f.read()).motion.joint_limits
    assert set(limits) == {"base_yaw", "base_pitch", "elbow_pitch", "wrist_roll", "wrist_pitch"}
    assert limits["base_yaw"] == (-105.0, 105.0)
    assert all(lo < hi for lo, hi in limits.values())


def test_joint_limits_parse_and_stop_at_the_next_motion_key():
    m = _motion("  joint_limits:\n    base_yaw: [-10, 10.5]   # note\n    elbow_pitch: [0, 5]\n"
                "  max_speed: 120\n")
    assert m.joint_limits == {"base_yaw": (-10.0, 10.5), "elbow_pitch": (0.0, 5.0)}
    assert m.max_speed == 120


@pytest.mark.parametrize("body", [
    "  joint_limits:\n    base_yaw: [10, -10]\n",
    "  joint_limits:\n    base_yaw: 10\n",
    "  joint_limits:\n  max_speed: 120\n",
])
def test_a_malformed_joint_limit_fails_loud(body):
    with pytest.raises(ValueError):
        _motion(body)


def test_undeclared_means_no_limits():
    assert _motion("  max_speed: 120\n").joint_limits is None


def test_every_goal_write_is_clamped_into_the_limits():
    pytest.importorskip("lerobot")
    from hal.follower.hal_follower import LeLampFollower
    written = {}
    robot = SimpleNamespace(
        is_connected=True, goal_writes_blocked=False, _last_limit_log=0.0,
        joint_limits={"base_pitch": (-2.0, 67.0), "base_yaw": (-105.0, 105.0)},
        config=SimpleNamespace(max_relative_target=None), last_goal={},
        bus=SimpleNamespace(sync_write=lambda reg, goal: written.update(goal)),
    )
    sent = LeLampFollower.send_action(
        robot, {"base_pitch.pos": -18.0, "base_yaw.pos": 170.0, "elbow_pitch.pos": 58.0})
    assert written == {"base_pitch": -2.0, "base_yaw": 105.0, "elbow_pitch": 58.0}
    assert sent["base_pitch.pos"] == -2.0
