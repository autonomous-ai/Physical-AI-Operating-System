"""Servo overload cut-off: the guard's timing, and the service's cut / lockout / retry."""

import sys
import time
import types
from unittest import mock

import pytest

from hal.drivers.motors.contact_profile import ContactProfile
from hal.drivers.motors.overload import OverloadGuard, load_magnitude
from hal.presets import SERVO_CMD_PLAY

JOINTS = ("base_yaw", "base_pitch", "elbow_pitch", "wrist_roll", "wrist_pitch")
IDLE = {joint: 120 for joint in JOINTS}


class _Clock:
    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now

    def advance(self, seconds):
        self.now += seconds


def _guard(threshold=800, hold_s=1.0, retry_s=120.0):
    clock = _Clock()
    return OverloadGuard(threshold, hold_s, retry_s, clock=clock), clock


def _stalled(joint="elbow_pitch", load=950):
    return {**IDLE, joint: load}


# --- load_magnitude -----------------------------------------------------------


def test_load_magnitude_drops_the_direction_bit():
    assert load_magnitude(900) == 900
    assert load_magnitude(0x400 | 900) == 900
    assert load_magnitude(1000) == 1000
    assert load_magnitude(0x400 | 1000) == 1000
    assert load_magnitude(0) == 0


# --- OverloadGuard ------------------------------------------------------------


def test_normal_load_never_trips():
    guard, clock = _guard()
    for _ in range(100):
        assert guard.observe(IDLE) is None
        clock.advance(0.1)
    assert not guard.locked
    assert guard.trips == 0


def test_a_spike_shorter_than_the_hold_does_not_trip():
    guard, clock = _guard()
    assert guard.observe(_stalled()) is None
    clock.advance(0.9)
    assert guard.observe(_stalled()) is None
    clock.advance(0.1)
    assert guard.observe(IDLE) is None
    clock.advance(0.1)
    # The earlier spike must not count towards a later one.
    assert guard.observe(_stalled()) is None
    clock.advance(0.9)
    assert guard.observe(_stalled()) is None
    assert not guard.locked


def test_sustained_high_load_trips_once_the_hold_has_passed():
    guard, clock = _guard()
    assert guard.observe(_stalled()) is None
    clock.advance(0.5)
    assert guard.observe(_stalled()) is None
    clock.advance(0.5)
    assert guard.observe(_stalled()) == ("elbow_pitch", 950)
    assert guard.locked
    assert guard.trips == 1
    assert guard.last_trip == ("elbow_pitch", 950)
    assert guard.retry_in_s() == 120.0


def test_load_exactly_at_the_threshold_counts():
    guard, clock = _guard()
    guard.observe(_stalled(load=800))
    clock.advance(1.0)
    assert guard.observe(_stalled(load=800)) == ("elbow_pitch", 800)


def test_the_most_loaded_stalled_joint_is_reported():
    guard, clock = _guard()
    both = {**IDLE, "base_pitch": 850, "wrist_pitch": 990}
    guard.observe(both)
    clock.advance(1.0)
    assert guard.observe(both) == ("wrist_pitch", 990)


def test_a_failed_read_restarts_the_timing():
    guard, clock = _guard()
    guard.observe(_stalled())
    clock.advance(5.0)
    assert guard.observe(None) is None
    # Two high samples either side of a gap are not one sustained overload.
    assert guard.observe(_stalled()) is None
    clock.advance(0.5)
    assert guard.observe({}) is None
    clock.advance(0.5)
    assert guard.observe(_stalled()) is None
    assert not guard.locked


def test_nothing_is_observed_during_the_lockout_and_retry_is_due_once():
    guard, clock = _guard()
    guard.observe(_stalled())
    clock.advance(1.0)
    assert guard.observe(_stalled()) is not None
    clock.advance(60.0)
    assert guard.observe(_stalled()) is None
    assert not guard.retry_due()
    assert guard.retry_in_s() == 60.0
    clock.advance(60.0)
    assert guard.retry_due()
    assert not guard.locked
    assert not guard.retry_due()
    assert guard.retry_in_s() == 0.0


def test_it_trips_again_after_the_retry_if_still_stalled():
    guard, clock = _guard()
    guard.observe(_stalled())
    clock.advance(1.0)
    guard.observe(_stalled())
    clock.advance(120.0)
    assert guard.retry_due()
    # A fresh hold is needed: the pre-lockout timing does not carry over.
    assert guard.observe(_stalled()) is None
    clock.advance(1.0)
    assert guard.observe(_stalled()) == ("elbow_pitch", 950)
    assert guard.trips == 2


def test_threshold_zero_disables_the_guard():
    guard, clock = _guard(threshold=0)
    assert not guard.enabled
    for _ in range(30):
        assert guard.observe(_stalled(load=1000)) is None
        clock.advance(0.1)
    assert not guard.locked


def test_a_per_joint_map_sets_each_floor_and_skips_joints_left_out():
    clock = _Clock()
    guard = OverloadGuard({"base_yaw": 650, "elbow_pitch": 950}, 0.05, 3.0, clock=clock)
    assert guard.enabled
    # 900 on base_pitch is not watched; 900 on elbow is under its floor.
    heavy = {**IDLE, "base_pitch": 1000, "elbow_pitch": 900}
    for _ in range(5):
        assert guard.observe(heavy) is None
        clock.advance(0.05)
    blocked_yaw = {**heavy, "base_yaw": 700}
    assert guard.observe(blocked_yaw) is None
    clock.advance(0.06)
    assert guard.observe(blocked_yaw) == ("base_yaw", 700)
    assert guard.threshold_for("wrist_roll") is None


def test_latest_and_peak_load_are_kept_for_tuning():
    guard, clock = _guard()
    guard.observe({**IDLE, "base_yaw": 400})
    clock.advance(0.1)
    guard.observe({**IDLE, "base_yaw": 250})
    assert guard.load["base_yaw"] == 250
    assert guard.peak["base_yaw"] == 400
    assert guard.peak["elbow_pitch"] == 120


# --- AnimationService integration (fake bus, no hardware) -----------------------


