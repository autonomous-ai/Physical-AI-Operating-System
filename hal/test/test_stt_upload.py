"""STT network stalls cannot block capture or make partial fallback look complete."""

import threading
from unittest.mock import Mock

import pytest

from hal.drivers.voice._internal.stt_upload import STTUpload


class Session:
    def __init__(self):
        self.connected = False
        self.closed = False
        self.calls = []
        self.audio = []
        self.start_entered = threading.Event()
        self.connect_release = threading.Event()
        self.send_entered = threading.Event()
        self.send_release = threading.Event()
        self.send_release.set()
        self.close_entered = threading.Event()
        self.close_release = threading.Event()
        self.close_release.set()
        self.callback = None
        self.start_result = True
        self.fail_send = False
        self.fail_close = False

    def start(self, callback):
        self.calls.append(('start', threading.get_ident()))
        self.callback = callback
        self.start_entered.set()
        assert self.connect_release.wait(2)
        self.connected = self.start_result
        return self.start_result

    def is_closed(self):
        return self.closed

    def send_audio(self, pcm):
        assert self.connected
        self.calls.append(('send', threading.get_ident()))
        self.send_entered.set()
        assert self.send_release.wait(2)
        if self.fail_send:
            raise IOError('network send failed')
        self.audio.append(pcm)

    def close(self):
        self.calls.append(('close', threading.get_ident()))
        self.close_entered.set()
        assert self.close_release.wait(2)
        if self.callback:
            self.callback('Final request', True)
        self.closed = True
        if self.fail_close:
            raise IOError('network close failed')


@pytest.fixture
def setup():
    sessions = []
    workers = []

    def create(max_bytes=16, **kwargs):
        session = Session()
        callback = Mock()
        worker = STTUpload(session, callback, max_bytes=max_bytes, **kwargs)
        sessions.append(session)
        workers.append(worker)
        return session, callback, worker

    yield create
    for session in sessions:
        session.connect_release.set()
        session.send_release.set()
        session.close_release.set()
    for worker in workers:
        worker.cancel()
        if worker._thread is not None:
            assert worker.done.wait(3)
            worker._thread.join(1)
            assert not worker._thread.is_alive()


def test_slow_connect_keeps_capture_nonblocking_and_uploads_all_pcm_in_order(setup):
    session, callback, worker = setup()
    worker.start()
    assert session.start_entered.wait(1)
    assert worker.append(b'prefix')
    assert worker.append(b'tail')
    worker.finish()
    assert session.audio == []
    assert not worker.done.is_set()
    session.connect_release.set()
    assert worker.done.wait(1)
    assert session.audio == [b'prefix', b'tail']
    assert worker.outcome.complete
    assert worker.outcome.sent_bytes == worker.outcome.accepted_bytes == 10
    callback.assert_called_once_with('Final request', True)
    assert [name for name, _ in session.calls] == ['start', 'send', 'send', 'close']
    assert len({thread for _, thread in session.calls}) == 1
    assert session.calls[0][1] != threading.get_ident()


def test_cancel_during_connect_closes_late_socket_and_retires_callbacks(setup):
    session, callback, worker = setup()
    worker.start()
    assert session.start_entered.wait(1)
    assert worker.append(b'audio')
    worker.cancel()
    worker.cancel()
    assert not worker.done.is_set()
    session.callback('Late interim', False)
    session.connect_release.set()
    assert worker.done.wait(1)
    assert worker.outcome.status == 'cancelled'
    assert session.closed and not session.audio
    callback.assert_not_called()
    assert [name for name, _ in session.calls] == ['start', 'close']


def test_overflow_never_drops_prefix_and_claims_remaining_tail_is_complete(setup):
    session, callback, worker = setup(max_bytes=4)
    worker.start()
    assert session.start_entered.wait(1)
    assert worker.append(b'1234')
    assert not worker.append(b'5')
    assert not worker.append(b'6')
    worker.finish()
    session.connect_release.set()
    assert worker.done.wait(1)
    assert worker.outcome.status == 'overflow'
    assert not worker.outcome.complete
    assert session.audio == []
    callback.assert_not_called()


def test_inflight_send_counts_toward_capacity_and_slow_send_does_not_block_append(setup):
    session, _, worker = setup(max_bytes=4)
    session.connect_release.set()
    session.send_release.clear()
    worker.start()
    assert worker.append(b'123')
    assert session.send_entered.wait(1)
    assert worker.append(b'4')
    assert not worker.append(b'5')
    session.send_release.set()
    assert worker.done.wait(1)
    assert session.audio == [b'123']
    assert worker.outcome.status == 'overflow'
    assert not worker.outcome.complete


@pytest.mark.parametrize('failure,status', [('start', 'start_failed'), ('send', 'send_failed'), ('close', 'close_failed')])
def test_network_failure_never_claims_full_fallback(setup, failure, status):
    session, _, worker = setup()
    session.start_result = failure != 'start'
    session.fail_send = failure == 'send'
    session.fail_close = failure == 'close'
    session.connect_release.set()
    assert worker.append(b'audio')
    worker.finish()
    worker.start()
    assert worker.done.wait(1)
    assert worker.outcome.status == status
    assert not worker.outcome.complete
    assert [name for name, _ in session.calls].count('close') == 1


def test_done_waits_for_close_even_after_capture_finished(setup):
    session, callback, worker = setup()
    session.connect_release.set()
    session.close_release.clear()
    worker.start()
    assert worker.append(b'audio')
    worker.finish()
    assert session.close_entered.wait(1)
    assert not worker.done.is_set()
    assert worker.outcome is None
    assert not worker.append(b'late')
    session.close_release.set()
    assert worker.done.wait(1)
    assert worker.outcome.complete
    callback.reset_mock()
    session.callback('After close', True)
    callback.assert_not_called()


def test_preconnected_session_uses_explicit_binder_without_restarting(setup):
    def bind(session, callback):
        session.callback = callback

    session, callback, worker = setup(bind_preconnected=bind)
    session.connected = True
    assert worker.append(b'pre-roll')
    worker.finish()
    worker.start()
    assert worker.done.wait(1)
    assert worker.outcome.complete
    assert [name for name, _ in session.calls] == ['send', 'close']
    callback.assert_called_once_with('Final request', True)


def test_cancel_before_start_still_closes_preexisting_resource_once(setup):
    session, callback, worker = setup()
    worker.cancel()
    worker.start()
    assert worker.done.wait(1)
    assert [name for name, _ in session.calls] == ['close']
    assert worker.outcome.status == 'cancelled'
    callback.assert_not_called()
    with pytest.raises(RuntimeError):
        worker.start()


def test_preconnected_closed_session_does_not_claim_upload_succeeded(setup):
    session, callback, worker = setup(bind_preconnected=lambda session, cb: setattr(session, 'callback', cb))
    session.closed = True
    worker.append(b'audio')
    worker.finish()
    worker.start()
    assert worker.done.wait(1)
    assert worker.outcome.status == 'start_failed'
    callback.assert_not_called()


def test_provider_closing_during_silent_noop_send_is_incomplete(setup):
    session, _, worker = setup()
    session.connect_release.set()

    def no_op_send(pcm):
        session.closed = True

    session.send_audio = no_op_send
    worker.append(b'audio')
    worker.finish()
    worker.start()
    assert worker.done.wait(1)
    assert worker.outcome.status == 'send_failed'
    assert worker.outcome.sent_bytes == 0
