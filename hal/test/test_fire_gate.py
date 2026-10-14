"""On-lamp fire/smoke gate: preprocessing, decoding, fail-open loading."""

import logging
import os
import threading
import time
from pathlib import Path

import cv2
import numpy as np
import pytest

from hal.drivers.sensing.perceptions.processors import fire_gate
from hal.drivers.sensing.perceptions.processors.fire_gate import (
    FireGate,
    GateVerdict,
    best_target,
    letterbox,
    parse_names,
    preprocess,
)

_NAMES = "{0: 'fire', 1: 'other', 2: 'smoke'}"


class _Meta:
    def __init__(self, names: str):
        self.custom_metadata_map = {"names": names}


class _IO:
    def __init__(self, name, shape):
        self.name = name
        self.shape = shape


class _FakeSession:
    def __init__(self, out=None, names=_NAMES, in_shape=(1, 3, 320, 320), raise_on_run=False):
        self.out = out if out is not None else np.zeros((1, 300, 6), dtype=np.float32)
        self._names = names
        self._in_shape = list(in_shape)
        self.raise_on_run = raise_on_run
        self.runs = 0

    def get_inputs(self):
        return [_IO("images", self._in_shape)]

    def get_outputs(self):
        return [_IO("output0", [1, 300, 6])]

    def get_modelmeta(self):
        return _Meta(self._names)

    def run(self, _outputs, feeds):
        self.runs += 1
        if self.raise_on_run:
            raise RuntimeError("boom")
        assert feeds["images"].shape == tuple(self._in_shape)
        return [self.out]


def _out_with(score: float, cls: int) -> np.ndarray:
    out = np.zeros((1, 300, 6), dtype=np.float32)
    out[0, 0] = [10, 10, 50, 50, score, cls]
    return out


@pytest.fixture
def no_download(monkeypatch):
    monkeypatch.setattr(fire_gate, "ensure_downloaded", lambda p, _url: p)


def _wait_state(gate: FireGate, want: str, timeout: float = 2.0) -> None:
    # perf_counter: some tests patch time.monotonic (the gate's retry clock).
    deadline = time.perf_counter() + timeout
    while time.perf_counter() < deadline:
        if gate.state == want:
            return
        time.sleep(0.005)
    raise AssertionError(f"gate state {gate.state!r}, want {want!r}")


def _ready_gate(session, **kw) -> FireGate:
    gate = FireGate(model_path="/nonexistent/gate.onnx", url=None, session_factory=lambda _p, _o: session, **kw)
    frame = np.zeros((240, 320, 3), dtype=np.uint8)
    assert gate.run(frame).verdict is GateVerdict.UNAVAILABLE
    _wait_state(gate, "ready")
    return gate


def test_letterbox_keeps_aspect_and_pads_114():
    img = np.zeros((480, 640, 3), dtype=np.uint8)
    out, r, px, py = letterbox(img, 320)
    assert out.shape == (320, 320, 3)
    assert r == 0.5
    assert (px, py) == (0, 40)
    assert out[0, 0].tolist() == [114, 114, 114]
    assert out[160, 160].tolist() == [0, 0, 0]


def test_preprocess_is_rgb_chw_unit_range():
    img = np.zeros((320, 320, 3), dtype=np.uint8)
    img[:, :, 0] = 255  # pure blue in BGR
    x, r, px, py = preprocess(img, 320)
    assert x.dtype == np.float32
    assert x.shape == (1, 3, 320, 320)
    assert x[0, 2, 100, 100] == 1.0
    assert x[0, 0, 100, 100] == 0.0
    assert (r, px, py) == (1.0, 0, 0)


def test_parse_names_python_literal():
    assert parse_names({"names": _NAMES}) == {0: "fire", 1: "other", 2: "smoke"}
    assert parse_names({"names": "{0: 'fire', 1: 'light', 2: 'nonfire', 3: 'smoke'}"}) == {
        0: "fire", 1: "light", 2: "nonfire", 3: "smoke",
    }


def test_best_target_ignores_other_classes():
    out = np.zeros((1, 300, 6), dtype=np.float32)
    out[0, 0] = [0, 0, 1, 1, 0.9, 1]
    out[0, 1] = [0, 0, 1, 1, 0.25, 2]
    score, label = best_target(out, {0: "fire", 1: "other", 2: "smoke"})
    assert label == "smoke"
    assert score == pytest.approx(0.25)
    assert best_target(np.zeros((1, 300, 6), dtype=np.float32), {1: "other"}) == (0.0, None)


def test_run_hit_and_miss_at_threshold(no_download):
    frame = np.zeros((240, 320, 3), dtype=np.uint8)
    hit = _ready_gate(_FakeSession(out=_out_with(0.2, 0))).run(frame)
    assert hit.verdict is GateVerdict.HIT
    assert hit.label == "fire"
    assert hit.infer_ms is not None
    miss = _ready_gate(_FakeSession(out=_out_with(0.19, 0))).run(frame)
    assert miss.verdict is GateVerdict.MISS


