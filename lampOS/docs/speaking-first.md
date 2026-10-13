# Speaking first

`crates/live/src/speak_up.rs`, with `lamp-eyes --presence-file` and a written
request in `lamp-gemini`. Status on 2026-10-13: built, host tests, and the
provider's part tried on lamp-4ace without a sound being played. The lamp
has not yet said hello to anyone. On lamp-4ace it runs in its watching
mode: the moments it would have spoken are recorded and nothing is said.

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

Someone already there when the service starts was never away and gets none.
A hello that is due but cannot be begun (the lamp is in a turn) stays due
until it can be.

`LAMP_EYES_PRESENCE_FILE` in the voice service's environment names the
eyes' file; without it none of this runs. `LAMP_SPEAK_FIRST=on` lets the
lamp say the hello. Without that the moment is recorded (`hello_due` in the
session record, a line in the journal) and nothing is said.

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

Tried on lamp-4ace through its relay with `lamp-live provider-check CONFIG
--written NOTE`, which opens a session, sends the one request and plays
nothing: with a note much like this the model answered "Hey! There you are.
Good to see you.", 2.5 s of sound, the first of it 0.91 s after the note.
The other form a note could take, text inside a request's own activity
(`--note`), made the service close the connection (code 1007), three times
in three; `try_note` is kept for a service that takes it and nothing here
uses it.

A written request goes only onto a connection that owes nothing. Sent
while an earlier answer is unsettled it is not written, and ends as a
request the service left unanswered.

## Tests

`speak_up::tests`: one hello to someone back after five minutes and none
while they stay an hour, looking away now and then; none for someone there
at the start, away two minutes, walking past, or arriving into a room in
conversation until it has been quiet for half a minute; none again within
twenty minutes; a hello that could not be begun stays due; and the eyes'
file counts only fresh and only for a near face. `session::tests` in
`lamp-gemini`: a written request is one message with no activity opened,
what comes back is its own, and an ordinary request follows it. The eyes'
line has a test beside the kept frame's.

Not shown: the lamp saying it. What a hello sounds like in a room, whether
five minutes and twenty are the right numbers, and whether a face of 0.14
is a person at the desk and not one walking behind it, are to be seen with
people; the watching mode is there to count the last of these first.
