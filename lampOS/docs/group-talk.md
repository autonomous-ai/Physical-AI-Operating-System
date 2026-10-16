# People talking around the lamp

What the lamp does when it is not alone with one person: several people
standing at it, or colleagues in conversation at the next desk. Status on
2026-10-13: four measures are on lamp-4ace and each has been seen to act
there, the last two only in a staged scene of played voices. With real
people talking around it the last two are not yet shown.

## What happens

The lamp ends a turn after 0.6 s of quiet and its answer begins 1.0 to 1.5 s
after that. Any speech it hears in between takes the turn: the answer is
dropped (`user_interrupted`) and the new speech becomes the next thing sent
to the model. Alone with one person that is right: they paused in the middle
of a thought and went on. In a group it means the lamp never gets a word in.

On lamp-4ace on 2026-10-13 people stood at the lamp for seven minutes,
talking to it and to each other (47 turns in the service's record):

- 21 of the 47 turns were replaced by more talk, 0.01 to 1.25 s after the
  end of speech was decided, all before any of the provider's answer had
  arrived.
- One of them asked "Why you not speaking up?".
- Twice the lamp answered a scrap of their talk to each other.

Later the same day, with two colleagues in conversation beside it, three
questions played to it by name in one scene ("Hey Lamp, what do you see in
front of you right now?", "Lamp, what is two plus two?") all went
unanswered: each was replaced by their talk before the lamp spoke.

How loud a voice is does not tell these apart. People standing at the lamp
are all as loud as the person speaking to it (a median of -15 to -29 dBFS
after cancellation that morning, either way).

## What is done about it

**Voices that are too faint are not speech** (`--faint-floor-db`, see
[serve-runtime.md](serve-runtime.md), "How loud an utterance was"). At
-40 dBFS this removed the phantom utterances from across the office. It
does nothing for people at the lamp.

**The level follows the room** (`crates/voice/src/room_floor.rs`,
`LAMP_FAINT_FLOOR_MAX_DB`). -40 was not enough in a working office: the
talk at the next desks arrived at a median of -27 to -35 dBFS, every
sentence of it lit the lamp up, and about thirty times in fifty minutes the
lamp answered. Set to -28 by hand it heard a third as much and answered
none of it. But a level right for that room at four in the afternoon turns
a quiet voice away from an empty one at eight. With the environment
variable set, the floor moves between `--faint-floor-db` and that highest
level. It follows the speech that was not for the lamp: what the detector
scores as speech outside any turn, and what was heard in turns that ended
by themselves with the lamp saying nothing; never a turn the lamp
answered, anything heard while it speaks, or a turn replaced by more talk.
It sits three decibels above the middle of that as it fades (by half in
about two and a half minutes), needs about three seconds of such speech to
move at all, and is back at its lowest within about a quarter of an hour
of a room emptying (six halvings take the most there can be under those
three seconds). Each move of a decibel or more is in the record as
`room_floor`, with `room_alone_dbfs`: where the room's talk alone would
put the floor before the two limits. That number is recorded with a fixed
floor too, so a room can be watched before the floor is let go in it.
Host tests for the rule (`room_floor::tests`).

**A question put to the lamp stays its own** (the character file,
[character.md](character.md)): "you will often be asked something and then
hear other people talking to each other before you have had the chance to
answer … answer it at the first pause, once, as if the talk in between had
not happened". The model hears everything in the session, so it can do
this. With that line alone and the two colleagues still talking, six
questions played by name 25 s apart: two answered within 2 s, one 11 s late
at the first pause, three not at all (18 turns were replaced by their talk
during the run). Before the line: none of the three above.

**In a busy room a just-asked lamp keeps its turn**
(`crates/voice/src/coordinator.rs`, `HOLD_FLOOR_US`; `Attention::busy` in
`crates/voice/src/body_link.rs`). The room counts as busy when the last three
things the lamp heard went unanswered: nothing it said, in a row, with less
than half a minute of quiet since. Then, from the moment a person's speech
is judged ended until the answer begins to play, a new sound does not take
the turn: the candidate is suppressed as the lamp's own playback would be,
and the service records `floor_held` once for that turn. If no answer has
begun to arrive 1.8 s after the speech ended, the hold lapses and the next
voice takes the turn as before, so a turn the model declines to answer
costs the room at most that. Once the lamp answers, the count is zero again
and the next three turns are handled as they always were.

It is tied to a busy room because of what it costs: speech that begins
during a hold is not sent to the model at all. For one person who pauses
and goes on, that would lose the start of their second half, and the lamp
would answer the first.

**A short sound does not cut off an answer still to come
(`LAMP_WAIT_KEEPS_ANSWER=on`, #1123; off unless set).** On lamp-52e6 on
2026-10-16 (the sixth, 16:5x), short sounds with no words, 0.6 to 1.4 s
long, admitted 1.4 to 2.6 s after a question, ended 4 of 10 waiting answers
as `user_interrupted` on the backup voice, whose answers begin later. The
owner's session on lamp-4ace had dozens. A new sound took the turn as soon
as it began, before anything of it was known, and no words of it can be
had before then: the model hears them only once it takes the turn, which
is what ends the old answer. With the switch, in any room, while an answer
is awaited and none of it has sounded (the person's input ended, no
provider audio, nothing playing), a new sound waits instead of taking the
turn at once (`while_answer_waits` in `crates/voice/src/coordinator.rs`):
- it stays a pending candidate, its audio kept from its start;
- 1 s after its trigger (`WAIT_KEEPS_FOR_US`), or 100 ms before its own
  decision deadline if that is sooner (`WAIT_DECIDES_BEFORE_US`), it is
  decided: let go if it has been quiet for 300 ms (`WAIT_QUIET_BLOCKS`),
  and otherwise it takes the turn as before, from its start;
- one that has ended before then is let go at once.

Let go, the answer goes on; the record says `input_let_go_answer_waits`
with why (`quiet` or `ended`), and `answer_waits` once for each sound that
waited. Both voices are covered, as the backup voice takes its turns from
the same place (`coordinator/second.rs`). On the product's duplex hold a
candidate may wait 1.2 s for its decision, which is where the 1 s comes
from; on the standard one (200 ms) the wait ends 100 ms before it, so
nothing is lost there. The cost: a real follow-up shorter than about a
second ("No.", "Wait.") said while the answer is still to come is let go,
and the lamp answers what was asked first; said again once the answer
plays, it cuts in as before. Tests, on the real admission, detector and
mask with the duplex hold (`coordinator/tests/turn_taking.rs`):
`while_an_answer_waits_a_short_sound_is_let_go_and_the_answer_goes_on`,
`while_an_answer_waits_a_sound_that_goes_on_takes_the_turn_from_its_start`
(with a follow-up that pauses 0.2 s, taken),
`the_wait_is_only_while_an_answer_is_unheard_and_never_outlasts_the_hold`.

The first build of this ended the service the first two times it acted
(lamp-4ace, 12:22 and 12:28 on 2026-10-13, restarted by systemd four
seconds later each time, with its conversation gone). It dropped the
candidate with `suppress_prospective`, which checks the authority snapshot
against the time it is given, and it was given a time read a few
microseconds before that snapshot was issued. It now lets the candidate go
with `InputAdmission::release_prospective`, which consults no authority and
has its own test.

**In a busy room the lamp finishes the start of its answer**
(`KEEP_ANSWER_US`). A turn whose speech ended while the room was busy is
remembered, and for the first four seconds that the lamp is heard answering
it, what the microphone hears is not scored as speech: nothing over the
lamp's voice stops it and nothing starts a new turn. The service records
`answer_kept` once for the turn. Most answers are shorter than four
seconds; a story can be stopped once they have passed. The cost is that the
person who asked cannot stop it in those four seconds either.

## What is and is not shown

- `body_link::tests`: when a room counts as busy, and that an answer or
  half a minute of quiet ends it. `admission::tests`: that a pending
  candidate can be let go without authority and an accepted stream cannot.
  The hold and the kept answer themselves are a few lines each in the
  service's main loop and have no test of their own.
- A staged busy room on lamp-4ace (`hold.sh`: three lines between two
  played voices, a question to the lamp by name, and the other voice
  starting a third of a second after the question; twice over). With the
  hold alone: the second question's answer began 1.32 s after the
  end-of-speech decision, under the other voice ("It's a beautiful, bright
  blue."), `floor_held` once, and that voice stopped it 1.2 s in. With the
  kept answer as well: both questions answered whole ("Four. Easy one.",
  "It's bright blue! Such a nice, cheerful color."), 1.40 and 1.28 s after
  the decision, `answer_kept` twice, and nothing said to the lines that
  were not for it. No restart in either run.
- That is played speech, one scene, twice. With the two colleagues really
  talking beside it, only the first two measures have been tried.

Not solved, and known:

- After those four seconds, and in a room that does not count as busy, a
  voice over the lamp is judged by the rules for being interrupted
  ([duplex-listening.md](duplex-listening.md)), and people who carry on
  talking beside it can stop it.
- A room is busy only after three things in a row went unanswered. The
  first question put to the lamp in a conversation that has just begun
  beside it is handled as it always was.
- The lamp cannot tell who is speaking to it except by what is said. Its
  eyes report where a face is, and nothing uses that to decide whose turn
  it is.
- The lamp's light and head still react to every one of the turns it does
  not answer, until the head settles after two.
