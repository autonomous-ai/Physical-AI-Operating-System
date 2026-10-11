# The replies that interrupted themselves

Scope: an offline analysis for `MAIN-033` of the cohort at
`/private/tmp/lamp-rust-playout-evidence-20261011T033405Z`, trials 1 and 2,
target `a0ae`, base `02148ed4d` with the integrator's start gate, noise
suppression off, both volumes 100. Host only. I touched no device and no
cloud, and the recordings stay outside Git. No code changed.

## Answer

1. **All four admissions that cancelled a reply are explained by the lamp's
   own voice.** In each, accepted render was active, the microphone was
   explained 89 % to 99 % by that render, and the speech detector fired as
   the canceller's output rose. Trial 1's is established from the target's
   own signals. Trial 2's three are fully consistent with it, and the
   target's signals alone cannot exclude a talker as loud as the prompt
   underneath: the echo there is 15 dB to 20 dB above the prompt and
   clipped. The room audit can.
2. **The echo reference is on time.** No fault found in what I own.
3. **I have no canceller correction to propose.** Five were tried offline on
   these recordings and none removes the admissions. What the canceller is
   asked to do here is beyond a linear canceller: after the best linear fit,
   what is left of the lamp's own voice is as loud as the person it is
   listening for.

## The four admissions

Blocks are 10 ms capture blocks from the start of capture. "Explained" and
"floor" are from the [echo evidence](echo-evidence.md), read from the
device's own microphone and cancelled signals. Levels are below about 2 kHz.

| Trial, reply cancelled | Detector at 0.80 or more for six blocks | Render active, of the last 50 blocks | Microphone | Explained by the render | Cancelled signal | Playback floor |
|---|---|---|---|---|---|---|
| 1, reply 1, 1.2 s in | 700 to 705 | 37 to 41 | -15.0 to -16.5 dB | 99 % | -27.0 rising to -21.1 dB | 84 % rising to 97 % |
| 2, reply 1, 1.1 s in | 656 to 661 | 50 | -5.3 to -7.1 dB | 99 % | -63.8 rising to -50.4 dB | 0 |
| 2, reply 2, 0.3 s in | 904 to 909 | 20 to 25 | -5.1 to -6.7 dB | 89 % to 95 % | -54.3 rising to -29.8 dB | 0 |
| 2, reply 3, 0.4 s in | 1192 to 1197 | 37 to 42 | -8.3 to -9.1 dB | 89 % to 91 % | -22.4 rising to -12.6 dB | 0 rising to 76 % |

For comparison, the two prompts that were admitted correctly: no render in
the last 500 ms, microphone -31 dB rising to -19 dB, nothing to explain.

Three things in that table matter more than the rest.

- **The lamp hears itself 10 dB to 20 dB louder than it hears the prompt.**
  The prompt reached the microphone at about -22 dB to -26 dB. The lamp's own
  voice reached it at -3.5 dB to -13 dB, with samples at full scale in every
  reply: 392 of them in the first half second of trial 1's reply.
- **The detector does not care about level.** In trial 2's first admission
  the cancelled signal was 43 dB or more under the microphone, at -50 dB and
  below, and the detector scored it 0.87 to 0.99. A canceller that removes
  43 dB still leaves this detector something to fire on.
- **The floor is zero in half the windows.** It says "not determined"
  there, as it is built to: when the unexplained part of a loud capture could
  cover a quiet cancelled signal, accounting proves nothing. It is not a
  detector for this.

## What the target's signals can and cannot exclude

The most that can be anything but playback is the unexplained share of the
microphone.

| Admission | At most not playback | Against a prompt at about -24 dB |
|---|---|---|
| Trial 1, reply 1 | 1 % of -15 dB: under -35 dB | A talker at prompt level is excluded |
| Trial 2, reply 1 | 1 % to 4 % of -6 dB: -20 dB to -26 dB | Not excluded |
| Trial 2, reply 2 | 5 % to 11 % of -5 dB: -15 dB to -18 dB | Not excluded |
| Trial 2, reply 3 | 9 % to 11 % of -8.5 dB: about -19 dB | Not excluded |

The lower explained shares in trial 2 are what clipping does to a linear
fit. They are not evidence of a person. They are also not evidence of
nobody. I did not use any provider transcript.

## The reference

| | Trial 1 | Trial 2 |
|---|---|---|
| Playback epochs, DSP epochs, in the whole run | 1, 1 | 1, 1 |
| Reference packets ignored as stale | 0 | 0 |
| Reference analysed before each capture block was read, in the admitted windows | 0.1 ms to 2.1 ms | 0.1 ms to 4.2 ms |
| Reference ahead of the capture, in audio | 20 ms throughout | 20 ms throughout |
| Canceller's own alignment | 104 ms throughout | 112 ms throughout |
| Path delay measured independently on the first pilot | 104.25 ms | |
| Delay hint passed in | 18 ms to 19 ms | 16 ms to 17 ms |

The canceller had the right audio, early enough, at a steady alignment that
matches the path.

## Why it is left over

- **The path has gain.** The lamp's voice arrives at its own microphone
  louder than the reference the canceller is given: a median of 9.6 dB and
  9.9 dB in the two trials, 6.7 dB on the first pilot, up to 13 dB.
