# Hearing its name

`lamp-name` (crate `lamp-name`, `crates/name`) listens to what the lamp's
microphone hears for the lamp's name, "Hey Lamp" or "Lamp ơi", and says on
the bus when it heard it. It listens for nothing else and decides nothing.

## What it is for

The lamp answers when it is spoken to. The plainest sign of that is its
name, and the lamp could not hear it: the speech service writes down what
was said, and on lamp-4ace it wrote "Lamp" not once in 310 lines of one
day. It wrote "Milan", "Hey man", "Hey Mom", "Amen", or left the name out
([attention.md](attention.md)). So the text can show that the lamp was
called and can never show that it was not.

This part hears the name as a sound, on the lamp, before any text exists.
What it says is one sign among several for the part that decides whom the
lamp answers ([policy.md](policy.md)). It is not a gate: on the first
measurement it missed two or three calls in twenty and was set off by half
of the lines addressed to a similar name.

## The process

    lamp-name --models ABSOLUTE_DIR [--microphone mic1|mic2] [--least SCORE] [--bus-root ABSOLUTE_DIR]
    lamp-name check MODELS_DIR FILE...

| Option | What it is | If left out |
|---|---|---|
| `--models` | A directory holding `melspectrogram.onnx`, `embedding_model.onnx` and `name.onnx` | required |
| `--microphone` | Whose sound to listen to: the topic `mic1/audio` or `mic2/audio` | `mic2` |
| `--least` | The score from which the name counts as heard, from 0.5 to 1 | 0.9 |
| `--bus-root` | The bus's directory | `LAMP_BUS_ROOT`, or `/run/lampos-bus` |

It is its own process: with it gone the lamp talks on as before, and
nothing waits for it. A wrong option prints the usage and exits with 2.
Models that cannot be loaded, or that do not give a score for silence,
end it with 1 before it listens to anything.

| Topic | Type | |
|---|---|---|
| `mic2/audio` (or `mic1/audio`) | `lamp_msgs/Audio` | Reads: what the microphone hears, as the voice publishes it |
| `name/heard` | `lamp_msgs/NameHeard` | Publishes: a score and how long ago the name began, once for each hearing |
| `diagnostics` | `lamp_msgs/Diagnostic` | Publishes: how it is, about once a second, as `name` |

**It keeps no sound.** Samples are held for about a fifth of a second,
until they have been turned into the numbers its models work on, and of
those it holds the last two seconds' worth. Nothing is written down and
there is no option to. Its log holds counts, times and scores.

## Where the sound comes from

The voice owns the microphone. With `LAMP_BUS_AUDIO=on` in its unit, as
well as `LAMP_BUS=on`, it publishes what it hears on `mic2/audio`
(`crates/voice/src/bus_out.rs`, `MicOut`): the sound after its echo
canceller, 16 kHz, one channel, 20 ms to a message, each with a count
that shows when a message was lost. With the microphone closed (the
privacy switch, or a voice that is not permitted to listen) there is no
sound to publish and none is.

That switch is the voice's own and is off unless someone sets it. It is
the one topic that carries sound: with it on, any program that runs in
the lamp's group can hear the room. The always-on voice writes no sound
down (it refuses its recording option), and this does not change that: a
topic is not a file, and nothing on the lamp records it. But it is a door
that was not there, and it is why it has a switch of its own.

## How it hears

Three small models, run every 80 ms (`src/hear.rs`). The design is
openWakeWord's; the last model is ours.

| Model | Takes | Gives |
|---|---|---|
| `melspectrogram.onnx` | The newest 0.11 s of sound | Eight frames of 32 numbers, one for every 10 ms |
| `embedding_model.onnx`, Google's speech embedding | 76 frames, 0.76 s | 96 numbers that say what kind of speech sound that was |
| `name.onnx`, trained by us | The newest sixteen of those, 1.28 s | One score: how likely it is that the name was just said |

The first two are fixed and know nothing of lamps. The third is a small
classifier trained on generated speech of people saying "Hey Lamp" and
"Lamp ơi", and of people saying other things. The files are not in the
repository; they are installed beside the program, and
`lamp-name check` runs them over a file of sound where they are.

A name that is said raises the score for several steps in a row. That is
one hearing (`src/said.rs`): it is said once, when the score falls again
or after 0.8 s, with the highest score it reached and how long ago it
began. For a second after it, scores are passed over, so the same call
is not heard twice.

After sound was lost, or the microphone was closed for more than half a
second, the models start afresh: they need about two seconds of sound
before they give a score again.

## How it says it is

| Level | Message | When |
|---|---|---|
| `ok` | | It is listening, or the microphone is closed and there is nothing to listen to |
| `warn` | `no sound reaches it: the voice is not putting the microphone's sound on the bus` | Nobody publishes the topic it listens to |
| `error` | `its models stopped working: ...` | A model failed on sound it was given. It listens no more until it is started again |

With it: whether sound is coming, how many times it has heard the name,
how long a step takes it, and how much sound was lost on the way.

## Measured

On what lamp-4ace's own microphone recorded on 2026-10-14
([attention.md](attention.md), "Hearing its name in its own sound"): six
recordings of 86 s, 44 lines played from a loudspeaker in eleven
synthetic voices with nobody in the office. This program was run over
each whole recording from start to end, 20 ms at a time, as it would
hear it live, on a development Mac. A call counts as heard if a hearing
began between 0.3 s before the line was started and 1 s after it ended.

| Score from which it counts | "Hey Lamp" (20) | A similar name (10) | Office talk (10) | "Lamp ơi" (2) | Heard with nothing being played |
|---|---|---|---|---|---|
| 0.9, before the echo canceller | 18 | 5 | 0 | 2 | never, in 520 s |
| 0.9, after it, which is what it is given on the lamp | 17 | 6 | 0 | 2 | never |
| 0.999, before | 11 | 3 | 0 | 0 | never |
| 0.999, after | 9 | 2 | 0 | 0 | never |

The 520 s include the lamp's own answers to the lines and the quiet
between them. What sets it off is a line that begins with a name like
its own: "Hey Lan", "Hey Liam", "Hey Lance", "Hey Mom", "Hey man". A
step took 2.3 ms on the Mac.

## Not shown

- **A person.** Every voice was synthetic and played through a
  loudspeaker near the lamp. Real voices, accents, distance and a full
  office are untried, and are what the classifier has to be trained for.
- **Enough lines to tell 17 from 18.** Forty-four lines, four of them
  Vietnamese.
- **The lamp's processor.** The figures above are from recordings, on a
  Mac. What a step costs on the lamp is said in its own report once it
  runs there.
- **Its own tests are small**: the rule for a hearing (three), the
  options and the report (three). The models are not in the repository,
  so no test here runs them; `lamp-name check` and the measurement above
  are what exercises them.
- **The classifier's training** is outside the repository and was one
  afternoon's work on generated speech. A version trained with similar
  names in its speech did worse on the lamp's microphone, not better.
