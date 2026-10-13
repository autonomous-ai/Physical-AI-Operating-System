# The message types

`lamp-msgs` (`crates/msgs`) is what the first topics of [the bus](bus.md)
carry: one Rust structure for each, in one crate, so that a publisher and its
subscribers share a definition and not two copies of one. It is a library
and nothing else: no process, no socket, no clock.

## The rule

- One type per topic. Its publisher declares it, a subscriber names the one
  it expects, and the bus refuses a second publisher or a subscriber that
  names another ([bus.md](bus.md), "Declaring a topic"). The bus compares
  names. It does not look inside a payload.
- The types are here and nowhere else. A crate that publishes or subscribes
  depends on this one and does not write its own copy of a message.
- JSON. Each type is a small flat structure sent as one JSON object, with
  ROS's field names and units where ROS has the type: SI units, angles in
  radians, times in seconds.
- A field this version does not know is ignored, so that a newer publisher
  does not break an older subscriber. ROS refuses any difference in a
  definition; one team on one machine can do with less.
- A field that may be absent is an `Option`, left out when there is nothing
  to say. Every other field has to be there.
- A value of an enumerated field that this version does not know is an error
  for that message: a state is not guessed.
- Reading the JSON checks a message's shape. Its sense, a light's colour and
  level or the lengths of a joint state's lists, is checked by the type's
  `check()`, which the reader asks for.

Each type has `TYPE`, the name the bus compares (`lamp_msgs/JointState`),
`VERSION`, `TOPIC` or `TOPICS`, and `declaration()`, which its publisher
hands to `Bus::publisher_declared`. `first_topics()` lists every topic with
its declaration.

## The types

| Type | Topic | Kind | Period | What it says |
|---|---|---|---|---|
| `Audio` | `mic1/audio`, `mic2/audio` | event | 20 ms | A block's header: sample rate, channels, sample format, the block's count. The samples follow it in the payload as they are (`Audio::block`), under the encoding `json+pcm` |
| `ImageRef` | `camera/image` | latest | 500 ms | The absolute path of the newest frame's file, its width, height and encoding, and when it was captured |
| `JointState` | `body/joint_states` | latest | 100 ms | ROS's `sensor_msgs/JointState`: `name`, `position` (rad), `velocity` (rad/s), `effort` (N m). And `commanded` (rad), which ROS's has not |
| `Faces` | `humans/faces/tracked` | latest | none | A list of faces: a passing id, a box in fractions of the picture as in ROS4HRI, confidence, yaw if it was estimated, whether it faces the lamp if that was judged |
| `Speech` | `voice/heard`, `voice/said` | event | none | Text, whether it is finished, and the locale and the request's number if known. After `hri_msgs/LiveSpeech` |
| `VoiceState` | `voice/state` | latest | 1 s | Resting, listening, thinking or speaking |
| `VoiceLink` | `voice/link` | latest | 1 s | Ready, or unavailable with one of seven kinds of reason |
| `RoomState` | `room/state` | latest | none | Unknown, quiet or busy |
| `Light` | `light/command`, `light/state` | latest | 1 s | On or off, one of ten colours by name, a level from 1 to 5 |
| `Diagnostic` | `diagnostics` | event | 1 s | ROS's `DiagnosticStatus`: ok, warn, error or stale, a name, a message, a few keys and values |
| `Decision` | `policy/decisions` | event | none | What the policy decided ([policy.md](policy.md)): what it was about, the choice, its probability, the decision model's answer to each question by the question's name, who decided, how long the asking took, whether anything acts on it, and the request's number if known. Not what was heard |

The kind and the period are the table of first topics in [bus.md](bus.md)
read as declarations. A stream is `event` and a latched topic `latest`. "How
often" is the period, and where that table gives two rates the slower is
declared, because a period is a promise. "With each look", "with each thing
said" and "when it changes" promise none. No lifespan is declared: the table
has none. A topic that promises no period and can be silent for longer than
the bus's lease needs its publisher to call `alive()`.

`policy/decisions` was added after that table was written and is not in
it: this table is where it is listed. `first_topics()` has it with the
others.

## Adding one

1. Write the structure in `crates/msgs/src/lib.rs` with `Serialize` and
   `Deserialize`: flat, no generics, each field's unit in its comment, and
   without `deny_unknown_fields`.
2. Give it `TYPE` (`lamp_msgs/` and its name), `VERSION` 1, `TOPIC` and
   `declaration()`, and add it to `first_topics()`.
3. Test that it is written as the JSON intended, reads back, ignores an
   unknown field and refuses a missing one. Add its row to the test of
   declarations, to the table above and to the first topics in bus.md.

Changing one: an optional field can be added with nothing else. A new
required field, a new enumerated value, a changed unit or meaning is a new
`VERSION`, which the bus shows and does not check. Under a new name it is
another type, and a topic's declaration file has to be removed before the
topic can be published as that.

## Not shown

Run on a development Mac (x86_64) only: 12 tests of the types and one that
declares the fourteen topics on a real bus and sends six of the types
through it. The example in the crate's documentation is compiled, not run.
Type-checked for the lamp's processor, not run on Linux, never on a lamp.
The voice publishes `Speech` and `VoiceState`
(`crates/voice/src/bus_out.rs`), and the policy reads them and publishes
`Decision` ([policy.md](policy.md)). Nothing uses the other types yet. The
actions program still publishes
`{"rgb":[r,g,b]}` on `light/command` and reads the same from `light/state`,
which is not `Light`. `VoiceLink`'s seven reasons group the voice program's
fault categories and have not been agreed with its owner. Which way a face's
yaw is positive is written down and no detector has been held to it.
