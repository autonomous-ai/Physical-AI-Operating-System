# The lamp's eyes

`lamp-eyes` (crate `lamp-eyes`: `src/picture.rs`, `src/faces.rs`,
`src/sight.rs`, `src/main.rs`) owns the camera in the lamp's head, finds a
face in what it sees and tells the body where in the picture that face is.
The body (`lamp-body`, [body.md](body.md), "Looking at a face") turns the head
toward it while the lamp listens or speaks. That is all it is for: looking at
the person the lamp is talking with.

Status on 2026-10-13: built for the lamp and tested on a Mac and under
emulation. It has not run on a lamp, has not opened the lamp's camera, and the
body has not turned to a face. Everything below is what the code does.

## What it does and does not do

- One process owns the camera. It opens it through `lamp-camera`'s Linux
  port, which checks the device against the USB identity given (vendor,
  product and port) and takes the camera's lock before the first frame:
  320 by 240 MJPG at ten frames a second.
- It looks at the newest frame at most every 150 ms: decode the JPEG
  (`zune-jpeg`), put it into a 320 by 256 picture, run the face detector,
  pick one face.
- It sends one datagram per look to the body's gaze socket and nothing else
  anywhere:

  ```json
  {"face":{"x":0.05,"y":-0.43,"size":0.41,"turned":0.04}}
  {"face":null}
  ```

  `x` and `y` are the face's centre from the middle of the picture, -1 at the
  left or top edge to 1 at the right or bottom. `size` is its height as a
  share of the picture's. `turned` is how far its nose is from the point
  between its eyes, in eye distances: about 0 for a face turned to the
  camera.
- No frame is written to disk, kept between looks or sent anywhere. The
  program has no option that would do so. A line in the journal every ten
  seconds says how many frames came, how many were looked at, in how many
  there was a face and how long a look took.
- It never moves anything and decides nothing about the conversation. It does
  not recognise who a face belongs to.

## When the camera is open

Only while the voice service says so. `lamp-live serve`, when
`LAMP_EYES_PERMIT_SOCKET` is set, sends `{"camera":true}` to that socket twice
a second while the microphone is permitted, and `{"camera":false}` once, at
once, when it is not (`crates/live/src/body_link.rs`, `EyesLink`). The eyes
open the camera on the first yes and close it on a no or when no yes has come
for 1.5 s, so a voice service that is muted, stopped or gone closes the
lamp's eyes too. The rule is the same as for its ears: the camera may look
exactly while the microphone may listen.

This is a narrower arrangement than the camera worker described in
[camera-runtime.md](camera-runtime.md), which takes its permission from the
interaction authority's own camera grant and hands frames to other
processes. The eyes use that crate's port for the device and nothing of its
frame link. Joining the two is still to do.

## Which face

The detector is YuNet (`face_detection_yunet_2023mar.onnx`, 232,589 bytes,
SHA-256 `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`),
the model the old runtime used, run with `tract-onnx` as the endpoint worker
runs its model. The file is checked against that size and digest before it is
parsed; it is not in this repository (`hal/drivers/tracking/models/` in the
old runtime's tree has it).

A detection counts as a face from a score of 0.6. Boxes that overlap by more
than 0.3 are one face; a box that is not a number or lies off the picture is
dropped. Of the faces left, the largest whose height is a tenth of the
picture's or more is the one reported: somebody at the lamp, not somebody
across the room. With several people near, that is whoever is nearest the
camera, and it can change from look to look.

## Running it

```text
lamp-eyes --device /dev/video0 --lock-directory /home/WORKER/private-camera-locks --usb 01da:5875@3-1.1.4 --model /opt/lampOS/eyes/face_detection_yunet_2023mar.onnx --permit-socket /run/lampos-eyes/permit.sock --body-socket /run/lampos-body/gaze.sock
```

Every option is required. `--usb` is `VENDOR:PRODUCT@TOPOLOGY`, the two
numbers as four hex digits; the lock directory must be private to the user it
runs as. A camera that does not open, or fails, is tried again every five
seconds while permission lasts.

To look at one JPEG file without a camera:

```sh
cargo run --release -p lamp-eyes --example look -- MODEL.onnx PHOTO.jpg
```

## Verification without a lamp

Host tests, `cargo test -p lamp-eyes` (12 passed, 0 failed, on macOS), and the
same under emulated ARM64. They cover: a 320 by 240 frame keeps every pixel in
blue, green, red order with the spare rows black, and a larger one is shrunk
to fit; what is not a frame or not a JPEG is refused; a detection becomes a
box where its cell is, and weak, duplicate and senseless detections are
dropped; outputs of another shape are refused; a file that is not the pinned
model is refused; the largest near face is chosen and a small one alone is
not; the datagram's exact text; the command line and the permission message.

By hand, with the real model on a Mac (`look`): a 1920 by 1080 photo of a
person a metre from the camera gave one face, `x` 0.05, `y` -0.43, `size`
0.41, `turned` 0.04; two photos of the lamp in an empty office gave none. A
look took 29 to 38 ms there, a third of it decoding the large JPEG.

Not shown by any of this: how long a look takes on the lamp's processor; what
the lamp's camera sees from its head, how wide, and how well in office light;
whether a face at conversation distance is a tenth of that picture; how the
detector fares with a face lit from behind, in profile or with glasses, where
the old runtime's notes say it struggles; anything about the camera's lock and
identity check on the lamp itself.
