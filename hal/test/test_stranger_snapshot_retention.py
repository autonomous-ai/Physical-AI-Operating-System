"""Familiar-stranger snapshots are deleted once they pass the retention window."""

import os

from hal.drivers.sensing.perceptions.processors.faceid import perception as perception_mod


def test_prune_deletes_only_snapshots_past_retention(tmp_path, monkeypatch):
    monkeypatch.setattr(perception_mod, "_STRANGER_SNAPSHOTS_DIR", tmp_path)
    monkeypatch.setattr(perception_mod.config, "STRANGER_SNAPSHOT_RETENTION_S", 100.0)
    now = 10_000.0
    old = tmp_path / "stranger_1_old.jpg"
    fresh = tmp_path / "stranger_2_fresh.jpg"
    other = tmp_path / "notes.txt"
    for f, age in ((old, 101), (fresh, 99), (other, 1_000)):
        f.write_bytes(b"x")
        os.utime(f, (now - age, now - age))

    assert perception_mod.prune_stranger_snapshots(now) == 1
    assert not old.exists() and fresh.exists() and other.exists()


def test_prune_without_folder_is_a_no_op(tmp_path, monkeypatch):
    monkeypatch.setattr(perception_mod, "_STRANGER_SNAPSHOTS_DIR", tmp_path / "missing")
    assert perception_mod.prune_stranger_snapshots(10_000.0) == 0
