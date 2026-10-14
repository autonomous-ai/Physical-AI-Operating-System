"""Validate service activation promotion without publishing metadata."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'promote-ota.sh'
SOURCE = SCRIPT.read_text().split("<<'PY'\n", 1)[1].split('\nPY\n', 1)[0]


class PromotionTests(unittest.TestCase):
    def promote(self, activation='', device='lamp', component='lamp-os', floor='', overrides=None):
        entry = dict(service='lamp-os', version='1.2.3', min_version='0.0.0',
                     activate_by_device={'other': True}, min_version_by_device={'other': '1.0.0'})
        entry.update(overrides or {})
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'metadata.json'
            path.write_text(json.dumps({component: entry, 'untouched': {'version': '9.0.0'}}))
            result = subprocess.run([sys.executable, '-', str(path), component, floor, device, activation],
                                    input=SOURCE, text=True, capture_output=True)
            return result, json.loads(path.read_text())

    def test_activation_and_disable_preserve_other_devices(self):
        for value in ('1', '0'):
            result, data = self.promote(value)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(data['lamp-os']['activate_by_device'], {'lamp': value == '1', 'other': True})
            self.assertEqual(data['lamp-os']['min_version_by_device'],
                             {'lamp': '1.2.3', 'other': '1.0.0'} if value == '1' else {'other': '1.0.0'})
            self.assertEqual(data['untouched'], {'version': '9.0.0'})

    def test_ordinary_promotion_preserves_activation(self):
        result, data = self.promote()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(data['lamp-os']['activate_by_device'], {'other': True})

    def test_invalid_activation_is_rejected(self):
        cases = [dict(device=''), dict(component='hal'), dict(activation='yes'),
                 dict(floor='0.0.0'), dict(floor='2.0.0'), dict(floor='invalid'),
                 dict(overrides={'service': 'wrong'}), dict(overrides={'activate_by_device': []}),
                 dict(overrides={'activate_by_device': {'lamp': 'true'}})]
        for case in cases:
            with self.subTest(case=case):
                args = dict(activation='1'); args.update(case)
                result, _ = self.promote(**args)
                self.assertNotEqual(result.returncode, 0)

    def test_direct_script_requires_signing_before_fetch(self):
        env = dict(os.environ, ACTIVATE='1', FOR='lamp')
        env.pop('OTA_SIGNING_PRIVATE_KEY', None)
        env.pop('OTA_SIGNING_KEY_ID', None)
        result = subprocess.run(['bash', str(SCRIPT), 'lamp-os'], env=env, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Activation metadata must be signed', result.stderr)
        self.assertNotIn('Fetch metadata', result.stdout)


if __name__ == '__main__':
    unittest.main()
