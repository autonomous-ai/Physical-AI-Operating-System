"""On-lamp fire/smoke gate: a small YOLO26-S end-to-end ONNX model that decides
whether a frame is worth sending to the cloud OWLv2 fire-hazard check.

The gate only answers yes/no with a score, so there is no undo-letterbox step.
Loading is lazy and fail-open: until the model is downloaded and loaded (or
after a load or run error) the verdict is UNAVAILABLE, and the caller falls
back to calling OWLv2 every check, which is the behavior before the gate.
"""

import ast
import logging
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import Any

import cv2
import numpy as np
import numpy.typing as npt

import hal.config as config
from hal import cpu_affinity
from hal.drivers.sensing.perceptions.processors.faceid.model_store import ensure_downloaded

logger = logging.getLogger(__name__)

# After a failed load (no network at boot, 404, bad file), wait this long before trying again.
_LOAD_RETRY_S = 600.0
# At most one "inference failed" warning per this many seconds.
_RUN_WARN_INTERVAL_S = 60.0

_TARGETS: frozenset[str] = frozenset({"fire", "smoke"})

SessionFactory = Callable[[str, Any], Any]


class GateVerdict(StrEnum):
    HIT = "hit"
    MISS = "miss"
    UNAVAILABLE = "unavailable"


@dataclass(frozen=True)
class GateResult:
    verdict: GateVerdict
    score: float
    label: str | None
    infer_ms: float | None


_UNAVAILABLE = GateResult(GateVerdict.UNAVAILABLE, 0.0, None, None)


def letterbox(img: npt.NDArray[np.uint8], size: int) -> tuple[npt.NDArray[np.uint8], float, int, int]:
    """Resize keeping aspect, pad to size x size with 114 gray (Ultralytics default)."""
    h, w = img.shape[:2]
    r = min(size / h, size / w)
    nw, nh = round(w * r), round(h * r)
    if (nw, nh) != (w, h):
        img = cv2.resize(img, (nw, nh), interpolation=cv2.INTER_LINEAR)
    px, py = (size - nw) // 2, (size - nh) // 2
    out = np.full((size, size, 3), 114, dtype=np.uint8)
    out[py:py + nh, px:px + nw] = img
    return out, r, px, py


def preprocess(img_bgr: npt.NDArray[np.uint8], size: int) -> tuple[npt.NDArray[np.float32], float, int, int]:
    """Letterbox, BGR->RGB, HWC->CHW, scale to [0, 1], add the batch axis."""
    lb, r, px, py = letterbox(img_bgr, size)
    x = lb[:, :, ::-1].transpose(2, 0, 1)
    x = np.ascontiguousarray(x, dtype=np.float32)[None] / 255.0
    return x, r, px, py


def parse_names(meta: dict[str, str]) -> dict[int, str]:
    """Class names from Ultralytics ONNX metadata (a Python dict literal, not JSON)."""
    names = ast.literal_eval(meta["names"])
    return {int(k): str(v) for k, v in names.items()}


def best_target(
    out: npt.NDArray[np.float32],
    names: dict[int, str],
    targets: frozenset[str] = _TARGETS,
) -> tuple[float, str | None]:
    """Highest score among rows (x1, y1, x2, y2, score, cls) whose class is a target."""
    best_score, best_label = 0.0, None
    for row in out.reshape(-1, out.shape[-1]):
        label = names.get(int(row[5]))
        if label in targets and float(row[4]) > best_score:
            best_score, best_label = float(row[4]), label
    return best_score, best_label


def _default_session_factory(path: str, opts: Any) -> Any:
    import onnxruntime as ort

    return ort.InferenceSession(path, opts, providers=["CPUExecutionProvider"])


def _session_options(threads: int) -> Any:
    import onnxruntime as ort

    opts = ort.SessionOptions()
    opts.intra_op_num_threads = threads
    opts.inter_op_num_threads = 1
    return opts


