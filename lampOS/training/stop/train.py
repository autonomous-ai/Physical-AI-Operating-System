"""Step 5 of make.sh: train the listener and write `stop.onnx`.

    python train.py [--out PATH]

With no options it makes the model in models/stop (docs/stop-word.md):
the "time" shape, 64 filters, 12,000 steps, targets that are alone, through
the suppressor, or plainly 3 dB and more over the other voice, negatives
weighed 3 to 1, no extra draw of hard negatives. The other options are the
ones tried before it.

The model is small and plain, so that tract runs it as it runs name.onnx.
"time" (the one chosen):

    input [1, 16, 96] -> Transpose -> Conv (96 -> H, 3 wide) -> Relu -> Conv (H -> H, 3 wide)
                      -> Relu -> ReduceMax over time -> Gemm (H -> 1) -> Sigmoid -> output [1, 1]

"flat":

    input [1, 16, 96] -> Flatten -> Gemm (1536 -> H) -> Relu -> Gemm (H -> 64) -> Relu
                      -> Gemm (64 -> 1) -> Sigmoid -> output [1, 1]

opset 13. The shift and scale the embeddings are given while training are
folded into the first layer, so the file takes the embeddings exactly as
`hear.rs` makes them.

Each batch is a quarter target inputs, a quarter inputs with near misses or
other words, and half inputs of talk alone or of two voices. With `--hard 1`,
after half the steps the negatives the model then scores highest are drawn
as often as the rest. `--lowest` leaves out the targets put more than that
many dB under the other voice; `--heard` those plainly mixed under it.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
from pathlib import Path

import numpy as np

from hearing import hearings
from store import CACHE

THREADS = int(os.environ.get("OMP_NUM_THREADS", "6"))
import torch  # noqa: E402

torch.set_num_threads(THREADS)

INPUTS = CACHE / "inputs"
HERE = Path(__file__).resolve().parent
MODEL_DIR = HERE.parent.parent / "models" / "stop"


def load(pattern: str, folder: Path = INPUTS) -> dict:
    parts = [np.load(p) for p in sorted(glob.glob(str(folder / pattern)))]
    out = {}
    for key in parts[0].files:
        if key != "names":
            out[key] = np.concatenate([p[key] for p in parts])
    return out


def steps_since_first(x: np.ndarray) -> np.ndarray:
    """For each input, how many 80 ms steps after the first input of its example it is.

    The inputs of one example are kept in order, 80 ms apart, so each holds
    fifteen of the sixteen embeddings of the one before it. A target
    example's first input sees 0.02 to 0.10 s past the word's end, so this
    says how long ago the word ended.
    """
    same = np.zeros(len(x), bool)
    for i in range(1, len(x), 65536):
        j = min(len(x), i + 65536)
        same[i:j] = np.all(x[i:j, :15] == x[i - 1 : j - 1, 1:], axis=(1, 2))
    out = np.zeros(len(x), np.int16)
    for i in range(1, len(x)):
        if same[i]:
            out[i] = out[i - 1] + 1
    return out


class Listener(torch.nn.Module):
    """`flat`: two Gemm layers on all sixteen embeddings at once. `time`: the
    same small filters slid along the sixteen (two 1-D convolutions, three
    embeddings wide), the largest of each filter's answers kept, then one Gemm."""

    def __init__(self, hidden: int, mean: np.ndarray, scale: np.ndarray, shape: str = "flat"):
        super().__init__()
        self.shape = shape
        self.register_buffer("mean", torch.from_numpy(mean.astype(np.float32)))
        self.register_buffer("scale", torch.from_numpy(scale.astype(np.float32)))
        if shape == "flat":
            self.first = torch.nn.Linear(16 * 96, hidden)
            self.second = torch.nn.Linear(hidden, 64)
            self.last = torch.nn.Linear(64, 1)
        else:
            self.first = torch.nn.Conv1d(96, hidden, 3)
            self.second = torch.nn.Conv1d(hidden, hidden, 3)
            self.last = torch.nn.Linear(hidden, 1)
        self.drop = torch.nn.Dropout(0.2)

    def logits(self, x: torch.Tensor) -> torch.Tensor:
        x = (x - self.mean) / self.scale
        if self.shape == "flat":
            x = x.reshape(len(x), -1)
            x = self.drop(torch.relu(self.first(x)))
            x = self.drop(torch.relu(self.second(x)))
            return self.last(x)[:, 0]
        x = x.transpose(1, 2)
        x = self.drop(torch.relu(self.first(x)))
        x = self.drop(torch.relu(self.second(x)))
        return self.last(x.amax(dim=2))[:, 0]


