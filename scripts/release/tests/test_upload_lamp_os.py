"""Exercise the release command without network, compilation or publication."""
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'upload-lamp-os.sh'
MOCK = r'''
import io, json, os, pathlib, shutil, sys, tarfile
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
root = pathlib.Path(os.environ['MOCK_ROOT'])
with (root / 'calls').open('a') as f:
    f.write(json.dumps([name, *args]) + '\n')
if name == 'git':
    if 'get-url' in args:
        print('https://example.invalid/repo.git')
    elif 'fetch' in args and os.environ.get('FETCH_FAIL'):
        sys.exit(1)
    elif 'rev-parse' in args:
        print('a' * 40)
    elif 'archive' in args:
        with tarfile.open(fileobj=sys.stdout.buffer, mode='w|') as tar:
            for path in ['lampOS/Cargo.lock', 'lampOS/rust-toolchain.toml']:
                data = b'fixture\n'
                item = tarfile.TarInfo(path); item.size = len(data)
                tar.addfile(item, io.BytesIO(data))
elif name == 'cargo' and '--help' not in args:
    if os.environ.get('BUILD_FAIL'):
        sys.exit(1)
    path = pathlib.Path(os.environ['CARGO_TARGET_DIR']) / 'aarch64-unknown-linux-gnu/release/lamp-keeper'
    path.parent.mkdir(parents=True)
    data = bytearray(64); data[:6] = b'\x7fELF\x02\x01'; data[18:20] = (183).to_bytes(2, 'little')
    if os.environ.get('BAD_ARCH'):
        data[18:20] = (62).to_bytes(2, 'little')
    path.write_bytes(data)
elif name == 'gsutil':
    sources = args[args.index('cp') + 1:-1]; dest = args[-1]
    if dest.startswith('gs://'):
        (root / 'destination').write_text(dest)
        for src in sources:
            shutil.copy2(src, root / pathlib.Path(src).name)
    else:
        shutil.copy2(root / 'lamp-os', dest)
        if os.environ.get('CORRUPT_DOWNLOAD'):
            pathlib.Path(dest).write_bytes(b'corrupt')
'''


class UploadLampOSTests(unittest.TestCase):
    def run_case(self, initial_version="0.0.9", cargo_off_path=False, **flags):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = root / 'checkout'
            scripts = checkout / 'scripts/release'; scripts.mkdir(parents=True)
            shutil.copy2(SCRIPT, scripts / SCRIPT.name)
            shutil.copy2(SCRIPT.parent / 'ota-config.sh', scripts / 'ota-config.sh')
            version_file = checkout / 'system/VERSION_LAMP_OS'
            version_file.parent.mkdir(); version_file.write_text(initial_version + '\n')
            bindir = root / 'bin'; bindir.mkdir()
            for name in ('git', 'cargo', 'zig', 'gsutil'):
                tool = bindir / name
                tool.write_text(f'#!{sys.executable}\n' + MOCK)
                tool.chmod(0o755)
            env = dict(os.environ, MOCK_ROOT=str(root), PATH=f'{bindir}:{os.environ["PATH"]}',
                       GCS_BUCKET='test-bucket', BUCKET_PREFIX='test-prefix', **flags)
            if cargo_off_path:
                cargo_home = root / 'rustup-cargo'
                (cargo_home / 'bin').mkdir(parents=True)
                shutil.move(bindir / 'cargo', cargo_home / 'bin/cargo')
                env['CARGO_HOME'] = str(cargo_home)
                env['PATH'] = f'{bindir}:/usr/bin:/bin:/usr/sbin:/sbin'
            result = subprocess.run(['bash', str(scripts / SCRIPT.name)], env=env, text=True, capture_output=True)
            calls = [json.loads(line) for line in (root / 'calls').read_text().splitlines()]
            manifest = json.loads((root / 'build.json').read_text()) if (root / 'build.json').exists() else None
            destination = (root / 'destination').read_text() if (root / 'destination').exists() else ''
            if manifest:
                self.assertEqual(version_file.read_text().strip(), manifest['version'])
                self.assertEqual((root / 'VERSION').read_text().strip(), manifest['version'])
            else:
                self.assertEqual(version_file.read_text().strip(), initial_version)
            return result, calls, manifest, destination

    def test_builds_pinned_branch_and_verifies_binary_without_promoting(self):
        result, calls, manifest, destination = self.run_case()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any('refs/heads/lamp-voice' in call for call in calls))
        self.assertTrue(any('--locked' in call and 'lamp-keeper' in call for call in calls))
        self.assertEqual(manifest['source_commit'], 'a' * 40)
        self.assertFalse(manifest['complete_ota_package'])
        self.assertEqual(manifest['version'], '0.0.10')
        self.assertIn('/builds/0.0.10/', destination)
        self.assertIn('/' + 'a' * 40 + '/' + manifest['sha256'] + '/', destination)
        uploads = [call for call in calls if call[0] == 'gsutil']
        self.assertEqual(len(uploads), 2)
        self.assertFalse(any('metadata.json' in arg for call in uploads for arg in call))

    def test_finds_installed_cargo_outside_shell_path(self):
        result, _, manifest, _ = self.run_case(cargo_off_path=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(manifest['version'], '0.0.10')

    def test_failure_before_valid_binary_never_uploads(self):
        for flag in ('FETCH_FAIL', 'BUILD_FAIL', 'BAD_ARCH'):
            with self.subTest(flag=flag):
                result, calls, _, _ = self.run_case(**{flag: '1'})
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(call[0] == 'gsutil' for call in calls))

    def test_initial_version_and_invalid_version(self):
        result, _, manifest, _ = self.run_case(initial_version='0.0.0')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(manifest['version'], '0.0.1')
        result, calls, _, _ = self.run_case(initial_version='bad-version')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] == 'gsutil' for call in calls))

    def test_corrupt_stored_binary_is_not_reported_as_success(self):
        result, _, _, _ = self.run_case(CORRUPT_DOWNLOAD='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('SHA-256 mismatch', result.stderr)


if __name__ == '__main__':
    unittest.main()
