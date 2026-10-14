"""Passive learning of the contact envelope from the recordings the lamp plays anyway."""

from hal.drivers.motors import contact_profile as cp
from hal.drivers.motors.contact_profile import ContactProfile, PassiveLearner

TOTAL = 10


class _Clock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


def _learner(tmp_path, data=None, defaults=None, load_ceiling=1000):
    profile = ContactProfile(str(tmp_path / "p.json"), margin=100, data=data, lag_margin=30)
    clock = _Clock()
    learner = PassiveLearner(profile, defaults, load_ceiling=load_ceiling, clock=clock)
    return learner, profile, clock


def _run(learner, load, rec="idle", frames=range(TOTAL), lag=None):
    """One play of `rec`, every frame at `load` (and `lag`), then the next one starts."""
    for frame in frames:
        samples = {"base_pitch": load}
        if lag is not None:
            samples["lag:base_pitch"] = lag
        learner.observe(rec, frame, TOTAL, samples)
    learner.end()


def test_one_clean_run_changes_nothing(tmp_path):
    learner, profile, _ = _learner(tmp_path)
    _run(learner, 300)
    assert profile.floors("idle", 5) is None
    assert learner.runs() == {"idle": 1}


def test_two_clean_runs_learn_like_a_learn_run(tmp_path):
    """A channel with no fixed floor: the loosest of the two runs, at once."""
    learner, profile, _ = _learner(tmp_path)
    for load in (300, 340):
        _run(learner, load)
    assert profile.floors("idle", 5)["base_pitch"] == 340 + 100


def test_a_first_envelope_starts_from_the_fixed_floor(tmp_path):
    """The lamp stood at the fixed 57 % floor before it learned; it comes down a quarter."""
    learner, profile, _ = _learner(tmp_path, defaults={"base_pitch": 570})
    for _ in range(cp.PASSIVE_MIN_RUNS):
        _run(learner, 270)
    # Start envelope 570 - 100 = 470, 200 over the runs: down 50.
    assert profile.floors("idle", 5)["base_pitch"] == 470 - 50 + 100


def test_a_frame_first_sampled_later_falls_from_its_neighbours(tmp_path):
    """20 Hz sampling skips frames: one first seen later starts where its window stands."""
    learner, profile, _ = _learner(tmp_path, defaults={"base_pitch": 570})
    for _ in range(cp.PASSIVE_MIN_RUNS):
        _run(learner, 270, frames=[f for f in range(TOTAL) if f != 5])
    assert profile.envelope("idle")["base_pitch"][4] == 420
    _run(learner, 270)
    values = profile.envelope("idle")["base_pitch"]
    assert values[5] == values[4] == 420 - 38                 # not back up at 470


def test_a_frame_with_no_learned_neighbour_falls_from_the_fixed_floor(tmp_path):
    learner, profile, _ = _learner(tmp_path, defaults={"base_pitch": 570})
    total = 20
    for _ in range(cp.PASSIVE_MIN_RUNS):
        for frame in range(total):
            if frame < 12:
                learner.observe("idle", frame, total, {"base_pitch": 270})
        learner.observe("idle", total - 1, total, {"base_pitch": 270})
        learner.end()
    for frame in range(total):
        learner.observe("idle", frame, total, {"base_pitch": 270})
    learner.end()
    assert profile.envelope("idle")["base_pitch"][15] == 470 - 50


def test_a_first_envelope_over_the_fixed_floor_takes_the_runs(tmp_path):
    """The original bug: a heavy frame over the fixed floor. It rises at once."""
    learner, profile, _ = _learner(tmp_path, defaults={"base_pitch": 550})
    for _ in range(cp.PASSIVE_MIN_RUNS):
        _run(learner, 600)
    assert profile.floors("idle", 5)["base_pitch"] == 600 + 100


def test_a_later_update_drops_the_single_highest_run(tmp_path):
    """One run a hand brushed does not raise the envelope."""
    learner, profile, _ = _learner(tmp_path, {"idle": {"base_pitch": [300] * TOTAL}})
    for load in (300, 300, 300, 300, 600):
        _run(learner, load)
    assert profile.floors("idle", 5)["base_pitch"] == 300 + 100


