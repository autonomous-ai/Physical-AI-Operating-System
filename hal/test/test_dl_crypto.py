"""DL backend clients never fall back to plaintext while encryption is on."""

import json

import numpy as np
import pytest

from hal import config
from hal.drivers.sensing import crypto as crypto_module
from hal.drivers.sensing.crypto import DLCrypto, EncryptionUnavailable


@pytest.fixture
def no_key(monkeypatch):
    calls = []

    def resolve(*args):
        calls.append(args)
        return None

    monkeypatch.setattr(crypto_module, "resolve_public_key", resolve)
    monkeypatch.setattr(config, "DL_ENCRYPTION_ENABLED", True)
    return calls


def test_missing_key_refuses_and_backs_off(no_key, monkeypatch):
    clock = [1000.0]
    monkeypatch.setattr(crypto_module.time, "monotonic", lambda: clock[0])
    c = DLCrypto("test")

    with pytest.raises(EncryptionUnavailable):
        c.http_session()
    with pytest.raises(EncryptionUnavailable):
        c.http_session()  # within the backoff: no second fetch
    assert len(no_key) == 1

    clock[0] += crypto_module._KEY_RETRY_S + 1
    with pytest.raises(EncryptionUnavailable):
        c.http_session()
    assert len(no_key) == 2


def test_plaintext_only_when_operator_turns_encryption_off(no_key, monkeypatch):
    monkeypatch.setattr(config, "DL_ENCRYPTION_ENABLED", False)
    assert DLCrypto("test").http_session() is None
    assert no_key == []


def test_ws_key_exchange_failure_refuses(monkeypatch):
    from cryptography.hazmat.primitives.asymmetric import rsa

    key = rsa.generate_private_key(public_exponent=65537, key_size=2048).public_key()
    monkeypatch.setattr(crypto_module, "resolve_public_key", lambda *a: key)
    monkeypatch.setattr(config, "DL_ENCRYPTION_ENABLED", True)

    class WS:
        def __init__(self, reply):
            self.reply = reply

        def send(self, _):
            pass

        def recv(self, timeout):
            return json.dumps(self.reply)

    c = DLCrypto("test")
    with pytest.raises(EncryptionUnavailable):
        c.ws_key_exchange(WS({"status": "error"}), 1.0)
    assert c.ws_key_exchange(WS({"status": "key_exchange_ok"}), 1.0) is not None


def test_emotion_client_sends_nothing_without_a_key(no_key, monkeypatch):
    from hal.drivers.sensing.perceptions.processors import emotion

    posts = []
    monkeypatch.setattr(emotion.requests, "post", lambda *a, **k: posts.append(a) or None)
    rec = emotion.RemoteEmotionRecognizer(base_url="https://dl.example", api_key="k")
    assert rec.recognize(np.zeros((64, 64, 3), dtype=np.uint8)) is None
    assert posts == [], "a face crop was sent without encryption"


def test_fire_client_sends_nothing_without_a_key(no_key, monkeypatch):
    from hal.drivers.sensing.perceptions.processors import fire_hazard

    posts = []
    monkeypatch.setattr(fire_hazard.requests, "post", lambda *a, **k: posts.append(a) or None)
    det = fire_hazard.RemoteFireHazardDetector(base_url="https://dl.example", api_key="k")
    assert det.detect(np.zeros((64, 64, 3), dtype=np.uint8)) is None
    assert posts == [], "a camera frame was sent without encryption"
