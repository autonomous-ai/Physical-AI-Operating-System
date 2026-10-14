"""Parity check: old single OWLv2 graph vs the text + image split, on real images.

Runs both with the 46 HAL fire-hazard prompts and compares raw outputs, post-processed
detections (threshold 0.1 + NMS, the server's path) and what HAL would bucket
(conf >= 0.3). Exits 1 on any breach of the tolerances below.

--provider cpu|cuda runs fp32 and checks strict parity. --provider trt runs TensorRT fp16
with prod's options (engines cached in --trt-cache, never prod's cache). fp16 rounds
differently per engine, so trt gates on what HAL acts on: detections at conf >= 0.3.

    python scripts/owlv2_split_parity.py --old owlv2_raw.onnx \
        --text owlv2_text_v1.onnx --image owlv2_image_v1.onnx [--images 'dir/*.jpg']
"""

import argparse
import glob
import json
import sys
import time
from collections import Counter
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))

import cv2
import numpy as np
import onnxruntime as ort
from transformers import Owlv2Processor

from core.perception.object.predictors.onnx.base import ONNXObjectDetector
from core.utils.detection import owlv2_preprocess, unowlv2_boxes

HF_MODEL_ID = "google/owlv2-large-patch14-ensemble"
BUCKET_FILES = ("fire", "smoke", "flammable", "safe")  # RemoteFireHazardDetector order
SERVER_THRESHOLD = 0.1  # OWLv2ONNXDetector.DEFAULT_THRESHOLD
HAL_THRESHOLD = 0.3  # FIRE_HAZARD_CONFIDENCE_THRESHOLD

TOL_RAW = 1e-4
TOL_LABEL_PROB = 0.05  # labels must agree where the top prob is at least this
TOL_IOU = 0.999
TOL_CONF = 1e-3
TRT_TOL_IOU = 0.95
TRT_TOL_CONF = 0.02


GPU_MEM_LIMIT = 6 << 30  # this script shares the GPU with live dlserver; cap its CUDA arena


def make_providers(kind: str, trt_cache: str | None) -> list:
    """Same provider options as core.utils.runtime.prepare_ort_session, minus prod's cache path.

    Adds a CUDA arena cap so a test run can never starve a dlserver on the same GPU.
    """
    cuda = ("CUDAExecutionProvider", {
        "arena_extend_strategy": "kSameAsRequested", "cudnn_conv_algo_search": "DEFAULT",
        "do_copy_in_default_stream": True, "cudnn_conv_use_max_workspace": False,
        "gpu_mem_limit": GPU_MEM_LIMIT,
    })
    if kind == "cpu":
        return ["CPUExecutionProvider"]
    if kind == "cuda":
        return [cuda, "CPUExecutionProvider"]
    assert trt_cache, "--provider trt needs --trt-cache"
    Path(trt_cache).mkdir(parents=True, exist_ok=True)
    return [("TensorrtExecutionProvider", {
        "device_id": 0, "trt_fp16_enable": True,
        "trt_engine_cache_enable": True, "trt_engine_cache_path": trt_cache,
        "trt_timing_cache_enable": True, "trt_timing_cache_path": trt_cache,
        "trt_builder_optimization_level": 3, "trt_max_workspace_size": 2 << 30,
        "trt_layer_norm_fp32_fallback": True, "trt_cuda_graph_enable": False,
    }), cuda, "CPUExecutionProvider"]


def session(path: str, providers: list) -> ort.InferenceSession:
    opts = ort.SessionOptions()
    opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    opts.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    t0 = time.perf_counter()
    s = ort.InferenceSession(path, sess_options=opts, providers=providers)
    print(f"loaded {Path(path).name} on {s.get_providers()[0]} in {time.perf_counter() - t0:.1f} s")
    wanted = providers[0] if isinstance(providers[0], str) else providers[0][0]
    if s.get_providers()[0] != wanted:  # ORT falls back silently (e.g. TensorRT libs not on LD_LIBRARY_PATH)
        sys.exit(f"{wanted} not active for {Path(path).name}; got {s.get_providers()}")
    return s


