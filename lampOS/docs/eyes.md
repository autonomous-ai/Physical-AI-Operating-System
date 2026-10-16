# The lamp's eyes

`lamp-eyes` (crate `lamp-eyes`: `src/picture.rs`, `src/faces.rs`,
`src/sight.rs`, `src/kept.rs`, `src/main.rs`) owns the camera in the lamp's
head, finds a face in what it sees and tells the body where in the picture
that face is. The body (`lamp-body`, [body.md](body.md), "Looking at a face")
turns the head toward it while the lamp listens or speaks. Started with
`--sight-file` it also keeps its newest frame for the voice service to show
the model, so that the lamp can say what it sees.

Status on 2026-10-13: running on lamp-4ace since that morning. With a
photograph of a face on a screen in front of the lamp it reported the face in
66 of 66 looks, 53 to 55 ms a look. With people standing at the lamp for
seven minutes it reported a face in 580 of 2,732 looks, and the body turned
toward one as far as its yaw reach goes (250 counts) with the face still at
0.44 to 0.75 of the half-picture off centre: they stood further round than
the head turns. The picture for the model is described under "What the model
sees" with what has and has not been shown of it.

## What it does and does not do

- One process owns the camera. It opens it through `lamp-camera`'s Linux
  port, which checks the device against the USB identity given (vendor,
  product and port) and takes the camera's lock before the first frame:
  320 by 240 MJPG at ten frames a second.
- It looks at the newest frame at most every 250 ms while there is a face
  to follow and for four seconds after the last one, and every 600 ms with
  nobody there: decode the JPEG (`zune-jpeg`), put it into a 320 by 256
  picture, run the face detector, pick one face. It was every 150 ms
  always, and a look takes 52 to 55 ms of one core on lamp-4ace. Measured
  there on 2026-10-13: the processor read 63 to 64 degrees C with the eyes
  stopped and 70 to 75 with them looking six times a second, above the
  70 degrees at which its kernel's first thermal trip point sits. The voice
  service had ended once that afternoon on a late audio reference, in the
  same second as the SD card retuned itself; whether heat had a part in
  that is not known.
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
- Without `--sight-file` no frame is written anywhere, kept between looks
  or sent anywhere. With it, the newest frame and no other is kept in that
  one file ("What the model sees", below). A line in the journal every ten
  seconds says how many frames came, how many were looked at, in how many
  there was a face and how long a look took.
- It never moves anything and decides nothing about the conversation. It
  recognises a face only for someone who asked the lamp to remember them
  ("Who is there", below), and nobody else.

## When the camera is open

Only while the voice service says so. `lamp-voice serve`, when
`LAMP_EYES_PERMIT_SOCKET` is set, sends `{"camera":true}` to that socket twice
a second while the microphone is permitted, and `{"camera":false}` once, at
once, when it is not (`crates/voice/src/body_link.rs`, `EyesLink`). The eyes
open the camera on the first yes and close it on a no or when no yes has come
for 1.5 s, so a voice service that is muted, stopped or gone closes the
lamp's eyes too. The rule is the same as for its ears: the camera may look
exactly while the microphone may listen.

This is a narrower arrangement than the camera worker described in
[camera-runtime.md](camera-runtime.md), which takes its permission from the
interaction authority's own camera grant and hands frames to other
processes. The eyes use that crate's port for the device and nothing of its
frame link. Joining the two is still to do.

## What the model sees

People standing at lamp-4ace on 2026-10-13 asked it "What you see in front of
you?" within their first minutes, and it answered that it could not see.

With `--sight-file PATH`, each frame the eyes look at (about six a second)
also replaces the file at `PATH`: written beside it and renamed over it, so a
reader gets one whole frame or the one before, mode 0600. The camera's frames
are complete JPEGs of 12 to 17 KiB at 320 by 240 (checked on lamp-4ace: each
carries its own Huffman tables); a frame that is not whole, or is over
64 KiB, is not kept and the last good one stays. The file is removed when the
camera closes, for whatever reason, and when the program ends. On lamp-4ace
`PATH` is in the service's runtime directory, which is memory, not disk.
There is never more than that one frame, and no history.

