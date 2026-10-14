"""OWLv2 text/image split: export wrappers vs the old single graph (tiny random model) and the
two-session detector with in-memory presets (fake sessions). No download."""

from pathlib import Path

import numpy as np
import onnxruntime as ort
import pytest
import torch
from transformers import Owlv2Config, Owlv2ForObjectDetection

from core.export.entries.export_owlv2 import (
    OWLv2ImageONNX,
    OWLv2TextONNX,
    _export_image,
    _export_text,
)
from core.export.utils.nms import xyxy_to_xywh_normalized
from core.perception.object.predictors.onnx import owlv2 as owlv2_mod
from core.perception.object.predictors.onnx.owlv2 import OWLv2ONNXDetector

BOS, EOS = 49406, 49407


class _OldOWLv2ONNX(torch.nn.Module):
    """Frozen copy of the pre-split single-graph wrapper (nms=False): the reference."""

    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, images, class_tokens):
        attention_mask = (class_tokens != 0).long()
        outputs = self.model(input_ids=class_tokens, attention_mask=attention_mask, pixel_values=images)
        probs = outputs.logits.sigmoid()
        cx, cy, w, h = outputs.pred_boxes.unbind(-1)
        boxes_xyxy = torch.stack([cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2], dim=-1)
        xywh = xyxy_to_xywh_normalized(boxes_xyxy, input_hw=(1, 1))
        _, labels = probs.max(dim=-1)
        return xywh, probs, labels


@pytest.fixture(scope="module")
def tiny():
    torch.manual_seed(0)
    cfg = Owlv2Config(
        text_config={"hidden_size": 32, "num_hidden_layers": 1, "num_attention_heads": 2,
                     "intermediate_size": 37, "max_position_embeddings": 16, "vocab_size": 49408},
        vision_config={"hidden_size": 32, "num_hidden_layers": 1, "num_attention_heads": 2,
                       "intermediate_size": 37, "image_size": 56, "patch_size": 14},
        projection_dim=32,
    )
    model = Owlv2ForObjectDetection(cfg).eval()
    # 3 queries of different lengths, padded with 0 like the OWLv2 tokenizer does.
    tokens = torch.zeros(3, 16, dtype=torch.long)
    for i, n in enumerate((4, 7, 10)):
        tokens[i, 0] = BOS
        tokens[i, 1:n - 1] = torch.randint(1, 49000, (n - 2,))
        tokens[i, n - 1] = EOS
    image = torch.rand(1, 3, 56, 56)
    return model, tokens, image


def test_text_wrapper_matches_hf_text_embeds(tiny):
    model, tokens, image = tiny
    with torch.no_grad():
        ours = OWLv2TextONNX(model)(tokens)
        hf = model.owlv2(input_ids=tokens, pixel_values=image, attention_mask=(tokens != 0).long()).text_embeds
    torch.testing.assert_close(ours, hf, atol=1e-6, rtol=0)


def test_image_wrapper_matches_old_single_graph(tiny):
    model, tokens, image = tiny
    with torch.no_grad():
        old = _OldOWLv2ONNX(model)(image, tokens)
        new = OWLv2ImageONNX(model)(image, OWLv2TextONNX(model)(tokens))
    torch.testing.assert_close(new[0], old[0], atol=1e-5, rtol=0)
    torch.testing.assert_close(new[1], old[1], atol=1e-5, rtol=0)
    assert torch.equal(new[2], old[2])


def test_image_wrapper_broadcasts_queries_over_batch(tiny):
    model, tokens, image = tiny
    with torch.no_grad():
        q = OWLv2TextONNX(model)(tokens)
        one = OWLv2ImageONNX(model)(image, q)
        two = OWLv2ImageONNX(model)(image.repeat(2, 1, 1, 1), q)
    for a, b in zip(one, two):
        assert b.shape[0] == 2
        torch.testing.assert_close(b[0], a[0], atol=1e-6, rtol=0)
        torch.testing.assert_close(b[1], a[0], atol=1e-6, rtol=0)


