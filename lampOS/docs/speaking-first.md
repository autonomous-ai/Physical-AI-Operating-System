# Speaking first

`crates/voice/src/speak_up.rs`, with `lamp-eyes --presence-file` and a written
request in `lamp-gemini`. Status on 2026-10-13: built, host tests, and the
provider's part tried on lamp-4ace without a sound being played. The lamp
has not yet said hello to anyone, nor taken a look on being asked. On
lamp-4ace the hello runs in its watching mode: the moments it would have
spoken are recorded and nothing is said.

## What it is for

Everything else the lamp says is an answer. A person at the next desk also
looks up and says a word when a colleague comes back. This is that, and
only that: one short hello to someone who comes and sits down. It is not an
announcement, it is not for people walking past, and it never goes into a
conversation.

## When

The eyes keep one line fresh while a face is near (`--presence-file`: the
face's height as a share of the picture's, no picture, nothing else; a file
not written for a second and a half means nobody). From that the voice
service decides (`Arrival`). A hello is due when all of these hold:

| | |
|---|---|
| someone is there | a face at least 0.14 of the picture's height, for three seconds without a break |
| they were away | nobody for five minutes or more before that. A face lost for under twenty seconds is someone who looked away |
| the lamp is at rest | nothing said to it or by it for half a minute, nothing being heard, a provider ready |
| it has not just done this | no hello in the last twenty minutes |
| it is not paused | the keeper has not said `{"paused": true}` on `lamp/pause` ([keeper.md](keeper.md), "Named states and the pause") |

Someone already there when the service starts was never away and gets none.
A hello that is due but cannot be begun (the lamp is in a turn, or paused)
stays due until it can be. The pause holds every turn the lamp begins
itself the same way: the look it said it would take, the second look, a
reminder and news.

`LAMP_EYES_PRESENCE_FILE` in the voice service's environment names the
eyes' file; without it none of this runs. `LAMP_SPEAK_FIRST=on` lets the
lamp say the hello. Without that the moment is recorded (`hello_due` in the
session record, a line in the journal) and nothing is said.

On lamp-4ace it was only recorded from 2026-10-13 to 15, and in 680 voice
sessions it was never once due. Fifteen hours of what the eyes published
(a bus recording from 2026-10-14 18:23 to 10-15 08:46) say why: no face
reached the 0.14 of the picture's height that counts as someone at the
lamp (the largest were 0.11 to 0.14). At rest lamp-4ace's camera looks
across the side of the desk, and the people at the computer were at the
very edge of its view. Turning the lamp to face the seat is the fix; the first five minutes
([onboarding.md](onboarding.md)) has the lamp ask its person to do that. A
false "face" that the eyes report all night in one spot is #763. It is on since 2026-10-15, 07:34. Its voice side was tried at 07:59
the same day by writing a near face's size into the presence file by hand
for eight seconds, 21 minutes after the last person was seen: the lamp
said, unasked, "Oh, hey! Back again. It's good to have some company." The
eyes writing the file for a real person sitting down is still to be seen.

## How it is said

The lamp cannot be handed words to speak: its voice is the model's. So the
turn is a request like any other, with a written note in place of speech
(`ProviderInput::Written`, `InputSender::try_written`): one `clientContent`
message, complete, outside any activity, and whatever the model answers is
played like any answer. The note is:

```text
(Nobody has spoken. Someone has just come and sat down in front of you
after being away for a while. Say hello the way a friend at the next desk
would: a few warm words and no question they have to answer.)
```