The voice service reads it (`crates/voice/src/sight.rs`). When
`LAMP_EYES_SIGHT_FILE` is set in the service's environment, the provider
worker is started with `--eyes-sight PATH`, and while a person is speaking
it offers the file's frame to the model through `Supervisor::try_image`
([gemini-visual-input.md](gemini-visual-input.md)) as a person begins to
speak, at most once every thirty seconds. A file last written
more than a second ago is not offered, so closed eyes show the model nothing
rather than something stale. The offer has the limits that call already
has: only for the request whose input is open, 64 KiB, never sent again on a
later request or another connection. The worker's line in the journal says
for each look whether a picture went and how old it was, or why none did.

What a picture costs. The service counts a picture as tokens of the
conversation, and bills the whole conversation again with every turn. In the
first run on lamp-4ace the worker looked twice with every request, and over
a request with two pictures the prompt outside audio grew by 506 tokens. In
an office where the lamp hears something every few seconds that adds up
quickly, which is one reason looks are spaced out, and why
`"small_pictures": true` in the provider configuration asks the service to
read pictures at its low resolution (`mediaResolution` in the setup). What
that saves on this service is not measured yet.

The other reason is time. A picture in a question makes its answer later.
Measured on lamp-4ace on 2026-10-13 with `sight_cost.sh`: seven ordinary
played questions in each of six fresh sessions, three with the eyes shut
(no picture) and three with a picture in every question, in the order shut,
open, open, shut, shut, open. First sound came a median 1.32 s after the
end-of-speech decision without pictures (19 replies; the three sessions'
medians 1.29, 1.24 and 1.61 s) and 1.78 s with them (20 replies; 1.75, 1.86
and 1.75 s). So looks were moved from four seconds apart to thirty.

That did not bring the speed back, and the reason is worse than a picture
costing its own answer. In a fresh session, one question with a picture
(answered after 1.39 s) and then thirteen without any: medians of 1.73 and
1.83 s in two runs of the seven questions. Once a session's history holds a
picture, every later answer in that session is about as slow as if each
carried one. The first answer, the one the picture came with, is not.

So on lamp-4ace the model's sight is switched off again (13:38 on
2026-10-13): `LAMP_EYES_SIGHT_FILE` is commented out in the voice unit, the
eyes run without `--sight-file`, and the character file says it cannot see
yet. Afterwards the provider's first audio came a median 1.21 s after the
end-of-speech decision and the speaker's first write 0.21 s later, over
nine answers. The code stays. What it needs before it is turned on again
is a way to look only when looking was asked for, and a session without
the picture afterwards (a renewal carries the conversation across, see
[conversation-thread.md](conversation-thread.md)). Neither is built.

On lamp-4ace, 2026-10-13, first run: a picture of 13.7 to 17.7 KiB, 18 to
157 ms old, went with each of six requests. A played "Hey Lamp, what do you
see in front of you right now?" was answered "I see a desk including an iMac
and another display, along with a keyboard, mouse, and some personal items
like a mug and a picture frame", which is what a frame copied by hand a
minute earlier showed (and a man standing beside the desk, whom it did not
mention and was not asked about). That is one question, played. Not shown:
a person asking about themselves or something they hold up, how it does in
poor light, and whether seeing changes what it says when nobody asked.

What this changes about privacy, plainly: with the option on, a picture of
whoever is in front of the lamp goes to the provider with what they say, as
their voice already does. Nothing but the one newest frame is on the lamp.
The camera is still open only while the microphone is permitted. Without the
option, and without the environment variable, everything is as it was.

The character file has to say the lamp can see
([character.md](character.md)); without that the model has the picture and
still answers that it cannot.

## Which face

