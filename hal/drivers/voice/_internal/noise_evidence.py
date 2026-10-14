"""Bounded per-capture streaming speech evidence, with exact-prefix verification."""

from collections import deque
import hashlib
import logging
import math
import threading


logger = logging.getLogger("hal.voice")
_BLOCK_SAMPLES = 512
_BLOCK_BYTES = _BLOCK_SAMPLES * 2
_RATE = 16000


class NoiseEvidence:
    """One nonblocking producer and one classifier owner for a single capture.

    The factory runs on the worker and returns ``feed(pcm512_bytes) -> float``.
    Feed must preserve its private model state and report errors, not fail-open
    scores. Only complete 512-sample blocks are scored; finish never pads a tail.
    ``metrics`` reuses only a processed, aligned, byte-identical PCM prefix.
    Queue capacity includes a block currently in inference and any partial tail.
    The owner must retain its worker slot until ``done``, including cancellation.
    """

    def __init__(self, classifier_factory, *, max_samples: int,
                 speech_threshold: float = .15, max_queue_bytes: int = 262144):
        if max_samples <= 0 or max_queue_bytes < _BLOCK_BYTES:
            raise ValueError("positive sample limit and at least one queued block are required")
        if not math.isfinite(speech_threshold) or not 0 <= speech_threshold <= 1:
            raise ValueError("speech threshold must be a probability")
        self._factory = classifier_factory
        self._max_bytes = int(max_samples) * 2
        self._queue_limit = int(max_queue_bytes)
        self._threshold = speech_threshold
        self._condition = threading.Condition()
        self._queue = deque()
        self._pending_bytes = 0
        self._accepted_bytes = 0
        self._prefixes = {}
        self._scores = []
        self._finished = False
        self._invalid = False
        self._thread = None
        self.done = threading.Event()

    def start(self):
        with self._condition:
            if self._thread is not None:
                raise RuntimeError("noise evidence cannot be restarted")
            self._thread = threading.Thread(target=self._run, name="voice-noise-evidence", daemon=True)
            try:
                self._thread.start()
            except Exception:
                self._invalidate_locked()
                self.done.set()
                logger.exception("[noise-evidence] worker start failed; using endpoint fallback")
        return self

    def _invalidate_locked(self):
        self._invalid = True
        self._queue.clear()
        self._prefixes.clear()
        self._scores.clear()
        self._condition.notify_all()

    def append(self, pcm: bytes) -> bool:
        """Accept immutable PCM without waiting for classifier initialization/I/O."""
        if not isinstance(pcm, bytes):
            raise TypeError("noise evidence requires immutable PCM bytes")
        with self._condition:
            if self._finished or self._invalid or self.done.is_set():
                return False
            if (len(pcm) % 2 or self._pending_bytes + len(pcm) > self._queue_limit
                    or self._accepted_bytes + len(pcm) > self._max_bytes):
                self._invalidate_locked()
                return False
            if pcm:
                self._queue.append(pcm)
                self._pending_bytes += len(pcm)
                self._accepted_bytes += len(pcm)
                self._condition.notify_all()
            return True

    def finish(self) -> None:
        """Seal the capture; let its worker drain without padding partial audio."""
        with self._condition:
            self._finished = True
            self._condition.notify_all()

    def close(self) -> None:
        """Retire every result immediately; never wait on a running inference."""
        with self._condition:
            self._invalidate_locked()

    def metrics(self, pcm_bytes: bytes) -> tuple | None:
        """Return measured evidence only for an exact processed aligned prefix."""
        if not isinstance(pcm_bytes, bytes) or not pcm_bytes or len(pcm_bytes) % _BLOCK_BYTES:
            return None
        with self._condition:
            if self._invalid:
                return None
            record = self._prefixes.get(len(pcm_bytes))
            if record is None:
                return None
            scores = tuple(self._scores[:record[2]])
        # Hash/sum outside the lock. Summing only on lookup matches the current
        # runtime's sum(confs) exactly, without an O(n-squared) worker hot path.
        digest = hashlib.sha256(pcm_bytes).digest()
        measured = record[1]
        mean = sum(scores) / len(scores)
        with self._condition:
            if self._invalid or digest != record[0]:
                return None
            return measured[0], mean, measured[1], measured[2], measured[3]

    def _run(self):
        pending = bytearray()
        digest = hashlib.sha256()
        count = voiced_count = 0
        first_voiced = last_voiced = None
        peak = 0.0
        try:
            with self._condition:
                if self._invalid:
                    return
            feed = self._factory()
            if not callable(feed):
                raise TypeError("classifier factory must return a callable")
            while True:
                with self._condition:
                    self._condition.wait_for(lambda: self._invalid or self._queue or self._finished)
                    if self._invalid:
                        return
                    if not self._queue:
                        return
                    chunk = self._queue.popleft()
                pending.extend(chunk)
                offset = 0
                while len(pending) - offset >= _BLOCK_BYTES:
                    with self._condition:
                        if self._invalid:
                            return
                    block = bytes(pending[offset:offset + _BLOCK_BYTES])
                    score = float(feed(block))  # Never hold the producer lock across inference.
                    if not math.isfinite(score) or not 0 <= score <= 1:
                        raise ValueError("classifier returned an invalid probability")
                    digest.update(block)
                    peak = max(peak, score)
                    if score >= self._threshold:
                        voiced_count += 1
                        first_voiced = count if first_voiced is None else first_voiced
                        last_voiced = count
                    count += 1
                    ratio = voiced_count / count
                    if first_voiced is None:
                        span_ratio, span_seconds = ratio, 0.0
                    else:
                        span = last_voiced - first_voiced + 1
                        span_ratio, span_seconds = voiced_count / span, span * _BLOCK_SAMPLES / _RATE
                    measured = (peak, ratio, span_ratio, span_seconds)
                    record = (digest.digest(), measured, count)
                    with self._condition:
                        if self._invalid:
                            return
                        self._scores.append(score)
                        self._prefixes[count * _BLOCK_BYTES] = record
                        self._pending_bytes -= _BLOCK_BYTES
                        self._condition.notify_all()
                    offset += _BLOCK_BYTES
                if offset:
                    del pending[:offset]
        except Exception as error:
            with self._condition:
                self._invalidate_locked()
            logger.warning("[noise-evidence] worker failed (%s); using endpoint fallback", type(error).__name__)
        finally:
            self._factory = None
            with self._condition:
                self._queue.clear()
                self._pending_bytes = 0
                self.done.set()
                self._condition.notify_all()
