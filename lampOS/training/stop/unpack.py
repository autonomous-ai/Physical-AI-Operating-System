"""Step 1 of make.sh: unpack what was downloaded into lampOS/.cache/stop/data.

Only the MSWC keywords used here are taken out of its shards. Archives are
read as data: member names are checked to stay inside the target directory.
"""

from __future__ import annotations

import sys
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CACHE = HERE.parent.parent / ".cache" / "stop"
DOWNLOADS = CACHE / "downloads"
DATA = CACHE / "data"

MSWC_WORDS = {"stop", "stopped", "stopping", "stops", "wait", "waited", "waiting", "hold", "holding"}


def safe(base: Path, name: str) -> Path:
    out = (base / name).resolve()
    if not str(out).startswith(str(base.resolve())):
        raise SystemExit(f"refusing archive member outside {base}: {name}")
    return out


def unpack_mswc() -> None:
    """The words used here, all of them; and of every other word in the same
    shards (words near "hold", "stop" and "wait" in the alphabet: "home",
    "start", "step", "walk", "want"...) a few clips, as words that are not
    targets: six a word from a train shard, two from a dev or test shard."""
    out = DATA / "mswc"
    others = DATA / "mswc_other"
    for shard in sorted((DOWNLOADS / "mswc").glob("*_*.tar.gz")):
        taken, other_taken = 0, 0
        each = {}
        most = 6 if shard.name.startswith("train") else 2
        with tarfile.open(shard, "r|gz") as archive:
            for member in archive:
                # Members are named "<word>_common_voice_en_<n>.opus"; the
                # split lists name the same clip "<word>/common_voice_en_<n>.opus".
                name = member.name.strip("/").split("/")[-1]
                word, _, rest = name.partition("_common_voice_")
                if not member.isfile() or not rest:
                    continue
                if word in MSWC_WORDS:
                    target = safe(out, f"{word}/common_voice_{rest}")
                    taken += 1
                elif word.isalpha() and len(word) > 1 and each.get(word, 0) < most:
                    each[word] = each.get(word, 0) + 1
                    target = safe(others, f"{word}/common_voice_{rest}")
                    other_taken += 1
                else:
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.extractfile(member).read())
        print(f"{shard.name}: {taken} clips of the words used, {other_taken} of {len(each)} other words")


def unpack_librispeech(names: list[str]) -> None:
    for name in names:
        archive_path = DOWNLOADS / f"{name}.tar.gz"
        if not archive_path.exists():
            print(f"{name}: not downloaded, skipped")
            continue
        with tarfile.open(archive_path, "r|gz") as archive:
            for member in archive:
                if not member.isfile():
                    continue
                target = safe(DATA, member.name)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.extractfile(member).read())
        print(f"{name}: unpacked")


if __name__ == "__main__":
    what = sys.argv[1:] or ["mswc", "test-clean", "test-other", "dev-clean", "dev-other", "train-clean-100"]
    if "mswc" in what:
        unpack_mswc()
    unpack_librispeech([w for w in what if w != "mswc"])