def test_onnx_export_roundtrip(tiny, tmp_path):
    model, tokens, image = tiny
    text_path, image_path = tmp_path / "text.onnx", tmp_path / "image.onnx"
    _export_text(model, tokens, text_path, opset=17)
    with torch.no_grad():
        ref_q = OWLv2TextONNX(model)(tokens)
        ref = OWLv2ImageONNX(model)(image, ref_q)
    _export_image(model, image, ref_q, image_path, opset=17)

    providers = ["CPUExecutionProvider"]
    text = ort.InferenceSession(str(text_path), providers=providers)
    img = ort.InferenceSession(str(image_path), providers=providers)
    assert [i.name for i in text.get_inputs()] == ["class_tokens"]
    assert [i.name for i in img.get_inputs()] == ["images", "query_embeds"]

    # A different query count than the export dummy proves the query axis is dynamic.
    q = text.run(None, {"class_tokens": tokens[:2].numpy()})[0]
    np.testing.assert_allclose(q, ref_q[:2].numpy(), atol=1e-4)
    boxes, probs, _labels = img.run(None, {"images": image.numpy(), "query_embeds": q})
    with torch.no_grad():
        ref2 = OWLv2ImageONNX(model)(image, ref_q[:2])
    np.testing.assert_allclose(boxes, ref2[0].numpy(), atol=1e-4)
    np.testing.assert_allclose(probs, ref2[1].numpy(), atol=1e-4)
    assert probs.shape[-1] == 2
    assert ref[1].shape[-1] == 3


# ---------------------------------------------------------------------------
# OWLv2ONNXDetector: two sessions + in-memory presets (fake sessions, no model)
# ---------------------------------------------------------------------------

_D = 8
_REPO_ROOT = Path(__file__).resolve().parents[4]
_HAL_RESOURCES = _REPO_ROOT / "hal/drivers/sensing/perceptions/processors/resources"


class _FakeTokenizer:
    def __call__(self, text, images=None, return_tensors="np"):
        tokens = np.zeros((len(text), 16), dtype=np.int64)
        tokens[:, 0] = BOS
        return {"input_ids": tokens}


class _FakeTextSession:
    def __init__(self):
        self.calls: list[int] = []

    def run(self, _outputs, inputs):
        q = inputs["class_tokens"].shape[0]
        self.calls.append(q)
        # Distinct rows: row i is a scaled one-hot, so the image fake can tell queries apart.
        embeds = np.zeros((q, _D), dtype=np.float32)
        for i in range(q):
            embeds[i, i % _D] = 1.0 + i
        return [embeds]


class _FakeImageSession:
    def __init__(self):
        self.query_embeds: list[np.ndarray] = []

    def run(self, _outputs, inputs):
        b = inputs["images"].shape[0]
        q = inputs["query_embeds"].shape[0]
        self.query_embeds.append(inputs["query_embeds"])
        boxes = np.tile(np.array([[[0.5, 0.5, 0.2, 0.2]]], dtype=np.float32), (b, 1, 1))
        probs = np.zeros((b, 1, q), dtype=np.float32)
        probs[:, 0, 0] = 0.9
        labels = np.zeros((b, 1), dtype=np.int64)
        return [boxes, probs, labels]


