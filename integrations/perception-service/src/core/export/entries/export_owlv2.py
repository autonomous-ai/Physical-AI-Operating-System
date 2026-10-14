"""Export OWLv2 zero-shot object detector to ONNX as two graphs.

Text graph:  class_tokens [Q, 16] int64 -> query_embeds [Q, D] float32 (L2-normalized).
Image graph: images [B, 3, 1008, 1008], query_embeds [Q, D]
             -> boxes [B, P, 4] xywh in [0,1], probs [B, P, Q], labels [B, P].

Splitting lets the server encode a fixed prompt list (a preset) once at startup and run
only the image graph per request. The image outputs keep the contract of the old single
graph, so post-processing is unchanged. The image size is static: OWLv2 adds a fixed
box bias for its 72x72 patch grid, so any other size gives wrong boxes.
"""

import argparse
import logging
from pathlib import Path

import numpy as np
import onnxruntime as ort
import torch
from transformers import Owlv2ForObjectDetection, Owlv2Processor
from typing_extensions import override

from core.enums.files import ModelEnum
from core.export.utils.nms import xyxy_to_xywh_normalized
from core.export.utils.onnx import run_shape_inference
from core.utils.files import get_default_model_path

logger: logging.Logger = logging.getLogger(__name__)
logger.setLevel(logging.INFO)

IMAGE_SIZE: int = 1008


class OWLv2TextONNX(torch.nn.Module):
    """class_tokens -> normalized text embeddings, as Owlv2Model.forward computes them."""

    def __init__(self, model: Owlv2ForObjectDetection):
        super().__init__()
        self.model = model

    @override
    def forward(self, class_tokens: torch.Tensor) -> torch.Tensor:
        attention_mask = (class_tokens != 0).long()
        pooled = self.model.owlv2.text_model(input_ids=class_tokens, attention_mask=attention_mask)[1]
        embeds = self.model.owlv2.text_projection(pooled)
        return embeds / torch.linalg.norm(embeds, ord=2, dim=-1, keepdim=True)


class OWLv2ImageONNX(torch.nn.Module):
    """images + query embeddings -> boxes/probs/labels, as Owlv2ForObjectDetection.forward.

    The same queries apply to every image in the batch. No query mask: HF masks queries
    whose first token is 0, and every real query starts with BOS.
    """

    def __init__(self, model: Owlv2ForObjectDetection):
        super().__init__()
        self.model = model

    @override
    def forward(self, images: torch.Tensor, query_embeds: torch.Tensor):
        feature_map = self.model.image_embedder(pixel_values=images)[0]  # [B, h, w, C]
        b, h, w, c = feature_map.shape
        image_feats = feature_map.reshape(b, h * w, c)
        queries = query_embeds.unsqueeze(0).expand(b, -1, -1)

        logits = self.model.class_predictor(image_feats, queries, None)[0]  # [B, P, Q]
        pred_boxes = self.model.box_predictor(image_feats, feature_map)  # [B, P, 4] cxcywh

        probs = logits.sigmoid()

        # cxcywh → xyxy
        cx, cy, bw, bh = pred_boxes.unbind(-1)
        boxes_xyxy = torch.stack([cx - bw / 2, cy - bh / 2, cx + bw / 2, cy + bh / 2], dim=-1)

        xywh = xyxy_to_xywh_normalized(boxes_xyxy, input_hw=(1, 1))
        _, labels = probs.max(dim=-1)
        return xywh, probs, labels


def _export_text(model: Owlv2ForObjectDetection, class_tokens: torch.Tensor, dest: Path, opset: int) -> None:
    with torch.no_grad():
        torch.onnx.export(
            OWLv2TextONNX(model).eval(),
            (class_tokens,),
            str(dest),
            input_names=["class_tokens"],
            output_names=["query_embeds"],
            dynamic_axes={"class_tokens": {0: "num_classes"}, "query_embeds": {0: "num_classes"}},
            opset_version=opset,
            do_constant_folding=True,
        )
    run_shape_inference(dest)