def export(model: Listener, path: Path) -> None:
    """Write the model as plain ONNX ops, the shift and scale folded into the first layer."""
    import onnx
    from onnx import TensorProto, helper, numpy_helper

    mean = model.mean.numpy()
    scale = model.scale.numpy()
    w1 = model.first.weight.detach().numpy()
    b1 = model.first.bias.detach().numpy()
    if model.shape == "flat":
        mean16 = np.broadcast_to(mean, (16, 96)).reshape(-1)
        w1 = w1 / np.broadcast_to(scale, (16, 96)).reshape(-1)[None, :]
        b1 = b1 - w1 @ mean16
    else:
        w1 = w1 / scale[None, :, None]
        b1 = b1 - (w1 * mean[None, :, None]).sum(axis=(1, 2))
    weights = {
        "w1": w1, "b1": b1,
        "w2": model.second.weight.detach().numpy(), "b2": model.second.bias.detach().numpy(),
        "w3": model.last.weight.detach().numpy(), "b3": model.last.bias.detach().numpy(),
    }
    inits = [numpy_helper.from_array(v.astype(np.float32), k) for k, v in weights.items()]
    if model.shape == "flat":
        nodes = [
            helper.make_node("Flatten", ["input"], ["flat"], axis=1),
            helper.make_node("Gemm", ["flat", "w1", "b1"], ["h1"], transB=1),
            helper.make_node("Relu", ["h1"], ["r1"]),
            helper.make_node("Gemm", ["r1", "w2", "b2"], ["h2"], transB=1),
            helper.make_node("Relu", ["h2"], ["r2"]),
            helper.make_node("Gemm", ["r2", "w3", "b3"], ["h3"], transB=1),
            helper.make_node("Sigmoid", ["h3"], ["output"]),
        ]
    else:
        nodes = [
            helper.make_node("Transpose", ["input"], ["channels"], perm=[0, 2, 1]),
            helper.make_node("Conv", ["channels", "w1", "b1"], ["h1"], kernel_shape=[3]),
            helper.make_node("Relu", ["h1"], ["r1"]),
            helper.make_node("Conv", ["r1", "w2", "b2"], ["h2"], kernel_shape=[3]),
            helper.make_node("Relu", ["h2"], ["r2"]),
            helper.make_node("ReduceMax", ["r2"], ["most"], axes=[2], keepdims=0),
            helper.make_node("Gemm", ["most", "w3", "b3"], ["h3"], transB=1),
            helper.make_node("Sigmoid", ["h3"], ["output"]),
        ]
    graph = helper.make_graph(
        nodes,
        "stop",
        [helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 16, 96])],
        [helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 1])],
        inits,
    )
    onnx_model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)], producer_name="lampOS training/stop")
    onnx_model.ir_version = 8
    onnx.checker.check_model(onnx_model)
    path.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(onnx_model, str(path))


def scores_of(model: Listener, x: torch.Tensor) -> np.ndarray:
    model.eval()
    out = []
    with torch.inference_mode():
        for i in range(0, len(x), 16384):
            out.append(torch.sigmoid(model.logits(x[i : i + 16384].float())).numpy())
    model.train()
    return np.concatenate(out) if out else np.zeros(0)


