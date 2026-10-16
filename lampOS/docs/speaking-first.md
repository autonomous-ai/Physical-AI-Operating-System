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
  larger still (`{"look_full":ID}` on the eyes' permit socket, a new id
  for every ask, counted from the clock when the service began;
  `look_full_asked` in the record), taken once the head has settled
  ([eyes.md](eyes.md), "A full look"), while the lamp is still saying
  "Let me look". The eyes mark the still with the id (a JPEG comment,
  `lamp-look ID`) in their own file, `--full-look-file`, which the voice
  reads from `LAMP_EYES_FULL_LOOK_FILE` (a full look needs both it and
  `LAMP_LOOK_FULL=on`; the worker is given that path as `--eyes-look`). The
  working frame stays in the sight file. The look then waits until that file holds the still
  with this ask's id, or at most 2.6 s after the lamp said it would look
  (`look_full_still` in the record, with how long after and whether the
  still was there). The worker is started with `--eyes-look-full` after
  `--eyes-look PATH` and takes a picture up to 15 s old instead of 1 s
  (`FULL_FRESH`, as long as a look may wait to begin, and as long as the
  eyes keep the still; at 3 s a look that began late went without it, #842),
  and of up to 184 KiB instead of 64 (`lamp_gemini::MAX_LOOK_IMAGE_BYTES`).
- A still for writing on paper (#941): with `LAMP_LOOK_FULL_SIDE=N` (320 to
  4096, off unless set) every full look asks the eyes for a still N across,
  `{"look_full":ID,"side":N}`, made to 184 KiB instead of 1024 across and
  64 KiB; `side` goes into `look_full_asked` in the record. One look may ask
  its own side instead (`sight::Look::side_next`): what a skill tool's look
  step will ask with `look = { side = N }` once skill tools (#984) carry it to
  the voice. Unset and unasked, the ask and the still are as before.
  A full look that goes without its still says so in one journal line,
  "a full look went without its still: asked for ID, the file held ..., N
  ms old" (nothing, another ask's still, or a picture with no id; no
  picture), and a worker that finds no picture to send says "the lamp
  looked: no picture with request N: WHY". The
  note (`LOOK_FULL_NOTE`) adds: "If you cannot make out what they are
  showing you, say so plainly and ask them to hold it a bit closer to you;
  never guess what it might be." A full look without its own still goes
  with no picture and the note that the eye was shut (`NO_LOOK_NOTE`): no
  still, an earlier look's still the eyes still hold, a cancelled look's
  still written after this ask, or eyes started
  without `--full-look`, which write no still. A write time could not tell these apart; the id
  does.
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

## Asked by another part: the say service (#939)

Off unless `LAMP_SAY=on`. Another part of the lamp, `lamp-skills` first
([skills.md](skills.md)), asks the voice to say something unprompted, and
is told what became of it (`crates/voice/src/say.rs`). The hello, task
news and reminders are to move onto it.

**The socket** is `LAMP_SAY_SOCKET`, `/run/lampos-say/say.sock` if unset,
in a directory of its own: the voice runs as root and its own runtime
directory is closed to others. With `LAMP_SAY_GROUP` (a group's name, as in
`/etc/group`) the directory is 0750 and the socket 0660, both that
group's; without it both are the voice's user's alone (0700, 0600). A
socket left by a voice that has gone is taken over; one that still answers
is not. One JSON line in:

    {"skill": "reminders", "note": "Remind them, in one short line, to call the bank.", "priority": 8, "expires_in_ms": 120000}

- `skill`: 1 to 64 of `a-z`, `0-9`, `-` and `_`. Logged and traced only.
- Exactly one of `note`, an instruction the model phrases (as the hello's
  and the news'), and `text`, words to say as they are: the model is told
  "Say exactly these words, in your own voice, and nothing else:" before
  them. At most 900 bytes, no control characters.
- `priority`, 0 to 9, 5 if left out: higher first, the oldest first among
  equals.
- `expires_in_ms`, 1 ms to an hour, from when its line was read: not begun
  by then, it is `expired`. Its time is looked at again as it is begun, so
  that none begins after it, however long it waited for the voice's loop
  (the reviewer on #954). A line is at most 4,096 bytes with its newline.

When the voice begins the turn, one line, so that the body and the light
can move with the words (the first builder on #939):
`{"event":"begun","at_us":…}`, the moment on the shared monotonic clock
(`lamp_ipc::monotonic_us`) in microseconds; the first sound follows when
the model's comes, about a second later. A say that is never begun has no
such line. Then one line, once the outcome is known; the connection stays
open until then:
- `{"outcome":"spoken"}`: the turn completed with the model's audio played
  whole, the proof a reminder's acknowledgement takes (complete, with
  audio, no playback gaps, no owner left, the microphone allowed).
- `{"outcome":"dropped","why":"…"}`: begun and not spoken: the reason the
  turn was cancelled (`user_interrupted`, …), or what became of it
  (`no_audio_answer`, `provider_failed`,
  `audio_written_with_playback_gaps_unscored`). Never tried again here: the
  asker decides.
- `{"outcome":"expired"}`.
- `{"outcome":"refused","why":"…"}`: a request that is not one (the reason
  names the field and never repeats what was asked), no whole line within
  1 s of being taken (one deadline for the line, so that a client sending a
  little at a time cannot keep its place), 16 already waiting, or the voice
  going.

**When it is said.** After a due reminder and before the hello and the
news, through a reminder's gates: no reply owed or queued, the speaker
free, nobody's utterance pending, the microphone's capture fresh and a
second since the last speech, the playback mask free, a ready session and
the keeper's pause off; and a quiet since the last turn of 15 s (as the
news), or 1 s from priority 8 up (as a reminder). It is a written turn like
the others (`skill_says` in the record, with the skill and the priority;
never the words), the lamp's own turn to the attention rule and the
Policy's hold. The Policy is not asked here: whoever asks the voice to
speak has asked the Policy's gate first ([skills.md](skills.md)).

**Most likely wrong:** a say of priority 8 or more in the second after a
conversation's last turn: a reminder takes that second today, and a skill
may not need it.

## Tests

`say::tests`: a request read and checked, each refusal naming its field
and never repeating what was asked; the highest priority first, then the
oldest, the urgent needing a second's quiet and the rest fifteen; those
not begun in time told `expired`, also when the voice takes one only after
its time (looked at again as it is begun); a line of 4,096 bytes with its
newline read and one of 4,097 refused; a ticket dropped unanswered told
`dropped`; the answers one line each; the group found by its name; and
through a real socket, an ask handed to the voice and its answer written
back, begun first and spoken after, a bad one refused at once, sixteen
sending a little every 300 ms refused a second after each was taken and
their places given back to a prompt ask, a request of 1 ms first looked at
200 ms after its line was read told `expired` and never begun, the socket the voice user's
alone, a
second voice unable to take a socket still answered, the socket gone with
the voice. `coordinator::tests::say`: through the real written turn and
speaker control, two generations accepted and retired and the provider's
completion `spoken` (not before the turn settles); a turn cut short by a
person `dropped` as `user_interrupted`; one the model left silent `dropped`
as `no_audio_answer`. The gates in the voice's loop are a reminder's, with
no test of their own here: they are to be seen on lamp-4ace.

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
