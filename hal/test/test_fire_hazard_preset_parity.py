"""HAL's 46 fire-hazard prompts must match the server's fire_hazard_v1 preset byte for byte.

HAL buckets OWLv2 detections by class-name string, so any drift between the two
lists silently drops a class.
"""

from pathlib import Path

from hal.drivers.sensing.perceptions.processors.fire_hazard import RemoteFireHazardDetector

_PRESET = (
    Path(__file__).resolve().parents[2]
    / "integrations/perception-service/src/core/perception/object/resources/presets/fire_hazard_v1.txt"
)


def test_hal_prompts_equal_server_preset():
    det = RemoteFireHazardDetector(base_url="", api_key="")
    server = [line.strip() for line in _PRESET.read_text().strip().splitlines() if line.strip()]
    assert len(det._all_classes) == 46
    assert det._all_classes == server