class _FakeBus:
    def __init__(self):
        self.motors = {joint: types.SimpleNamespace(id=i + 1) for i, joint in enumerate(JOINTS)}
        self.loads = dict(IDLE)
        self.positions = {joint: 10.0 for joint in JOINTS}
        self.goals = dict(self.positions)
        self.fail_read = False
        self.writes = []
        self.reads = []
        self.raw_writes = []
        self.port_handler = object()
        self.packet_handler = self

    def sync_read(self, name, normalize=True):
        if self.fail_read:
            raise ConnectionError("no status packet")
        self.reads.append(name)
        if name == "Present_Position":
            return dict(self.positions)
        if name == "Goal_Position":
            return dict(self.goals)
        assert (name, normalize) == ("Present_Load", False)
        return dict(self.loads)

    def write(self, name, motor, value):
        self.writes.append((name, motor, value))

    # scservo_sdk packet-handler surface used by the raw paths.
    def ping(self, ph, sid):
        return 0, 0, 0

    def read2ByteTxRx(self, ph, sid, reg):
        return 2048, 0, 0

    def write1ByteTxRx(self, ph, sid, reg, value):
        self.raw_writes.append((sid, reg, value))

    def write2ByteTxRx(self, ph, sid, reg, value):
        self.raw_writes.append((sid, reg, value))


class _FakeRobot:
    is_connected = True

    def __init__(self):
        self.bus = _FakeBus()
        self.goal_writes_blocked = False
        self.sent = []
        self.last_goal = {}
        self.last_write_monotonic = 0.0

    def forget_goals(self):
        self.last_goal = {}

    def send_action(self, action):
        # Mirrors LeLampFollower.send_action's overload gate.
        if self.goal_writes_blocked:
            return {}
        self.sent.append(dict(action))
        return action


def _animation_service_cls():
    """AnimationService, with hal.follower stubbed where lerobot is not installed."""
    try:
        import hal.follower  # noqa: F401
    except Exception:
        stub = types.ModuleType("hal.follower")
        stub.LeLampFollowerConfig = lambda **kwargs: types.SimpleNamespace(**kwargs)
        stub.LeLampFollower = object
        with mock.patch.dict(sys.modules, {"hal.follower": stub}):
            from hal.drivers.motors.animation_service import AnimationService
        return AnimationService
    from hal.drivers.motors.animation_service import AnimationService
    return AnimationService


@pytest.fixture
def rig():
    """A service wired to a fake bus, driven tick by tick (the monitor thread never starts)."""
    guard, clock = _guard()
    fired = []
    svc = _animation_service_cls()(
        port="/dev/null", lamp_id="test",
        overload_guard=guard, on_overload=lambda joint, load: fired.append((joint, load)),
    )
    svc.robot = _FakeRobot()
    svc._running.set()
    svc.resume = mock.Mock()
    return types.SimpleNamespace(svc=svc, guard=guard, clock=clock, fired=fired, bus=svc.robot.bus)


def _stall(rig, joint="elbow_pitch", load=950):
    """Hold one joint over the threshold until the cut-off fires."""
    rig.bus.loads = _stalled(joint, load)
    rig.svc._overload_tick()
    rig.clock.advance(1.0)
    rig.svc._overload_tick()


def test_normal_load_leaves_the_servos_alone(rig):
    for _ in range(50):
        rig.svc._overload_tick()
        rig.clock.advance(0.1)
    assert rig.bus.writes == []
    assert not rig.svc.overload_active
    assert rig.fired == []


def test_a_stall_cuts_torque_on_every_servo_and_reports_it(rig):
    _stall(rig)
    assert rig.bus.writes == [("Torque_Enable", joint, 0) for joint in JOINTS]
    assert rig.svc.robot.goal_writes_blocked is True
    assert rig.svc._halt.is_set()
    assert rig.svc.overload_active
    assert rig.fired == [("elbow_pitch", 950)]


def test_the_direction_bit_does_not_hide_a_stall(rig):
    _stall(rig, load=0x400 | 950)
    assert rig.fired == [("elbow_pitch", 950)]


def test_a_brief_spike_does_not_cut_torque(rig):
    rig.bus.loads = _stalled()
    rig.svc._overload_tick()
    rig.clock.advance(0.5)
    rig.bus.loads = dict(IDLE)
    rig.svc._overload_tick()
    rig.clock.advance(1.0)
    rig.svc._overload_tick()
    assert rig.bus.writes == []
    assert not rig.svc.overload_active


def test_a_failed_load_read_never_cuts_torque(rig):
    rig.bus.loads = _stalled()
    rig.bus.fail_read = True
    for _ in range(30):
        rig.svc._overload_tick()
        rig.clock.advance(0.1)
    assert rig.bus.writes == []
    assert not rig.svc.overload_active


def test_a_failing_handler_does_not_undo_the_cut(rig):
    rig.svc._on_overload = mock.Mock(side_effect=RuntimeError("tts down"))
    _stall(rig)
    assert rig.svc.overload_active
    assert rig.svc.robot.goal_writes_blocked is True


def test_commanded_moves_are_refused_during_the_lockout(rig):
    _stall(rig)
    target = {"base_yaw.pos": 10.0}
    for call in (
        lambda: rig.svc.move_to(target, duration=0.1),
        lambda: rig.svc.move_and_hold(target, duration=0.1),
        lambda: rig.svc.move_and_hold(target, duration=0),
        lambda: rig.svc.send_positions(target),
    ):
        with pytest.raises(RuntimeError, match="overload cut-off active"):
            call()
    assert rig.svc.robot.sent == []


def test_no_goal_or_torque_on_write_reaches_the_bus_during_the_lockout(rig):
    _stall(rig)
    rig.svc.halt()                                  # would pin the current pose
    rig.svc.robot.send_action({"base_yaw.pos": 5.0})  # tracker / playback path
    rig.svc.move_to_raw({"base_yaw": 2048}, duration=0.1)
    with mock.patch.dict(sys.modules, {"scservo_sdk": types.SimpleNamespace(COMM_SUCCESS=0)}):
        rig.svc._configure_servos_raw(energize=True)
    assert rig.svc.robot.sent == []
    goal_or_torque_on = [
        w for w in rig.bus.raw_writes if w[1] == 42 or (w[1] == 40 and w[2] == 1)
    ]
    assert goal_or_torque_on == []


def _trip_now(rig):
    """Cut immediately, the way observe() + _overload_cut() do, from inside a bus call."""
    rig.guard._locked_until = rig.clock() + rig.guard.retry_s
    rig.svc._overload_cut("elbow_pitch", 950)


def test_a_cut_mid_raw_move_stops_its_remaining_goal_writes(rig):
    """Startup / zero / park moves must not keep re-engaging torque after the cut."""
    rig.svc.fps = 100
    goals_before_cut = []
    real = rig.bus.write2ByteTxRx

    def write2(ph, sid, reg, value):
        real(ph, sid, reg, value)
        # Trips on the last write of frame 2's batch: a real cut needs the bus lock, so it
        # can only land between batches.
        if reg == 42 and len(goals_before_cut) == 0 and len(rig.bus.raw_writes) >= 4:
            goals_before_cut.append(len([w for w in rig.bus.raw_writes if w[1] == 42]))
            _trip_now(rig)

    rig.bus.write2ByteTxRx = write2
    rig.svc.move_to_raw({"base_yaw": 2048, "base_pitch": 2048}, duration=0.05)
    goals_total = len([w for w in rig.bus.raw_writes if w[1] == 42])
    assert goals_before_cut and goals_total == goals_before_cut[0]
    assert rig.svc.overload_active


