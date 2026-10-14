"""A recording of the microphone's sound on the bus, as a WAV file `lamp-name check` and listen.py read.

    python from_bus.py RECORD.jsonl OUT.wav

RECORD.jsonl is what `lamp-bus record DIR mic2/audio --seconds N` writes
(`DIR/mic2_audio.jsonl`): one line a message, its payload in base64, each
payload a JSON header, a line end, then 16-bit samples. A block that was
lost (a step of more than one in `sequence`) is written as silence, so that
times in the file stay times on the lamp.
"""

from __future__ import annotations

import base64
import json
import sys
import wave

import numpy as np


def main() -> None:
    source, target = sys.argv[1], sys.argv[2]
    parts = []
    last = None
    lost = 0
    for line in open(source):
        row = json.loads(line)
        if "payload_b64" not in row:
            continue
        payload = base64.b64decode(row["payload_b64"])
        head, _, samples = payload.partition(b"\n")
        header = json.loads(head)
        if header["sample_rate"] != 16_000 or header["channels"] != 1 or header["sample_format"] != "S16LE":
            raise SystemExit(f"not 16 kHz mono S16LE: {header}")
        block = np.frombuffer(samples, "<i2")
        if last is not None and header["sequence"] > last + 1:
            missing = header["sequence"] - last - 1
            lost += missing
            parts.append(np.zeros(missing * len(block), np.int16))
        last = header["sequence"]
        parts.append(block)
    sound = np.concatenate(parts) if parts else np.zeros(0, np.int16)
    with wave.open(target, "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(16_000)
        out.writeframes(sound.astype("<i2").tobytes())
    print(f"{target}: {len(sound) / 16_000:.1f} s, {lost} blocks lost and written as silence")


if __name__ == "__main__":
    main()
