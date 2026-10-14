"""Bounded microphone consumption independent of automatic turn processing."""

from collections import deque
import threading
import time


class ContinuousCapture:
    """Pump an already-open, optionally AEC-wrapped microphone.

    This preserves recent eligible input, not full-duplex conversation. The owner
    must stop this reader before closing the backend. ``abort`` must unblock the
    backend's read without closing descriptors under that read. Suppressed input
    yields source-paced zero frames, allowing the caller to observe mode/stop
    state without receiving playback audio. A zero marker never displaces a
    subsequent clean prefix. Optional ``silenced`` replaces cue samples with
    zeros while preserving sequence and queued speech, without a hard reset.
    """

    def __init__(self, mic, frame_size, *, max_frames, max_age_s,
                 suppressed, abort, silenced=None, clock=time.monotonic):
        if frame_size <= 0 or max_frames <= 0 or max_age_s <= 0:
            raise ValueError("capture dimensions and age must be positive")
        self._mic = mic
        self._frame_size = frame_size
        self._max_frames = max_frames
        self._max_age_s = max_age_s
        self._suppressed = suppressed
        self._silenced = silenced or (lambda: False)
        self._abort = abort
        self._clock = clock
        self._condition = threading.Condition()
        self._frames = deque()
        self._stopped = False
        self._error = None
        self._discontinuity = False
        self._last_sequence = -1
        self._last_generation = 0
        self._thread = None

    def start(self):
        with self._condition:
            if self._thread is not None or self._stopped:
                raise RuntimeError("capture pump cannot be restarted")
            self._thread = threading.Thread(
                target=self._run, name="automatic-mic-capture", daemon=True,
            )
            self._thread.start()
        return self

    def _run(self):
        sequence = -1
        generation = 0
        try:
            while True:
                with self._condition:
                    if self._stopped:
                        return
                muted_before = self._suppressed()
                silenced_before = self._silenced()
                started_at = self._clock()
                if muted_before:
                    with self._condition:
                        if self._frames and not self._frames[0][2]:
                            self._frames.clear()
                data, overflowed = self._mic.read(self._frame_size)
                muted_after = self._suppressed()
                silenced_after = self._silenced()
                sequence += 1
                with self._condition:
                    if self._stopped:
                        return
                    if muted_before or muted_after or overflowed:
                        self._frames.clear()
                        generation += 1
                        self._frames.append((started_at, data * 0, True,
                                             sequence, generation))
                        self._condition.notify_all()
                        continue
                    if self._frames and self._frames[0][2]:
                        self._frames.clear()
                    # A short cue masks only its samples, preserving the input
                    # timeline and queued user speech on either side. Real
                    # queue loss/overflow still signals a discontinuity.
                    if silenced_before or silenced_after:
                        data = data * 0
                    if len(self._frames) == self._max_frames:
                        self._frames.popleft()
                    self._frames.append((started_at, data, False,
                                         sequence, generation))
                    self._condition.notify_all()
        except Exception as exc:
            with self._condition:
                if not self._stopped:
                    self._error = exc
                self._condition.notify_all()

    def read(self, frames):
        if frames != self._frame_size:
            raise ValueError("capture pump requires its configured frame size")
        with self._condition:
            if self._thread is None:
                raise RuntimeError("capture pump has not started")
            while True:
                if self._stopped:
                    raise InterruptedError("capture pump stopped")
                if self._error is not None:
                    raise IOError("continuous microphone capture failed") from self._error
                cutoff = self._clock() - self._max_age_s
                while self._frames and self._frames[0][0] < cutoff:
                    self._frames.popleft()
                if self._frames:
                    _, data, _, sequence, generation = self._frames.popleft()
                    self._discontinuity |= (
                        sequence != self._last_sequence + 1
                        or generation != self._last_generation
                    )
                    self._last_sequence = sequence
                    self._last_generation = generation
                    return data, False
                self._condition.wait()

    def consume_discontinuity(self):
        """Return/reset loss evidence belonging to frames already delivered.

        Producer activity between read and this call cannot consume the next
        frame's discontinuity. Call after each read before processing its data.
        """
        with self._condition:
            value = self._discontinuity
            self._discontinuity = False
            return value

    def stop(self, timeout=3.0):
        """Unblock and join; a timeout must prevent the owner closing the mic."""
        with self._condition:
            self._stopped = True
            self._frames.clear()
            self._condition.notify_all()
            thread = self._thread
        if thread is None or not thread.is_alive():
            return
        self._abort()
        thread.join(timeout)
        if thread.is_alive():
            raise RuntimeError("capture reader did not stop; backend must remain open")
