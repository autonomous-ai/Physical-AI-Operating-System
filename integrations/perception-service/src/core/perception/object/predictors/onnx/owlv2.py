"""OWLv2 zero-shot object detector using ONNX Runtime: a text graph and an image graph.

The text graph turns tokenized prompts into normalized query embeddings. The image graph
takes images plus those embeddings and returns boxes/probs/labels. Presets (fixed prompt
lists) are encoded once at startup and kept in memory, so a preset request runs only the
image graph. A ``classes`` request runs the text graph, then the image graph.
"""

from pathlib import Path
from typing import Any, ClassVar

import cv2.typing as cv2t
import numpy as np
import numpy.typing as npt
import onnxruntime as ort
from transformers import Owlv2Processor
from typing_extensions import override

from core.enums.files import ModelEnum
from core.models.object import RawObjectDetection
from core.utils.common import get_or_default
from core.utils.detection import owlv2_preprocess, unowlv2_boxes
from core.utils.files import ensure_downloaded, get_default_cdn_url, get_default_model_path
from core.utils.runtime import prepare_ort_session

from .base import ONNXObjectDetector


def read_onnx_inputs(path: Path) -> dict[str, list[int | str]]:
    """Graph input name -> declared shape (ints, or names for dynamic axes), without a session."""
    import onnx

    model = onnx.load(str(path), load_external_data=False)
    initializers = {i.name for i in model.graph.initializer}
    return {
        i.name: [d.dim_value or d.dim_param for d in i.type.tensor_type.shape.dim]
        for i in model.graph.input
        if i.name not in initializers
    }


