"""Flux sessions must not send KeepAlive (the server closes the stream on it)."""
from unittest.mock import MagicMock

from hal.drivers.voice.stt.autonomous import AutonomousSTTSession


def _session(url: str) -> AutonomousSTTSession:
    session = AutonomousSTTSession(url, "key", 16000)
    session._ws = MagicMock()
    return session


def test_flux_session_never_sends_keepalive():
    session = _session("wss://x/ws/audio/transcriptions?model=flux-general-en&encoding=linear16")
    session.send_keepalive()
    session._ws.send.assert_not_called()


def test_nova_session_still_sends_keepalive():
    session = _session("wss://x/ws/audio/transcriptions?model=nova-3-general&encoding=linear16")
    session.send_keepalive()
    session._ws.send.assert_called_once_with('{"type": "KeepAlive"}')
