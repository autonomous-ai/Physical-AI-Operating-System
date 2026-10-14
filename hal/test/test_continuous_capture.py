"""Capture keeps progressing while response work stops consuming input."""

import queue
import threading

import pytest

from hal.drivers.voice._internal.continuous_capture import ContinuousCapture


class BlockingMic:
    def __init__(self):
        self.input = queue.Queue()
        self.condition = threading.Condition()
        self.reads = 0
        self.aborted = False

    def read(self, frames):
        assert frames == 320
        with self.condition:
            self.reads += 1
            self.condition.notify_all()
        value = self.input.get(timeout=3)
        if isinstance(value, Exception):
            raise value
        if isinstance(value, tuple):
            return value
        return value, False

    def wait_read(self, count):
        with self.condition:
            assert self.condition.wait_for(lambda: self.reads >= count, timeout=2)

    def abort(self):
        self.aborted = True
        self.input.put(InterruptedError("aborted"))


@pytest.fixture
def capture():
    mic = BlockingMic()
    muted = threading.Event()
    now = [10.0]
    pump = ContinuousCapture(
        mic, 320, max_frames=3, max_age_s=1,
        suppressed=muted.is_set, abort=mic.abort, clock=lambda: now[0],
    ).start()
    mic.wait_read(1)
    yield pump, mic, muted, now
    pump.stop()
    assert not pump._thread.is_alive()


def test_producer_continues_without_consumer_and_retains_bounded_latest_prefix(capture):
    pump, mic, _, _ = capture
    for n in range(5):
        mic.input.put(n)
    mic.wait_read(6)
    assert len(pump._frames) == 3
    assert [pump.read(320)[0] for _ in range(3)] == [2, 3, 4]
    assert pump.consume_discontinuity()
    assert not pump.consume_discontinuity()


def test_playback_crossing_clears_old_backlog_and_retains_clean_prefix_once(capture):
    pump, mic, muted, _ = capture
    mic.input.put("old")
    mic.wait_read(2)
    muted.set()
    mic.input.put("playback-start-crossing")
    mic.wait_read(3)
    muted.clear()
    mic.input.put("playback-end-crossing")
    mic.wait_read(4)
    mic.input.put("hello")
    mic.input.put("lamp")
    mic.wait_read(6)
    assert pump.read(320) == ("hello", False)
    assert pump.consume_discontinuity()
    assert pump.read(320) == ("lamp", False)
    assert not pump.consume_discontinuity()
    assert not pump._frames


def test_old_queue_and_read_started_before_age_cutoff_are_not_replayed(capture):
    pump, mic, _, now = capture
    mic.input.put("old")
    mic.wait_read(2)
    now[0] = 12.0
    mic.input.put("read-started-before-pause")
    mic.wait_read(3)
    mic.input.put("fresh")
    mic.wait_read(4)
    assert pump.read(320) == ("fresh", False)
    assert pump.consume_discontinuity()


def test_suppressed_input_returns_source_paced_silence_for_control_polling(capture):
    pump, mic, muted, _ = capture
    muted.set()
    mic.input.put(27)
    mic.wait_read(2)
    assert pump.read(320) == (0, False)
    assert pump.consume_discontinuity()
    mic.input.put(98)
    mic.wait_read(3)
    assert pump.read(320) == (0, False)


def test_backend_overflow_clears_contaminated_prefix_without_losing_next_frame(capture):
    pump, mic, _, _ = capture
    mic.input.put(1)
    mic.input.put((2, True))
    mic.input.put(3)
    mic.wait_read(4)
    assert pump.read(320) == (3, False)
    assert pump.consume_discontinuity()
    assert not pump._frames


def test_future_queue_loss_is_not_consumed_as_an_earlier_frames_discontinuity(capture):
    pump, mic, _, _ = capture
    mic.input.put(0)
    mic.wait_read(2)
    assert pump.read(320) == (0, False)
    for n in range(1, 5):
        mic.input.put(n)
    mic.wait_read(6)
    # Producer dropped frame 1 after frame 0 was delivered. The gap belongs to
    # frame 2, not to the pending consume call for the already-delivered frame 0.
    assert not pump.consume_discontinuity()
    assert pump.read(320) == (2, False)
    assert pump.consume_discontinuity()
    assert pump.read(320) == (3, False)
    assert not pump.consume_discontinuity()


def test_playback_start_between_read_and_consume_does_not_erase_its_later_gap(capture):
    pump, mic, muted, _ = capture
    mic.input.put(10)
    mic.wait_read(2)
    assert pump.read(320) == (10, False)
    muted.set()
    mic.input.put(20)
    mic.wait_read(3)
    assert not pump.consume_discontinuity()
    assert pump.read(320) == (0, False)
    assert pump.consume_discontinuity()


def test_stop_unblocks_producer_and_waiting_consumer(capture):
    pump, mic, _, _ = capture
    outcome = []
    waiting = threading.Event()

    def consume():
        waiting.set()
        try:
            pump.read(320)
        except InterruptedError:
            outcome.append("stopped")

    consumer = threading.Thread(target=consume)
    consumer.start()
    assert waiting.wait(2)
    pump.stop()
    consumer.join(2)
    assert not consumer.is_alive()
    assert mic.aborted
    assert outcome == ["stopped"]


def test_capture_failure_reaches_consumer_instead_of_hanging(capture):
    pump, mic, _, _ = capture
    mic.input.put(IOError("USB disappeared"))
    with pytest.raises(IOError, match="continuous microphone") as raised:
        pump.read(320)
    assert str(raised.value.__cause__) == "USB disappeared"


def test_fixed_frame_contract(capture):
    pump, _, _, _ = capture
    with pytest.raises(ValueError, match="configured frame size"):
        pump.read(160)
    with pytest.raises(RuntimeError, match="restarted"):
        pump.start()


def test_stuck_reader_stop_reports_that_backend_cannot_be_closed():
    mic = BlockingMic()
    pump = ContinuousCapture(
        mic, 320, max_frames=3, max_age_s=1,
        suppressed=lambda: False, abort=lambda: None,
    ).start()
    mic.wait_read(1)
    try:
        with pytest.raises(RuntimeError, match="backend must remain open"):
            pump.stop(timeout=0.01)
    finally:
        mic.abort()
        pump.stop()
    assert not pump._thread.is_alive()
