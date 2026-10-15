#!/usr/bin/env python3
"""Optional: install Amazon's Selling Partner agent skills after the Amazon MCP is connected.

The skills (seller analytics, listing fixes, stockout checks, FBA inbound, ...) are published by Amazon as
plain markdown in https://github.com/amzn/selling-partner-agentic-toolkit (Apache-2.0). They are optional:
the Amazon MCP works without them. Only run `install` after the user said yes.

    python3 scripts/amazon_toolkit.py status
    python3 scripts/amazon_toolkit.py install --confirmed [--commit <40-hex sha>]
    python3 scripts/amazon_toolkit.py remove

Safety: downloads only from GitHub over https at a pinned commit, copies only .md/.json files, never executes
anything, skips links and path tricks, caps sizes, and writes only to the agent's skills folder under an
`amazon-` prefix with a marker file so `remove` touches nothing else.
"""
import io
import json
import os
import re
import shutil
import sys
import tarfile
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

REPO = "amzn/selling-partner-agentic-toolkit"
PINNED_COMMIT = "8d90d3b7c1537254b53c82651ac31fdbf0089368"
ALLOWED_HOSTS = ("codeload.github.com", "github.com")
# Install next to this skill: <skills root>/connectors/scripts/amazon_toolkit.py -> <skills root>. That is the
# folder the agent runtime actually reads (OpenClaw and Hermes keep their skills in different places).
SKILLS_DIR = Path(os.environ.get("AMAZON_TOOLKIT_SKILLS_DIR") or Path(__file__).resolve().parents[2])
PREFIX = "amazon-"
MARKER = ".amazon-toolkit.json"
ALLOWED_SUFFIXES = (".md", ".json")
MAX_ARCHIVE_BYTES = 8 * 1024 * 1024
MAX_FILE_BYTES = 512 * 1024
MAX_FILES = 400
SKILL_NAME = re.compile(r"^[a-z0-9][a-z0-9-]{0,63}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")


class Failure(Exception):
    pass


def fetch_archive(commit):
    """Return the tar.gz bytes for `commit`. AMAZON_TOOLKIT_TARBALL (a local path) replaces the download, for tests."""
    local = os.environ.get("AMAZON_TOOLKIT_TARBALL")
    if local:
        return Path(local).read_bytes()
    url = f"https://codeload.github.com/{REPO}/tar.gz/{commit}"

    class _Guard(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            parsed = urllib.parse.urlparse(newurl)
            if parsed.scheme != "https" or parsed.hostname not in ALLOWED_HOSTS:
                raise Failure(f"refusing a redirect to {parsed.hostname}")
            return super().redirect_request(req, fp, code, msg, headers, newurl)

    opener = urllib.request.build_opener(_Guard)
    try:
        with opener.open(urllib.request.Request(url, headers={"User-Agent": "autonomous-os-amazon-toolkit"}), timeout=60) as r:
            data = r.read(MAX_ARCHIVE_BYTES + 1)
    except (urllib.error.URLError, TimeoutError) as e:
        raise Failure(f"download failed: {e}")
    if len(data) > MAX_ARCHIVE_BYTES:
        raise Failure("archive is larger than the 8 MB limit")
    return data


def select_files(archive):
    """Yield (skill, relative path, bytes) for the safe files under skills/<skill>/."""
    out, total = [], 0
    try:
        tf = tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz")
    except tarfile.TarError as e:
        raise Failure(f"not a valid archive: {e}")
    with tf:
        for m in tf:
            if not m.isfile():  # skips directories, symlinks, hardlinks, devices
                continue
            parts = Path(m.name).parts
            # GitHub tarballs have one top folder: <repo>-<sha>/skills/<skill>/<path...>
            if len(parts) < 4 or parts[1] != "skills":
                continue
            skill, rel = parts[2], Path(*parts[3:])
            if not SKILL_NAME.match(skill) or any(p in ("..", "") for p in rel.parts) or rel.is_absolute():
                continue
            if rel.suffix.lower() not in ALLOWED_SUFFIXES or m.size > MAX_FILE_BYTES:
                continue
            f = tf.extractfile(m)
            if f is None:
                continue
            body = f.read()
            total += len(body)
            if len(out) + 1 > MAX_FILES or total > MAX_ARCHIVE_BYTES:
                raise Failure("archive has more files or bytes than allowed")
            out.append((skill, rel, body))
    if not out:
        raise Failure("no skills found in the archive")
    return out


def prefixed_skill_md(body, skill):
    """Rename `name:` in the SKILL.md front matter so it matches the prefixed folder."""
    text = body.decode("utf-8", "replace")
    return re.sub(r"(?m)^name:\s*.*$", f"name: {PREFIX}{skill}", text, count=1).encode("utf-8")


def installed():
    if not SKILLS_DIR.is_dir():
        return []
    found = []
    for d in sorted(SKILLS_DIR.iterdir()):
        m = d / MARKER
        if d.name.startswith(PREFIX) and m.is_file():
            try:
                found.append((d.name, json.loads(m.read_text()).get("commit", "?")))
            except (OSError, ValueError):
                found.append((d.name, "?"))
    return found


def cmd_status():
    items = installed()
    print(json.dumps({"installed": [{"skill": n, "commit": c} for n, c in items], "skills_dir": str(SKILLS_DIR),
                      "pinned_commit": PINNED_COMMIT}, indent=2))
    return 0


def cmd_install(commit):
    files = select_files(fetch_archive(commit))
    SKILLS_DIR.mkdir(parents=True, exist_ok=True)
    skills = sorted({s for s, _, _ in files})
    for skill in skills:
        dest = SKILLS_DIR / f"{PREFIX}{skill}"
        if dest.exists() and not (dest / MARKER).is_file():
            raise Failure(f"{dest.name} exists and was not installed by this tool; not touching it")
    for skill in skills:
        dest = SKILLS_DIR / f"{PREFIX}{skill}"
        if dest.exists():
            shutil.rmtree(dest)
        for s, rel, body in files:
            if s != skill:
                continue
            target = dest / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(prefixed_skill_md(body, skill) if rel.as_posix() == "SKILL.md" else body)
        (dest / MARKER).write_text(json.dumps({"source": f"github.com/{REPO}", "commit": commit, "license": "Apache-2.0"}))
    print(json.dumps({"installed": [f"{PREFIX}{s}" for s in skills], "commit": commit, "skills_dir": str(SKILLS_DIR)}, indent=2))
    return 0


def cmd_remove():
    removed = []
    for name, _ in installed():
        shutil.rmtree(SKILLS_DIR / name)
        removed.append(name)
    print(json.dumps({"removed": removed}, indent=2))
    return 0


def main(argv):
    try:
        if argv[:1] == ["status"]:
            return cmd_status()
        if argv[:1] == ["remove"]:
            return cmd_remove()
        if argv[:1] == ["install"]:
            if "--confirmed" not in argv:
                raise Failure("install needs --confirmed, and only after the user agreed to install Amazon's skills")
            commit = PINNED_COMMIT
            if "--commit" in argv:
                commit = argv[argv.index("--commit") + 1] if argv.index("--commit") + 1 < len(argv) else ""
            if not COMMIT.match(commit):
                raise Failure("--commit must be a full 40-character commit sha")
            return cmd_install(commit)
        print(__doc__)
        return 2
    except Failure as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