def test_a_cut_landing_before_configure_takes_the_lock_still_wins(rig):
    """The torque-on decision is made under the bus lock, not before it."""
    real_lock = rig.svc.bus_lock
    tripped = []

    class Lock:
        def __enter__(self):
            if not tripped:
                tripped.append(True)
                _trip_now(rig)
            return real_lock.__enter__()

        def __exit__(self, *exc):
            return real_lock.__exit__(*exc)

    rig.svc.bus_lock = Lock()
    with mock.patch.dict(sys.modules, {"scservo_sdk": types.SimpleNamespace(COMM_SUCCESS=0)}):
        rig.svc._configure_servos_raw(energize=True)
    assert [w for w in rig.bus.raw_writes if w[1] == 40 and w[2] == 1] == []


def test_a_failed_torque_off_is_retried_until_confirmed(rig):
    failing = {"wrist_pitch"}
    real = rig.bus.write

    def flaky(name, motor, value):
        if motor in failing:
            raise ConnectionError("no status packet")
        real(name, motor, value)

    rig.bus.write = flaky
    _stall(rig)
    status = rig.svc.overload_status()
    assert status["active"] and status["cut_complete"] is False
    assert status["pending_off"] == ["wrist_pitch"]
    assert rig.svc.robot.goal_writes_blocked is True
    # Still failing: retried every tick, still pending.
    rig.clock.advance(0.1)
    rig.svc._overload_tick()
    assert rig.svc.overload_status()["pending_off"] == ["wrist_pitch"]
    # Bus recovers: the next tick confirms it.
    failing.clear()
    rig.clock.advance(0.1)
    rig.svc._overload_tick()
    status = rig.svc.overload_status()
    assert status["cut_complete"] is True and status["pending_off"] == []
    assert ("Torque_Enable", "wrist_pitch", 0) in rig.bus.writes
    assert rig.svc.overload_active


def test_a_torque_off_that_never_confirms_is_dropped_at_recovery(rig):
    rig.bus.write = mock.Mock(side_effect=ConnectionError("bus down"))
    _stall(rig)
    assert rig.svc.overload_status()["pending_off"] == sorted(JOINTS)
    for _ in range(5):
        rig.clock.advance(0.1)
        rig.svc._overload_tick()
    assert rig.bus.write.call_count == len(JOINTS) * 6
    rig.clock.advance(120.0)
    rig.svc._overload_tick()
    assert not rig.svc.overload_active
    assert rig.svc.overload_status()["pending_off"] == []
    rig.svc.resume.assert_called_once_with()


def test_configure_still_energizes_outside_a_lockout(rig):
    with mock.patch.dict(sys.modules, {"scservo_sdk": types.SimpleNamespace(COMM_SUCCESS=0)}):
        rig.svc._configure_servos_raw(energize=True)
    assert [(sid, 40, 1) for sid in range(1, 6)] == [
        w for w in rig.bus.raw_writes if w[1] == 40 and w[2] == 1
    ]


def test_configure_writes_each_declared_torque_limit():
    svc = _animation_service_cls()(
        port="/dev/null", lamp_id="test", torque_limits={"base_pitch": 700, "elbow_pitch": 700},
    )
    svc.robot = _FakeRobot()
    with mock.patch.dict(sys.modules, {"scservo_sdk": types.SimpleNamespace(COMM_SUCCESS=0)}):
        svc._configure_servos_raw(energize=True)
    # Torque_Limit is register 48; base_pitch is ID 2 and elbow_pitch ID 3.
    assert [w for w in svc.robot.bus.raw_writes if w[1] == 48] == [(2, 48, 700), (3, 48, 700)]


def test_the_lockout_ends_after_the_retry_delay_with_a_resume(rig):
    _stall(rig)
    rig.clock.advance(119.0)
    rig.svc._overload_tick()
    assert rig.svc.overload_active
    rig.svc.resume.assert_not_called()
    rig.clock.advance(1.0)
    rig.svc._overload_tick()
    assert not rig.svc.overload_active
    assert rig.svc.robot.goal_writes_blocked is False
    rig.svc.resume.assert_called_once_with()


def test_a_body_parked_during_the_lockout_is_not_woken_by_the_retry(rig):
    _stall(rig)
    rig.svc._running.clear()        # release() (sleep) or zero_pose() stopped the loop
    rig.clock.advance(120.0)
    rig.svc._overload_tick()
    assert not rig.svc.overload_active
    assert rig.svc.robot.goal_writes_blocked is False
    rig.svc.resume.assert_not_called()


def test_still_blocked_after_the_retry_cuts_again(rig):
    _stall(rig)
    rig.clock.advance(120.0)
    rig.svc._overload_tick()
    rig.bus.writes.clear()
    _stall(rig)
    assert rig.bus.writes == [("Torque_Enable", joint, 0) for joint in JOINTS]
    assert rig.fired == [("elbow_pitch", 950), ("elbow_pitch", 950)]
    assert rig.guard.trips == 2


def test_status_reports_the_lockout_for_health(rig):
    status = rig.svc.overload_status()
    assert status["active"] is False and status["trips"] == 0 and status["last_trip"] is None
    _stall(rig)
    rig.clock.advance(20.0)
    status = rig.svc.overload_status()
    assert status["active"] is True
    assert status["retry_in_s"] == 100.0
    assert status["last_trip"] == {"joint": "elbow_pitch", "load": 950}
    assert (status["threshold"], status["hold_s"], status["retry_s"]) == (800, 1.0, 120.0)
    assert status["peak"]["elbow_pitch"] == 950


def test_a_service_without_a_guard_has_no_cut_off():
    svc = _animation_service_cls()(port="/dev/null", lamp_id="test")
    assert svc.overload_status() is None
    assert not svc.overload_active
    svc._refuse_if_overloaded()


def test_the_follower_drops_goals_while_blocked():
    """The real gate, where lerobot is installed."""
    pytest.importorskip("lerobot")
    from hal.follower.hal_follower import LeLampFollower

    robot = types.SimpleNamespace(
        is_connected=True, goal_writes_blocked=True, bus=mock.Mock(),
        config=types.SimpleNamespace(max_relative_target=None), last_write_monotonic=0.0, joint_limits={},
        last_goal={},
    )
    assert LeLampFollower.send_action(robot, {"base_yaw.pos": 5.0}) == {}
    robot.bus.sync_write.assert_not_called()
    assert robot.last_goal == {}
    robot.goal_writes_blocked = False
    LeLampFollower.send_action(robot, {"base_yaw.pos": 5.0})
    robot.bus.sync_write.assert_called_once_with("Goal_Position", {"base_yaw": 5.0})
    assert robot.last_goal == {"base_yaw": 5.0}


