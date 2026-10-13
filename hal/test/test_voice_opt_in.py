"""Speaker ID and speech emotion upload voice audio only after the owner opts in."""

from unittest import mock

import pytest

from hal import config
from hal.drivers.voice._internal.speaker_decorate import SpeakerDecorator
from hal.drivers.voice.speaker_recognizer.speaker_recognizer import (
    SpeakerRecognizer,
    SpeakerRecognizerError,
)
from hal.drivers.voice.speech_emotion.emotion2vec import Emotion2VecRecognizer
from hal.models import CloudPerceptionRequest
from hal.routes.sensing import set_cloud_perception


@pytest.fixture
def opted_out(monkeypatch):
    monkeypatch.setattr(config, "SPEAKER_RECOGNITION_ENABLED", False)
    monkeypatch.setattr(config, "SPEECH_EMOTION_ENABLED", False)


def test_flags_default_off_without_saved_consent(monkeypatch):
    monkeypatch.setattr(config, "_os_cfg_get", lambda key, default=None: default)
    assert config._cloud_perception_enabled("speaker_id") is False
    monkeypatch.setattr(config, "_os_cfg_get", lambda key, default=None: {"speaker_id": True})
    assert config._cloud_perception_enabled("speaker_id") is True
    assert config._cloud_perception_enabled("speech_emotion") is False


def test_opted_out_decorator_starts_nothing_and_uploads_nothing(opted_out, monkeypatch):
    speaker, emotion = mock.Mock(), mock.Mock()
    monkeypatch.setattr(SpeakerDecorator, "_init_speaker", staticmethod(speaker))
    monkeypatch.setattr(SpeakerDecorator, "_init_speech_emotion", staticmethod(emotion))
    d = SpeakerDecorator(["lamp"], 0)
    d._speaker = mock.Mock()  # even a live service left from before must not be used
    d._speech_emotion = mock.Mock(available=True)
    try:
        assert d.recognize_speaker([b"\x00\x00" * 1600]) is None
        d.submit_speech_emotion_from_session([b"\x00\x00" * 1600])
        d._speaker.recognize.assert_not_called()
        d._speech_emotion.submit.assert_not_called()
        speaker.assert_not_called()
        emotion.assert_not_called()
    finally:
        d.close()


def test_opting_in_live_starts_init_on_next_use(opted_out, monkeypatch):
    speaker = mock.Mock(return_value=None)
    monkeypatch.setattr(SpeakerDecorator, "_init_speaker", staticmethod(speaker))
    monkeypatch.setattr(SpeakerDecorator, "_init_speech_emotion", staticmethod(mock.Mock(return_value=None)))
    d = SpeakerDecorator(["lamp"], 0)
    try:
        monkeypatch.setattr(config, "SPEAKER_RECOGNITION_ENABLED", True)
        d.recognize_speaker([b"\x00\x00" * 1600])
        for t in d._init_threads:
            t.join(timeout=2)
        speaker.assert_called_once()
    finally:
        d.close()


def test_every_speaker_upload_is_refused_when_opted_out(opted_out):
    recognizer = SpeakerRecognizer.__new__(SpeakerRecognizer)
    recognizer._api_url = "https://example.invalid/embed"
    with pytest.raises(SpeakerRecognizerError, match="Privacy"):
        recognizer._call_embedding_api(["AAAA"])


def test_speech_emotion_engine_does_not_post_when_opted_out(opted_out, monkeypatch):
    engine = Emotion2VecRecognizer.__new__(Emotion2VecRecognizer)
    engine._url, engine._timeout, engine._api_key = "https://example.invalid/ser", 1.0, ""
    engine._crypto, engine._silero = None, None
    post = mock.Mock()
    monkeypatch.setattr("hal.drivers.voice.speech_emotion.emotion2vec.requests.post", post)
    assert engine.recognize(b"RIFF") is None
    post.assert_not_called()


def test_route_applies_voice_opt_ins_live(opted_out, monkeypatch):
    monkeypatch.setattr("hal.routes.sensing.state.sensing_service", None)
    set_cloud_perception(CloudPerceptionRequest(speaker_id=True, speech_emotion=True))
    assert config.SPEAKER_RECOGNITION_ENABLED and config.SPEECH_EMOTION_ENABLED
    set_cloud_perception(CloudPerceptionRequest())
    assert not config.SPEAKER_RECOGNITION_ENABLED and not config.SPEECH_EMOTION_ENABLED
