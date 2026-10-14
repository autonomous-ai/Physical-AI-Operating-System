"""Streaming evidence is usable only for exact PCM from the same model."""
from types import SimpleNamespace
from unittest.mock import Mock
import numpy as np
import pytest
from hal.drivers.voice import voice_service as voice
from hal.drivers.voice._internal.noise_evidence import NoiseEvidence


@pytest.mark.parametrize('mode', ['match', 'changed_pcm', 'changed_model', 'cancelled'])
def test_stream_evidence_reuse_or_original_guard_fallback(monkeypatch, mode):
    monkeypatch.setattr(voice.hal_config, 'REALTIME_NOISE_SPEECH_RATIO', .45)
    monkeypatch.setattr(voice.hal_config, 'VOICE_NOISE_MIN_VOICED_MS', 160)
    model = object()
    vad = SimpleNamespace(available=True, _session=model,
                          speech_metrics=Mock(return_value=(.9, .9, 1., 1., .192)), reset_state=Mock())
    service = SimpleNamespace(_rt_noise_vad=vad, _np=np)
    pcm = np.ones(3072, dtype=np.int16)
    evidence = NoiseEvidence(lambda: lambda block: .9, max_samples=16000, speech_threshold=.15).start()
    try:
        assert evidence.append(pcm.tobytes())
        evidence.finish()
        assert evidence.done.wait(2)
        cache = {'stream': evidence, 'stream_source': {'session': model}}
        if mode == 'changed_pcm':
            pcm = pcm.copy();pcm[0] = 2
        elif mode == 'changed_model':
            cache['stream_source']['session'] = object()
        elif mode == 'cancelled':
            evidence.close()
        assert voice.VoiceService._rt_noise_is_speech(service, pcm, fail_open=False, metrics_cache=cache)
        assert vad.speech_metrics.call_count == (0 if mode == 'match' else 1)
        assert cache['stream'] is evidence
        assert voice.VoiceService._rt_noise_is_speech(service, pcm, fail_open=False, metrics_cache=cache)
        assert vad.speech_metrics.call_count == (0 if mode == 'match' else 1)
    finally:
        evidence.close()


def test_capture_feeds_same_pcm_and_closes_owned_worker(monkeypatch):
    import threading
    from hal.test.test_turn_endpoint_capture import capture
    instances=[]
    class EvidenceSpy:
        def __init__(self, factory, **kwargs):
            self.frames=[];self.events=[];self.done=threading.Event();instances.append(self)
        def start(self):return self
        def append(self, frame):self.frames.append(frame);return True
        def finish(self):self.events.append('finish');self.done.set()
        def close(self):self.events.append('close')
    monkeypatch.setattr(voice, 'NoiseEvidence', EvidenceSpy)
    monkeypatch.setattr(voice.voice_cfg, 'STREAM_NOISE_EVIDENCE_ENABLED', True)
    monkeypatch.setattr(voice.hal_config, 'REALTIME_PROVIDER', 'gemini')
    with capture(monkeypatch, [(1,True,'What is two plus three?'),(4,False,None)], realtime=True) as result:
        assert len(instances)==1
        assert instances[0].frames == [c.args[0] for c in result.stt.send_audio.call_args_list]
        assert instances[0].events == ['finish','close']
