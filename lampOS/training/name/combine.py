"""Put name models into one file that `lamp-name` runs as it runs one.

    python combine.py beside FIRST.onnx SECOND.onnx OUT.onnx [--second-least L]
    python combine.py mean OUT.onnx MODEL.onnx [MODEL.onnx ...] [--least L]

Every file made here takes the input `[1, 16, 96]` and gives `[1, 1]`, as
`hear.rs` wants, so no code changes:

- "beside": the larger of two models' scores. It hears whatever either
  hears and is set off by whatever sets either off: the lamp's current
  `name.onnx` can stay as it is, with a second model beside it in the same
  file. `--second-least L` holds the second to a stricter threshold: its
  own score of L comes out as 0.9, the threshold `lamp-name` uses.
- "mean": the models' average, taken on the logit scale (each score turned
  back into the number before its last sigmoid, kept within +-16), then a
  sigmoid. `--least L` moves the result so that what would have been L
  comes out as 0.9: a model can be given to the lamp at the threshold it
  is best at, without changing `--least` there.
"""

from __future__ import annotations

import argparse
import math

import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper

EPS = 1e-7


def renamed(graph: onnx.GraphProto, prefix: str) -> tuple[list, list, str]:
    """The graph's nodes and weights with every name given a prefix, reading `input`."""
    inp = graph.input[0].name
    out = graph.output[0].name

    def name(n: str) -> str:
        if n == inp:
            return "input"
        return n if n == "" else prefix + n

    nodes = []
    for node in graph.node:
        copy = onnx.NodeProto()
        copy.CopyFrom(node)
        copy.name = prefix + node.name if node.name else ""
        del copy.input[:]
        copy.input.extend(name(i) for i in node.input)
        del copy.output[:]
        copy.output.extend(name(o) for o in node.output)
        nodes.append(copy)
    inits = []
    for init in graph.initializer:
        copy = onnx.TensorProto()
        copy.CopyFrom(init)
        copy.name = name(init.name)
        inits.append(copy)
    return nodes, inits, name(out)


def shift_for(least: float) -> float:
    """What to add to a logit so that a score of `least` becomes 0.9."""
    return math.log(0.9 / 0.1) - math.log(least / (1 - least))


def logit_nodes(score: str, prefix: str) -> tuple[list, list, str]:
    inits = [
        numpy_helper.from_array(np.array(EPS, np.float32), prefix + "eps"),
        numpy_helper.from_array(np.array(1 - EPS, np.float32), prefix + "top"),
        numpy_helper.from_array(np.array(1.0, np.float32), prefix + "one"),
    ]
    nodes = [
        helper.make_node("Clip", [score, prefix + "eps", prefix + "top"], [prefix + "p"]),
        helper.make_node("Sub", [prefix + "one", prefix + "p"], [prefix + "q"]),
        helper.make_node("Div", [prefix + "p", prefix + "q"], [prefix + "odds"]),
        helper.make_node("Log", [prefix + "odds"], [prefix + "logit"]),
    ]
    return nodes, inits, prefix + "logit"


def moved(logit: str, shift: float, prefix: str) -> tuple[list, list, str]:
    inits = [numpy_helper.from_array(np.array(shift, np.float32), prefix + "by")]
    nodes = [
        helper.make_node("Add", [logit, prefix + "by"], [prefix + "moved"]),
        helper.make_node("Sigmoid", [prefix + "moved"], [prefix + "score"]),
    ]
    return nodes, inits, prefix + "score"


def save(nodes: list, inits: list, models: list, out: str, name: str) -> None:
    graph = helper.make_graph(
        nodes,
        name,
        [helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 16, 96])],
        [helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 1])],
        inits,
    )
    opset = max([13] + [o.version for m in models for o in m.opset_import if o.domain in ("", "ai.onnx")])
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", opset)], producer_name="lampOS training/name combine.py")
    model.ir_version = max([8] + [m.ir_version for m in models])
    onnx.checker.check_model(model)
    onnx.save(model, out)


