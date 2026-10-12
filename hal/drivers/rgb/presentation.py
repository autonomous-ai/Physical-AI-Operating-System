"""One bounded mailbox for optional presentation, outside the audio writer."""

import logging
import threading
from dataclasses import dataclass
from time import monotonic
from hal.board.presets_overlay import DEFAULT_LED_COUNT

logger = logging.getLogger("hal.rgb.presentation")


@dataclass(frozen=True)
class GuardedFrame:
    """Recheck speech ownership and frame age at the final RGB driver boundary."""

    value: object
    current: object
    created_at: float

    def valid(self):
        return self.current() and monotonic() - self.created_at < 0.1


class SpeechRGB:
    """An effect cannot enqueue or paint after its speech presentation is revoked."""

    def __init__(self, service, current, stop):
        self._service, self._current, self._stop = service, current, stop
        self.led_count = getattr(service, "led_count", DEFAULT_LED_COUNT)

    def dispatch(self, command, value):
        if not self._current():
            self._stop.set()
            return
        self._service.dispatch(command, GuardedFrame(value, self._current, monotonic()))


class LatestPresentation:
    """Keep only the latest pending state; never hold admission across rendering."""

    def __init__(self, render, *, name):
        self._render = render
        self._name = name
        self._condition = threading.Condition()
        self._thread = None
        self._pending = None
        self._generation = 0
        self._busy = False
        self._closed = False

    def start(self):
        with self._condition:
            if self._thread is None and not self._closed:
                self._thread = threading.Thread(target=self._run, name=self._name, daemon=True)
                self._thread.start()

    def submit(self, state):
        with self._condition:
            if self._closed:
                return
            self._generation += 1
            self._pending = (self._generation, state)
            self.start()
            self._condition.notify()

    def _current(self, generation):
        with self._condition:
            return not self._closed and generation == self._generation

    def _run(self):
        while True:
            with self._condition:
                self._condition.wait_for(lambda: self._pending is not None or self._closed)
                if self._closed:
                    return
                generation, state = self._pending
                self._pending = None
                self._busy = True
            try:
                self._render(state, lambda: self._current(generation))
            except Exception:
                logger.exception("Speech LED presentation failed")
            finally:
                with self._condition:
                    self._busy = False
                    self._condition.notify_all()

    def wait_idle(self, timeout):
        """Diagnostics/tests only; audio callbacks must never wait for presentation."""
        with self._condition:
            return self._condition.wait_for(
                lambda: not self._busy and self._pending is None, timeout=timeout,
            )

    def close(self, timeout=2.5):
        """Reject late callbacks and retire pending work before hardware teardown."""
        with self._condition:
            self._closed = True
            self._generation += 1
            self._pending = None
            self._condition.notify_all()
            worker = self._thread
        if worker is not None:
            worker.join(timeout=timeout)
            if worker.is_alive():
                logger.warning("Speech LED worker has not retired before teardown")