def test_unavailable_while_loading_then_ready(no_download):
    release = threading.Event()
    session = _FakeSession(out=_out_with(0.5, 2))

    def factory(_p, _o):
        release.wait(2.0)
        return session

    gate = FireGate(model_path="/nonexistent/gate.onnx", url=None, session_factory=factory)
    frame = np.zeros((240, 320, 3), dtype=np.uint8)
    assert gate.run(frame).verdict is GateVerdict.UNAVAILABLE
    assert gate.state == "loading"
    assert gate.run(frame).verdict is GateVerdict.UNAVAILABLE
    loaders = [t for t in threading.enumerate() if t.name == "fire-gate-load"]
    assert len(loaders) == 1
    release.set()
    _wait_state(gate, "ready")
    res = gate.run(frame)
    assert res.verdict is GateVerdict.HIT and res.label == "smoke"


def test_download_failure_is_unavailable_and_logged(monkeypatch, caplog):
    def fail(_p, _u):
        raise RuntimeError("HTTP Error 404")

    monkeypatch.setattr(fire_gate, "ensure_downloaded", fail)
    gate = FireGate(model_path="/nonexistent/gate.onnx", url="https://x/y.onnx",
                    session_factory=lambda _p, _o: _FakeSession())
    with caplog.at_level(logging.WARNING):
        assert gate.run(np.zeros((8, 8, 3), np.uint8)).verdict is GateVerdict.UNAVAILABLE
        _wait_state(gate, "unavailable")
    assert any("[fire_gate] model unavailable" in r.getMessage() and "404" in r.getMessage()
               for r in caplog.records)


def test_bad_model_shape_is_unavailable(no_download):
    gate = FireGate(model_path="/nonexistent/gate.onnx", url=None,
                    session_factory=lambda _p, _o: _FakeSession(names="{0: 'person'}"))
    gate.run(np.zeros((8, 8, 3), np.uint8))
    _wait_state(gate, "unavailable")


def test_gate_retries_after_failure(monkeypatch):
    clock = [1000.0]
    monkeypatch.setattr(fire_gate.time, "monotonic", lambda: clock[0])
    attempts = []

    def fail(p, _u):
        attempts.append(p)
        raise RuntimeError("offline")

    monkeypatch.setattr(fire_gate, "ensure_downloaded", fail)
    gate = FireGate(model_path="/nonexistent/gate.onnx", url="https://x/y.onnx",
                    session_factory=lambda _p, _o: _FakeSession())
    frame = np.zeros((8, 8, 3), np.uint8)
    gate.run(frame)
    _wait_state(gate, "unavailable")
    assert len(attempts) == 1

    clock[0] += fire_gate._LOAD_RETRY_S - 1
    for _ in range(3):
        assert gate.run(frame).verdict is GateVerdict.UNAVAILABLE
    time.sleep(0.05)
    assert len(attempts) == 1

    clock[0] += 1
    for _ in range(3):
        gate.run(frame)
    deadline = time.perf_counter() + 2
    while len(attempts) < 2 and time.perf_counter() < deadline:
        time.sleep(0.005)
    _wait_state(gate, "unavailable")
    time.sleep(0.05)
    assert len(attempts) == 2


def test_run_error_is_unavailable_for_that_call(no_download):
    session = _FakeSession()
    gate = _ready_gate(session)
    session.raise_on_run = True
    assert gate.run(np.zeros((8, 8, 3), np.uint8)).verdict is GateVerdict.UNAVAILABLE
    assert gate.state == "ready"
    session.raise_on_run = False
    session.out = _out_with(0.3, 0)
    assert gate.run(np.zeros((8, 8, 3), np.uint8)).verdict is GateVerdict.HIT


def test_session_uses_two_intra_threads(no_download):
    seen = {}

    def factory(_p, opts):
        seen["intra"] = opts.intra_op_num_threads
        seen["inter"] = opts.inter_op_num_threads
        return _FakeSession()

    gate = FireGate(model_path="/nonexistent/gate.onnx", url=None, session_factory=factory)
    gate.run(np.zeros((8, 8, 3), np.uint8))
    _wait_state(gate, "ready")
    assert seen == {"intra": 2, "inter": 1}


def test_closed_gate_is_unavailable(no_download):
    gate = _ready_gate(_FakeSession(out=_out_with(0.9, 0)))
    gate.close()
    assert gate.run(np.zeros((8, 8, 3), np.uint8)).verdict is GateVerdict.UNAVAILABLE


@pytest.mark.skipif(not os.environ.get("HAL_FIRE_GATE_TEST_MODEL"), reason="set HAL_FIRE_GATE_TEST_MODEL to a gate .onnx")
def test_real_model_on_fire_fixture():
    model = os.environ["HAL_FIRE_GATE_TEST_MODEL"]
    img_path = (Path(__file__).resolve().parents[2]
                / "integrations/perception-service/tests/fixtures/images/fire-1.jpg")
    frame = cv2.imread(str(img_path))
    assert frame is not None
    gate = FireGate(model_path=model, url=None)
    gate.run(frame)
    _wait_state(gate, "ready", timeout=30.0)
    res = gate.run(frame)
    ms = []
    for _ in range(20):
        ms.append(gate.run(frame).infer_ms)
    print(f"\n[fire_gate] fire-1.jpg verdict={res.verdict} score={res.score:.3f} label={res.label} "
          f"infer_ms p50={np.median(ms):.1f} max={max(ms):.1f}")
    assert res.verdict is GateVerdict.HIT
    gate.close()
