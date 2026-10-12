"""Exercise disk thresholds and recovery instructions without device operations."""
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class DiskSpaceTests(unittest.TestCase):
    def check_space(self, available, required=4194304):
        command = f'''
set -euo pipefail
source {shlex.quote(str(ROOT / "spike-lib.sh"))}
df() {{ printf 'header\\nroot 9999999 0 %s 0%% /\\n' "$AVAILABLE"; }}
require_disk_space {required} "test install"
printf 'continued\\n'
'''
        return subprocess.run(["/bin/bash", "-c", command],
                              env={**os.environ, "AVAILABLE": available},
                              text=True, capture_output=True)

    def test_one_kib_short_is_visible_instead_of_rounded_success(self):
        result = self.check_space("4194303")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("available 4194303 KiB", result.stderr)
        self.assertIn("required 4194304 KiB (4096.000 MiB)", result.stderr)
        self.assertIn("shortfall 1 KiB (0.001 MiB)", result.stderr)
        self.assertNotIn("continued", result.stdout)

    def test_exact_thresholds_pass_and_one_kib_below_fails(self):
        for required in (1048576, 1572864, 4194304):
            with self.subTest(required=required):
                self.assertEqual(self.check_space(str(required), required).returncode, 0)
                self.assertNotEqual(self.check_space(str(required - 1), required).returncode, 0)

    def test_invalid_disk_reading_fails_closed(self):
        for available in ("", "unknown", "-1"):
            with self.subTest(available=available):
                result = self.check_space(available)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("invalid free-space reading", result.stderr)

    def test_hal_selects_fresh_incremental_and_no_deps_thresholds(self):
        script = (ROOT / "spike-hal.sh").read_text()
        preflight = script.split('say "1/4  Preflight: disk space"', 1)[1].split('say "2/4', 1)[0]
        with tempfile.TemporaryDirectory() as directory:
            venv = Path(directory) / ".venv/bin/uvicorn"
            for installed, skip, expected in ((False, "0", "4194304"),
                                              (True, "0", "1048576"),
                                              (True, "1", "")):
                if installed:
                    venv.parent.mkdir(parents=True, exist_ok=True)
                    venv.touch()
                    venv.chmod(0o755)
                result = subprocess.run(
                    ["/bin/bash", "-c", 'set -eu\nrequire_disk_space() { echo "$1"; }\n' + preflight],
                    env={**os.environ, "HAL_DIR": directory, "SKIP_DEPS": skip},
                    text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), expected)


class ResumeTests(unittest.TestCase):
    def run_installer(self, failed, args=()):
        with tempfile.TemporaryDirectory(prefix="reachy test ") as directory:
            folder = Path(directory)
            (folder / "spike.sh").write_text((ROOT / "spike.sh").read_text())
            (folder / "spike-lib.sh").write_text('''
DEVICE_TYPE=reachy-mini
info() { echo "$*" >&2; }
say() { :; }
ensure_root() { :; }
ensure_tools() { :; }
clear_metadata_cache() { :; }
metadata_url() { echo unused; }
''')
            for step in ("device", "hal", "os", "web", "agent", "bootstrap"):
                (folder / f"spike-{step}.sh").write_text(
                    f'echo "{step}:$*" >> "$EVENTS"\n'
                    f'[ "$FAIL_STEP" != "{step}" ] || exit 17\n')
            result = subprocess.run(
                ["/bin/bash", str(folder / "spike.sh"), *args],
                env={**os.environ, "FAIL_STEP": failed, "EVENTS": str(folder / "events")},
                text=True, capture_output=True)
            events = (folder / "events").read_text().splitlines()
            commands = [shlex.split(line.strip()) for line in result.stderr.splitlines()
                        if line.startswith("  sudo bash ")]
            return result, events, commands, folder

    def test_agent_failure_stops_and_only_prints_agent_and_bootstrap(self):
        result, events, commands, folder = self.run_installer("agent")
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(events, ["device:", "hal:", "os:", "web:", "agent:"])
        self.assertEqual(commands, [["sudo", "bash", str(folder / f"spike-{s}.sh")]
                                    for s in ("agent", "bootstrap")])

    def test_resume_preserves_no_deps_and_omits_skipped_steps(self):
        result, events, commands, folder = self.run_installer("hal", ("--no-deps", "--skip", "web"))
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(events, ["device:", "hal:--no-deps"])
        self.assertEqual(commands, [
            ["sudo", "bash", str(folder / "spike-hal.sh"), "--no-deps"],
            *[["sudo", "bash", str(folder / f"spike-{s}.sh")]
              for s in ("os", "agent", "bootstrap")],
        ])

    def test_last_step_failure_prints_only_last_step(self):
        result, _, commands, folder = self.run_installer("bootstrap")
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(commands, [["sudo", "bash", str(folder / "spike-bootstrap.sh")]])


if __name__ == "__main__":
    unittest.main()
