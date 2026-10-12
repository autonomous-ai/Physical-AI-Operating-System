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
