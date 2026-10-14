"""The numbers the lamp's listeners work on, made exactly as `lamp-name` makes them.

`crates/name/src/hear.rs` runs openWakeWord's two fixed models every 80 ms:

1. `melspectrogram.onnx` on the newest 1,760 samples (the step and the 30 ms
   before it): eight frames of 32 numbers, one for every 10 ms. Each number
   is then scaled as `x / 10 + 2`.
2. `embedding_model.onnx` on 76 frames: 96 numbers. Embedding k is of frames
   8k to 8k + 76, so a new one comes every 80 ms.
3. The newest sixteen embeddings, `[1, 16, 96]`, go to the last model.

The samples are given to the first model on the scale of 16-bit sound
(-32768 to 32767), not -1 to 1.

Here the same is done for a whole recording at once, in batches: chunk i
is samples 1280 i to 1280 i + 1760, exactly as the loop in `hear.rs` cuts
them. `check_against_streaming` runs the step-by-step version beside it.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

import numpy as np
import onnxruntime as ort

# The cut-short mel model leaves one constant unused, which onnxruntime warns of.
ort.set_default_logger_severity(3)

STEP = 1_280
CHUNK = 1_760
BINS = 32
FRAMES_A_STEP = 8
FRAMES = 76
EMBEDDING = 96
CONTEXT = 16
# Chunks and samples one window of sixteen embeddings needs: the last
# embedding ends at frame 8 * 15 + 76 = 196, in chunk 25. 32,480 samples is
# the "first 2.03 s" after which `lamp-name check` gives its first score.
CHUNKS_A_WINDOW = -(-(FRAMES_A_STEP * (CONTEXT - 1) + FRAMES) // FRAMES_A_STEP)
WINDOW = STEP * (CHUNKS_A_WINDOW - 1) + CHUNK
assert (CHUNKS_A_WINDOW, WINDOW) == (25, 32_480), WINDOW

PINNED = {
    "melspectrogram.onnx": "ba2b0e0f8b7b875369a2c89cb13360ff53bac436f2895cced9f479fa65eb176f",
    "embedding_model.onnx": "70d164290c1d095d1d4ee149bc5e00543250a7316b59f31d056cff7bd3075c1f",
}


def _without_floor(path: Path) -> bytes:
    """The mel model, ending at its log-mel numbers before the floor at 80 dB under the largest."""
    import onnx

    model = onnx.load(str(path))
    last = "onnx::ReduceMax_31"
    keep = []
    for node in model.graph.node:
        keep.append(node)
        if last in node.output:
            break
    else:
        raise SystemExit(f"{path}: not the mel model this was written for (no {last})")
    del model.graph.node[:]
    model.graph.node.extend(keep)
    output = onnx.helper.make_tensor_value_info(last, onnx.TensorProto.FLOAT, None)
    del model.graph.output[:]
    model.graph.output.extend([output])
    return model.SerializeToString()


def sha256(path: Path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class FrontEnd:
    """openWakeWord v0.5.1's mel and embedding models, checked against their pinned hashes."""

    def __init__(self, directory: Path, threads: int = 6):
        options = ort.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        sessions = []
        for name, pinned in PINNED.items():
            path = Path(directory) / name
            found = sha256(path)
            if found != pinned:
                raise SystemExit(f"{path}: SHA-256 {found}, not the pinned {pinned}")
            sessions.append(ort.InferenceSession(str(path), options, providers=["CPUExecutionProvider"]))
        self.mel, self.embedding = sessions
        self.mel_input = self.mel.get_inputs()[0].name
        self.embedding_input = self.embedding.get_inputs()[0].name
        # The mel model ends by flooring every number at 80 dB under the
        # largest in the whole tensor it is given. The lamp gives it one
        # chunk at a time, so in a batch that floor would be the batch's,
        # not the chunk's. The same model cut short before that floor runs
        # on batches here, and the floor is put back for each chunk alone
        # (`chunk_mel`); `check_against_streaming` compares the two.
        self.mel_unfloored = ort.InferenceSession(
            _without_floor(Path(directory) / "melspectrogram.onnx"), options, providers=["CPUExecutionProvider"]
        )

    def chunk_mel(self, chunks: np.ndarray) -> np.ndarray:
        """`melspectrogram.onnx` of each chunk `[b, 1760]` as if run on it alone: `[b, 8, 32]`, unscaled."""
        mel = self.mel_unfloored.run(None, {self.mel_input: np.asarray(chunks, np.float32)})[0]
        mel = mel.reshape(len(chunks), FRAMES_A_STEP, BINS)
        floor = mel.max(axis=(1, 2), keepdims=True) - np.float32(80.0)
        return np.maximum(mel, floor)

    def frames(self, samples: np.ndarray) -> np.ndarray:
        """Mel frames, scaled, `[n_chunks * 8, 32]`, for as many whole chunks as the samples hold."""
        samples = np.asarray(samples, dtype=np.float32)
        chunks = 0 if len(samples) < CHUNK else (len(samples) - CHUNK) // STEP + 1
        if chunks == 0:
            return np.zeros((0, BINS), np.float32)
        index = np.arange(chunks)[:, None] * STEP + np.arange(CHUNK)[None, :]
        out = []
        for start in range(0, chunks, 512):
            batch = samples[index[start : start + 512]]
            out.append(self.chunk_mel(batch))
        mel = np.concatenate(out).reshape(-1, BINS)
        return mel / 10.0 + 2.0

    def embeddings(self, frames: np.ndarray) -> np.ndarray:
        """Embedding k of frames 8k to 8k + 76, `[n, 96]`."""
        count = 0 if len(frames) < FRAMES else (len(frames) - FRAMES) // FRAMES_A_STEP + 1
        if count == 0:
            return np.zeros((0, EMBEDDING), np.float32)
        index = np.arange(count)[:, None] * FRAMES_A_STEP + np.arange(FRAMES)[None, :]
        out = []
        for start in range(0, count, 1024):
            batch = frames[index[start : start + 1024]][..., None].astype(np.float32)
            out.append(self.embedding.run(None, {self.embedding_input: batch})[0].reshape(-1, EMBEDDING))
        return np.concatenate(out)

    def of(self, samples: np.ndarray) -> np.ndarray:
        """All embeddings of a recording, `[n, 96]`: embedding k is the one `hear.rs` makes at step k + 9."""
        return self.embeddings(self.frames(samples))

    def many(self, windows: np.ndarray) -> np.ndarray:
        """Windows of exactly `WINDOW` samples each, `[b, WINDOW]`, to `[b, 16, 96]`: one model input each."""
        windows = np.asarray(windows, dtype=np.float32)
        b = len(windows)
        assert windows.shape[1] == WINDOW
        c = CHUNKS_A_WINDOW
        index = np.arange(c)[:, None] * STEP + np.arange(CHUNK)[None, :]
        out = np.zeros((b, CONTEXT, EMBEDDING), np.float32)
        group = 64
        for start in range(0, b, group):
            batch = windows[start : start + group]
            n = len(batch)
            chunks = batch[:, index].reshape(n * c, CHUNK)
            mel = self.chunk_mel(chunks).reshape(n, c * FRAMES_A_STEP, BINS) / 10.0 + 2.0
            eindex = np.arange(CONTEXT)[:, None] * FRAMES_A_STEP + np.arange(FRAMES)[None, :]
            stacks = mel[:, eindex].reshape(n * CONTEXT, FRAMES, BINS, 1).astype(np.float32)
            emb = self.embedding.run(None, {self.embedding_input: stacks})[0]
            out[start : start + n] = emb.reshape(n, CONTEXT, EMBEDDING)
        return out