class FireGate:
    """Lazy-loaded YOLO26-S flame/smoke gate. `run()` never blocks on loading."""

    def __init__(
        self,
        model_path: str = config.FIRE_HAZARD_GATE_MODEL_PATH,
        url: str | None = config.FIRE_HAZARD_GATE_MODEL_URL,
        threshold: float = config.FIRE_HAZARD_GATE_SCORE_THRESHOLD,
        threads: int = config.FIRE_HAZARD_GATE_THREADS,
        session_factory: SessionFactory | None = None,
    ):
        self._model_path: str = model_path
        self._url: str | None = url or None
        self._threshold: float = threshold
        self._threads: int = threads
        self._session_factory: SessionFactory = session_factory or _default_session_factory

        self._lock: threading.Lock = threading.Lock()
        self._state: str = "loading"
        self._loader: threading.Thread | None = None
        self._retry_at: float = 0.0
        self._closed: bool = False
        self._session: Any = None
        self._input_name: str = ""
        self._size: int = 0
        self._names: dict[int, str] = {}
        self._last_run_warn: float = float("-inf")

    @property
    def state(self) -> str:
        with self._lock:
            return self._state

    def run(self, frame_bgr: npt.NDArray[np.uint8]) -> GateResult:
        with self._lock:
            if self._closed:
                return _UNAVAILABLE
            if self._state != "ready":
                self._maybe_start_load_locked()
                return _UNAVAILABLE
            session, input_name, size, names = self._session, self._input_name, self._size, self._names

        try:
            x, _, _, _ = preprocess(frame_bgr, size)
            t0 = time.perf_counter()
            out = session.run(None, {input_name: x})[0]
            infer_ms = (time.perf_counter() - t0) * 1000.0
        except Exception as exc:
            now = time.monotonic()
            if now - self._last_run_warn >= _RUN_WARN_INTERVAL_S:
                self._last_run_warn = now
                logger.warning("[fire_gate] inference failed (%s); OWLv2 runs this check", exc)
            return _UNAVAILABLE

        score, label = best_target(np.asarray(out), names)
        verdict = GateVerdict.HIT if score >= self._threshold else GateVerdict.MISS
        return GateResult(verdict, score, label, infer_ms)

    def close(self) -> None:
        with self._lock:
            self._closed = True
            self._session = None

    def _maybe_start_load_locked(self) -> None:
        if self._loader is not None or time.monotonic() < self._retry_at:
            return
        self._state = "loading"
        self._loader = threading.Thread(target=self._load, name="fire-gate-load", daemon=True)
        self._loader.start()

    def _load(self) -> None:
        # ORT's pool threads inherit this affinity, keeping inference off the voice cores.
        cpu_affinity.pin_current_thread(cpu_affinity.SLOW)
        t0 = time.monotonic()
        try:
            path = ensure_downloaded(Path(self._model_path), self._url)
            session = self._session_factory(str(path), _session_options(self._threads))
            inp = session.get_inputs()[0]
            shape = list(inp.shape)
            if (len(shape) != 4 or shape[0] != 1 or shape[1] != 3
                    or not isinstance(shape[2], int) or shape[2] != shape[3]):
                raise ValueError(f"unexpected input shape {shape}")
            out_shape = list(session.get_outputs()[0].shape)
            if not out_shape or out_shape[-1] != 6:
                raise ValueError(f"unexpected output shape {out_shape}")
            names = parse_names(session.get_modelmeta().custom_metadata_map)
            if not _TARGETS & set(names.values()):
                raise ValueError(f"no fire/smoke class in {names}")
            size = int(shape[2])
            session.run(None, {inp.name: np.zeros((1, 3, size, size), dtype=np.float32)})
        except Exception as exc:
            logger.warning("[fire_gate] model unavailable (%s); OWLv2 runs every check until it loads", exc)
            with self._lock:
                self._state = "unavailable"
                self._retry_at = time.monotonic() + _LOAD_RETRY_S
                self._loader = None
            return

        with self._lock:
            self._loader = None
            if self._closed:
                return
            self._session, self._input_name, self._size, self._names = session, inp.name, size, names
            self._state = "ready"
        logger.info(
            "[fire_gate] model ready in %.0f ms (%s, %dx%d, %d threads, threshold %.2f, names %s)",
            (time.monotonic() - t0) * 1000.0, self._model_path, size, size,
            self._threads, self._threshold, names,
        )