The detector is YuNet (`face_detection_yunet_2023mar.onnx`, 232,589 bytes,
SHA-256 `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`),
the model the old runtime used, run with `tract-onnx` as the endpoint worker
runs its model. The file is checked against that size and digest before it is
parsed; it is not in this repository (`hal/drivers/tracking/models/` in the
old runtime's tree has it).

A detection counts as a face from a score of 0.6. Boxes that overlap by more
than 0.3 are one face; a box that is not a number or lies off the picture is
dropped. Of the faces left, the largest whose height is 15% of the
picture's or more is the one reported: somebody at the lamp, not somebody
across the room. With several people near, that is whoever is nearest the
camera, and it can change from look to look.

### A face at a place where no person has been seen (#763)

With `--leave-still-faces on` (off unless given; `src/still.rs`), a face
at a place where only small, faint faces have ever been seen is not a
person. lamp-4ace's eyes reported a face all night in an empty office (14
to 15 October, 01:00 to 05:59): 55 of 55 detections at one spot, about 0.7
across and 0.6 down, 0.09 to 0.12 of the picture's height, at 0.60 to
0.79. Its box is not still: it wanders 0.01 to 0.02 of the picture from
look to look, as much as a person sitting at the desk does, so a stillness
rule cannot tell them apart. Replayed through the day's recording, a box
held within two pixels for two minutes left out 4 of the 55, and a centre
held within 0.02 for five minutes left out over a third of the morning's
real faces. What tells them apart is that the spot never gave a face 0.14
of the height or taller, nor one scored 0.8 or more, in a day, while the
people at the desk did within seconds and the people at the picture's edge,
small too, scored up to 0.90.

