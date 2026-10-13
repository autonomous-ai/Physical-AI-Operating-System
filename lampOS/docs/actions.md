# Actions: doing what the lamp is asked

`lamp-actions` (`crates/actions`) carries out what the lamp is asked to do
to itself. On Codex's design page it is the "executive"; in the loop
drawing it is the arrow from the lamp to its actions. It is step 3 of
[brain-build.md](brain-build.md). What the voice expects of it, and the
wire between them, is in [tools.md](tools.md).

## What it is for

The model that holds the conversation can talk and cannot do anything.
When a person says "a bit quieter", the model names an action
(`volume_down`). This program then does five things, and is the only
place where they are done:

1. checks that the action is one it offers now, and that nothing came with
   it but what that action takes;
2. (later) asks "shall I?" for the actions the owner said need a yes;
3. hands it to the part that owns the hardware, or changes the one control
   it owns itself;
4. reads back that it happened;
5. answers with what was established, in a sentence the lamp can say.

It is general. The conversation is its first caller; the decision model,
the touch pad and schedules are to call it the same way. It knows nothing
about the provider: a call is one line on a local socket.

## The process

    lamp-actions --dir ABSOLUTE_DIR [--mixer-card NAME --mixer-control NAME [--volume-ceiling 1..100]] [--bus-root ABSOLUTE_DIR]

In `--dir` it keeps `tools.json` (what it can do now), `call.sock` and
`ready`. Calls are taken one at a time: each is a few tens of
milliseconds, and two changes to one control must not cross. A caller has
one second to send its line. A second `lamp-actions` on the same directory
is refused while the first answers. It cannot catch a signal; killed, it
leaves the call it was in unanswered, and the voice tells its model so.

Only what can be done now is offered. With no mixer given there are no
volume actions; the light's are offered only while the ring's owner is
saying what it shows, and are withdrawn within about two seconds of it
stopping.

## The actions

| Action | What it does | What it says back |
|---|---|---|
| `volume_up` | One step louder, to the ceiling at most | "My volume is now 70 out of 100; it was 50." or that it is already as loud as it is allowed to go |
| `volume_down` | One step quieter, to the floor at least | The same form, or that it is already as quiet as it goes without being muted |
| `volume_mute` | Volume to nothing, remembering where it was | "My speaker is muted." |
| `volume_unmute` | Back to where it was, or the middle if that is not known | "My speaker is back, at 70 out of 100." |
| `light_on`, `light_off` | On, in the colour and at the level it had; off, which leaves a faint glow | "My light is on, blue." / "My light is off. The faint glow that is left shows I am still listening." |
| `light_colour` | One of ten named colours, and nothing else. Switches the light on | "My light is blue now." |
| `light_brighter`, `light_dimmer` | One level of five either way, keeping the colour. Brighter brings a light that is off back on; dimmer stops at the lowest and does not switch off | "My light is a step brighter, at level 4 of 5." / "My light is already at its dimmest. I can switch it off if you like." |

A failure is said as one: "I could not read my speaker's volume.", "I
could not confirm my speaker's volume changed.", "I asked for it but could
not confirm my light changed.", and for anything not offered "I can't do
that."

The speaker's volume is one ALSA control (on lamp-4ace, `Speaker` on the
card `allwinnerhdmi`: a hundred points over 30 dB). This program is its
only writer. The light is not written here: the wanted light is published
on the bus (`light/command`, `lamp_msgs/Light`: on or off, a colour by
name, a level from 1 to 5) and what the ring's owner reports
(`light/state`) is read back for up to 0.6 s. The ring's owner is the
voice's ring worker ([ring-choreography.md](ring-choreography.md), "The
lamp's own light"), which is also where each colour's and each level's
look is decided.

## What was taken from V1, and what was not

The owner's rule (2026-10-13): V1 is not the reference; each part of it
is judged before anything is carried over. For V1's intent component
(`system/intent`), read in full by a second reader and spot-checked:

| In V1 | Verdict | Why |
|---|---|---|
| The model names a command and never a payload; steps and limits are the code's | Kept | It is what makes a wrong or hostile request harmless |
| Read, change, read back; a result that says what happened | Kept | "Done" must mean done |
| A closed list of ten colour names | Kept | A name a person says, and nothing to get wrong |
| Finding the command by matching the words of the request against lists | Not taken | The conversation model hears the request itself and names the action. In V1 only exact strings ran locally; the rest went to a hosted model with a three-second budget |
| The model moving the body by writing `[HW:/path:{json}]` tags into its reply, extracted by a pattern | Not taken | A reply is for a person; an action is a call with a result |
| Louder by a tenth of the range, quieter by half | Rebuilt | One "a bit quieter" took three or more "louder" to undo. Here both are one step of 20 points (about 6 dB), and quieter stops at a floor of 10; silence is mute's. The step is reasoned, not yet heard by a person |
| The light as arithmetic on red, green and blue (halve; a quarter more; rounding that ends in black) | Rebuilt | A colour by name and one of five levels. The pixels are the ring's owner's, and "dimmer" can no longer round a light into darkness: off is asked for by name |
| On is always warm white | Rebuilt | On is the light as it was left. A person who set it blue and switched it off expects blue back |
| Seven scenes, following, music, "stop talking" | Not yet judged | Each when it is built |

## Not shown

Host tests: the rules against a control that holds, refuses or ignores
what it is given (seven of the volume, five of the light), the list
and the answers' form (two), the program through its socket with a
stand-in control and a real bus in a directory of its own (three), and
the options (one).

On lamp-4ace the volume's functions have been called by the real
conversation model ([tools.md](tools.md)). The step sizes as a person
hears them, and what a volume change does to the voice's hearing of
someone who speaks over it, are unmeasured. The light's rules were
rewritten on 2026-10-14 (a colour by name and five levels); what was run
of them on a lamp is in [ring-choreography.md](ring-choreography.md).
