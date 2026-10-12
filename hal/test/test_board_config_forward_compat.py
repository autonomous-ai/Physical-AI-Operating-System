"""A device package newer than HAL: unknown config fields are logged and ignored.

The device package and HAL update separately. lamp-52e6 got device 0.0.109 with HAL
0.1.282, whose servo_overload loader rejected `contact.release_after_s` and failed
boot six times. Every board loader now drops a field it does not know instead.
"""

import json
import logging
import shutil
from pathlib import Path

import pytest

from hal.board.gpio_button import load_button_configs
from hal.board.mpr121 import load_mpr121_config
from hal.board.privacy_button import load_privacy_button_config
from hal.board.scd41 import load_scd41_config
from hal.board.sen55 import load_sen55_config
from hal.board.sen63c import load_sen63c_config
from hal.board.servo_overload import load_servo_overload_config
from hal.board.ttp223 import load_touch_config

LAMP = Path(__file__).resolve().parents[2] / "robots" / "lamp"
BOARD = "orangepi_sun60"

LOADERS = [
    ("gpio_button.json", load_button_configs),
    ("mpr121.json", load_mpr121_config),
    ("privacy_button.json", lambda d, b: load_privacy_button_config(d, b, "lamp")),
    ("scd41.json", load_scd41_config),
    ("sen55.json", load_sen55_config),
    ("sen63c.json", load_sen63c_config),
    ("ttp223.json", load_touch_config),
    ("servo_overload.json", load_servo_overload_config),
]


@pytest.mark.parametrize("name,load", LOADERS, ids=[name for name, _ in LOADERS])
def test_a_field_from_a_newer_device_package_is_ignored(tmp_path, caplog, name, load):
    shutil.copy(LAMP / name, tmp_path / name)
    expected = load(str(tmp_path), BOARD)

    data = json.loads((tmp_path / name).read_text())
    entry = data["boards"][BOARD]
    entry["field_from_the_future"] = 1
    if isinstance(entry.get("contact"), dict):
        entry["contact"]["also_from_the_future"] = 2
    (tmp_path / name).write_text(json.dumps(data))

    with caplog.at_level(logging.WARNING):
        assert load(str(tmp_path), BOARD) == expected
    assert "field_from_the_future" in caplog.text


ROBOTS = LAMP.parent
SHIPPED = sorted(
    (path, load) for name, load in LOADERS for path in ROBOTS.rglob(name)
)


@pytest.mark.parametrize("path,load", SHIPPED, ids=[str(p.relative_to(ROBOTS)) for p, _ in SHIPPED])
def test_shipped_configs_have_no_unknown_fields(caplog, path, load):
    """HAL ignores unknown fields on the device; CI must not: a typo in an optional
    safety field (say `torque_limt`) would otherwise ship as that field turned off."""
    boards = json.loads(path.read_text())["boards"]
    with caplog.at_level(logging.WARNING, logger="hal.board.config_fields"):
        for board in boards:
            load(str(path.parent), board)
    assert "ignoring fields" not in caplog.text, caplog.text


def test_a_typo_in_a_shipped_config_is_caught(tmp_path, caplog):
    """The check above has teeth: a misspelled optional field is flagged."""
    data = json.loads((LAMP / "servo_overload.json").read_text())
    entry = data["boards"][BOARD]
    entry["torque_limt"] = entry.pop("torque_limit")
    (tmp_path / "servo_overload.json").write_text(json.dumps(data))
    with caplog.at_level(logging.WARNING, logger="hal.board.config_fields"):
        assert load_servo_overload_config(str(tmp_path), BOARD).torque_limit is None
    assert "torque_limt" in caplog.text