def load_prompts(prompts_dir: Path) -> tuple[list[str], dict[str, str]]:
    classes, bucket_of = [], {}
    for bucket in BUCKET_FILES:
        for line in (prompts_dir / f"{bucket}_classes.txt").read_text().splitlines():
            if line.strip():
                classes.append(line.strip())
                bucket_of[line.strip()] = bucket
    return classes, bucket_of


def postprocess(raw: list[np.ndarray], classes: list[str], hw: tuple[int, int]):
    """The server's own post-processing (ONNXObjectDetector._postprocess_batch) on a stub detector."""
    stub = SimpleNamespace(
        _threshold=SERVER_THRESHOLD, _nms=True,
        _get_label_pool=lambda c: c, _to_orig_normalized=unowlv2_boxes,
    )
    return ONNXObjectDetector._postprocess_batch(stub, raw, classes, [hw])[0]


def iou_xywh(a: np.ndarray, b: np.ndarray) -> float:
    ax1, ay1, ax2, ay2 = a[0] - a[2] / 2, a[1] - a[3] / 2, a[0] + a[2] / 2, a[1] + a[3] / 2
    bx1, by1, bx2, by2 = b[0] - b[2] / 2, b[1] - b[3] / 2, b[0] + b[2] / 2, b[1] + b[3] / 2
    inter = max(0.0, min(ax2, bx2) - max(ax1, bx1)) * max(0.0, min(ay2, by2) - max(ay1, by1))
    union = a[2] * a[3] + b[2] * b[3] - inter
    return float(inter / union) if union > 0 else 0.0


def above(det, thr: float):
    keep = [i for i, c in enumerate(det.confidence) if c >= thr]
    return SimpleNamespace(class_names=[det.class_names[i] for i in keep],
                           bbox_xywh=det.bbox_xywh[keep], confidence=det.confidence[keep])


def compare_detections(old, new, tol_iou: float = TOL_IOU, tol_conf: float = TOL_CONF) -> tuple[list[str], dict]:
    errors = []
    if Counter(old.class_names) != Counter(new.class_names):
        errors.append(f"class multiset differs: old={Counter(old.class_names)} new={Counter(new.class_names)}")
        return errors, {}
    used, min_iou, max_dconf = set(), 1.0, 0.0
    for i, name in enumerate(old.class_names):
        cands = [j for j, n in enumerate(new.class_names) if n == name and j not in used]
        j = max(cands, key=lambda k: iou_xywh(old.bbox_xywh[i], new.bbox_xywh[k]))
        used.add(j)
        min_iou = min(min_iou, iou_xywh(old.bbox_xywh[i], new.bbox_xywh[j]))
        max_dconf = max(max_dconf, abs(float(old.confidence[i]) - float(new.confidence[j])))
    if min_iou < tol_iou:
        errors.append(f"matched box IoU {min_iou:.5f} < {tol_iou}")
    if max_dconf > tol_conf:
        errors.append(f"matched |dconf| {max_dconf:.2e} > {tol_conf}")
    return errors, {"min_iou": min_iou, "max_dconf": max_dconf}