def test_a_lower_envelope_comes_down_a_share_of_the_gap_per_update(tmp_path):
    learner, profile, _ = _learner(tmp_path, {"idle": {"base_pitch": [700] * TOTAL}})
    for _ in range(cp.PASSIVE_MIN_RUNS):
        _run(learner, 300)
    assert profile.floors("idle", 5)["base_pitch"] == 700 - 100 + 100   # 25 % of 400
    _run(learner, 300)
    assert profile.floors("idle", 5)["base_pitch"] == 600 - 75 + 100


def test_a_lower_envelope_gets_there_in_the_end(tmp_path):
    learner, profile, _ = _learner(tmp_path, {"idle": {"base_pitch": [700] * TOTAL}})
    for _ in range(40):
        _run(learner, 300)
    assert profile.floors("idle", 5)["base_pitch"] == 300 + 100


def test_a_higher_envelope_rises_at_once(tmp_path):
    learner, profile, _ = _learner(tmp_path, {"idle": {"base_pitch": [300] * TOTAL, "lag:base_pitch": [20] * TOTAL}})
    for _ in range(cp.PASSIVE_MIN_RUNS):
        _run(learner, 500, lag=80)
    floors = profile.floors("idle", 5)
    assert floors["base_pitch"] == 500 + 100
    assert floors["lag:base_pitch"] == 80 + 30


def test_rising_stops_at_the_ceiling(tmp_path):
    learner, profile, _ = _learner(tmp_path, {"idle": {"base_pitch": [300] * TOTAL}}, load_ceiling=800)
    for _ in range(cp.PASSIVE_MIN_RUNS):
        _run(learner, 950, lag=900)
    floors = profile.floors("idle", 5)
    assert floors["base_pitch"] == 800 + 100
    assert floors["lag:base_pitch"] == cp.PASSIVE_CEILING_LAG + 30


# --- A stop nobody held through ---------------------------------------------------


def test_a_false_stop_raises_the_envelope_around_its_frame(tmp_path):
    learner, profile, _ = _learner(tmp_path, {"idle": {"base_pitch": [300] * 20}})
    assert learner.loosen("idle", 10, "base_pitch", 450)   # floor 400 stopped it at 450
    values = profile.envelope("idle")["base_pitch"]
    assert values[7:14] == [450 - 100 + 1] * 7                 # floor now just over 450
    assert values[6] == values[14] == 300


def test_a_false_stop_raises_no_further_than_its_sample_needs(tmp_path):
    learner, profile, _ = _learner(tmp_path, {"idle": {"base_pitch": [300] * 20}})
    learner.loosen("idle", 10, "base_pitch", 410)
    assert profile.floors("idle", 10)["base_pitch"] == 411
    assert not learner.loosen("idle", 10, "base_pitch", 410)


def test_false_stops_loosen_a_few_times_a_day(tmp_path):
    learner, profile, clock = _learner(tmp_path, {"idle": {"base_pitch": [300] * 20}})
    for i in range(cp.PASSIVE_LOOSEN_PER_DAY):
        assert learner.loosen("idle", 10, "base_pitch", 900 + i)
    assert not learner.loosen("idle", 10, "base_pitch", 2000)
    clock.now += 86400.0
    assert learner.loosen("idle", 10, "base_pitch", 2000)


def test_a_false_stop_holds_the_envelope_up_for_a_day(tmp_path):
    learner, profile, clock = _learner(tmp_path, {"idle": {"base_pitch": [300] * TOTAL}})
    learner.loosen("idle", 5, "base_pitch", 500)
    raised = profile.floors("idle", 5)["base_pitch"]
    for _ in range(cp.PASSIVE_RUNS):
        _run(learner, 200)
    assert profile.floors("idle", 5)["base_pitch"] == raised
    clock.now += cp.PASSIVE_HOLD_S
    _run(learner, 200)
    assert profile.floors("idle", 5)["base_pitch"] < raised


def test_a_capped_joint_stopped_at_its_cap_rises_to_the_cap_throughout(tmp_path):
    """lamp-c0a4: the elbow hits its 60 % cap at varying idle frames; its load says nothing."""
    learner, profile, _ = _learner(tmp_path, {"idle": {"elbow_pitch": [300] * 20}})
    learner._caps = {"elbow_pitch": 600}
    assert learner.loosen("idle", 2, "elbow_pitch", 600)
    assert profile.envelope("idle")["elbow_pitch"] == [600] * 20
    assert profile.floors("idle", 15)["elbow_pitch"] == 700   # past what the cap lets it read