Tried on lamp-4ace through its relay with `lamp-voice provider-check CONFIG
--written NOTE`, which opens a session, sends the one request and plays
nothing: with a note much like this the model answered "Hey! There you are.
Good to see you.", 2.5 s of sound, the first of it 0.91 s after the note.
The other form a note could take, text inside a request's own activity
(`--note`), made the service close the connection (code 1007), three times
in three. That was a request with a note and no sound in it. Beside a
request's sound the service takes one, and that is how the model is told
the lamp's name was heard ([attention.md](attention.md)); `provider-check
CONFIG --note-in-speech NOTE SOUND` tries that form without a microphone
or a speaker, sending a recorded question as if heard.

A written request goes only onto a connection that owes nothing. Sent
while an earlier answer is unsettled it is not written, and ends as a
request the service left unanswered.

## Taking a look

The same written request is how the lamp sees when it is asked to, without
paying for sight in every answer ([eyes.md](eyes.md): a picture anywhere in
a session's history made every later answer about half a second later, so
giving the model one with everything said was switched off).

- The character file tells the lamp that it sees when it looks, and that
  when someone asks what it sees, shows it something or asks how they look,
  it says "Let me look." and nothing more.
- The coordinator hears the lamp's own words as they arrive. An answer that
  opens with "let me look", "let me take a look" or "let me have a look"
  (`speak_up::asks_to_look`; not "let me see", which people say before any
  answer) is the lamp asking for its eyes: `look_asked` in the record.
- When the lamp has finished saying so, the next turn is its own: a written
  request with `look` set. The provider worker puts the eyes' newest frame
  with the note (`--eyes-look PATH`, from `LAMP_EYES_LOOK_FILE`; a frame
  written in the last second, at most 64 KiB) and the model answers what
  was asked from what it sees. If the eyes are shut the note says so
  instead, and the model is not left to describe a picture it was not
  given. If someone speaks before the look begins, they have the turn and
  the look is dropped.
- Once nothing has been submitted for twenty seconds after a look, the
  worker renews the session (`renew_when_idle`), so the picture leaves the
  history and answers are as fast as before. The conversation, with what
  the lamp said it saw, goes across in the thread
  ([conversation-thread.md](conversation-thread.md)). Until then a
  follow-up ("and what colour is it?") is answered from the same picture.

- With `LAMP_LOOK_FULL=on` (#836, off unless set) a look is a full look.
  As soon as the lamp's words ask for its eyes, the eyes are asked for one
  larger still (`{"look_full":true}` on the eyes' permit socket,
  `look_full_asked` in the record), taken once the head has settled
  ([eyes.md](eyes.md), "A full look"), while the lamp is still saying
  "Let me look". The look then waits until the eyes' file has been written
  since that ask, or at most 2.6 s after the lamp said it would look
  (`look_full_still` in the record, with how long after and whether the
  still was there). The worker is started with `--eyes-look-full` after
  `--eyes-look PATH` and takes a picture up to 3 s old instead of 1 s. The
  note (`LOOK_FULL_NOTE`) adds: "If you cannot make out what they are
  showing you, say so plainly and ask them to hold it a bit closer to you;
  never guess what it might be." Eyes without `--full-look` take the ask
  for nothing and keep writing their working frame, so the look goes at
  once with that frame, as before but with the new note. A full look whose
  file was not written since its own ask (no still, or only an earlier
  look's still the eyes still hold) goes with no picture and the note that
  the eye was shut (`NO_LOOK_NOTE`).
  Without the variable nothing is asked, waited for or said differently.

A picture leaves the lamp only on such a look: when a person has asked the
lamp to and the lamp has said it will.

Tried on lamp-4ace without a sound being played (`lamp-voice provider-check
CONFIG --look PICTURE.jpg NOTE`): given a 21 KiB photograph of the office
and a note like the one above, the model answered "I see a busy desk and a
whole office full of chairs and computers.", the first of it 1.49 s after
the request.

Played on lamp-4ace on 2026-10-15 at 07:35, with the character's line in
(`lampOS/deploy/character/lamp.txt`): "Hey Lamp, what do you see right
now?" → "Let me look." (0.98 s after the end of speech), and 2.3 s later
"I see your desk with the computer monitors, a keyboard, and that white
water bottle." A picture of 13,048 bytes went with the look. The talk then
paused, so the session was renewed without the picture, and the follow-up
23 s later ("And what colour is the wall?") took a look of its own: "The
wall behind your monitors looks white."

## Telling it something that needs no answer

A third written form tells the model a fact and asks for nothing: one
`clientContent` message whose turn is left incomplete
(`InputSender::try_context` in `lamp-gemini`, `wire::encode_context`). It
is no request: nothing is owed for it, the next thing heard is numbered as
if it had not been written, and it is written only at a moment when no
request is open and no answer is unsettled (otherwise it is dropped, for
whoever offered it to offer again).

Tried on lamp-4ace through its relay without a sound being played
(`lamp-voice provider-check CONFIG --context FACT --written NOTE`, and
`--context FACT --then-sound-and-written NOTE`, which sends a second of
near-silence as a heard request in between):

- Nothing came back for the fact alone, in each of twelve runs (nine
  waited three seconds for it; three went on to the near-silence).
- It was taken in. Told a name this way, the model answered "do you know
  my name?" with it, 0.93 s after the question; the same question to a
  session not told got "I'm afraid I don't!". One run each.
- A session takes sound after it: three times in three the near-silence
  ended as unanswered, the connection stayed ready, and the written
  question after it was answered ("Four.").
- It changes what the model does with a written line, one run each: "you
  hear someone say, without calling you: what is the weather like
  outside?" went unanswered by itself, was answered after "the office has
  emptied... what they say aloud is said to you", and after "the office is
  full and people are talking to each other" ended with "..." and no
  sound.

The first use is for a new session (`crates/voice/src/session_fact.rs`,
`LAMP_SESSION_FACT=on`): being new, a session takes the first thing it
hears for the start of a conversation with it. Of six that opened on
lamp-4ace in a talking office with nothing told to them, two answered a
colleague's sentence to a colleague. With the switch on, each new session
is told once, unless the lamp said something to someone in the last 45
seconds:

```text
(For you to know, not to answer: you have been at your desk for a while,
and nobody is talking to you at the moment.)
```

The second use is the room (`crates/voice/src/room_told.rs`,
`LAMP_ROOM_FACTS=on`; [character.md](character.md), "The rule follows the
room"). The provider worker sees every request and whether the lamp spoke
in it, and from that alone calls the room quiet (nothing said for five
minutes; a sound the coordinator dropped does not count) or busy (three things in a row, inside a minute, that the lamp
said nothing to). When that changes, and when a session opens, the session
is told one of:

```text
(For you to know, not to answer: the room around you has gone quiet.)
(For you to know, not to answer: the office around you is busy, with
people talking to each other.)
```

only when nothing has been heard or said for two seconds and no
conversation is going on. With both switches on, a session that has been
told how the room is is not also told the plain fact. Host tests for the
rule (`room_told::tests`); the worker's telling is seen in the journal
("told the session"). Not yet heard on a lamp.

It describes and does not ask for quiet, and a test holds it to that
([conversation-thread.md](conversation-thread.md) has what asking did).
Told this, a session still answered a question put to it by name in a
written line, three times in three. The switch is off: what the fact does
to the first thing a session hears has not been measured, and written
lines in place of speech cannot measure it, since an uncalled written line
is left alone with or without it.

## Tests

`speak_up::tests`: one hello to someone back after five minutes and none
while they stay an hour, looking away now and then; none for someone there
at the start, away two minutes, walking past, or arriving into a room in
conversation until it has been quiet for half a minute; none again within
twenty minutes; a hello that could not be begun stays due; the eyes'
file counts only fresh and only for a near face; and only the lamp saying
it will look asks for a look. `session::tests` in
`lamp-gemini`: a written request is one message with no activity opened,
what comes back is its own, an ordinary request follows it, and a look
is the same one message with the picture before the note; a fact is one
incomplete turn, is no request, and is not written while an answer is
owed. `session_fact::tests`: only `on` turns it on, and the fact asks for
no quiet. The eyes' line has a test beside the kept frame's. The worker
telling a new session has no host test: it is seen in the service's
journal ("told the new session").

Not shown: the lamp saying it. What a hello sounds like in a room, whether
five minutes and twenty are the right numbers, and whether a face of 0.14
is a person at the desk and not one walking behind it, are to be seen with
people; the watching mode is there to count the last of these first.