def hal_buckets(det, bucket_of: dict[str, str]) -> Counter:
    return Counter(bucket_of[n] for n, c in zip(det.class_names, det.confidence) if c >= HAL_THRESHOLD)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--old", required=True, help="old single graph (prod owlv2_raw.onnx)")
    ap.add_argument("--text", required=True)
    ap.add_argument("--image", required=True)
    ap.add_argument("--images", default=str(ROOT / "tests/fixtures/images/fire-*.jpg"))
    ap.add_argument("--prompts-dir", default=str(ROOT.parent.parent / "hal/drivers/sensing/perceptions/processors/resources"))
    ap.add_argument("--json", default=None)
    ap.add_argument("--provider", choices=("cpu", "cuda", "trt"), default="cpu")
    ap.add_argument("--trt-cache", default=None, help="engine cache dir for --provider trt (not prod's)")
    args = ap.parse_args()

    classes, bucket_of = load_prompts(Path(args.prompts_dir))
    tokens = Owlv2Processor.from_pretrained(HF_MODEL_ID)(
        text=[f"a photo of {c}" for c in classes], images=None, return_tensors="np"
    )["input_ids"].astype(np.int64)
    print(f"{len(classes)} prompts, tokens {tokens.shape}")

    paths = sorted(glob.glob(args.images))
    if not paths:
        print(f"no images match {args.images}")
        return 1
    providers = make_providers(args.provider, args.trt_cache)
    fp16 = args.provider == "trt"
    inputs = {p: owlv2_preprocess(cv2.imread(p))[0][None] for p in paths}

    def timed_runs(sess: ort.InferenceSession, feed) -> dict:
        out = {}
        for p in paths:
            sess.run(None, feed(p))  # warm; the first run may build a TensorRT engine
            t0 = time.perf_counter()
            out[p] = (sess.run(None, feed(p)), (time.perf_counter() - t0) * 1e3)
        return out

    # One model on the GPU at a time: old graph first, then the split graphs.
    old = session(args.old, providers)
    old_out = timed_runs(old, lambda p: {"images": inputs[p], "class_tokens": tokens})
    del old
    text = session(args.text, providers)
    query_embeds = text.run(None, {"class_tokens": tokens})[0]
    t0 = time.perf_counter()
    query_embeds = text.run(None, {"class_tokens": tokens})[0]
    text_ms = (time.perf_counter() - t0) * 1e3
    del text
    image = session(args.image, providers)
    new_out = timed_runs(image, lambda p: {"images": inputs[p], "query_embeds": query_embeds})
    del image

    report, failed = {"provider": args.provider, "prompts": len(classes),
                      "text_graph_ms": round(text_ms, 1), "images": {}}, False
    for path in paths:
        hw = cv2.imread(path).shape[:2]
        raw_old, old_ms = old_out[path]
        raw_new, new_ms = new_out[path]

        errors = []
        d_boxes = float(np.abs(raw_old[0] - raw_new[0]).max())
        d_probs = float(np.abs(raw_old[1] - raw_new[1]).max())
        det_old, det_new = postprocess(raw_old, classes, hw), postprocess(raw_new, classes, hw)
        if fp16:  # raw diffs are reported, not gated
            det_errors, det_stats = compare_detections(
                above(det_old, HAL_THRESHOLD), above(det_new, HAL_THRESHOLD), TRT_TOL_IOU, TRT_TOL_CONF)
        else:
            if d_boxes > TOL_RAW:
                errors.append(f"raw boxes max abs {d_boxes:.2e} > {TOL_RAW}")
            if d_probs > TOL_RAW:
                errors.append(f"raw probs max abs {d_probs:.2e} > {TOL_RAW}")
            confident = raw_old[1].max(-1) >= TOL_LABEL_PROB
            label_mismatch = int(((raw_old[2] != raw_new[2]) & confident).sum())
            if label_mismatch:
                errors.append(f"{label_mismatch} labels differ where max prob >= {TOL_LABEL_PROB}")
            det_errors, det_stats = compare_detections(det_old, det_new)
        errors += det_errors
        b_old, b_new = hal_buckets(det_old, bucket_of), hal_buckets(det_new, bucket_of)
        if b_old != b_new:
            errors.append(f"HAL buckets differ: old={dict(b_old)} new={dict(b_new)}")

        failed |= bool(errors)
        name = Path(path).name
        print(f"{'FAIL' if errors else 'PASS'} {name}: raw boxes {d_boxes:.2e} probs {d_probs:.2e} | "
              f"{len(det_old.class_names)} dets | HAL {dict(b_old)} | old {old_ms:.0f} ms, image graph {new_ms:.0f} ms")
        for e in errors:
            print(f"    {e}")
        report["images"][name] = {
            "pass": not errors, "errors": errors, "raw_boxes_max_abs": d_boxes, "raw_probs_max_abs": d_probs,
            "detections": len(det_old.class_names), "hal_buckets": dict(b_old), **det_stats,
            "old_ms": round(old_ms, 1), "image_graph_ms": round(new_ms, 1),
        }

    print(f"text graph (all {len(classes)} prompts, once): {text_ms:.0f} ms")
    print("RESULT:", "FAIL" if failed else "PASS")
    if args.json:
        Path(args.json).write_text(json.dumps(report, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
