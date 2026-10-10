# Model assets

Each file here is a pinned runtime input. `lamp-perception` refuses bytes whose
size or SHA-256 differ from its built-in specification (`crates/perception/src/model.rs`).
Install onto a device with `lamp-models install` (see
[perception runtime](../../docs/perception-runtime.md)); never edit a model in place.

| File | Bytes | SHA-256 | License | Source |
|---|---|---|---|---|
| `face_detection_yunet_2023mar.onnx` | 232,589 | `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4` | MIT ([`LICENSE-face_detection_yunet`](LICENSE-face_detection_yunet)) | [OpenCV Zoo](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet), file last changed in upstream commit `f12e12798e8314f7c074a6656816c048dcc95b7a` ("update yunet to v2"). |

The YuNet bytes are identical to the copy V1 checked in at
`hal/drivers/tracking/models/face_detection_yunet_2023mar.onnx` (legacy commit
`d5efe9d7b73cc529b34cd4abe97624682a82ca94`, Git blob `f9beb304`). V1 ran it through
OpenCV with a 320×320 input, score threshold 0.35, NMS 0.3 and top-k 50.

V1's face-identity path used InsightFace SCRFD and EdgeFace models downloaded
without a hash from a company bucket. Those models are not reused: identity is
out of scope for this release, and InsightFace's pretrained detector weights are
published for non-commercial research use. Presence and attention need
detection and five landmarks, which YuNet provides under MIT.
