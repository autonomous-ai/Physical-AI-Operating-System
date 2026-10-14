"""What the name's training takes from training/stop, and where its own files go.

The front end (features.py), forced alignment (align.py), mixing (mix.py),
sets of clips (store.py), the rule for a hearing (hearing.py) and the
trainer (train.py) are training/stop's, imported from there unchanged
(this folder comes first on the path, so its own phrases.py, speak.py and
the rest are not stop's). Sets
that both tasks use (LibriSpeech and generated talk, Speech Commands and
MSWC other words, AMI) are read from lampOS/.cache/stop; the name's own
sets and inputs are in lampOS/.cache/name.
"""

from __future__ import annotations

import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
STOP = HERE.parent / "stop"
# After this folder, so that a module of the same name here wins.
if str(STOP) not in sys.path:
    sys.path.append(str(STOP))

LAMPOS = HERE.parent.parent
CACHE = LAMPOS / ".cache" / "name"
CLIPS = CACHE / "clips"
INPUTS = CACHE / "inputs"
STOP_CACHE = LAMPOS / ".cache" / "stop"
