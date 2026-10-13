"""Unknown faces can be deleted, and are deleted on their own after the retention window."""

import json
import threading
import time
from types import SimpleNamespace

import numpy as np
import pytest
from fastapi import HTTPException

from hal.drivers.sensing.perceptions.models import PersonKind
from hal.drivers.sensing.perceptions.processors.faceid import perception as perception_mod
from hal.drivers.sensing.perceptions.processors.faceid import recognizer as recognizer_mod
from hal.drivers.sensing.perceptions.processors.faceid.perception import FacePerception
from hal.drivers.sensing.perceptions.processors.faceid.recognizer import FaceRecognizer

TS = "%Y-%m-%dT%H:%M:%S%z"


def _ts(age_s: float) -> str:
    return time.strftime(TS, time.localtime(time.time() - age_s))


@pytest.fixture
def stores(tmp_path, monkeypatch):
    bank = tmp_path / "strangers"
    bank.mkdir()
    snaps = bank / "snapshots"
    snaps.mkdir()
    monkeypatch.setattr(recognizer_mod, "STRANGER_STATE_DIR", bank)
    monkeypatch.setattr(perception_mod, "_STRANGER_SNAPSHOTS_DIR", snaps)
    monkeypatch.setattr(perception_mod, "_STRANGER_STATS_FILE", tmp_path / ".stranger_stats.json")
    return SimpleNamespace(bank=bank, snaps=snaps, stats=tmp_path / ".stranger_stats.json")


def _recognizer(ids):
    r = FaceRecognizer.__new__(FaceRecognizer)
    r._lock = threading.RLock()
    r._stranger_counter = len(ids)
    r._stranger_embeddings = np.eye(len(ids), 4, dtype=np.float32)
    r._stranger_labels = np.array([FaceRecognizer.STRANGER_PREFIX + i for i in ids])
    r._save_strangers_state()
    return r


def _perception(recognizer, stats):
    p = FacePerception.__new__(FacePerception)
    p._state_lock = threading.RLock()
    p._stranger_visit_counts = stats
    p._people_data_dict = {sid: SimpleNamespace(kind=PersonKind.STRANGER) for sid in stats}
    p._ungreeted_strangers = {sid: None for sid in stats}
    p._face_recognizer = recognizer
    return p


def _rec(age_s):
    return {"count": 3, "first_seen": _ts(age_s + 10), "last_seen": _ts(age_s)}


def test_forgetting_one_stranger_removes_every_trace_and_survives_reload(stores):
    r = _recognizer(["stranger_1", "stranger_2"])
    p = _perception(r, {"stranger_1": _rec(5), "stranger_2": _rec(5)})
    (stores.snaps / "stranger_1_111.jpg").write_bytes(b"x")
    (stores.snaps / "stranger_2_222.jpg").write_bytes(b"x")

    assert p.forget_strangers({"stranger_1"}) == {"stranger_1"}

    assert set(json.loads(stores.stats.read_text())) == {"stranger_2"}
    assert "stranger_1" not in p._people_data_dict and "stranger_1" not in p._ungreeted_strangers
    assert not (stores.snaps / "stranger_1_111.jpg").exists()
    assert (stores.snaps / "stranger_2_222.jpg").exists()
    r._stranger_embeddings = r._stranger_labels = None  # the bank is re-read from disk every frame
    assert r.stranger_ids() == {"stranger_2"}
    assert int(np.load(stores.bank / "counter.npy")) == 2  # ids are not reused


def test_forgetting_all_strangers_clears_the_bank_files(stores):
    r = _recognizer(["stranger_1", "stranger_2"])
    p = _perception(r, {"stranger_1": _rec(5)})
    (stores.snaps / "stranger_2_222.jpg").write_bytes(b"x")

    p.forget_strangers(None)

    assert json.loads(stores.stats.read_text()) == {}
    assert not (stores.bank / "embeds.npy").exists() and not (stores.bank / "labels.npy").exists()
    assert r.stranger_ids() == set()
    assert list(stores.snaps.iterdir()) == []


def test_expiry_forgets_stale_and_orphaned_strangers_only(stores, monkeypatch):
    monkeypatch.setattr(perception_mod.config, "FACE_STRANGER_RETENTION_S", 1000.0)
    r = _recognizer(["stranger_1", "stranger_2", "stranger_3"])
    # stranger_3 has face data but no stats record: it can never be deleted from the web.
    p = _perception(r, {"stranger_1": _rec(2000), "stranger_2": _rec(10)})

    assert p.expire_strangers(time.time()) == {"stranger_1", "stranger_3"}

    assert set(p.stranger_stats()) == {"stranger_2"}
    assert r.stranger_ids() == {"stranger_2"}


def test_unreadable_last_seen_is_kept(stores, monkeypatch):
    monkeypatch.setattr(perception_mod.config, "FACE_STRANGER_RETENTION_S", 1000.0)
    r = _recognizer(["stranger_1"])
    p = _perception(r, {"stranger_1": {"count": 1, "last_seen": "garbage"}})
    assert p.expire_strangers(time.time()) == set()
    assert r.stranger_ids() == {"stranger_1"}


def test_routes_validate_the_id_and_report_unknown_ones(stores, monkeypatch):
    from hal.routes import sensing as routes

    r = _recognizer(["stranger_1"])
    p = _perception(r, {"stranger_1": _rec(5)})
    monkeypatch.setattr(routes, "_require_face_recognizer", lambda: p)

    with pytest.raises(HTTPException) as bad:
        routes.face_stranger_forget("../users")
    assert bad.value.status_code == 400
    with pytest.raises(HTTPException) as missing:
        routes.face_stranger_forget("stranger_9")
    assert missing.value.status_code == 404
    assert routes.face_stranger_forget("stranger_1") == {"status": "ok", "forgotten": ["stranger_1"]}
    assert routes.face_strangers_forget_all() == {"status": "ok", "forgotten": []}