So each place a face is seen is remembered: its centre and height, a face
being at it when both coordinates of its centre and its height are within
0.03 of the place's; with how many faces came from it, since when, and
whether any of them was a person's (0.14 of the height or more, or scored
0.8 or more). A face at a place that has given faces for half an hour or
more, ten or more of them, none of them a person's, is left out before
anything else uses the look: the body is not told of it, it is not on the
bus, and it is not recognised. A place nothing has been seen at for two
hours is forgotten (the night's face came back after at most 52 minutes);
at most 32 places are remembered, the one seen longest ago going first. The
journal's ten-second line says how many faces were left out so far.

Replayed in order through the whole recording (14 Oct 18:23 to 15 Oct
13:17, `docs/verification/still-face-lamp-4ace-20261014/`, a host test):
the night's 55 faces are all left out; at the false spot the rest of the
time 448 of 540 are (the first half hour after it is forgotten passes);
none of the 390 faces of the people at the edge in the evening, nor of the
10,853 at the desk in the morning, is.

Raising the detector's least score (`LEAST_SCORE`, 0.6) instead would cost
real faces before it removed the false one: at 0.75 it drops 574 of the
595 false-spot faces but also 105 of the 390 at the edge and 1,526 of the
10,853 at the desk; at 0.8, all 595 false ones, 160 at the edge and 2,398
at the desk.

Not shown yet: the day's recording replayed through it, with the counts
before and after, and whether raising the 0.6 score would do the same.

## Who is there

Since 2026-10-15 (#759; `docs/brain-build.md` step 4). "Lamp, remember my
face. I'm Minh." Later, sitting down: "Hey Minh." Until someone asks,
nobody is recognised: `docs/memory.md` decided that everything is "said in
this room" until someone asks to be recognised.

- **The print.** OpenCV's SFace recogniser
  (`face_recognition_sface_2021dec.onnx`, Apache-2.0, 38,696,353 bytes,
  SHA-256 `0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79`,
  fetched separately and checked before it is parsed) runs on tract as
  YuNet does. The near face is cut out upright from YuNet's five marks onto
  OpenCV's 112 by 112 reference (`who::align`). It gives 128 numbers of
  length one, and two prints of one person have a cosine of 0.363 or more
  (OpenCV's own threshold for this model). One print takes 152 ms on
  lamp-4ace's processor (236 ms the first time, 320 ms to load and
  optimise the model; 68 ms on the development Mac), and the 128 numbers
  were the same to the last digit on both. At one print every two seconds
  while someone is at the lamp that is about 8 % of one core.
- **When.** Only for the largest face that is at the lamp: at least a tenth
  of the picture's height (people at lamp-4ace's desk were 0.12 to 0.27).
  At most every two seconds, or at every look while someone is being
  remembered. The camera gives 320 by 240, so a face at the desk is 35 to
  65 pixels tall. Whether that is enough is to be measured with people
  (the camera can give 640 by 480 if not).
- **Kept.** `--people FILE` (on lamp-4ace `/var/lib/lampos-eyes/people.json`,
  in a directory of mode 0700 and a file of mode 0600). Per person: the
  name, up to five prints and when they were added. At most 20 people. No
  picture is kept, and no print leaves the lamp. A file that is too large
  or not of this shape stops the eyes rather than being guessed at.
- **Asked.** One line of JSON on `--who-socket` (on lamp-4ace
  `/run/lampos-eyes/who.sock`) and one line back:
  - `{"remember":"Minh"}` takes five prints over the next looks while that
    one face is at the lamp and answers `{"remembered":"Minh"}`. With no face
    in view at all it answers `no_face` after 1.5 s. With a face too far
    away, or two at the lamp, it gives them four seconds and then answers
    `too_far` or `two_faces`.
  - `{"forget":"Minh"}` answers `{"forgot":true}` once the file no longer
    has it.
  - `{"list":true}` answers the names.
  - `{"who":true}` answers who is at the lamp now, or `null`.
  - Remembering someone already known replaces their prints.
- **Said.** With `--known-file` (on lamp-4ace `/run/lampos-eyes/known`,
  beside the socket), the name of the known person at the lamp is kept
  there and renewed every two seconds while they are there. The file is
  removed when nobody is near.
- **The voice** (`crates/voice/src/known_faces.rs`, when
  `LAMP_EYES_WHO_SOCKET` names the socket):
  - The model has two functions, `remember_my_face` and `forget_my_face`,
    each with the name. They are called only when the person asks.
  - The hello for someone known is by name.
  - What a known person says is told their name once a session ("The person
    at the lamp now is Minh: you know their face.").
  - With a memory, what a known person asks it to keep and what they say
    goes to their own scope, and a recall asks theirs first, then the
    room's (#764, [memory.md](memory.md#each-persons-own-764)). Forgetting
    a face does not forget what that person said.
  - With a memory, each provider session is one person's: when the known
    person changes (someone else at once, nobody after 30 s unseen and two
    minutes with nothing said), a new
    session is opened between turns and told only their thread and kept
    things ([memory.md](memory.md#a-session-for-each-person-764-follow-up)).

On lamp-4ace on 2026-10-15 at 08:22, with the office empty, the socket
answered `list` and `who` at once, `remember` with `no_face` after 4.0 s,
and `forget` of an unknown name with `false`. By voice at 08:33, "Hey Lamp,
can you remember my face? I'm Sam." (played, nobody in view) was answered
"I couldn't quite see you. Would you mind looking right at me and asking
again?", the eyes having looked for 4.2 s. Since 08:39 the eyes say no face
after 1.5 s: the same request was answered 3.5 s after the end of speech.

## On the bus

With `LAMP_BUS=on` in its unit the eyes also publish every face the
detector finds, not only the one the body is told to look at, on
`humans/faces/tracked` ([bus.md](bus.md), "On the lamp";
`crates/eyes/src/bus_out.rs`). For each face:

- **a passing id** (`f1`, `f2`, ...): the same from one look to the next
  for as long as its box overlaps where it was within the last 1.5 s,
  a new one after that, never given twice, and never a person's name;
- **its box**, each edge as a fraction of the picture;
- **how sure the detector was**;
- **how far it is turned**, in radians, and **whether that counts as
  turned to the lamp** ([`facing`](../crates/eyes/src/facing.rs): within
  60 degrees in the middle of the picture, more toward its edges, where
  the lens shows a straight face turned). A face narrower than 7.5% of
  the picture, or with an eye or the nose outside it, has neither: it
  counts for nothing either way.

A list lasts two seconds, and an empty one says the camera looked and saw
nobody. With the camera closed nothing is published and the last list
runs out. This is the sign a person gives, without meaning to, that they
are speaking to the lamp. Nothing uses it yet: the part that decides
whom the lamp answers ([policy.md](policy.md)) is to. The numbers for
"turned to the lamp" are V1's, tuned on V1's lamps at twice this
picture's width, and the sign of the angle assumes the picture is not
mirrored: neither has been checked with a person in front of this lamp.

About once a second the eyes also say how they are, on `diagnostics`
under the name `eyes`, for the lamp's health ([health.md](health.md)):

| Level | Message | When |
|---|---|---|
| `ok` | | The camera is open, or closed because seeing is not permitted now. Closed is the eyes working as they should |
| `warn` | `the camera does not open` | Permitted, and the camera cannot be opened; it is tried again |
| `warn` | `the camera stopped giving pictures` | It was open and failed, until it is open again |

The one fact beside it is `camera`: `open`, `closed`, `not opening` or
`failed`. It is said on every turn of the loop that is due, whatever the
camera is doing, and the sign that the eyes are there is now given the
same way: before 2026-10-14 it was not given while the camera was not
permitted, so the eyes read as gone from the bus whenever they were not
looking.

## Whether a mouth moves (task 39, off unless set)

With `LAMP_EYES_MOUTH=on` beside `LAMP_BUS=on` the eyes also say, for each
face on `humans/faces/tracked`, how much its mouth moves, on
`humans/faces/mouths` (`crates/eyes/src/mouth.rs`). It is for the one
thing sound cannot do on this lamp: hear a person over the lamp's own
voice (#720; [duplex-listening.md](duplex-listening.md)). A mouth that
moves while the lamp talks is not the lamp's echo. Nothing uses it yet:
how the voice weighs it with a weak sound cue is the first builder's.

**How.** No model beyond the detector. YuNet already gives the nose and
the two corners of the mouth. From every camera frame while there is a
face (not only the four a second the detector looks at), the lower face
(from the nose down to 2.5 times the nose-to-lips drop below the lips, so
a dropping jaw stays in it; twice the mouth's width) and a band round the
eyes are each read as 16 by 12 points. Each frame is first lined up with
the one before: the shift of up to 6 pixels each way (`SEARCH`) that makes
the band round the eyes change least is how far the face moved in the
picture, the lower face is read shifted by as much, and the shifts add up
so the regions follow the face (the lamp's head follows it, so the face
always slides a little). A frame whose eyes, once lined up, still changed
by more than 10 grey levels on average is a head turning or nodding (or
the light), and is left out (`HEAD_MOVED`). Otherwise the share of the
lower face's points that changed by at least 12 grey levels (`CHANGED`)
is the mouth's movement, and a share of 0.15 is "fully moving"
(`FULL_SHARE`). All four numbers are first guesses, for the raw run to
set. 6 pixels: the gaze turns the head at 50 counts a second at most
(about 4.4 degrees a second, [body.md](body.md)), 0.6 degrees a frame at 7
frames a second, 2.5 to 3.4 pixels at 320 pixels over 60 to 78 degrees,
with as much again for the person's own sway. The
score is the mean over the frames kept in the 1.2 s before the newest
frame read, and needs five frames in the window. Between two looks a face
is read where the last look found it; a face that moved is read afresh.

```json
{"mouths":[{"id":"f3","moving":0.62,"window_ms":1200,"frames":8,"age_ms":190,"head_moved":1}]}
```

`id` is the face's passing id on `humans/faces/tracked`; `age_ms` is from
the newest frame's taking (the camera driver's clock) to the message;
`head_moved` (left out when 0) is how many of `frames` were left out; a
face whose every frame was left out says 0 with `head_moved` equal to
`frames`: not read, not still. A face not read in five frames of late is
left out.

With `LAMP_EYES_MOUTH_RAW=on` as well, each mouth also carries `raw`, each
frame in its window as numbers only: `taken_us`, the shift it was lined
up by (`shift_x`, `shift_y`, in the detector's pixels), the lower face's
mean change lined up (`lower_mean`), its share of points past 8, 12 and 16
grey levels (`lower_share8`, `lower_share12`, `lower_share16`), and the
eyes' mean change where they were (`eyes_mean`) and lined up
(`eyes_after`). Readers of the first version read both new fields as
absent. Without the switch no
frame is read for mouths, the topic is not declared, and every message on
`humans/faces/tracked` is byte for byte what it was (a test).

**What it costs, lamp-52e6, 2026-10-14.** 0.6 to 1.1 ms a frame (a grey
JPEG decode and 384 points) against 61 to 89 ms a look for the detector;
nothing at all with nobody in view. The camera gives about 7 frames a
second at 320 by 240, so a 1.2 s window holds about eight. The newest
frame read is about 190 ms old when it is said (the camera's own queue and
the look), which is why the window counts back from that frame, not from
when it is said. A two-word "Stop" (about 0.4 s) fills a third of it:
enough to lift the score, not to fill it.

**First run with a person, lamp-52e6, 2026-10-15 (0.5 s counted back from
the message).** The detector found the face in 39 of 40 looks, but 70 to
80% of messages had no mouth, and every one that had was 3 frames: the
window, less the newest frame's 190 ms, held 300 ms of frames. Three
frames came out 0 most of the time, so speech did not stand out (talking
0.080 against quiet 0.046; over the lamp 0.000; chewing's quiet 0.200).
Hence the 1.2 s window from the newest frame.

**Second run, 2026-10-15 (1.2 s from the newest frame, the first score).**
Speech still did not stand out: SPEAK 0.038 against QUIET 0.082, CHEW 0,
SMILE 0.176, silent 0. That score was the lower face's mean change less
the eyes' over 6 grey levels; the eyes change more than the lower face at
rest, so it was at or under 0 almost always, and lips opening by 5 to 14
pixels are about 5% of the lower face, a mean change of about 2 grey
levels, inside the noise. Chewing's jaw fell below the region. Hence the
share of changed points, the eyes as a gate only, and the deeper region,
with the raw changes said for the next run to set their numbers. The process: 22 MB, 15% of one core, as before; the board at 51 to
52 degrees.

**Third run, 2026-10-16 (the raw changes).** The medians of each frame's
raw changes (each frame once, from 0.5 s into each prompt) were alike
everywhere: talking QUIET `lower_mean` 10.6 and `lower_share12` 0.286
against SPEAK 11.8 and 0.297, silent 10.9 and 0.260, with `eyes_mean`
19 to 24 in all of them; chewing's 4 to 7. The whole face changed by 10
to 20 grey levels between frames even with nobody speaking: the lamp's
head followed the face (the body's gaze ran in the second and third runs;
in the first, with the body off, the changes were small), and the regions
stayed where the face was found. Hence the line-up above, with the shift
and the eyes' change after it said in `raw`.

**What it cannot do.** Tell talking from chewing, laughing or a smile; see
a mouth behind a hand, in a face turned side on, or in a face too small:
at a metre a face is about a fifth of the picture's height and the lower
face 25 points across, enough for frame differences, not for lip shapes.

### The study (2026-10-14)

| Way | What it is | On this board | Taken? |
|---|---|---|---|
| Frame differences round the lips | The oldest active-speaker cue: how much a window round the tracked lips changes between frames, over a threshold ([KU Leuven, 2015](https://homes.esat.kuleuven.be/~konijn/publications/2015/3946_postprint.pdf): flags faces whose lips move above a threshold, and notes it fails on a profile or a hand over the lips; [Caus et al. 2021](https://www.inf.uni-hamburg.de/en/inst/ab/cv/media/caus-etal-icvs2021.pdf): the lower face works better than the mouth alone) | Under 1 ms a frame on the CPU, no model, works on a 25 pixel mouth | **Yes, first**: the lightest thing that can work, and the recordings say whether it is enough |
| Optical flow in the mouth region | Flow magnitude or variance as a visual VAD ([Tao 2015](https://msp.utdallas.edu/publications/Tao_2015.pdf); [Liu, Cheung and Tang 2016](https://www.comp.hkbu.edu.hk/~ymc/papers/journal/YCVIU_2350_publication_version.pdf)); unstable when the light changes | A few ms a frame on the CPU at this size | Next if differences are too noisy: same input, less sensitive to texture |
| Lip landmarks (MediaPipe Face Landmarker, `jawOpen`) | 478 points and 52 blendshapes from a 256 by 256 face crop ([Google](https://ai.google.dev/mediapipe/solutions/vision/face_landmarker)); jitter needs smoothing | A second model of a few MB at 256 by 256 per face: tens of ms on the CPU; would need a crop from a larger frame, since the face is 40 to 50 pixels here | No: the face is too small in 320 by 240, and the cost sits beside a board at 74 degrees on lamp-4ace |
| Light-ASD | Audio-visual active speaker detection, about 1.0M parameters and 0.6 GFLOPs, 94.1% mAP on AVA ([Liao et al., CVPR 2023](https://openaccess.thecvf.com/content/CVPR2023/html/Liao_A_Light_Weight_Model_for_Active_Speaker_Detection_CVPR_2023_paper.html)); used on a Nao robot ([2024](https://arxiv.org/html/2407.11915v1)) | Could fit the A733's 3 TOPS VIP9000 NPU ([Radxa](https://docs.radxa.com/en/cubie/a7a/app-dev/npu-dev/cubie_acuity_sdk)) after an ACUITY conversion (not tried); it needs the sound too, and the sound on this lamp carries the lamp's own voice | Later, its visual half only, if the recordings show differences cannot part speech from chewing |
| TalkNet | Audio-visual, long-term attention ([Tao et al. 2021](https://arxiv.org/pdf/2107.06592)) | Far heavier than Light-ASD | No |
| Furhat, Misty, Jibo, Astro | Nothing published that we found on how they see who speaks | | |

The 8 MP camera could give a larger crop of the face (it is opened at 320
by 240 now); that is the way to a lip-shape model, and not taken here.

## Running it

```text
lamp-eyes --device /dev/video0 --lock-directory /home/WORKER/private-camera-locks --usb 01da:5875@3-1.1.4 --model /opt/lampOS/eyes/face_detection_yunet_2023mar.onnx --permit-socket /run/lampos-eyes/permit.sock --body-socket /run/lampos-body/gaze.sock
```

Every option shown is required; `--sight-file ABSOLUTE_PATH` and `--leave-still-faces on` may be added.
`--usb` is `VENDOR:PRODUCT@TOPOLOGY`, the two numbers as four hex digits; the
lock directory must be private to the user it runs as. A camera that does not open, or fails, is tried again every five
seconds while permission lasts. A start is given a 2 s budget (`OPEN_BUDGET_US`)
and a close 250 ms, so a camera slow to answer after a close is opened rather
than given up as late (#911). These are budgets the caller checks after each
camera call returns, not time limits: a call blocked in the driver is not
interrupted, and one that returns past the budget is reported as expired. A
close always closes the file, but releases the buffers first only when the
stream stopped cleanly; after a failed or late stop it skips the release and
relies on the close. The eyes do not look at the stop's result. An open can
outlast the 1.5 s permission, or the permission can be taken back while it
blocks: so the permission is checked again as soon as the open returns, and a
camera opened without it is closed at once, before any frame is read, kept or
published. A blocked open is not interrupted; it is closed when it returns.
The permit socket is heard all the while by a thread of its own
(`crates/eyes/src/permit.rs`), and each `{"camera":true}` counts from when it
arrived, not from when the eyes read it: a renewal that came early in a long
open and was the last one has run out by the time the open returns, and does
not keep the camera open. Each open and each failed one is logged
with how long it took: about 50 ms on lamp-52e6.

To look at one JPEG file without a camera:

```sh
cargo run --release -p lamp-eyes --example look -- MODEL.onnx PHOTO.jpg
```

## Verification without a lamp

Host tests, `cargo test -p lamp-eyes` (14 passed, 0 failed, on macOS; the 12
before the kept frame also passed under emulated ARM64). They cover: that the
kept file is the newest frame, private, never a second file, left alone by a
frame that is cut, has no tables or is too large, and gone when forgotten or
dropped; a 320 by 240 frame keeps every pixel in
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
whether a face at conversation distance is 15% of that picture, the line
V1 measured between colleagues' faces (8.3 to 13.6%) and its user's (19.2
to 46%) and V2 took on 2026-10-14 without anyone in front of this lamp; how the
detector fares with a face lit from behind, in profile or with glasses, where
the old runtime's notes say it struggles; anything about the camera's lock and
identity check on the lamp itself.