def _export_image(
    model: Owlv2ForObjectDetection, images: torch.Tensor, query_embeds: torch.Tensor, dest: Path, opset: int
) -> None:
    with torch.no_grad():
        torch.onnx.export(
            OWLv2ImageONNX(model).eval(),
            (images, query_embeds),
            str(dest),
            input_names=["images", "query_embeds"],
            output_names=["boxes", "probs", "labels"],
            dynamic_axes={
                "images": {0: "batch"},
                "query_embeds": {0: "num_classes"},
                "boxes": {0: "batch"},
                "probs": {0: "batch", 2: "num_classes"},
                "labels": {0: "batch"},
            },
            opset_version=opset,
            do_constant_folding=True,
        )
    run_shape_inference(dest)


def _verify(model: Owlv2ForObjectDetection, text_path: Path, image_path: Path,
            class_tokens: torch.Tensor, images: torch.Tensor) -> None:
    """Compare both ONNX graphs, chained, against the torch wrappers."""
    with torch.no_grad():
        ref_q = OWLv2TextONNX(model)(class_tokens)
        ref = OWLv2ImageONNX(model)(images, ref_q)
    providers = ["CPUExecutionProvider"]
    text = ort.InferenceSession(str(text_path), providers=providers)
    image = ort.InferenceSession(str(image_path), providers=providers)
    q = text.run(None, {"class_tokens": class_tokens.numpy()})[0]
    out = image.run(None, {"images": images.numpy(), "query_embeds": q})

    logger.info("Verification (torch vs ONNX, CPU):")
    logger.info(f"\tquery_embeds: max_err = {np.abs(q - ref_q.numpy()).max():.6f}")
    for name, o, r in zip(("boxes", "probs"), out[:2], ref[:2]):
        logger.info(f"\t{name}: max_err = {np.abs(o - r.numpy()).max():.6f}")
    # Where every prob is ~0 (a random image), argmax ties flip; only count meaningful labels.
    confident = ref[1].numpy().max(-1) >= 0.05
    logger.info(f"\tlabels: mismatches = {int(((out[2] != ref[2].numpy()) & confident).sum())} (max prob >= 0.05)")


def export(model_id: str, text_output: str | None = None, image_output: str | None = None, opset: int = 17):
    text_dest = Path(text_output or get_default_model_path(ModelEnum.OWLV2_TEXT_ONNX)).expanduser().resolve()
    image_dest = Path(image_output or get_default_model_path(ModelEnum.OWLV2_IMAGE_ONNX)).expanduser().resolve()
    for dest in (text_dest, image_dest):
        dest.parent.mkdir(parents=True, exist_ok=True)

    logger.info(f"Loading model from {model_id}...")
    model = Owlv2ForObjectDetection.from_pretrained(model_id)
    model.eval()

    processor = Owlv2Processor.from_pretrained(model_id)
    dummy_text = ["a photo of a cat", "a photo of a dog", "a photo of a person"]
    dummy_inputs = processor(
        text=dummy_text, images=[torch.zeros(3, IMAGE_SIZE, IMAGE_SIZE)], return_tensors="pt"
    )
    dummy_images = dummy_inputs["pixel_values"]  # [1, 3, 1008, 1008]
    dummy_class_tokens = dummy_inputs["input_ids"]  # [3, 16]
    assert dummy_images.shape[-2:] == (IMAGE_SIZE, IMAGE_SIZE), dummy_images.shape

    logger.info(f"Exporting text graph to {text_dest}...")
    _export_text(model, dummy_class_tokens, text_dest, opset)
    with torch.no_grad():
        dummy_query_embeds = OWLv2TextONNX(model)(dummy_class_tokens)
    logger.info(f"Exporting image graph to {image_dest}...")
    _export_image(model, dummy_images, dummy_query_embeds, image_dest, opset)

    for dest in (text_dest, image_dest):
        logger.info(f"Exported {dest} ({dest.stat().st_size / 1024 / 1024:.1f} MB)")

    _verify(model, text_dest, image_dest, dummy_class_tokens, torch.rand_like(dummy_images))


def entry():
    logging.basicConfig(level=logging.DEBUG)

    parser = argparse.ArgumentParser(description="Export OWLv2 to ONNX (text + image graphs)")
    parser.add_argument(
        "--model-id",
        default="google/owlv2-large-patch14-ensemble",
        help="HuggingFace model identifier",
    )
    parser.add_argument("--text-output", default=None)
    parser.add_argument("--image-output", default=None)
    parser.add_argument("--opset", type=int, default=17)
    args = parser.parse_args()

    export(args.model_id, args.text_output, args.image_output, args.opset)


if __name__ == "__main__":
    entry()