def test_the_follower_forgets_its_goals_when_a_write_fails():
    pytest.importorskip("lerobot")
    from hal.follower.hal_follower import LeLampFollower

    robot = types.SimpleNamespace(
        is_connected=True, goal_writes_blocked=False, bus=mock.Mock(),
        config=types.SimpleNamespace(max_relative_target=None), last_write_monotonic=0.0, joint_limits={},
        last_goal={"base_yaw": 5.0},
    )
    robot.forget_goals = lambda: setattr(robot, "last_goal", {})
    robot.bus.sync_write.side_effect = ConnectionError("port gone")
    with pytest.raises(ConnectionError):
        LeLampFollower.send_action(robot, {"base_yaw.pos": 9.0})
    assert robot.last_goal == {}


# --- Lag from the goals just written ---------------------------------------------


def _written(rig, goal=12.5, age=0.0):
    rig.svc.robot.last_goal = {joint: goal for joint in JOINTS}
    rig.svc.robot.last_write_monotonic = time.monotonic() - age


def test_lag_takes_the_goals_just_written_without_reading_them_back(rig):
    _written(rig)
    assert rig.svc._read_lags() == {"lag:" + joint: 25 for joint in JOINTS}
    assert rig.bus.reads == ["Present_Position"]


def test_lag_reads_the_register_for_a_joint_with_no_written_goal(rig):
    _written(rig)
    del rig.svc.robot.last_goal["wrist_pitch"]
    assert rig.svc._read_lags() == {"lag:" + joint: 0 for joint in JOINTS}
    assert "Goal_Position" in rig.bus.reads


def test_lag_reads_the_register_once_no_goal_was_written_for_a_while(rig):
    """A sync_write has no reply: a goal a servo missed is not trusted for long."""
    _written(rig, age=2.0)
    assert rig.svc._read_lags() == {"lag:" + joint: 0 for joint in JOINTS}
    assert "Goal_Position" in rig.bus.reads


def test_a_raw_move_forgets_the_written_goals(rig):
    _written(rig)
    rig.svc.move_to_raw({"base_yaw": 2048}, duration=0.05)
    assert rig.svc.robot.last_goal == {}


def test_a_torque_cut_forgets_the_written_goals(rig):
    _written(rig)
    _stall(rig)
    assert rig.svc.robot.last_goal == {}


# --- Contact stop (halt in place on a short hit) --------------------------------


@pytest.fixture
def contact_rig():
    """The rig plus a contact guard: 95 % for 0.1 s halts, 3 s pause."""
    guard, clock = _guard()
    contact = OverloadGuard(950, 0.1, 3.0, clock=clock)
    fired = []
    svc = _animation_service_cls()(
        port="/dev/null", lamp_id="test",
        overload_guard=guard, contact_guard=contact,
        on_overload=lambda joint, load: fired.append((joint, load)),
    )
    svc.robot = _FakeRobot()
    svc._running.set()
    svc.resume = mock.Mock()
    svc.dispatch = mock.Mock()
    return types.SimpleNamespace(
        svc=svc, guard=guard, contact=contact, clock=clock, fired=fired, bus=svc.robot.bus,
    )


def _hit(rig, joint="base_yaw", load=1000):
    """Saturate one joint past the contact stop's 0.1 s hold, sampled every 50 ms."""
    rig.bus.loads = _stalled(joint, load)
    for _ in range(4):
        rig.svc._overload_tick()
        rig.clock.advance(0.05)


def test_a_hit_halts_in_place_and_keeps_torque_on(contact_rig):
    contact_rig.bus.positions = {joint: float(i) for i, joint in enumerate(JOINTS)}
    _hit(contact_rig)
    svc = contact_rig.svc
    assert svc.contact_active and not svc.overload_active
    assert svc._halt.is_set()
    # The pose it stopped at is pinned, then no further goal gets through.
    assert svc.robot.sent == [{f"{joint}.pos": float(i) for i, joint in enumerate(JOINTS)}]
    assert svc.robot.goal_writes_blocked is True
    assert contact_rig.bus.writes == []          # no Torque_Enable write at all
    assert contact_rig.fired == [("base_yaw", 1000)]


def test_free_motion_peaks_do_not_stop_the_arm(contact_rig):
    # Highest free-motion loads measured on lamp-52e6 (shock, acknowledge), held long.
    contact_rig.bus.loads = {**IDLE, "base_pitch": 872, "elbow_pitch": 736}
    for _ in range(10):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert not contact_rig.svc.contact_active
    assert contact_rig.fired == []


def test_a_single_saturated_sample_does_not_stop_the_arm(contact_rig):
    contact_rig.bus.loads = _stalled("base_yaw", 1000)
    contact_rig.svc._overload_tick()
    contact_rig.clock.advance(0.05)
    contact_rig.bus.loads = dict(IDLE)
    contact_rig.svc._overload_tick()
    assert not contact_rig.svc.contact_active


def test_commanded_moves_are_refused_during_the_pause(contact_rig):
    _hit(contact_rig)
    with pytest.raises(RuntimeError, match="contact stop active"):
        contact_rig.svc.move_and_hold({"base_yaw.pos": 10.0}, duration=0)


def test_the_pause_ends_back_in_idle(contact_rig):
    _hit(contact_rig)
    contact_rig.bus.loads = dict(IDLE)
    contact_rig.clock.advance(2.0)
    contact_rig.svc._overload_tick()
    assert contact_rig.svc.contact_active
    contact_rig.clock.advance(1.0)
    contact_rig.svc._overload_tick()
    svc = contact_rig.svc
    assert not svc.contact_active
    assert svc.robot.goal_writes_blocked is False
    svc.dispatch.assert_called_once_with(SERVO_CMD_PLAY, svc.idle_recording)
    svc.resume.assert_not_called()               # torque never went off


def test_a_parked_body_stays_put_after_the_pause(contact_rig):
    _hit(contact_rig)
    contact_rig.bus.loads = dict(IDLE)
    contact_rig.svc._running.clear()
    contact_rig.clock.advance(3.0)
    contact_rig.svc._overload_tick()
    assert contact_rig.svc.robot.goal_writes_blocked is False
    contact_rig.svc.dispatch.assert_not_called()