class OWLv2ONNXDetector(ONNXObjectDetector):
    """Zero-shot object detection using the OWLv2 text + image ONNX graphs."""

    # model_path / remote_url (and SingleObjectDetectorSetting.model_path) are the image graph.
    DEFAULT_MODEL_PATH: Path | None = get_default_model_path(ModelEnum.OWLV2_IMAGE_ONNX)
    DEFAULT_REMOTE_URL: str | None = get_default_cdn_url(ModelEnum.OWLV2_IMAGE_ONNX)
    DEFAULT_TEXT_MODEL_PATH: Path | None = get_default_model_path(ModelEnum.OWLV2_TEXT_ONNX)
    DEFAULT_TEXT_REMOTE_URL: str | None = get_default_cdn_url(ModelEnum.OWLV2_TEXT_ONNX)
    DEFAULT_THRESHOLD: float = 0.1
    DEFAULT_HF_MODEL_ID: str = "google/owlv2-large-patch14-ensemble"

    TEXT_INPUT_NAMES: ClassVar[list[str]] = ["class_tokens"]
    TEXT_OUTPUT_NAMES: ClassVar[list[str]] = ["query_embeds"]
    ONNX_INPUT_NAMES: ClassVar[list[str]] = ["images", "query_embeds"]
    ONNX_OUTPUT_NAMES: ClassVar[list[str]] = ["boxes", "probs", "labels"]
    WARMUP_IMGSZ: int = 640

    PRESETS: tuple[str, ...] = ("fire_hazard_v1",)

    # TensorRT profile. The image graph is static 1008x1008 (fixed box bias for the 72x72
    # patch grid). Queries: opt = fire_hazard_v1 (46), max = ObjectDetectRequest.classes
    # max_length (500). An explicit profile means the engine is never rebuilt mid-request.
    IMAGE_SIZE: int = 1008
    TOKEN_LEN: int = 16
    TRT_OPT_QUERIES: int = 46
    TRT_MAX_QUERIES: int = 500

    def __init__(
        self,
        model_path: Path | None = None,
        remote_url: str | None = None,
        hf_model_id: str | None = None,
        classes_path: Path | None = None,
        threshold: float | None = None,
        batch_size: int | None = None,
        nms: bool = True,
        text_model_path: Path | None = None,
        text_remote_url: str | None = None,
    ) -> None:
        super().__init__(
            model_path=model_path,
            remote_url=remote_url,
            classes_path=classes_path,
            threshold=threshold,
            batch_size=batch_size,
            nms=nms,
        )
        text_model_path = get_or_default(text_model_path, self.DEFAULT_TEXT_MODEL_PATH)
        if text_model_path is None:
            raise RuntimeError("text_model_path must not be None")
        self._text_model_path: Path = text_model_path
        self._text_remote_url: str | None = get_or_default(text_remote_url, self.DEFAULT_TEXT_REMOTE_URL)
        self._hf_model_id: str = get_or_default(hf_model_id, self.DEFAULT_HF_MODEL_ID)
        self._processor: Owlv2Processor | None = None
        self._text_session: ort.InferenceSession | None = None
        self._image_session: ort.InferenceSession | None = None
        self._preset_embeds: dict[str, tuple[list[str], npt.NDArray[np.float32]]] = {}

    @property
    @override
    def preset_names(self) -> frozenset[str]:
        return frozenset(self._preset_embeds)

    @override
    def _start_tokenizer(self) -> None:
        self._logger.info("Loading tokenizer from %s", self._hf_model_id)
        self._processor = Owlv2Processor.from_pretrained(self._hf_model_id)

    @override
    def _start_impl(self) -> None:
        """Load both graphs and encode every preset. Any failure raises: no half-ready detector.

        Replaces the base ``_start_impl``: the base single ``self._session`` stays unused.
        """
        if self._running:
            self._logger.info("Already running")
            return

        try:
            self._start_sessions()
        except Exception:
            # Release whatever was loaded; the caller drops this detector.
            self._stop_impl()
            raise
        self._running = True
        self._logger.info(
            "Ready — %d default classes, presets=%s, nms=%s",
            len(self._class_names),
            sorted(self._preset_embeds),
            self._nms,
        )

    def _start_sessions(self) -> None:
        self._model_path = ensure_downloaded(self._model_path, remote=self._remote_url)
        self._text_model_path = ensure_downloaded(self._text_model_path, remote=self._text_remote_url)
        self._start_tokenizer()
        self._class_names = self._load_classes(self._classes_path)
        presets = {name: self._load_classes(self.PRESETS_DIR / f"{name}.txt") for name in self.PRESETS}

        # Text graph, warmed at the preset size and at Q=1.
        self._check_inputs(self._text_model_path, self.TEXT_INPUT_NAMES, "text")
        warm_names = presets[self.PRESETS[0]]
        q_opt, q_max = self.TRT_OPT_QUERIES, self.TRT_MAX_QUERIES
        t = self.TOKEN_LEN
        self._logger.info("Loading OWLv2 text graph from %s", self._text_model_path)
        self._text_session = prepare_ort_session(
            self._text_model_path,
            warmup_inputs=[
                {self.TEXT_INPUT_NAMES[0]: self._tokenize(warm_names)},
                {self.TEXT_INPUT_NAMES[0]: self._tokenize(warm_names[:1])},
            ],
            trt_profile={self.TEXT_INPUT_NAMES[0]: (f"1x{t}", f"{q_opt}x{t}", f"{q_max}x{t}")},
        )

        preset_embeds: dict[str, tuple[list[str], npt.NDArray[np.float32]]] = {}
        for name, prompts in presets.items():
            preset_embeds[name] = (prompts, self._encode(prompts))
            self._logger.info("Preset %s encoded (%d prompts)", name, len(prompts))

        # Image graph, warmed with the real preset embeddings and with Q=1.
        image_inputs = self._check_inputs(self._model_path, self.ONNX_INPUT_NAMES, "image")
        embeds = preset_embeds[self.PRESETS[0]][1]
        d = int(embeds.shape[1])
        declared_d = image_inputs[self.ONNX_INPUT_NAMES[1]][-1]
        if isinstance(declared_d, int) and declared_d not in (0, d):
            raise RuntimeError(f"OWLv2 text graph gives D={d}, image graph expects D={declared_d}")
        s, b = self.IMAGE_SIZE, self._batch_size
        img_batch = self._warmup_images()
        self._logger.info("Loading OWLv2 image graph from %s", self._model_path)
        self._image_session = prepare_ort_session(
            self._model_path,
            warmup_inputs=[
                {self.ONNX_INPUT_NAMES[0]: img_batch, self.ONNX_INPUT_NAMES[1]: embeds},
                {self.ONNX_INPUT_NAMES[0]: img_batch, self.ONNX_INPUT_NAMES[1]: embeds[:1]},
            ],
            trt_profile={
                self.ONNX_INPUT_NAMES[0]: (f"1x3x{s}x{s}", f"1x3x{s}x{s}", f"{b}x3x{s}x{s}"),
                self.ONNX_INPUT_NAMES[1]: (f"1x{d}", f"{q_opt}x{d}", f"{q_max}x{d}"),
            },
        )

        self._preset_embeds = preset_embeds

    def _check_inputs(self, path: Path, expected: list[str], role: str) -> dict[str, list[int | str]]:
        """Fail fast when a graph has the wrong inputs (e.g. MODEL_PATH still points at the old single graph)."""
        inputs = read_onnx_inputs(path)
        if list(inputs) != expected:
            raise RuntimeError(
                f"OWLv2 {role} graph expected inputs {expected}, got {list(inputs)}: "
                "is MODEL_PATH still the old single graph?"
            )
        return inputs

    def _warmup_images(self) -> npt.NDArray[np.float32]:
        sz = self.WARMUP_IMGSZ
        dummy = np.zeros((sz, sz, 3), dtype=np.uint8)
        return np.stack(self.preprocess([dummy] * self._batch_size), axis=0)

    @override
    def _stop_impl(self) -> None:
        super()._stop_impl()
        self._processor = None
        self._text_session = None
        self._image_session = None
        self._preset_embeds = {}

    @override
    def _is_ready_impl(self) -> bool:
        return (
            self._running
            and self._text_session is not None
            and self._image_session is not None
            and self._processor is not None
            and all(name in self._preset_embeds for name in self.PRESETS)
        )

    @override
    def preprocess(self, input: list[cv2t.MatLike]) -> list[cv2t.MatLike]:
        """BGR → padded square → CLIP-normalized CHW."""
        return [owlv2_preprocess(img)[0] for img in input]

    def _tokenize(self, classes: list[str]) -> npt.NDArray[np.int64]:
        if self._processor is None:
            raise RuntimeError("Tokenizer not loaded")
        text_queries = [f"a photo of {c}" for c in classes]
        tok = self._processor(text=text_queries, images=None, return_tensors="np")
        return tok["input_ids"].astype(np.int64)

    def _encode(self, classes: list[str]) -> npt.NDArray[np.float32]:
        """Prompts -> normalized query embeddings [Q, D]. The GPU lock covers only the run."""
        if self._text_session is None:
            raise RuntimeError(f"{self.__class__.__name__} text graph is not loaded")
        class_tokens = self._tokenize(classes)
        with self._gpu_lock:
            out = self._text_session.run(self.TEXT_OUTPUT_NAMES, {self.TEXT_INPUT_NAMES[0]: class_tokens})
        return np.asarray(out[0], dtype=np.float32)

    @override
    def _build_onnx_inputs(
        self,
        img_batch: npt.NDArray[np.float32],
        classes: list[str],
    ) -> dict[str, npt.NDArray]:
        return {
            self.ONNX_INPUT_NAMES[0]: img_batch,
            self.ONNX_INPUT_NAMES[1]: self._encode(classes),
        }

    @override
    def _predict_impl(
        self,
        input: list[cv2t.MatLike],
        *,
        preprocess: bool = True,
        classes: list[str] | None = None,
        preset: str | None = None,
        **kwargs: Any,
    ) -> list[RawObjectDetection]:
        if self._image_session is None:
            raise RuntimeError(f"{self.__class__.__name__} is not ready")

        # A preset wins over classes: its embeddings were encoded at startup.
        if preset is not None:
            entry = self._preset_embeds.get(preset)
            if entry is None:
                raise ValueError(f"Unknown preset: {preset}")
            names, embeds = entry
        else:
            names = classes if classes else self._class_names
            embeds = self._encode(names)

        orig_hws: list[tuple[int, int]] = [(img.shape[0], img.shape[1]) for img in input]
        if preprocess:
            input = self.preprocess(input)
        img_batch: npt.NDArray[np.float32] = np.stack(input, axis=0)

        onnx_inputs = {self.ONNX_INPUT_NAMES[0]: img_batch, self.ONNX_INPUT_NAMES[1]: embeds}
        with self._gpu_lock:
            raw_outputs = self._image_session.run(self.ONNX_OUTPUT_NAMES, onnx_inputs)

        return self._postprocess_batch(raw_outputs, names, orig_hws)

    @override
    def _to_orig_normalized(
        self,
        boxes_xywh: npt.NDArray[np.float32],
        orig_hw: tuple[int, int],
    ) -> npt.NDArray[np.float32]:
        return unowlv2_boxes(boxes_xywh, orig_hw)
