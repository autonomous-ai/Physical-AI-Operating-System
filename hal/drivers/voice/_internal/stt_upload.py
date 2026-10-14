"""Bounded, single-owner STT upload independent of microphone capture."""

from collections import deque
from concurrent.futures import Future
from dataclasses import dataclass
import threading
from typing import Callable


@dataclass(frozen=True)
class STTUploadOutcome:
    status: str
    accepted_bytes: int
    sent_bytes: int
    error: str = ""

    @property
    def complete(self) -> bool:
        """Only a fully uploaded and drained capture can supply a full fallback."""
        return self.status == "complete"


class STTUpload:
    """Own one session's start/send/close lifecycle on one worker.

    ``max_bytes`` bounds queued plus in-flight PCM; an overflow invalidates the
    whole STT take instead of silently dropping its prefix. The caller owns the
    capture duration limit and any separate PCM retained for realtime replay.
    ``done`` is set only after close finishes, including late start cancellation.
    Keep the service's worker slot reserved until then. Provider network methods
    must have their own timeouts: Python cannot interrupt a blocked socket call.

    A preconnected session needs an explicit ``bind_preconnected(session, cb)``
    adapter; no assumptions about provider-private callback attributes are made.
    Once passed here, nobody else may send to or close the session. Cancellation
    retires future callback deliveries; an already-running callback may finish.
    """

    def __init__(self, session, on_transcript: Callable[[str, bool], None], *,
                 max_bytes: int, bind_preconnected=None):
        if max_bytes <= 0:
            raise ValueError("max_bytes must be positive")
        self._session = session
        self._callback = on_transcript
        self._bind_preconnected = bind_preconnected
        self._max_bytes = max_bytes
        self._condition = threading.Condition()
        self._frames = deque()
        self._pending_bytes = 0
        self._accepted_bytes = 0
        self._sent_bytes = 0
        self._finished = False
        self._failure = ""
        self._error = ""
        self._retired = threading.Event()
        self._thread = None
        self._outcome = None
        self.done = threading.Event()
        self.completion = Future()

    @property
    def outcome(self):
        """Immutable terminal result, or None while resources are still owned."""
        with self._condition:
            return self._outcome

    def start(self):
        with self._condition:
            if self._thread is not None:
                raise RuntimeError("STT upload cannot be restarted")
            thread = threading.Thread(target=self._run, name="automatic-stt-upload", daemon=True)
            self._thread = thread
            thread.start()
        return self

    def append(self, pcm: bytes) -> bool:
        """Queue an entire PCM chunk without waiting for network I/O."""
        if not isinstance(pcm, bytes):
            raise TypeError("STT upload PCM must be immutable bytes")
        with self._condition:
            if self._finished or self._failure or self.done.is_set():
                return False
            if len(pcm) + self._pending_bytes > self._max_bytes:
                self._fail_locked("overflow", "STT upload backlog exceeded its byte limit")
                return False
            if pcm:
                self._frames.append(pcm)
                self._pending_bytes += len(pcm)
                self._accepted_bytes += len(pcm)
                self._condition.notify_all()
            return True

    def finish(self):
        """Seal input; the owner drains queued PCM and STT finals asynchronously."""
        with self._condition:
            self._finished = True
            self._condition.notify_all()

    def cancel(self):
        """Retire callbacks immediately; cleanup stays with the session owner."""
        with self._condition:
            if not self.done.is_set():
                self._fail_locked("cancelled", "")

    def _fail_locked(self, status, error):
        if not self._failure:
            self._failure, self._error = status, error
        self._retired.set()
        self._frames.clear()
        self._condition.notify_all()

    def _on_transcript(self, text, final):
        callback = self._callback
        if callback is not None and not self._retired.is_set():
            callback(text, final)

    def _run(self):
        phase = "start_failed"
        drained = False
        try:
            with self._condition:
                if self._failure:
                    return
            if self._bind_preconnected is not None:
                self._bind_preconnected(self._session, self._on_transcript)
                connected = not self._session.is_closed()
            else:
                connected = self._session.start(self._on_transcript)
            with self._condition:
                if self._failure:
                    return
                if not connected:
                    self._fail_locked("start_failed", "STT connection did not become ready")
                    return
            phase = "send_failed"
            while True:
                with self._condition:
                    self._condition.wait_for(lambda: self._failure or self._frames or self._finished)
                    if self._failure:
                        return
                    if not self._frames:
                        drained = True
                        return
                    frame = self._frames.popleft()
                if self._session.is_closed():
                    raise IOError("STT session closed before upload completed")
                self._session.send_audio(frame)
                if self._session.is_closed():
                    raise IOError("STT session closed while sending capture audio")
                with self._condition:
                    self._sent_bytes += len(frame)
                    self._pending_bytes -= len(frame)
        except Exception as error:
            with self._condition:
                self._fail_locked(phase, str(error))
        finally:
            # Only this worker closes, including start failures and late connects.
            try:
                self._session.close()
            except Exception as error:
                with self._condition:
                    self._fail_locked("close_failed", str(error))
            finally:
                self._retired.set()
                self._callback = None
                with self._condition:
                    status = self._failure or ("complete" if drained else "incomplete")
                    self._frames.clear()
                    self._pending_bytes = 0
                    self._outcome = STTUploadOutcome(
                        status, self._accepted_bytes, self._sent_bytes, self._error,
                    )
                    self.done.set()
                    self._condition.notify_all()
                self.completion.set_result(self._outcome)
