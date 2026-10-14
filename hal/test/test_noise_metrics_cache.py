"""A turn can reuse measured evidence, never another PCM take or failed inference."""

from unittest.mock import Mock

import numpy as np
import pytest

from hal.drivers.voice import voice_service as voice
from hal.drivers.voice._internal.realtime_turn import ROUTE_CANCELLED
from hal.test.test_automatic_stt_upload import setup as upload_setup


POSITIVE = (1.0, .4, .444, .48, 1.6)
NEGATIVE = (1.0, .273, .307, .407, 2.75)
SENTINEL = (1.0, 1.0, 1.0, 1.0, 0.0)


@pytest.fixture
def service(monkeypatch):
    monkeypatch.setattr(voice.hal_config, 'REALTIME_NOISE_SPEECH_RATIO', .45)
    monkeypatch.setattr(voice.hal_config, 'VOICE_NOISE_MIN_VOICED_MS', 160)
    service = object.__new__(voice.VoiceService)
    service._np = np
    service._rt_noise_vad = Mock(available=True)
    return service


@pytest.mark.parametrize('metrics,expected', [(POSITIVE, True), (NEGATIVE, False), ((0, 0, 0, 0, 0), False)])
def test_identical_pcm_reuses_one_inference_and_one_state_reset(service, metrics, expected):
    pcm = np.arange(1024, dtype=np.int16)
    cache = {}
    service._rt_noise_vad.speech_metrics.return_value = metrics
    assert service._rt_noise_is_speech(pcm, fail_open=False, metrics_cache=cache) is expected
    assert service._rt_noise_is_speech(pcm.copy(), metrics_cache=cache) is expected
    assert service._rt_noise_is_speech(pcm.copy(), fail_open=False, metrics_cache=cache) is expected
    service._rt_noise_vad.speech_metrics.assert_called_once()
    service._rt_noise_vad.reset_state.assert_called_once()


def test_trim_and_same_length_changed_pcm_each_recompute_and_replace_single_entry(service):
    pcm = np.arange(1024, dtype=np.int16)
    cache = {}
    service._rt_noise_vad.speech_metrics.side_effect = [NEGATIVE, POSITIVE, NEGATIVE, NEGATIVE]
    assert not service._rt_noise_is_speech(pcm, metrics_cache=cache)
    assert service._rt_noise_is_speech(pcm[:512], metrics_cache=cache)
    assert not service._rt_noise_is_speech(pcm[:512] + 1, metrics_cache=cache)
    assert not service._rt_noise_is_speech(pcm, metrics_cache=cache)
    assert service._rt_noise_vad.speech_metrics.call_count == 4
    assert service._rt_noise_vad.reset_state.call_count == 4
    assert set(cache) == {'key', 'metrics', 'detector'}
    assert cache['key'][1] == pcm.tobytes()


def test_cached_metrics_reapply_policy_instead_of_reusing_boolean(monkeypatch, service):
    cache = {}
    pcm = np.arange(1024, dtype=np.int16)
    service._rt_noise_vad.speech_metrics.return_value = POSITIVE
    assert service._rt_noise_is_speech(pcm, fail_open=False, metrics_cache=cache)
    monkeypatch.setattr(voice.hal_config, 'REALTIME_NOISE_SPEECH_RATIO', .6)
    assert not service._rt_noise_is_speech(pcm, metrics_cache=cache)
    service._rt_noise_vad.speech_metrics.assert_called_once()


@pytest.mark.parametrize('failure', [SENTINEL, RuntimeError('inference failed')])
def test_failure_evidence_is_never_cached_and_fail_open_policy_stays_distinct(service, failure):
    cache = {}
    pcm = np.arange(1024, dtype=np.int16)
    if isinstance(failure, Exception):
        service._rt_noise_vad.speech_metrics.side_effect = failure
    else:
        service._rt_noise_vad.speech_metrics.return_value = failure
    assert not service._rt_noise_is_speech(pcm, fail_open=False, metrics_cache=cache)
    assert cache == {}
    assert service._rt_noise_is_speech(pcm, fail_open=True, metrics_cache=cache)
    assert cache == {}
    assert service._rt_noise_vad.speech_metrics.call_count == 2


def test_unavailable_detector_cannot_reuse_prior_positive_evidence(service):
    cache = {}
    pcm = np.arange(1024, dtype=np.int16)
    service._rt_noise_vad.speech_metrics.return_value = POSITIVE
    assert service._rt_noise_is_speech(pcm, fail_open=False, metrics_cache=cache)
    service._rt_noise_vad.available = False
    service._rt_noise_vad.speech_metrics.return_value = SENTINEL
    assert not service._rt_noise_is_speech(pcm, fail_open=False, metrics_cache=cache)
    assert service._rt_noise_is_speech(pcm, fail_open=True, metrics_cache=cache)
    assert service._rt_noise_vad.speech_metrics.call_count == 2


@pytest.mark.parametrize('metrics', [(float('nan'), 0, 0, 0, 0), (2, .5, .5, .5, 1), (1, .5, .5, .5, -1)])
def test_invalid_metrics_are_not_retained(service, metrics):
    cache = {}
    service._rt_noise_vad.speech_metrics.return_value = metrics
    service._rt_noise_is_speech(np.zeros(512, dtype=np.int16), metrics_cache=cache)
    assert cache == {}


def test_detector_replacement_invalidates_pcm_cache(service):
    cache = {}
    pcm = np.arange(1024, dtype=np.int16)
    service._rt_noise_vad.speech_metrics.return_value = POSITIVE
    assert service._rt_noise_is_speech(pcm, metrics_cache=cache)
    replacement = Mock(available=True)
    replacement.speech_metrics.return_value = NEGATIVE
    service._rt_noise_vad = replacement
    assert not service._rt_noise_is_speech(pcm, metrics_cache=cache)
    replacement.speech_metrics.assert_called_once()


@pytest.fixture
def capture_setup(monkeypatch):
    yield from upload_setup.__wrapped__(monkeypatch)


def test_real_capture_owns_new_cache_after_each_turn_including_cancel(monkeypatch, capture_setup):
    service, stt, reply, _, run, _ = capture_setup
    monkeypatch.setattr(voice.hal_config, 'WAKEWORD_ENABLED', False)
    caches = []
    def evidence(pcm, *, fail_open=True, metrics_cache=None):
        assert metrics_cache is not None
        if not any(existing is metrics_cache for existing in caches):
            caches.append(metrics_cache)
        metrics_cache['test-owner'] = True
        return True
    service._rt_noise_is_speech.side_effect = evidence
    reply.side_effect = lambda *a, **kw: voice.RealtimeTurnResult(handled=True, transcript='Five.', user_transcript='Two plus three?')
    run()
    first = caches[-1]
    worker = service._automatic_stt_upload
    assert worker.done.wait(2)
    # End the next capture through the same cancellation path as a stopped reply.
    def cancelled(*args, **kwargs):
        kwargs['stop_event'].set()
        return voice.RealtimeTurnResult(route=ROUTE_CANCELLED)
    reply.side_effect = cancelled
    run()
    assert len(caches) == 2 and caches[-1] is not first
    assert service._automatic_stt_upload.done.wait(2)
    reply.side_effect = lambda *a, **kw: voice.RealtimeTurnResult(handled=True, transcript='Five.', user_transcript='Two plus three?')
    run()
    assert len(caches) == 3 and len({id(cache) for cache in caches}) == 3
    assert '_noise_metrics_cache' not in vars(service)