def test_still_forced_while_held_falls_through_to_the_cut_off(contact_rig):
    _hit(contact_rig, "elbow_pitch", 1000)
    # Someone keeps forcing the held arm: the cut-off still watches and cuts torque.
    for _ in range(25):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.overload_active
    assert contact_rig.bus.writes == [("Torque_Enable", joint, 0) for joint in JOINTS]
    # The contact pause ending must not reopen goal writes under the cut-off.
    contact_rig.svc._contact_recover()
    assert contact_rig.svc.robot.goal_writes_blocked is True


def test_a_hand_still_holding_through_the_pause_releases_torque(contact_rig):
    """Held, not bumped: past release_after_s into the pause the arm goes limp for retry_s."""
    contact_rig.svc._contact_release_s = 0.5     # under the rig cut-off's own 1 s hold
    _hit(contact_rig, "base_yaw", 1000)
    for _ in range(12):                          # the hand stays on for 0.6 s
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    svc = contact_rig.svc
    assert svc.overload_active and not svc.contact_active
    assert contact_rig.bus.writes == [("Torque_Enable", joint, 0) for joint in JOINTS]
    assert svc.robot.goal_writes_blocked is True
    assert contact_rig.fired[-1] == ("base_yaw", 1000)   # the cut-off's handler (chime)
    assert contact_rig.guard.retry_in_s() == pytest.approx(120.0, abs=0.1)


def _hit_under_learned_floor(rig, monkeypatch, joint, floor, load):
    """Stop the arm on a learned envelope floor far below Lamp's fixed ones."""
    rig.svc._contact.threshold = {"base_yaw": 550, "wrist_roll": 750, "wrist_pitch": 650}
    monkeypatch.setattr(rig.svc, "_contact_samples", lambda loads: (loads, {joint: floor}))
    _hit(rig, joint, load)
    assert rig.svc.contact_active and rig.svc._contact.last_floor == floor


def test_a_hold_at_the_force_that_stopped_the_arm_releases_torque(contact_rig, monkeypatch):
    """lamp-52e6: a hand stopped base_yaw at 28-40 % (envelope 18 %) and held it at 23-36 %,
    never near the fixed 55 %. base_yaw carries no weight, so the stopping floor decides."""
    contact_rig.svc._contact_release_s = 0.5
    _hit_under_learned_floor(contact_rig, monkeypatch, "base_yaw", 180, 396)
    contact_rig.bus.loads = _stalled("base_yaw", 232)
    for _ in range(12):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.overload_active and not contact_rig.svc.contact_active
    assert contact_rig.bus.writes == [("Torque_Enable", joint, 0) for joint in JOINTS]


def test_gravity_load_over_the_stopping_floor_only_pauses(contact_rig, monkeypatch):
    """lamp-a0ae went limp untouched: elbow_pitch stopped at 38.0 % over a 22.7 % envelope
    floor, then held 33.4 % on gravity alone in the pose it stopped in."""
    contact_rig.svc._contact_release_s = 0.5
    _hit_under_learned_floor(contact_rig, monkeypatch, "elbow_pitch", 227, 380)
    contact_rig.bus.loads = _stalled("elbow_pitch", 334)
    for _ in range(50):                          # the whole pause, short of its 3 s end
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.contact_active and not contact_rig.svc.overload_active
    assert contact_rig.bus.writes == []


def test_gravity_sag_over_a_learned_lag_floor_only_pauses(contact_rig, monkeypatch):
    """Pinned untouched, lamp-a0ae's elbow_pitch still trailed its goal by 2.1 deg."""
    contact_rig.svc._contact_release_s = 0.5
    _hit_under_learned_floor(contact_rig, monkeypatch, "lag:elbow_pitch", 15, 200)
    contact_rig.bus.loads = dict(IDLE)
    monkeypatch.setattr(contact_rig.svc, "_read_lags", lambda: {"lag:elbow_pitch": 21})
    for _ in range(50):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.contact_active and not contact_rig.svc.overload_active


def test_a_hand_pushing_a_pinned_base_yaw_off_its_pose_releases_torque(contact_rig, monkeypatch):
    """lamp-a0ae: a hand held base_yaw at only 12-16 % (stop floor 18 %) but 1.3-8.5 deg off,
    pushing it further than the pin had settled to."""
    contact_rig.svc._contact_release_s = 0.5
    _hit_under_learned_floor(contact_rig, monkeypatch, "base_yaw", 180, 320)
    contact_rig.bus.loads = _stalled("base_yaw", 140)
    lag = {"lag:base_yaw": 3}
    monkeypatch.setattr(contact_rig.svc, "_read_lags", lambda: dict(lag))
    for _ in range(11):                          # settles 0.3 deg off
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    lag["lag:base_yaw"] = 25                     # then pushed 2.5 deg off
    for _ in range(12):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.overload_active


def test_a_pinned_base_yaw_held_off_by_its_cable_only_pauses(contact_rig, monkeypatch):
    """lamp-75c9: pinned untouched at -72 deg, base_yaw held 40 % and sat 2.7 deg off its pin."""
    contact_rig.svc._contact_release_s = 0.5
    _hit_under_learned_floor(contact_rig, monkeypatch, "base_yaw", 180, 320)
    contact_rig.bus.loads = _stalled("base_yaw", 100)
    monkeypatch.setattr(contact_rig.svc, "_read_lags", lambda: {"lag:base_yaw": 27})
    for _ in range(50):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.contact_active and not contact_rig.svc.overload_active
    assert contact_rig.svc._contact_hold_floors["lag:base_yaw"] == 27 + 10


def test_a_hand_on_a_pinned_joint_from_the_stop_still_releases_past_the_cap(contact_rig, monkeypatch):
    contact_rig.svc._contact_release_s = 0.5
    _hit_under_learned_floor(contact_rig, monkeypatch, "base_yaw", 180, 320)
    contact_rig.bus.loads = _stalled("base_yaw", 140)
    monkeypatch.setattr(contact_rig.svc, "_read_lags", lambda: {"lag:base_yaw": 85})
    for _ in range(25):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.overload_active


def test_the_ramp_back_to_idle_is_not_held_to_the_still_floors(profile_rig, monkeypatch):
    """lamp-75c9: halted at a -72 deg aim, the ramp back trailed base_yaw 17 deg; idle's settled
    flag was still up and the still floor (10 deg) stopped it."""
    svc = profile_rig.svc
    svc._contact_off_playback = {"lag:base_yaw": 100}
    svc._current_recording = "idle"
    svc._idle_settled = True
    svc._interpolation_frames = 30
    monkeypatch.setattr(svc, "_read_lags", lambda: {"lag:base_yaw": 173})
    samples, floors = svc._contact_samples(dict(IDLE))
    assert floors is None and "lag:base_yaw" not in samples
    svc._interpolation_frames = 0
    samples, floors = svc._contact_samples(dict(IDLE))
    assert floors == {"lag:base_yaw": 100}