@pytest.fixture
def fakes(monkeypatch, tmp_path):
    text, image = _FakeTextSession(), _FakeImageSession()
    state = {"image_inputs": ["images", "query_embeds"], "profiles": {}}
    text_path, image_path = tmp_path / "owlv2_text_v1.onnx", tmp_path / "owlv2_image_v1.onnx"

    def fake_prepare(model_path, *, warmup_inputs=None, trt_profile=None):
        session = text if model_path == text_path else image
        state["profiles"][model_path] = trt_profile
        for inputs in warmup_inputs or []:
            session.run(None, inputs)
        return session

    def fake_inputs(path):
        if path == text_path:
            return {"class_tokens": ["num_classes", 16]}
        return {name: ["batch", 3, 1008, 1008] if name == "images" else ["num_classes", _D]
                for name in state["image_inputs"]}

    monkeypatch.setattr(owlv2_mod, "prepare_ort_session", fake_prepare)
    monkeypatch.setattr(owlv2_mod, "ensure_downloaded", lambda path, remote=None: path)
    monkeypatch.setattr(owlv2_mod, "read_onnx_inputs", fake_inputs)
    monkeypatch.setattr(owlv2_mod.Owlv2Processor, "from_pretrained", lambda _id: _FakeTokenizer())

    detector = OWLv2ONNXDetector(model_path=image_path, text_model_path=text_path)
    return detector, text, image, state


def _img() -> np.ndarray:
    return np.zeros((120, 160, 3), dtype=np.uint8)


def _preset_lines() -> list[str]:
    return (OWLv2ONNXDetector.PRESETS_DIR / "fire_hazard_v1.txt").read_text().split("\n")[:-1]


def test_presets_encoded_once_at_start(fakes):
    detector, text, image, state = fakes
    detector.start()
    # Warmup Q=46 and Q=1, then one encode of fire_hazard_v1 (Q=46).
    assert text.calls == [46, 1, 46]
    assert detector.preset_names == frozenset({"fire_hazard_v1"})
    assert detector.is_ready()
    # The image graph was warmed with the stored preset embeddings and with Q=1.
    assert [q.shape for q in image.query_embeds] == [(46, _D), (1, _D)]
    profiles = list(state["profiles"].values())
    assert profiles[0] == {"class_tokens": ("1x16", "46x16", "500x16")}
    assert profiles[1] == {
        "images": ("1x3x1008x1008", "1x3x1008x1008", "1x3x1008x1008"),
        "query_embeds": (f"1x{_D}", f"46x{_D}", f"500x{_D}"),
    }


def test_preset_request_skips_text_graph(fakes):
    detector, text, image, _ = fakes
    detector.start()
    n_text = len(text.calls)
    stored = detector._preset_embeds["fire_hazard_v1"][1]
    out = detector.predict([_img()], preset="fire_hazard_v1")
    assert len(text.calls) == n_text
    assert image.query_embeds[-1] is stored
    assert image.query_embeds[-1].shape == (46, _D)
    assert out[0].class_names == ["a single flame"]


def test_preset_wins_over_classes(fakes):
    detector, text, _, _ = fakes
    detector.start()
    n_text = len(text.calls)
    out = detector.predict([_img()], classes=["cat"], preset="fire_hazard_v1")
    assert out[0].class_names == ["a single flame"]
    assert len(text.calls) == n_text


def test_classes_request_uses_text_then_image(fakes):
    detector, text, image, _ = fakes
    detector.start()
    n_text = len(text.calls)
    out = detector.predict([_img()], classes=["cat", "dog"])
    assert text.calls[n_text:] == [2]
    assert image.query_embeds[-1].shape == (2, _D)
    assert out[0].class_names == ["cat"]


def test_unknown_preset_raises_value_error(fakes):
    detector, _, _, _ = fakes
    detector.start()
    with pytest.raises(ValueError, match="Unknown preset"):
        detector.predict([_img()], preset="nope")


def test_preset_file_matches_hal_prompts():
    hal = b"".join(
        (_HAL_RESOURCES / f"{name}_classes.txt").read_bytes() for name in ("fire", "smoke", "flammable", "safe")
    )
    preset = (OWLv2ONNXDetector.PRESETS_DIR / "fire_hazard_v1.txt").read_bytes()
    assert preset == hal
    assert len(_preset_lines()) == 46


def test_old_single_graph_rejected(fakes):
    detector, _, _, state = fakes
    state["image_inputs"] = ["images", "class_tokens"]
    with pytest.raises(RuntimeError, match="old single graph"):
        detector.start()
    assert not detector.is_ready()