def validate(model, check: dict, check_x: torch.Tensor, talk_x: torch.Tensor) -> dict:
    """On held-out speakers: the share of target examples whose best score of their nine inputs
    reaches each threshold, by level, and hearings an hour in an hour of held-out readers."""
    s = scores_of(model, check_x)
    talk = scores_of(model, talk_x)
    hours = len(talk) * 0.08 / 3600
    pos = np.flatnonzero(check["y"] == 1)
    example = np.cumsum(check["since"][pos] == 0) - 1
    best = np.full(example[-1] + 1, -1.0)
    np.maximum.at(best, example, s[pos])
    level = np.zeros(len(best), np.float32)
    level[example] = check["level"].astype(np.float32)[pos]
    cut = np.zeros(len(best), bool)
    cut[example] = check["suppressed"][pos] == 1
    bands = {
        "alone": level > 50,
        "plain 0..6": ~cut & (level >= 0) & (level <= 6),
        "plain -10..0": ~cut & (level >= -10) & (level < 0),
        "plain -20..-10": ~cut & (level < -10),
        "suppressed 0..6": cut & (level >= 0) & (level <= 6),
        "suppressed -10..0": cut & (level >= -10) & (level < 0),
        "suppressed -20..-10": cut & (level < -10),
    }
    out = {}
    for least in [0.5, 0.8, 0.9, 0.95, 0.98, 0.99]:
        row = {"fa_per_h": len(hearings(talk, least)) / hours}
        for band, sel in bands.items():
            row[band] = float(np.mean(best[sel] >= least)) if sel.any() else float("nan")
        out[least] = row
    return out


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--hidden", type=int, default=64)
    p.add_argument("--shape", choices=["flat", "time"], default="time")
    p.add_argument("--hard", type=int, default=0, help="draw the negatives scored highest more often after half the steps (1) or not (0)")
    p.add_argument("--steps", type=int, default=12000)
    p.add_argument("--batch", type=int, default=1024)
    p.add_argument("--lowest", type=float, default=-20.0)
    p.add_argument("--latest", type=int, default=8, help="train on target inputs up to this many steps after the first")
    p.add_argument("--heard", type=float, default=3.0, help="leave out targets mixed plainly (no suppressor) lower than this many dB")
    p.add_argument("--against", type=float, default=3.0, help="weight of a negative input against a target input in the loss")
    p.add_argument("--out", default=str(MODEL_DIR / "stop.onnx"))
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--inputs", default=str(INPUTS), help="the folder of inputs (training/name passes its own)")
    p.add_argument("--leave-out-way", type=int, default=0, help="training/name: leave out the calls of this way (3, a bare Lamp)")
    args = p.parse_args()
    torch.manual_seed(args.seed)
    rng = np.random.default_rng(args.seed)

    folder = Path(args.inputs)
    words = load("words-*.npz", folder)
    talk = load("talk-*.npz", folder)
    check = load("check-[0-9]*.npz", folder)
    check["since"] = steps_since_first(check["x"])
    check_talk = load("check-talk-*.npz", folder)
    wy = words["y"]
    wl = words["level"].astype(np.float32)
    since = steps_since_first(words["x"])
    kept = (wy == 1) & (wl >= args.lowest) & (since <= args.latest)
    if args.heard is not None:
        # Only targets a listener could hear: alone, through the suppressor,
        # or plainly mixed no lower than `--heard` dB.
        kept &= (wl > 50) | (words["suppressed"] == 1) | (wl >= args.heard)
    if args.leave_out_way and "way" in words:
        # Targets of one way of calling left out altogether (training/name).
        leave = words["way"] == args.leave_out_way
        kept &= ~leave
        wy = np.where(leave, -1, wy)
    pos = np.flatnonzero(kept)
    near = np.flatnonzero(wy == 0)
    wx = torch.from_numpy(words["x"])
    tx = torch.from_numpy(talk["x"])
    cx = torch.from_numpy(check["x"])
    ctx = torch.from_numpy(check_talk["x"])
    print(
        f"train: {len(pos)} target inputs (of {(wy == 1).sum()}, from {args.lowest} dB up), {len(near)} other words, "
        f"{len(tx)} talk; check: {int((check['y'] == 1).sum())} targets, {len(ctx) * 0.08 / 3600:.2f} h talk",
        flush=True,
    )

    sample = np.concatenate([words["x"][rng.choice(len(wy), 20000)], talk["x"][rng.choice(len(tx), 20000)]]).astype(np.float32)
    mean = sample.mean(axis=(0, 1))
    scale = sample.std(axis=(0, 1)) + 1e-3
    model = Listener(args.hidden, mean, scale, args.shape)
    opt = torch.optim.AdamW(model.parameters(), lr=1e-3, weight_decay=1e-3)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=2e-3, total_steps=args.steps, pct_start=0.1)
    loss_fn = torch.nn.BCEWithLogitsLoss(reduction="none")
    noise_scale = torch.from_numpy(scale.astype(np.float32)) * 0.05
    q = args.batch // 4
    hard_talk = hard_near = None
    for step in range(args.steps):
        if args.hard and step == args.steps // 2:
            ts = scores_of(model, tx)
            ns = scores_of(model, wx[near])
            hard_talk = np.argsort(ts)[-max(1000, len(ts) // 50) :]
            order = np.argsort(ns)[-max(1000, len(ns) // 50) :]
            hard_near = near[order]
            print(f"step {step}: hard negatives from talk score {ts[hard_talk].min():.3f} and up, from words {ns[order].min():.3f} and up", flush=True)
        bp = wx[rng.choice(pos, q)]
        if hard_near is not None:
            bn = torch.cat([wx[rng.choice(near, q // 2)], wx[rng.choice(hard_near, q - q // 2)]])
            bt = torch.cat([tx[rng.choice(len(tx), q)], tx[rng.choice(hard_talk, q)]])
        else:
            bn = wx[rng.choice(near, q)]
            bt = tx[rng.choice(len(tx), 2 * q)]
        x = torch.cat([bp, bn, bt]).float()
        y = torch.cat([torch.ones(len(bp)), torch.zeros(len(bn) + len(bt))])
        x = x + torch.randn_like(x) * noise_scale
        loss = loss_fn(model.logits(x), y)
        weight = torch.where(y > 0, torch.tensor(1.0), torch.tensor(args.against))
        loss = (loss * weight).mean()
        opt.zero_grad()
        loss.backward()
        opt.step()
        sched.step()
        if (step + 1) % 2000 == 0 or step + 1 == args.steps:
            v = validate(model, check, cx, ctx)
            line = "  ".join(f"{k}: " + "/".join(f"{x:.2f}" for x in r.values()) for k, r in v.items() if k in (0.5, 0.9, 0.98))
            print(f"step {step + 1} loss {loss.item():.4f} | thr: FA-per-h/" + "/".join(list(v[0.5])[1:]) + f" {line}", flush=True)
    model.eval()
    out = Path(args.out)
    export(model, out)
    runs = Path(args.inputs).parent / "runs"
    runs.mkdir(parents=True, exist_ok=True)
    torch.save({"state": model.state_dict(), "hidden": args.hidden, "shape": args.shape, "args": vars(args)}, runs / f"{out.stem}.pt")
    (runs / f"{out.stem}.validation.json").write_text(json.dumps({str(k): v for k, v in validate(model, check, cx, ctx).items()}, indent=1))
    print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
