import importlib.util
import io
import json
import os
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "amazon_toolkit", Path(__file__).resolve().parent.parent / "scripts" / "amazon_toolkit.py")
at = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(at)

TOP = "selling-partner-agentic-toolkit-" + at.PINNED_COMMIT


def archive(entries):
    """entries: list of (name, bytes | ('symlink', target))."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        for name, body in entries:
            if isinstance(body, tuple):
                ti = tarfile.TarInfo(name)
                ti.type = tarfile.SYMTYPE
                ti.linkname = body[1]
                tf.addfile(ti)
            else:
                ti = tarfile.TarInfo(name)
                ti.size = len(body)
                tf.addfile(ti, io.BytesIO(body))
    return buf.getvalue()


SKILL = b"---\nname: seller-analytics\ndescription: x\n---\n# body\n"


class ToolkitTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name) / "skills"
        at.SKILLS_DIR = self.dir
        self.tarball = Path(self.tmp.name) / "t.tgz"
        os.environ["AMAZON_TOOLKIT_TARBALL"] = str(self.tarball)

    def tearDown(self):
        os.environ.pop("AMAZON_TOOLKIT_TARBALL", None)
        self.tmp.cleanup()

    def run_main(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        old = sys.stdout, sys.stderr
        sys.stdout, sys.stderr = out, err
        try:
            code = at.main(list(argv))
        finally:
            sys.stdout, sys.stderr = old
        return code, out.getvalue(), err.getvalue()

    def good(self, extra=()):
        self.tarball.write_bytes(archive([
            (f"{TOP}/skills/seller-analytics/SKILL.md", SKILL),
            (f"{TOP}/skills/seller-analytics/references/m.md", b"# ref"),
            (f"{TOP}/skills/seller-analytics/eval/evals.json", b"{}"),
            (f"{TOP}/README.md", b"top"),
            (f"{TOP}/commands/sp-fix-listing.md", b"cmd"),
            *extra,
        ]))

    def test_default_target_is_the_folder_that_holds_this_skill(self):
        env = os.environ.pop("AMAZON_TOOLKIT_SKILLS_DIR", None)
        try:
            spec = importlib.util.spec_from_file_location("at2", Path(at.__file__ if hasattr(at, "__file__") else SPEC.origin))
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            self.assertEqual(mod.SKILLS_DIR, Path(SPEC.origin).resolve().parents[2])
        finally:
            if env is not None:
                os.environ["AMAZON_TOOLKIT_SKILLS_DIR"] = env

    def test_install_needs_confirmed(self):
        self.good()
        code, _, err = self.run_main("install")
        self.assertEqual(code, 1)
        self.assertIn("--confirmed", err)
        self.assertFalse(self.dir.exists())

    def test_install_copies_only_skills_with_prefix_and_marker(self):
        self.good()
        code, out, _ = self.run_main("install", "--confirmed")
        self.assertEqual(code, 0, out)
        d = self.dir / "amazon-seller-analytics"
        self.assertTrue((d / "references" / "m.md").is_file())
        self.assertTrue((d / at.MARKER).is_file())
        self.assertIn("name: amazon-seller-analytics", (d / "SKILL.md").read_text())
        self.assertFalse((self.dir / "README.md").exists())
        self.assertEqual([p.name for p in self.dir.iterdir()], ["amazon-seller-analytics"])

    def test_skips_links_traversal_and_non_text_files(self):
        self.good([
            (f"{TOP}/skills/seller-analytics/link.md", ("symlink", "/etc/passwd")),
            (f"{TOP}/skills/seller-analytics/../../evil.md", b"x"),
            (f"{TOP}/skills/seller-analytics/run.sh", b"#!/bin/sh"),
            (f"{TOP}/skills/seller-analytics/tool.py", b"print(1)"),
            (f"{TOP}/skills/Bad Name/SKILL.md", b"x"),
        ])
        self.assertEqual(self.run_main("install", "--confirmed")[0], 0)
        d = self.dir / "amazon-seller-analytics"
        names = sorted(p.name for p in d.rglob("*") if p.is_file())
        self.assertEqual(names, [at.MARKER, "SKILL.md", "evals.json", "m.md"])
        self.assertFalse((self.dir.parent / "evil.md").exists())

    def test_oversized_file_is_skipped(self):
        self.good([(f"{TOP}/skills/seller-analytics/big.md", b"x" * (at.MAX_FILE_BYTES + 1))])
        self.run_main("install", "--confirmed")
        self.assertFalse((self.dir / "amazon-seller-analytics" / "big.md").exists())

    def test_refuses_to_overwrite_a_folder_it_did_not_install(self):
        self.good()
        (self.dir / "amazon-seller-analytics").mkdir(parents=True)
        (self.dir / "amazon-seller-analytics" / "mine.md").write_text("keep")
        code, _, err = self.run_main("install", "--confirmed")
        self.assertEqual(code, 1)
        self.assertIn("not touching", err)
        self.assertTrue((self.dir / "amazon-seller-analytics" / "mine.md").exists())

    def test_reinstall_replaces_and_status_lists(self):
        self.good()
        self.run_main("install", "--confirmed")
        self.run_main("install", "--confirmed")
        status = json.loads(self.run_main("status")[1])
        self.assertEqual(status["installed"], [{"skill": "amazon-seller-analytics", "commit": at.PINNED_COMMIT}])

    def test_remove_only_touches_marked_folders(self):
        self.good()
        self.run_main("install", "--confirmed")
        (self.dir / "other-skill").mkdir()
        (self.dir / "amazon-handmade").mkdir()  # prefix but no marker
        out = json.loads(self.run_main("remove")[1])
        self.assertEqual(out["removed"], ["amazon-seller-analytics"])
        self.assertTrue((self.dir / "other-skill").exists())
        self.assertTrue((self.dir / "amazon-handmade").exists())

    def test_bad_commit_and_bad_archive(self):
        self.good()
        self.assertEqual(self.run_main("install", "--confirmed", "--commit", "main")[0], 1)
        self.tarball.write_bytes(b"not a tarball")
        self.assertEqual(self.run_main("install", "--confirmed")[0], 1)
        self.tarball.write_bytes(archive([(f"{TOP}/README.md", b"x")]))
        self.assertEqual(self.run_main("install", "--confirmed")[0], 1)

    def test_redirects_to_other_hosts_are_refused(self):
        guard = None
        os.environ.pop("AMAZON_TOOLKIT_TARBALL")
        import urllib.request
        # Build the guard the same way fetch_archive does and feed it a hostile redirect.
        orig = urllib.request.build_opener
        captured = {}

        def fake_build(*handlers):
            captured["h"] = handlers[0]
            raise at.Failure("stop before any network call")

        urllib.request.build_opener = fake_build
        try:
            with self.assertRaises(at.Failure):
                at.fetch_archive(at.PINNED_COMMIT)
        finally:
            urllib.request.build_opener = orig
        guard = captured["h"]()
        with self.assertRaises(at.Failure):
            guard.redirect_request(urllib.request.Request("https://codeload.github.com/x"), None, 302, "Found", {}, "https://evil.example/x")
        with self.assertRaises(at.Failure):
            guard.redirect_request(urllib.request.Request("https://codeload.github.com/x"), None, 302, "Found", {}, "http://github.com/x")


if __name__ == "__main__":
    unittest.main()