def load(path: str) -> onnx.ModelProto:
    m = onnx.load(path)
    dims = [d.dim_value for d in m.graph.input[0].type.tensor_type.shape.dim]
    assert dims == [1, 16, 96], (path, dims)
    return m


def beside(first: str, second: str, out: str, second_least: float = 0.9) -> None:
    a, b = load(first), load(second)
    na, ia, out_a = renamed(a.graph, "first/")
    nb, ib, out_b = renamed(b.graph, "second/")
    nodes, inits = na + nb, ia + ib
    if abs(second_least - 0.9) > 1e-9:
        n1, i1, logit = logit_nodes(out_b, "second/shift/")
        n2, i2, out_b = moved(logit, shift_for(second_least), "second/shift/")
        nodes, inits = nodes + n1 + n2, inits + i1 + i2
    nodes.append(helper.make_node("Max", [out_a, out_b], ["output"]))
    save(nodes, inits, [a, b], out, "name, two models beside each other")


def mean(out: str, paths: list[str], least: float = 0.9) -> None:
    models = [load(p) for p in paths]
    nodes, inits, logits = [], [], []
    for k, m in enumerate(models):
        n, i, score = renamed(m.graph, f"m{k}/")
        n2, i2, logit = logit_nodes(score, f"m{k}/logit/")
        nodes += n + n2
        inits += i + i2
        logits.append(logit)
    nodes.append(helper.make_node("Sum", logits, ["mean/sum"]))
    inits.append(numpy_helper.from_array(np.array(1.0 / len(models), np.float32), "mean/n"))
    nodes.append(helper.make_node("Mul", ["mean/sum", "mean/n"], ["mean/logit"]))
    n, i, score = moved("mean/logit", shift_for(least), "mean/")
    nodes += n
    inits += i
    nodes.append(helper.make_node("Identity", [score], ["output"]))
    save(nodes, inits, models, out, f"name, the mean of {len(models)} models")


def expected(scores: list[float], how: str, least: float) -> float:
    def logit(p):
        p = min(max(p, EPS), 1 - EPS)
        return math.log(p / (1 - p))

    if how == "beside":
        b = 1 / (1 + math.exp(-(logit(scores[1]) + shift_for(least)))) if abs(least - 0.9) > 1e-9 else scores[1]
        return max(scores[0], b)
    z = sum(logit(p) for p in scores) / len(scores) + shift_for(least)
    return 1 / (1 + math.exp(-z))


def check(paths: list[str], out: str, how: str, least: float) -> float:
    """The largest difference between the file's score and what it should be, on random inputs."""
    import onnxruntime as ort

    rng = np.random.default_rng(0)
    runs = [ort.InferenceSession(p, providers=["CPUExecutionProvider"]) for p in paths]
    whole = ort.InferenceSession(out, providers=["CPUExecutionProvider"])
    worst = 0.0
    for _ in range(200):
        x = (rng.standard_normal((1, 16, 96)) * 3).astype(np.float32)
        scores = [r.run(None, {r.get_inputs()[0].name: x})[0].item() for r in runs]
        got = whole.run(None, {"input": x})[0].item()
        worst = max(worst, abs(got - expected(scores, how, least)))
    return worst


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="how", required=True)
    b = sub.add_parser("beside")
    b.add_argument("first")
    b.add_argument("second")
    b.add_argument("out")
    b.add_argument("--second-least", type=float, default=0.9)
    m = sub.add_parser("mean")
    m.add_argument("out")
    m.add_argument("models", nargs="+")
    m.add_argument("--least", type=float, default=0.9)
    a = p.parse_args()
    if a.how == "beside":
        beside(a.first, a.second, a.out, a.second_least)
        worst = check([a.first, a.second], a.out, "beside", a.second_least)
    else:
        mean(a.out, a.models, a.least)
        worst = check(a.models, a.out, "mean", a.least)
    print(f"wrote {a.out}; differs from what it should give by at most {worst:.2e}")