def test_the_resume_after_a_cut_off_does_not_stop_on_its_own_ramp(contact_rig):
    """lamp-75c9: the ramp back from limp hit base_yaw's 60 % cap, stopped, released and looped."""
    _trip_now(contact_rig)
    contact_rig.clock.advance(120.0)
    contact_rig.svc._overload_tick()              # lockout over: resume
    contact_rig.svc.resume.assert_called_once()
    contact_rig.bus.loads = _stalled("base_yaw", 1000)
    for _ in range(8):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert not contact_rig.svc.contact_active
    contact_rig.bus.loads = dict(IDLE)
    contact_rig.clock.advance(contact_rig.svc.duration + 0.5)
    contact_rig.svc._overload_tick()
    _hit(contact_rig, "base_yaw", 1000)
    assert contact_rig.svc.contact_active


def test_a_pinned_base_yaw_left_alone_only_pauses(contact_rig, monkeypatch):
    contact_rig.svc._contact_release_s = 0.5
    _hit_under_learned_floor(contact_rig, monkeypatch, "base_yaw", 180, 320)
    contact_rig.bus.loads = dict(IDLE)
    monkeypatch.setattr(contact_rig.svc, "_read_lags", lambda: {"lag:base_yaw": 0, "lag:elbow_pitch": 12})
    for _ in range(50):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert contact_rig.svc.contact_active and not contact_rig.svc.overload_active


def test_a_bump_let_go_within_the_window_only_pauses(contact_rig):
    contact_rig.svc._contact_release_s = 1.0
    _hit(contact_rig, "base_yaw", 1000)
    contact_rig.bus.loads = dict(IDLE)           # the hand is gone
    for _ in range(70):
        contact_rig.svc._overload_tick()
        contact_rig.clock.advance(0.05)
    svc = contact_rig.svc
    assert not svc.overload_active and not svc.contact_active
    assert contact_rig.bus.writes == []
    svc.dispatch.assert_called_once_with(SERVO_CMD_PLAY, svc.idle_recording)


def test_status_reports_the_contact_stop(contact_rig):
    assert contact_rig.svc.overload_status()["contact"]["trips"] == 0
    _hit(contact_rig)
    contact = contact_rig.svc.overload_status()["contact"]
    assert contact["active"] is True
    assert contact["last_trip"] == {"joint": "base_yaw", "load": 1000}
    assert (contact["threshold"], contact["hold_s"], contact["pause_s"]) == (950, 0.1, 3.0)


def test_without_a_contact_guard_status_has_none(rig):
    assert rig.svc.overload_status()["contact"] is None


# --- Learned contact envelope -------------------------------------------------


def test_profile_keeps_the_max_per_frame_and_adds_the_margin(tmp_path):
    profile = ContactProfile(str(tmp_path / "p.json"), margin=150)
    profile.learn("stretching", 10, {"base_pitch": 300, "elbow_pitch": 200})
    profile.learn("stretching", 10, {"base_pitch": 340, "elbow_pitch": 180})
    profile.learn("stretching", 12, {"base_pitch": 600})
    assert profile.floors("stretching", 10) == {"base_pitch": 750, "elbow_pitch": 350}
    # The window reaches three frames either side; far from anything learned, nothing.
    assert profile.floors("stretching", 15)["base_pitch"] == 750
    assert profile.floors("stretching", 40) is None
    assert profile.floors("shock", 10) is None


def test_profile_round_trips_through_its_file(tmp_path):
    path = str(tmp_path / "sub" / "p.json")
    profile = ContactProfile(path, margin=150)
    profile.learn("nod", 3, {"base_yaw": 120})
    profile.save()
    again = ContactProfile.load(path, margin=100)
    assert again.recordings() == ["nod"]
    assert again.floors("nod", 3) == {"base_yaw": 220}
    (tmp_path / "bad.json").write_text("{")
    assert ContactProfile.load(str(tmp_path / "bad.json"), 150).recordings() == []
    assert ContactProfile.load(str(tmp_path / "none.json"), 150).recordings() == []


def test_a_unit_that_never_learned_has_no_envelope(tmp_path):
    """No shared default: another unit's envelope tripped lamp-a0ae's idle every 8 s."""
    unit = str(tmp_path / "unit.json")
    profile = ContactProfile.load(unit, 150)
    assert profile.floors("nod", 0) is None
    assert profile.path == unit                      # a learn run saves per unit
    (tmp_path / "unit.json").write_text('{"nod": {"base_yaw": [300]}}')
    assert ContactProfile.load(unit, 150).floors("nod", 0) == {"base_yaw": 450}


def test_per_sample_floors_override_the_configured_ones():
    clock = _Clock()
    guard = OverloadGuard({"base_yaw": 650}, 0.05, 3.0, clock=clock)
    pushed = {**IDLE, "base_pitch": 500}
    # base_pitch has no fixed floor; the learned one (350) makes 500 a hit.
    for _ in range(2):
        hit = guard.observe(pushed, {"base_pitch": 350})
        clock.advance(0.06)
    assert hit == ("base_pitch", 500)


@pytest.fixture
def profile_rig(contact_rig, tmp_path):
    profile = ContactProfile(str(tmp_path / "p.json"), margin=150)
    svc = contact_rig.svc
    svc._contact_profile = profile
    svc._current_recording = "stretching"
    svc._current_actions = [{}] * 100
    svc._current_frame_index = 20
    svc._interpolation_frames = 0
    contact_rig.profile = profile
    return contact_rig


def test_a_push_above_the_envelope_stops_a_weight_bearing_joint(profile_rig):
    profile_rig.profile.learn("stretching", 20, {**IDLE, "base_pitch": 300})
    profile_rig.bus.loads = {**IDLE, "base_pitch": 500}   # far under any fixed floor
    for _ in range(4):
        profile_rig.svc._overload_tick()
        profile_rig.clock.advance(0.05)
    assert profile_rig.svc.contact_active
    assert profile_rig.fired == [("base_pitch", 500)]


def test_an_envelope_hit_on_a_joint_without_a_fixed_floor_still_halts(profile_rig, tmp_path):
    # Lamp's real shape: base_pitch has no fixed floor, only the learned one.
    svc = profile_rig.svc
    svc._contact = OverloadGuard({"base_yaw": 650}, 0.05, 3.0, clock=profile_rig.clock)
    profile_rig.profile.learn("stretching", 20, {**IDLE, "base_pitch": 300})
    profile_rig.bus.loads = {**IDLE, "base_pitch": 500}
    for _ in range(3):
        svc._overload_tick()
        profile_rig.clock.advance(0.06)
    assert svc.contact_active
    assert svc._halt.is_set() and svc.robot.goal_writes_blocked is True
    assert profile_rig.fired == [("base_pitch", 500)]
    assert svc._contact.last_floor == 450