def test_a_capped_joint_stopped_under_its_cap_rises_around_the_frame_only(tmp_path):
    learner, profile, _ = _learner(tmp_path, {"idle": {"elbow_pitch": [300] * 20}})
    learner._caps = {"elbow_pitch": 600}
    learner.loosen("idle", 2, "elbow_pitch", 450)
    assert profile.envelope("idle")["elbow_pitch"][15] == 300


def test_a_recording_with_no_envelope_is_not_loosened(tmp_path):
    learner, _, _ = _learner(tmp_path)
    assert not learner.loosen("idle", 5, "base_pitch", 500)


def test_a_run_cut_short_does_not_count(tmp_path):
    learner, _, _ = _learner(tmp_path)
    _run(learner, 300, frames=range(5))
    assert learner.runs() == {}


def test_a_run_joined_midway_does_not_count(tmp_path):
    learner, _, _ = _learner(tmp_path)
    _run(learner, 300, frames=range(6, TOTAL))
    assert learner.runs() == {}


def test_a_discarded_run_does_not_count_but_the_next_does(tmp_path):
    learner, _, _ = _learner(tmp_path)
    for frame in range(TOTAL):
        learner.observe("idle", frame, TOTAL, {"base_pitch": 300})
        if frame == 4:
            learner.discard()
    learner.end()
    assert learner.runs() == {}
    _run(learner, 300)
    assert learner.runs() == {"idle": 1}


def test_a_looping_recording_counts_each_pass(tmp_path):
    """Idle and music loop: the frame index wrapping ends one run and starts the next."""
    learner, _, _ = _learner(tmp_path)
    for _ in range(3):
        for frame in range(TOTAL):
            learner.observe("idle", frame, TOTAL, {"base_pitch": 300})
    learner.end()
    assert learner.runs() == {"idle": 3}


def test_frames_sampled_sparsely_still_complete_a_run(tmp_path):
    """The monitor runs at 20 Hz against 30 fps playback: frames get skipped."""
    learner, _, _ = _learner(tmp_path)
    _run(learner, 300, frames=range(0, TOTAL - 2, 3))
    assert learner.runs() == {"idle": 1}


def test_a_learn_run_forgets_the_passive_runs(tmp_path):
    learner, _, _ = _learner(tmp_path)
    _run(learner, 300)
    learner.forget("idle")
    assert learner.runs() == {}


def test_a_changed_recording_length_starts_over(tmp_path):
    learner, _, _ = _learner(tmp_path)
    _run(learner, 300)
    _run(learner, 300)
    for frame in range(TOTAL + 5):
        learner.observe("idle", frame, TOTAL + 5, {"base_pitch": 300})
    learner.end()
    assert learner.runs() == {"idle": 1}


def test_an_update_is_saved_at_most_every_save_interval(tmp_path):
    learner, profile, clock = _learner(tmp_path, {"idle": {"base_pitch": [300] * TOTAL}})
    for _ in range(cp.PASSIVE_RUNS):
        _run(learner, 200)
    learner.observe("idle", 0, TOTAL, {"base_pitch": 300})
    assert not (tmp_path / "p.json").exists()
    clock.now += cp.PASSIVE_SAVE_S
    learner.observe("idle", 1, TOTAL, {"base_pitch": 300})
    saved = ContactProfile.load(profile.path, 100).floors("idle", 5)["base_pitch"]
    assert saved == profile.floors("idle", 5)["base_pitch"] < 300 + 100


# --- Signature: what the envelope was learned under ------------------------------


def test_an_envelope_saved_under_other_torque_caps_is_dropped(tmp_path):
    path = str(tmp_path / "p.json")
    ContactProfile(path, 100, {"idle": {"base_pitch": [300]}}, signature="caps=600").save()
    assert ContactProfile.load(path, 100, signature="caps=600").floors("idle", 0) == {"base_pitch": 400}
    assert ContactProfile.load(path, 100, signature="caps=700").floors("idle", 0) is None


def test_an_envelope_saved_before_signatures_is_kept(tmp_path):
    path = str(tmp_path / "p.json")
    ContactProfile(path, 100, {"idle": {"base_pitch": [300]}}).save()
    assert ContactProfile.load(path, 100, signature="caps=700").floors("idle", 0) == {"base_pitch": 400}
