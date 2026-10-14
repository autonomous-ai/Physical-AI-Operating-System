"""MPR121 per-unit tuning: config layers, per-pad thresholds, live signal and recovery."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import hal.board.mpr121 as board
from hal.board.mpr121 import MPR121Config, load_mpr121_config, load_mpr121_layers, merge_layers
from hal.drivers.mpr121 import MPR121Handler

LAMP = Path(__file__).resolve().parents[2] / "robots" / "lamp"
PACKAGE = {"bus": 0, "touch_threshold": 6, "release_threshold": 4}


class TestTuningLayers(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.factory = Path(self.tmp.name) / "mpr121.factory.json"
        self.user = Path(self.tmp.name) / "mpr121.user.json"
        patches = [mock.patch.object(board, "FACTORY_PATH", str(self.factory)),
                   mock.patch.object(board, "USER_PATH", str(self.user))]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        self.addCleanup(self.tmp.cleanup)

    def test_no_layers_is_the_package(self):
        config = load_mpr121_config(LAMP, "orangepi_sun60")
        self.assertEqual((config.touch_threshold, config.release_threshold), (6, 4))
        self.assertEqual(load_mpr121_layers(LAMP, "orangepi_sun60")["sources"], {})

    def test_user_over_factory_over_package_per_key(self):
        self.factory.write_text(json.dumps({"touch_thresholds": [9] * 12, "release_thresholds": [5] * 12,
                                            "nclf": 32, "noise": [1] * 12}))
        self.user.write_text(json.dumps({"touch_thresholds": [12] * 12, "release_thresholds": [6] * 12,
                                         "sensitivity": 3}))
        loaded = load_mpr121_layers(LAMP, "orangepi_sun60")
        config = loaded["config"]
        self.assertEqual(config.touch_for(0), 12)
        self.assertEqual(config.nclf, 32)
        self.assertEqual(config.tap_min_electrodes, 3)  # wiring/gestures stay with the package
        self.assertEqual(loaded["sources"], {"touch_thresholds": "user", "release_thresholds": "user",
                                             "nclf": "factory"})

    def test_global_threshold_in_higher_layer_replaces_lower_lists(self):
        config, _ = merge_layers(PACKAGE, [("factory", {"touch_thresholds": [9] * 12, "release_thresholds": [5] * 12}),
                                           ("user", {"touch_threshold": 20, "release_threshold": 10})])
        self.assertEqual([config.touch_for(i) for i in range(12)], [20] * 12)
        self.assertEqual(config.release_for(11), 10)

    def test_invalid_or_unreadable_layer_falls_back(self):
        self.factory.write_text(json.dumps({"touch_thresholds": [9] * 12, "release_thresholds": [5] * 12}))
        for bad in ("{not json", json.dumps({"touch_thresholds": [3] * 12, "release_thresholds": [5] * 12}),
                    json.dumps({"touch_thresholds": [3] * 5}), json.dumps(["list"])):
            with self.subTest(bad=bad):
                self.user.write_text(bad)
                with self.assertLogs("hal.board", level="WARNING"):
                    config = load_mpr121_config(LAMP, "orangepi_sun60")
                self.assertEqual(config.touch_for(4), 9)

    def test_layers_cannot_change_wiring(self):
        self.user.write_text(json.dumps({"bus": 3, "address": 91, "touch_threshold": 8}))
        with self.assertLogs("hal.board", level="WARNING"):
            config = load_mpr121_config(LAMP, "orangepi_sun60")
        self.assertEqual((config.bus, config.address, config.touch_threshold), (0, 0x5A, 8))

    def test_per_pad_validation(self):
        with self.assertRaisesRegex(ValueError, "electrode 2"):
            MPR121Config(bus=0, touch_thresholds=[6, 6, 3, 6, 6, 6, 6, 6, 6, 6, 6, 6], touch_threshold=6, release_threshold=4)
        with self.assertRaisesRegex(ValueError, "12 integers"):
            MPR121Config(bus=0, touch_thresholds=[6] * 11)
        with self.assertRaisesRegex(ValueError, "stuck_reset_s"):
            MPR121Config(bus=0, stuck_reset_s=2)
        with self.assertRaisesRegex(ValueError, "nhdf"):
            MPR121Config(bus=0, nhdf=64)


def signal_bus(filtered, baseline, touched=0):
    bus = mock.Mock()

    def read(address, register, length):
        if register == 0x00:
            return touched.to_bytes(2, "little")
        if register == 0x04:
            return b"".join(v.to_bytes(2, "little") for v in filtered)
        if register == 0x1E:
            return bytes(b >> 2 for b in baseline)
        if register == 0x5D:
            return b"\x24"
        raise AssertionError(hex(register))
    bus.read_regs.side_effect = read
    return bus


class TestDriverTuning(unittest.TestCase):
    def handler(self, **kwargs):
        return MPR121Handler(MPR121Config(bus=0, **kwargs))

    def test_per_pad_thresholds_and_tracking_are_written(self):
        handler = self.handler(touch_thresholds=list(range(6, 18)), release_thresholds=list(range(2, 14)),
                               nhdf=2, nclf=40, fdlf=3, nhdt=1, nclt=5, fdlt=6)
        handler._bus = bus = signal_bus([0] * 12, [0] * 12)
        handler._initialize()
        calls = bus.write_reg.call_args_list
        for electrode in range(12):
            self.assertIn(mock.call(0x5A, 0x41 + 2 * electrode, 6 + electrode), calls)
            self.assertIn(mock.call(0x5A, 0x42 + 2 * electrode, 2 + electrode), calls)
        for register, value in ((0x30, 2), (0x31, 40), (0x32, 3), (0x33, 1), (0x34, 5), (0x35, 6)):
            self.assertIn(mock.call(0x5A, register, value), calls)

    def test_signal_is_read_only_while_watched(self):
        handler = self.handler()
        handler._bus = bus = signal_bus([700 - i for i in range(12)], [700] * 12, touched=0b100)
        handler._detector.update(False, 0)
        handler._poll_once()
        self.assertFalse(any(c.args[1] == 0x04 for c in bus.read_regs.call_args_list))
        self.assertIsNone(handler.signal())
        handler._poll_once()
        snap = handler.signal()
        self.assertEqual(snap["delta"], list(range(12)))
        self.assertEqual(snap["touched"][2], True)
        self.assertEqual(snap["touch"], [2] * 12)

    def test_reconfigure_rewrites_chip_in_poll_thread(self):
        handler = self.handler(poll_ms=1)
        bus = signal_bus([0] * 12, [0] * 12)
        with mock.patch("hal.drivers.mpr121.I2CBus", return_value=bus), \
                mock.patch("hal.drivers.harness.gestures.HarnessGestures"):
            handler.start()
            try:
                self.assertTrue(handler.reconfigure(MPR121Config(bus=0, touch_threshold=20, release_threshold=9)))
            finally:
                handler.stop()
        self.assertIn(mock.call(0x5A, 0x41, 20), bus.write_reg.call_args_list)
        self.assertEqual(handler.config.touch_threshold, 20)
        with self.assertRaisesRegex(ValueError, "bus/address"):
            handler.reconfigure(MPR121Config(bus=1))

    def test_stuck_touch_reinitializes(self):
        handler = self.handler(stuck_reset_s=5)
        handler._bus = bus = signal_bus([0] * 12, [0] * 12, touched=0b1)
        handler._last_raw_mask = 0b1
        handler._check_stuck(100)
        handler._check_stuck(104.9)
        bus.write_reg.assert_not_called()
        with self.assertLogs("hal.drivers.mpr121", level="WARNING"):
            handler._check_stuck(105)
        self.assertIn(mock.call(0x5A, 0x5E, 0x8F), bus.write_reg.call_args_list)
        self.assertIsNone(handler._touch_since)
        # no signal published while the reset settles
        self.assertIsNone(handler.signal())
        self.assertGreater(handler._signal_at, 105)

    def test_stuck_reset_off(self):
        handler = self.handler(stuck_reset_s=0)
        handler._bus = bus = signal_bus([0] * 12, [0] * 12)
        handler._last_raw_mask = 0b1
        handler._check_stuck(0)
        handler._check_stuck(10_000)
        bus.write_reg.assert_not_called()

    def test_i2c_fault_recovers(self):
        handler = self.handler(i2c_recover_attempts=3)
        handler._bus = mock.Mock()
        fresh = signal_bus([0] * 12, [0] * 12)
        with mock.patch("hal.drivers.mpr121.I2CBus", side_effect=[OSError("nack"), fresh]), \
                mock.patch.object(handler._stop, "wait", return_value=False), \
                self.assertLogs("hal.drivers.mpr121", level="INFO") as logs:
            self.assertTrue(handler._recover(OSError("disconnected")))
        self.assertIs(handler._bus, fresh)
        self.assertTrue(any("i2c_recovered attempt=2" in line for line in logs.output))

    def test_i2c_fault_gives_up(self):
        handler = self.handler(i2c_recover_attempts=2)
        handler._bus = mock.Mock()
        with mock.patch("hal.drivers.mpr121.I2CBus", side_effect=OSError("gone")), \
                mock.patch.object(handler._stop, "wait", return_value=False), \
                self.assertLogs("hal.drivers.mpr121", level="WARNING"):
            self.assertFalse(handler._recover(OSError("disconnected")))


if __name__ == "__main__":
    unittest.main()