def stacks(embeddings: np.ndarray) -> np.ndarray:
    """Every model input a recording gives, `[n - 15, 16, 96]`: input j is embeddings j to j + 15."""
    n = len(embeddings) - CONTEXT + 1
    if n <= 0:
        return np.zeros((0, CONTEXT, EMBEDDING), np.float32)
    index = np.arange(n)[:, None] + np.arange(CONTEXT)[None, :]
    return embeddings[index]


def score_time(j: int) -> float:
    """Seconds into a recording at which `lamp-name` gives its score for input j (as `check` prints)."""
    return 2.03 + j * 0.08


def check_against_streaming(front: FrontEnd, samples: np.ndarray) -> float:
    """The largest difference between the batch embeddings and the step-by-step loop of `hear.rs`."""
    pending = np.zeros(0, np.float32)
    frames: list[np.ndarray] = []
    frames_gone = 0
    made = 0
    streamed = []
    for block in np.array_split(np.asarray(samples, np.float32), max(1, len(samples) // 320)):
        pending = np.concatenate([pending, block])
        while len(pending) >= CHUNK:
            mel = front.mel.run(None, {front.mel_input: pending[None, :CHUNK]})[0].reshape(FRAMES_A_STEP, BINS)
            frames.extend(list(mel / 10.0 + 2.0))
            pending = pending[STEP:]
            while FRAMES_A_STEP * made + FRAMES <= frames_gone + len(frames):
                start = FRAMES_A_STEP * made - frames_gone
                flat = np.stack(frames[start : start + FRAMES])[None, :, :, None].astype(np.float32)
                streamed.append(front.embedding.run(None, {front.embedding_input: flat})[0].reshape(EMBEDDING))
                made += 1
                while frames_gone < FRAMES_A_STEP * made:
                    frames.pop(0)
                    frames_gone += 1
    batch = front.of(samples)
    streamed = np.stack(streamed)
    assert batch.shape == streamed.shape, (batch.shape, streamed.shape)
    return float(np.abs(batch - streamed).max())
