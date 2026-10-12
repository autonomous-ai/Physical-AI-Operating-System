"""Check Node compatibility handling without network or package changes."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "spike-agent.sh"
ENGINE = ">=24.16.0 <25 || >=26.1.0"


def shell_function(name):
    match = re.search(
        rf"^{re.escape(name)}\(\) \{{\n.*?^\}}$(?=\n\n|\Z)",
        SCRIPT.read_text(),
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"Missing shell function: {name}")
    return match.group(0)


MOCKS = r'''
event() { printf '%s\n' "$*" >> "$EVENTS"; }
info() { :; }
die() { printf '%s\n' "$*" >&2; exit 1; }
command() {
  if [ "$1" = -v ] && [ "$2" = "$MISSING" ] && [ ! -f "$UPGRADED" ]; then
    return 1
  fi
  builtin command "$@"
}
npm() {
  event "npm $*"
  if [ "$1" = view ]; then
    [ "$FAIL" != metadata ] || return 1
    case "$FAIL" in
      null_metadata) printf '%s\n' 'null' ;;
      empty_metadata) printf '%s\n' '""' ;;
      object_metadata) printf '%s\n' '{}' ;;
      malformed_metadata) printf '%s\n' '{' ;;
      *) printf '%s\n' '">=24.16.0 <25 || >=26.1.0"' ;;
    esac
  fi
}
node() { printf '%s\n' 'v24.16.0'; }
node_satisfies_engine() {
  event "engine $*"
  [ "$FAIL" != checker ] || return 2
  [ "$SCENARIO" = compatible ] && return 0
  [ "$FAIL" != incompatible ] && [ -f "$UPGRADED" ]
}
retry() { eval "$1"; }
curl() { event "curl $*"; [ "$FAIL" != download ]; }
bash() { event "bash $*"; [ "$FAIL" != setup ]; }
apt-get() {
  event "apt-get $*"
  [ "$FAIL" != apt ] || return 1
  touch "$UPGRADED"
}
'''


class NodeSetupTests(unittest.TestCase):
    def run_setup(self, scenario="incompatible", failure="", missing=""):
        functions = "\n".join(shell_function(name) for name in (
            "install_node24", "ensure_openclaw_node"))
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            events = directory / "events"
            result = subprocess.run(
                ["/bin/bash", "-c", "set -euo pipefail\n" + functions + MOCKS
                 + "\nensure_openclaw_node\nnpm install -g openclaw@$NPM_SPEC\n"],
                env={**os.environ, "EVENTS": str(events),
                     "UPGRADED": str(directory / "upgraded"),
                     "TMPDIR": temporary, "SCENARIO": scenario,
                     "FAIL": failure, "MISSING": missing, "NPM_SPEC": "2026.9.3"},
                capture_output=True, text=True,
            )
            return result, events.read_text().splitlines() if events.exists() else []

    def test_compatible_node_skips_upgrade(self):
        result, events = self.run_setup(scenario="compatible")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("npm view openclaw@2026.9.3 engines.node --json", events)
        self.assertIn("npm install -g openclaw@2026.9.3", events)
        self.assertFalse(any(event.startswith(("curl ", "bash ", "apt-get "))
                             for event in events), events)

    def test_incompatible_node_upgrades_then_rechecks(self):
        result, events = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any("setup_24.x" in event for event in events), events)
        checks = [i for i, event in enumerate(events) if event.startswith("engine ")]
        self.assertEqual(len(checks), 2, events)
        upgrade = events.index("apt-get install -y nodejs")
        self.assertLess(checks[0], upgrade)
        self.assertLess(upgrade, checks[1])
        self.assertLess(checks[1], events.index("npm install -g openclaw@2026.9.3"))

    def test_missing_node_or_npm_bootstraps_before_metadata_query(self):
        for missing in ("node", "npm"):
            with self.subTest(missing=missing):
                result, events = self.run_setup(missing=missing)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(events.count("apt-get install -y nodejs"), 1, events)
                self.assertLess(events.index("apt-get install -y nodejs"),
                                events.index("npm view openclaw@2026.9.3 engines.node --json"))
                self.assertIn("npm install -g openclaw@2026.9.3", events)

    def test_failures_stop_before_package_install(self):
        early_failures = ("metadata", "null_metadata", "empty_metadata",
                          "object_metadata", "malformed_metadata", "checker")
        for failure in early_failures + ("download", "setup", "apt", "incompatible"):
            with self.subTest(failure=failure):
                result, events = self.run_setup(failure=failure)
                self.assertNotEqual(result.returncode, 0, events)
                self.assertFalse(any(event.startswith("npm install ") for event in events), events)
                if failure in early_failures:
                    self.assertFalse(any(event.startswith(("curl ", "bash ", "apt-get "))
                                         for event in events), events)
                if failure == "download":
                    self.assertFalse(any(event.startswith(("bash ", "apt-get "))
                                         for event in events), events)
                if failure == "setup":
                    self.assertNotIn("apt-get install -y nodejs", events)


class SemverTests(unittest.TestCase):
    def test_actual_checker_honors_minor_versions_upper_bounds_and_union(self):
        node = shutil.which("node")
        npm = shutil.which("npm")
        if not node or not npm:
            self.skipTest("Local Node and npm are required for the real semver check")
        with tempfile.TemporaryDirectory() as temporary:
            preload = Path(temporary) / "version.cjs"
            preload.write_text(
                "Object.defineProperty(process, 'version', {value: process.env.TEST_NODE_VERSION});\n"
            )
            for version, expected in (("22.23.3", 1), ("24.15.0", 1),
                                      ("24.16.0", 0), ("25.0.0", 1),
                                      ("26.0.0", 1), ("26.1.0", 0)):
                with self.subTest(version=version):
                    result = subprocess.run(
                        ["/bin/bash", "-c", shell_function("node_satisfies_engine")
                         + '\nnode_satisfies_engine "$ENGINE"'],
                        env={**os.environ, "ENGINE": ENGINE,
                             "TEST_NODE_VERSION": "v" + version,
                             "NODE_OPTIONS": f"--require={preload}"},
                        capture_output=True, text=True,
                    )
                    self.assertEqual(result.returncode, expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