def test_free_motion_within_the_envelope_does_not_stop(profile_rig):
    profile_rig.profile.learn("stretching", 20, {**IDLE, "base_pitch": 700})
    profile_rig.bus.loads = {**IDLE, "base_pitch": 820}
    for _ in range(5):
        profile_rig.svc._overload_tick()
        profile_rig.clock.advance(0.05)
    assert not profile_rig.svc.contact_active


def test_the_ramp_into_a_recording_has_no_envelope(profile_rig):
    profile_rig.profile.learn("stretching", 20, {**IDLE, "base_pitch": 300})
    profile_rig.svc._interpolation_frames = 10
    profile_rig.bus.loads = {**IDLE, "base_pitch": 500}
    for _ in range(5):
        profile_rig.svc._overload_tick()
        profile_rig.clock.advance(0.05)
    assert not profile_rig.svc.contact_active


def test_a_learn_run_records_instead_of_stopping(profile_rig):
    profile_rig.svc._learning = {"stretching"}
    profile_rig.bus.loads = {**IDLE, "base_pitch": 500}
    for _ in range(5):
        profile_rig.svc._overload_tick()
        profile_rig.clock.advance(0.05)
    assert not profile_rig.svc.contact_active
    assert profile_rig.profile.floors("stretching", 20)["base_pitch"] == 650


def test_learning_needs_a_profile(contact_rig):
    with pytest.raises(RuntimeError, match="No contact profile"):
        contact_rig.svc.learn_contact_profile(["nod"])


def _wait_with(svc, frames, timeout=90.0):
    """Run _wait_recording_done while each poll steps the playback to the next frame index.

    `frames` is the frame index seen at each poll; past its end the state stays put.
    Returns (result, polls).
    """
    svc._current_actions = [{}] * 10
    svc._idle_settled = False
    clock = [0.0]
    polls = [0]

    def sleep(dt):
        clock[0] += dt
        polls[0] += 1
        if polls[0] < len(frames):
            svc._current_frame_index = frames[polls[0]]

    svc._current_frame_index = frames[0]
    fake_time = types.SimpleNamespace(monotonic=lambda: clock[0], sleep=sleep)
    with mock.patch("hal.drivers.motors.animation_service.time", fake_time):
        return svc._wait_recording_done(svc._current_recording, timeout), polls[0]


def test_a_recording_holding_its_final_pose_is_done_once_played_through(contact_rig):
    # sleepy never falls back to idle: a learn run used to wait out the 90 s timeout,
    # three attempts per run, then log "never played through".
    svc = contact_rig.svc
    svc._current_recording = "sleepy"
    done, polls = _wait_with(svc, [0, 3, 6, 9, 10])
    assert done is True
    assert polls == 4


def test_a_held_final_pose_left_from_before_the_play_is_not_played_through(contact_rig):
    svc = contact_rig.svc
    svc._current_recording = "sleepy"
    done, _ = _wait_with(svc, [10], timeout=2.0)
    assert done is False


def test_a_stale_held_pose_then_a_fresh_play_waits_for_the_fresh_one(contact_rig):
    svc = contact_rig.svc
    svc._current_recording = "sleepy"
    done, polls = _wait_with(svc, [10, 10, 0, 5, 10])
    assert done is True
    assert polls == 4


def test_a_joint_trailing_its_learned_lag_stops_even_at_the_torque_cap(profile_rig):
    profile_rig.profile.lag_margin = 40
    # Learned: base_pitch at the 70 % cap here already, 3 deg behind its goal.
    profile_rig.profile.learn("stretching", 20, {**IDLE, "base_pitch": 700, "lag:base_pitch": 30})
    bus = profile_rig.bus
    bus.loads = {**IDLE, "base_pitch": 700}               # no load headroom left
    bus.goals = {**bus.positions, "base_pitch": 10.0 + 9.0}  # a hand holds it 9 deg back
    for _ in range(4):
        profile_rig.svc._overload_tick()
        profile_rig.clock.advance(0.05)
    assert profile_rig.svc.contact_active
    assert profile_rig.fired == [("lag:base_pitch", 90)]


def test_learned_lag_is_ignored_without_a_lag_margin(profile_rig):
    profile_rig.profile.learn("stretching", 20, {**IDLE, "lag:base_pitch": 30})
    profile_rig.bus.goals = {**profile_rig.bus.positions, "base_pitch": 30.0}
    for _ in range(4):
        profile_rig.svc._overload_tick()
        profile_rig.clock.advance(0.05)
    assert not profile_rig.svc.contact_active


def test_a_learn_run_records_the_lag_too(profile_rig):
    profile_rig.profile.lag_margin = 40
    profile_rig.svc._learning = {"stretching"}
    profile_rig.bus.goals = {**profile_rig.bus.positions, "elbow_pitch": 12.5}
    profile_rig.svc._overload_tick()
    assert profile_rig.profile.floors("stretching", 20)["lag:elbow_pitch"] == 25 + 40


def test_off_playback_floors_watch_gaze_moves_and_holds(contact_rig):
    svc = contact_rig.svc
    svc._contact = OverloadGuard({"base_yaw": 650}, 0.05, 3.0, clock=contact_rig.clock)
    svc._contact_off_playback = {"base_pitch": 620, "lag:base_pitch": 120}
    svc._current_recording = None                      # a gaze move owns the body
    bus = contact_rig.bus
    bus.goals = {**bus.positions, "base_pitch": 10.0 + 13.0}   # held 13 deg back
    for _ in range(3):
        svc._overload_tick()
        contact_rig.clock.advance(0.06)
    assert svc.contact_active
    assert contact_rig.fired == [("lag:base_pitch", 130)]


def test_off_playback_floors_sit_out_a_direct_move_and_its_settle(contact_rig):
    """A fast gaze/search move outruns the servo's ramp: lag then is not a hand."""
    from hal.drivers.motors.animation_service import OFF_PLAYBACK_SETTLE_S
    svc = contact_rig.svc
    svc._contact = OverloadGuard({"base_yaw": 650}, 0.05, 3.0, clock=contact_rig.clock)
    svc._contact_off_playback = {"lag:wrist_roll": 90}
    svc._current_recording = None
    bus = contact_rig.bus
    bus.goals = {**bus.positions, "wrist_roll": 10.0 + 13.0}   # 13 deg behind, as on a0ae

    def ticks(n):
        for _ in range(n):
            svc._overload_tick()
            contact_rig.clock.advance(0.06)

    svc._direct_move = True
    ticks(5)
    assert not svc.contact_active, "lag during the move itself"
    svc._direct_move = False
    svc._direct_move_ended = contact_rig.clock()
    ticks(int(OFF_PLAYBACK_SETTLE_S / 0.06) - 1)
    assert not svc.contact_active, "lag while the servo catches up"
    ticks(4)
    assert svc.contact_active, "still that far behind after settling: a hand"


