"""Intentional backchannel masking must not discard an ongoing user turn."""

import threading

import numpy as np
import pytest

from hal.drivers.voice import voice_service as voice
from hal.drivers.voice._internal.continuous_capture import ContinuousCapture
from hal.test.test_continuous_capture import BlockingMic
from hal.test import test_automatic_stt_upload as upload_fixture


@pytest.fixture
def setup(monkeypatch):
    yield from upload_fixture.setup.__wrapped__(monkeypatch)


def filled_pump(*, max_frames=8, arrays=False):
    mic = BlockingMic()
    cue = threading.Event()
    pump = ContinuousCapture(mic, 320, max_frames=max_frames, max_age_s=10,
                             suppressed=lambda: False, silenced=cue.is_set, abort=mic.abort).start()
    def frame(value):
        return np.full((320, 1), value, dtype=np.int16) if arrays else value
    mic.wait_read(1)
    mic.input.put(frame(10000))
    mic.wait_read(2)
    cue.set()
    mic.input.put(frame(20000))
    mic.wait_read(3)
    cue.clear()
    mic.input.put(frame(30000))  # The block crossing the end of the cue is masked too.
    mic.wait_read(4)
    mic.input.put(frame(12000))
    mic.wait_read(5)
    return pump, mic


def test_soft_cue_preserves_queued_prefix_and_timeline():
    pump, _ = filled_pump()
    try:
        assert [pump.read(320)[0] for _ in range(4)] == [10000, 0, 0, 12000]
        assert not pump.consume_discontinuity()
    finally:
        pump.stop()


def test_soft_cue_does_not_hide_real_backlog_overflow():
    pump, _ = filled_pump(max_frames=2)
    try:
        assert [pump.read(320)[0] for _ in range(2)] == [0, 12000]
        assert pump.consume_discontinuity()
    finally:
        pump.stop()


def test_cue_inside_running_capture_delivers_prefix_zero_and_suffix_without_gap(monkeypatch, setup):
    service, stt, reply, _, _, _ = setup
    monkeypatch.setattr(voice.hal_config, 'REALTIME_PROVIDER', 'openai')
    stt.close.side_effect = lambda: stt._on_transcript_cb('Please finish my request', True)
    pump, mic = filled_pump(arrays=True)
    mic.input.put(np.zeros((320, 1), dtype=np.int16))
    mic.wait_read(6)
    monkeypatch.setattr(voice, 'turn_should_close', lambda *args: stt.send_audio.call_count >= 5)
    try:
        voice.VoiceService._stream_session(
            service, pump, 320, 16000, preconnected_session=stt,
            harness_voice={'enabled': False, 'generation': 1},
        )
        samples = [np.frombuffer(call.args[0], dtype=np.int16) for call in stt.send_audio.call_args_list]
        assert [int(frame[0]) for frame in samples] == [10000, 0, 0, 12000, 0]
        service._realtime.discard_open_activity.assert_not_called()
        reply.assert_called_once()
    finally:
        pump.stop()