- **The path is not linear.** The best single linear filter of 20 ms, fitted
  afterwards to each stretch with the answer in hand, leaves:

  | Stretch | Microphone | Clipped samples | Left by the best linear fit | Left by the device |
  |---|---|---|---|---|
  | Trial 1, reply 1, first phrase | -3.5 dB | 392 | -13.6 dB | -73.7 dB |
  | Trial 1, reply 1, second phrase, admitted | -13.1 dB | 1 | -33.1 dB | -24.3 dB |
  | Trial 1, reply 2 | -7.7 dB | 6 | -21.5 dB | -37.9 dB |
  | Trial 2, reply 1 | -7.3 dB | 20 | -13.3 dB | -15.1 dB |
  | Trial 2, reply 3, admitted | -6.5 dB | 46 | -21.7 dB | -15.2 dB |
  | First pilot, the reply | -11.9 dB | 1 | -18.6 dB | -23.5 dB |

  Full band, 16 kHz. A canceller with one fixed filter cannot do better
  than the fit, and a real one has to learn its filter as it goes. So what
  linear cancellation leaves of the lamp's voice is -13 dB to -33 dB, and the
  prompt is at -22 dB to -26 dB.
- **Everything beyond that is suppression, and suppression is a guess.**
  Where the device left -74 dB it was muting. Where it left -15 dB to -24 dB
  it had taken the leftover for a person. Trial 1 shows both inside one
  reply, 300 ms apart.

I have not established what makes the path non-linear: the speaker driven
at volume 100, the microphone's own processing, or both. Clipping at the
converter alone does not reproduce it: in a scratch scene, not in the suite,
a scripted linear path clipped at full scale was still cancelled to under
-47 dB in nine blocks of ten by the same canceller.

## Tried offline, and not proposed

The certified bundles replay through `lamp-audio replay-aec`, and the
replayed output scored with the runtime's own `SpeechProbability` reproduces
three of the four device admissions at the same block (705, 661, 909). That
makes a candidate correction testable without a device. Each row is seven
replies: the six of these two trials and the first pilot's.

| Change to the canceller's input or setting | Replies with six blocks at 0.80 or more | Which |
|---|---|---|
| None | 3 of 7 | trial 1 reply 1; trial 2 replies 1 and 2 |
| Transparent mode `Hmm` | the same as none, block for block | |
| Noise suppression on | 3 of 6, pilot not scored | trial 1 reply 1; trial 2 replies 3 and 4 |
| Reference 6 dB louder | 3 of 7 | trial 1 reply 1; trial 2 replies 1 and 2 |
| Reference 12 dB louder | 4 of 7 | trial 1 reply 1; trial 2 replies 2 and 4; the pilot |
| Reference 18 dB louder | 3 of 7 | trial 1 reply 1; trial 2 replies 3 and 4 |
| Capture 12 dB quieter going in, restored coming out | 2 of 7 | trial 1 reply 1; trial 2 reply 4 |

The louder reference was the one I expected to work. The canceller's source
estimates leftover echo as the render's power times an assumed path gain of
one whenever its filter is not usable, and here the path gain is three. It
moved the admissions around and removed none reliably. In every variant the
prompts passed unchanged, at the same block and level.

These experiments ran in a separate build with the change switched by an
environment variable. None of it is in the tree.

## Proposed

1. **No change to the canceller or the reference.** Nothing I can justify.
2. **The level is the lever, and it can be checked offline before a
   conversation is tried.** For any bundle: the lamp's own voice at its
   microphone against the prompt at the same microphone, and the clipped
   samples during a reply. In these runs that is +10 dB to +20 dB and
   hundreds of samples. While the lamp is louder to itself than the person
   is, no measurement taken at the microphone can see the person, and my
   evidence cannot either: `capture_explained` reads 97 % or more with or
   without a prompt-level talker underneath. The volume and the capture gain
   are yours.
3. **Speech probability alone cannot admit a turn while the lamp is
   speaking.** In all four admissions `render_active_blocks` was 20 to 50 of
   the last 50, and in both correct ones it was 0. That is not an echo
   threshold and needs none: it is whether accepted speaker PCM was
   non-silent in the last half second, which the capture worker already
   knows. What admission does with it is yours. I am not proposing to hide
   or alter the probability at its source.
4. **The offline check, for whichever change you make.** Replay the bundle,
   score `segment-*-recorded_post_aec.wav` with `SpeechProbability`, and
   look for six blocks at 0.80 or more inside a reply. The scratch test I
   used is twenty lines and touches your crate, so it is not in the tree. I
   will send it as a proposal if you want it.

## Not established

- **That nobody spoke.** See the second table. The room audit decides
  trial 2.
- **Why the path is non-linear**, and at what volume it stops being so.
  There is no recording at another volume with the same build.
- **What a person speaking over the lamp looks like here.** No recording has
  one.
- **The fourth admission offline.** The replay differs from the device in
  1.7 % to 2.5 % of samples with noise suppression off, and trial 2's third
  admission is not reproduced by it.
- The linear fits use a crude 24 kHz to 16 kHz conversion of the reference.
  The same fit at the 4 kHz analysis rate, where both streams are reduced
  the same way, gave the same figure for the pilot.