def test_off_playback_floors_stay_out_of_a_playing_recording(contact_rig):
    svc = contact_rig.svc
    svc._contact_off_playback = {"lag:base_pitch": 120}
    svc._current_recording = "shock"
    svc._idle_settled = False
    contact_rig.bus.goals = {**contact_rig.bus.positions, "base_pitch": 30.0}
    for _ in range(4):
        svc._overload_tick()
        contact_rig.clock.advance(0.05)
    assert not svc.contact_active


def test_a_contact_stop_uses_its_own_handler_when_given(contact_rig):
    contact_rig.svc._on_contact = lambda joint, load: contact_rig.fired.append(("contact", joint))
    _hit(contact_rig)
    assert contact_rig.fired == [("contact", "base_yaw")]


def test_settled_idle_uses_this_units_envelope_over_off_playback(profile_rig):
    """lamp-a0ae's elbow idles at 54-60 %: its own envelope, not 52e6's 52 % floor."""
    svc = profile_rig.svc
    svc._contact = OverloadGuard({"base_yaw": 650}, 0.05, 3.0, clock=profile_rig.clock)
    svc._contact_off_playback = {"elbow_pitch": 520}
    svc._current_recording = "idle"
    svc._idle_settled = True
    profile_rig.bus.loads = {**IDLE, "elbow_pitch": 580}

    def ticks():
        for _ in range(4):
            svc._overload_tick()
            profile_rig.clock.advance(0.05)

    profile_rig.profile.learn("idle", 20, {**IDLE, "elbow_pitch": 600})
    ticks()
    assert not svc.contact_active, "inside this unit's learned idle"

    profile_rig.profile.forget("idle")
    ticks()
    assert svc.contact_active, "not learned: the off_playback floor still watches"


# --- Passive learning wired into the monitor --------------------------------------


@pytest.fixture
def passive_rig(profile_rig):
    from hal.drivers.motors.contact_profile import PassiveLearner
    profile_rig.svc._passive = PassiveLearner(profile_rig.profile)
    profile_rig.svc._current_actions = [{}] * 8
    profile_rig.svc._current_frame_index = 0
    return profile_rig


def _play_through(rig, frames=8):
    for frame in range(frames):
        rig.svc._current_frame_index = frame
        rig.svc._overload_tick()
        rig.clock.advance(0.05)
    rig.svc._current_frame_index = frames   # finished: no recording owns its frames
    rig.svc._overload_tick()


def test_a_free_run_feeds_the_passive_learner(passive_rig):
    _play_through(passive_rig)
    assert passive_rig.svc._passive.runs() == {"stretching": 1}
    assert passive_rig.svc.overload_status()["contact"]["profile"]["passive_runs"] == {"stretching": 1}


def test_a_run_with_a_contact_stop_is_not_learned(passive_rig):
    svc = passive_rig.svc
    svc._current_frame_index = 2
    svc._overload_tick()
    passive_rig.bus.loads = _stalled("base_yaw", 1000)
    for _ in range(4):
        svc._overload_tick()
        passive_rig.clock.advance(0.05)
    assert svc.contact_active
    svc._current_frame_index = 8
    svc._overload_tick()
    assert svc._passive.runs() == {}


def test_a_run_during_a_learn_run_is_left_to_the_learn_run(passive_rig):
    passive_rig.svc._learning = {"stretching"}
    _play_through(passive_rig)
    assert passive_rig.svc._passive.runs() == {}


def _stop_on_envelope(rig):
    """A learned base_pitch envelope of 30 % (floor 45 %) stops the arm at 50 %."""
    for frame in range(8):
        rig.profile.learn("stretching", frame, {**IDLE, "base_pitch": 300})
    rig.svc._current_frame_index = 4
    rig.bus.loads = {**IDLE, "base_pitch": 500}
    for _ in range(4):
        rig.svc._overload_tick()
        rig.clock.advance(0.05)
    assert rig.svc.contact_active


def test_a_stop_nobody_holds_through_loosens_the_envelope_there(passive_rig):
    _stop_on_envelope(passive_rig)
    passive_rig.bus.loads = dict(IDLE)
    passive_rig.clock.advance(3.0)
    passive_rig.svc._overload_tick()
    assert not passive_rig.svc.contact_active
    assert passive_rig.profile.envelope("stretching")["base_pitch"][4] == 500 - 150 + 1


def test_a_stop_held_into_a_torque_release_leaves_the_envelope(passive_rig):
    passive_rig.svc._contact_release_s = 0.5
    _stop_on_envelope(passive_rig)
    for _ in range(12):
        passive_rig.svc._overload_tick()
        passive_rig.clock.advance(0.05)
    assert passive_rig.svc.overload_active
    passive_rig.svc._contact_recover()
    assert passive_rig.profile.envelope("stretching")["base_pitch"][4] == 300


def test_a_hand_on_the_arm_short_of_a_release_is_no_false_stop(passive_rig):
    """lamp-75c9: hands held elbow_pitch at 30-45 % under its release floor; each pause
    loosened the envelope, and one at its 60 % cap lifted idle's whole elbow channel."""
    passive_rig.svc._contact_release_s = 5.0        # past the pause: never releases here
    _stop_on_envelope(passive_rig)
    passive_rig.clock.advance(0.6)
    passive_rig.bus.loads = {**IDLE, "base_pitch": 960}
    passive_rig.svc._overload_tick()                 # a moment over its floor, then let go
    passive_rig.bus.loads = dict(IDLE)
    passive_rig.clock.advance(2.4)
    passive_rig.svc._overload_tick()
    assert not passive_rig.svc.contact_active
    assert passive_rig.profile.envelope("stretching")["base_pitch"][4] == 300


def test_the_stop_itself_settling_is_not_taken_for_a_hand(passive_rig):
    passive_rig.svc._contact_release_s = 5.0
    _stop_on_envelope(passive_rig)
    passive_rig.bus.loads = {**IDLE, "base_pitch": 960}
    passive_rig.svc._overload_tick()                 # still decelerating, inside the settle
    passive_rig.bus.loads = dict(IDLE)
    passive_rig.clock.advance(3.0)
    passive_rig.svc._overload_tick()
    assert passive_rig.profile.envelope("stretching")["base_pitch"][4] == 500 - 150 + 1
