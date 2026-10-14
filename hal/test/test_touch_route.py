"""/touch/* routes: user layer writes, sensitivity scaling, calibration, reset."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from fastapi import FastAPI
from fastapi.testclient import TestClient

import hal.app_state as state
import hal.board.mpr121 as board
from hal.routes import touch

LAMP = Path(__file__).resolve().parents[2] / "robots" / "lamp"


class FakeHandler:
    def __init__(self, deltas):
        self.deltas = deltas
        self.applied = []
        self.config = board.load_mpr121_config(LAMP, "orangepi_sun60")
        self._poll_thread = None
        self.t = 0

    def signal(self, watch_s=2.0):
        self.t += 1
        return {"t": self.t, "delta": self.deltas, "touched": [False] * 12,
                "touch": [6] * 12, "release": [4] * 12}

    def reconfigure(self, config, timeout=2.0):
        self.config = config
        self.applied.append(config)
        return True


class TestTouchRoutes(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.factory = Path(tmp.name) / "mpr121.factory.json"
        self.user = Path(tmp.name) / "mpr121.user.json"
        self.handler = FakeHandler([1, 2, 0, 3, 1, 1, 0, 2, 1, 1, 4, 1])
        for patch in (mock.patch.object(board, "FACTORY_PATH", str(self.factory)),
                      mock.patch.object(board, "USER_PATH", str(self.user)),
                      mock.patch.object(state, "mpr121_handler", self.handler),
                      mock.patch.object(state, "mpr121_source", (str(LAMP), "orangepi_sun60")),
                      mock.patch.object(touch.asyncio, "sleep", mock.AsyncMock())):
            patch.start()
            self.addCleanup(patch.stop)
        app = FastAPI()
        app.include_router(touch.router)
        self.client = TestClient(app)

    def test_config_reports_layers(self):
        self.factory.write_text(json.dumps({"touch_thresholds": [8] * 12, "release_thresholds": [4] * 12,
                                            "noise": [3] * 12}))
        data = self.client.get("/touch/config").json()
        self.assertEqual(data["touch"], [8] * 12)
        self.assertEqual(data["sources"]["touch_thresholds"], "factory")
        self.assertEqual(data["sensitivity"], 5)
        self.assertFalse(data["user_adjusted"])
        self.assertEqual(data["swipe_axis"][0], 11)

    def test_sensitivity_scales_factory_and_saves_user_layer(self):
        self.factory.write_text(json.dumps({"touch_thresholds": [10] * 12, "release_thresholds": [5] * 12}))
        data = self.client.put("/touch/config", json={"sensitivity": 7}).json()
        self.assertEqual(data["touch"], [13] * 12)  # 10 x 1.15^2
        self.assertEqual(data["sensitivity"], 7)
        saved = json.loads(self.user.read_text())
        self.assertEqual(saved["source"], "web")
        self.assertEqual(self.handler.applied[-1].touch_for(0), 13)
        light = self.client.put("/touch/config", json={"sensitivity": 1}).json()
        self.assertEqual(light["touch"], [6] * 12)  # 10 x 1.15^-4
        self.assertTrue(all(r < t for r, t in zip(light["release"], light["touch"])))

    def test_reset_returns_to_factory(self):
        self.factory.write_text(json.dumps({"touch_threshold": 9, "release_threshold": 4}))
        self.client.put("/touch/config", json={"sensitivity": 9})
        data = self.client.delete("/touch/config/user").json()
        self.assertEqual(data["touch"], [9] * 12)
        self.assertFalse(self.user.exists())

    def test_calibrate_saves_and_scales_from_calibration(self):
        result = self.client.post("/touch/calibrate", json={"seconds": 3}).json()
        self.assertEqual(result["noise"], self.handler.deltas)
        self.assertEqual(result["touch"][10], 10)  # 2 x 4 + 2
        self.assertEqual(result["touch"][0], 6)    # floor
        self.assertTrue(result["saved"])
        firmer = self.client.put("/touch/config", json={"sensitivity": 6}).json()
        self.assertEqual(firmer["touch"][10], 12)  # scales the calibration, not the package

    def test_calibrate_ignores_negative_swings(self):
        self.handler.deltas = [-25] * 11 + [3]
        result = self.client.post("/touch/calibrate", json={"seconds": 3, "save": False}).json()
        self.assertEqual(result["noise"], [0] * 11 + [3])
        self.assertEqual(result["touch"], [6] * 11 + [8])

    def test_calibrate_without_save_and_touched(self):
        result = self.client.post("/touch/calibrate", json={"seconds": 3, "save": False}).json()
        self.assertFalse(result["saved"])
        self.assertFalse(self.user.exists())
        self.handler.deltas = [0] * 11 + [80]
        response = self.client.post("/touch/calibrate", json={"seconds": 3})
        self.assertEqual(response.status_code, 409)

    def test_invalid_explicit_thresholds_rejected(self):
        response = self.client.put("/touch/config", json={"touch_thresholds": [3] * 12,
                                                          "release_thresholds": [5] * 12})
        self.assertEqual(response.status_code, 422)
        self.assertFalse(self.user.exists())

    def test_no_strip_is_404(self):
        with mock.patch.object(state, "mpr121_handler", None):
            self.assertEqual(self.client.get("/touch/config").status_code, 404)


if __name__ == "__main__":
    unittest.main()
