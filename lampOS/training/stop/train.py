"""Step 5 of make.sh: train the listener and write `stop.onnx`.

    python train.py [--hidden 128] [--steps 20000] [--lowest -20] [--out PATH]

The model is small and plain, so that tract runs it as it runs name.onnx:

    input [1, 16, 96] -> Flatten -> Gemm (1536 -> H) -> Relu -> Gemm (H -> 64) -> Relu
                      -> Gemm (64 -> 1) -> Sigmoid -> output [1, 1]

opset 13. The shift and scale the embeddings are given while training are
folded into the first Gemm, so the file takes the embeddings exactly as
`hear.rs` makes them.

Each batch is a quarter target inputs, a quarter inputs with near misses or
other words, and half inputs of talk alone or of two voices. After half the
steps, the negatives the model then scores highest are drawn as often as
the rest. `--lowest` leaves out of training the targets put more than that
many dB under the other voice.
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


def load(pattern: str) -> dict:
    parts = [np.load(p) for p in sorted(glob.glob(str(INPUTS / pattern)))]
    out = {}
    for key in parts[0].files:
        if key != "names":
            out[key] = np.concatenate([p[key] for p in parts])
    return out


class Listener(torch.nn.Module):
    def __init__(self, hidden: int, mean: np.ndarray, scale: np.ndarray):
        super().__init__()
        self.register_buffer("mean", torch.from_numpy(mean.astype(np.float32)))
        self.register_buffer("scale", torch.from_numpy(scale.astype(np.float32)))
        self.first = torch.nn.Linear(16 * 96, hidden)
        self.second = torch.nn.Linear(hidden, 64)
        self.last = torch.nn.Linear(64, 1)
        self.drop = torch.nn.Dropout(0.2)

    def logits(self, x: torch.Tensor) -> torch.Tensor:
        x = ((x - self.mean) / self.scale).reshape(len(x), -1)
        x = self.drop(torch.relu(self.first(x)))
        x = self.drop(torch.relu(self.second(x)))
        return self.last(x)[:, 0]


def export(model: Listener, path: Path) -> None:
    """Write the model as plain ONNX ops, the shift and scale folded into the first Gemm."""
    import onnx
    from onnx import TensorProto, helper, numpy_helper

    mean = np.broadcast_to(model.mean.numpy(), (16, 96)).reshape(-1)
    scale = np.broadcast_to(model.scale.numpy(), (16, 96)).reshape(-1)
    w1 = model.first.weight.detach().numpy() / scale[None, :]
    b1 = model.first.bias.detach().numpy() - w1 @ mean
    weights = {
        "w1": w1, "b1": b1,
        "w2": model.second.weight.detach().numpy(), "b2": model.second.bias.detach().numpy(),
        "w3": model.last.weight.detach().numpy(), "b3": model.last.bias.detach().numpy(),
    }
    inits = [numpy_helper.from_array(v.astype(np.float32), k) for k, v in weights.items()]
    nodes = [
        helper.make_node("Flatten", ["input"], ["flat"], axis=1),
        helper.make_node("Gemm", ["flat", "w1", "b1"], ["h1"], transB=1),
        helper.make_node("Relu", ["h1"], ["r1"]),
        helper.make_node("Gemm", ["r1", "w2", "b2"], ["h2"], transB=1),
        helper.make_node("Relu", ["h2"], ["r2"]),
        helper.make_node("Gemm", ["r2", "w3", "b3"], ["h3"], transB=1),
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
    """On held-out speakers: the share of target inputs scored at least each threshold, by level,
    and hearings an hour in an hour of held-out readers."""
    s = scores_of(model, check_x)
    talk = scores_of(model, talk_x)
    hours = len(talk) * 0.08 / 3600
    pos = check["y"] == 1
    level = check["level"].astype(np.float32)
    bands = {"alone": level > 50, "0..6": (level >= 0) & (level <= 6), "-10..0": (level >= -10) & (level < 0), "-15..-10": (level >= -15) & (level < -10), "-20..-15": level < -15}
    out = {}
    for least in [0.5, 0.8, 0.9, 0.95, 0.98, 0.99]:
        row = {"fa_per_h": len(hearings(talk, least)) / hours}
        for band, sel in bands.items():
            sel = sel & pos
            row[band] = float(np.mean(s[sel] >= least)) if sel.any() else float("nan")
        out[least] = row
    return out


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--hidden", type=int, default=128)
    p.add_argument("--steps", type=int, default=20000)
    p.add_argument("--batch", type=int, default=1024)
    p.add_argument("--lowest", type=float, default=-20.0)
    p.add_argument("--out", default=str(MODEL_DIR / "stop.onnx"))
    p.add_argument("--seed", type=int, default=0)
    args = p.parse_args()
    torch.manual_seed(args.seed)
    rng = np.random.default_rng(args.seed)

    words = load("words-*.npz")
    talk = load("talk-*.npz")
    check = load("check-[0-9]*.npz")
    check_talk = load("check-talk-*.npz")
    wy = words["y"]
    wl = words["level"].astype(np.float32)
    pos = np.flatnonzero((wy == 1) & (wl >= args.lowest))
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
    model = Listener(args.hidden, mean, scale)
    opt = torch.optim.AdamW(model.parameters(), lr=1e-3, weight_decay=1e-3)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=2e-3, total_steps=args.steps, pct_start=0.1)
    loss_fn = torch.nn.BCEWithLogitsLoss(reduction="none")
    noise_scale = torch.from_numpy(scale.astype(np.float32)) * 0.05
    q = args.batch // 4
    hard_talk = hard_near = None
    for step in range(args.steps):
        if step == args.steps // 2:
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
        weight = torch.where(y > 0, torch.tensor(1.0), torch.tensor(2.0))
        loss = (loss * weight).mean()
        opt.zero_grad()
        loss.backward()
        opt.step()
        sched.step()
        if (step + 1) % 2000 == 0 or step + 1 == args.steps:
            v = validate(model, check, cx, ctx)
            line = "  ".join(f"{k}: " + "/".join(f"{x:.2f}" for x in r.values()) for k, r in v.items() if k in (0.5, 0.9, 0.98))
            print(f"step {step + 1} loss {loss.item():.4f} | thr: FA-per-h/alone/0..6/-10..0/-15..-10/-20..-15 {line}", flush=True)
    model.eval()
    out = Path(args.out)
    export(model, out)
    runs = CACHE / "runs"
    runs.mkdir(parents=True, exist_ok=True)
    torch.save({"state": model.state_dict(), "hidden": args.hidden, "args": vars(args)}, runs / f"{out.stem}.pt")
    (runs / f"{out.stem}.validation.json").write_text(json.dumps({str(k): v for k, v in validate(model, check, cx, ctx).items()}, indent=1))
    print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
