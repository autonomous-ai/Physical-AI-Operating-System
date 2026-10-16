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

**A short sound does not cut off an answer still to come
(`LAMP_WAIT_KEEPS_ANSWER=on`, #1123; off unless set).** On lamp-52e6 on
2026-10-16 (the sixth, 16:5x), short sounds with no words, 0.6 to 1.4 s
long, admitted 1.4 to 2.6 s after a question, ended 4 of 10 waiting answers
as `user_interrupted` on the backup voice, whose answers begin later. The
owner's session on lamp-4ace had dozens. A new sound took the turn as soon
as it began, before anything of it was known, and no words of it can be
had before then: the model hears them only once it takes the turn, which
is what ends the old answer. With the switch, in any room, while an answer
to someone is awaited and none of it has sounded (the person's input
ended, no provider audio, nothing playing), a new sound waits instead of
taking the turn at once (`while_answer_waits` in
`crates/voice/src/coordinator.rs`):
- it stays a pending candidate, its audio kept from its start (the mask is
  given the memory for it, `PlaybackMask::keep_waited_sound`: a candidate
  holds its 30 blocks of pre-roll and up to 1.2 s more, and with the usual
  128 blocks the oldest were gone and it was rejected,
  `retained_history_unavailable`, losing what was said; the reviewer and
  the first builder on #1130);
- 1 s after its trigger (`WAIT_KEEPS_FOR_US`), or 100 ms before its own
  decision deadline if that is sooner (`WAIT_DECIDES_BEFORE_US`), it is
  decided: let go if it has been quiet for 300 ms (`WAIT_QUIET_BLOCKS`),
  and otherwise it takes the turn as before, from its start;
- one that has ended before then is let go at once;
- once it has begun to wait it keeps these rules when the answer's audio
  arrives, for a cough is no less a cough then; but once the answer sounds
  (its audio waiting for the speaker or playing, as the duplex mask hears
  it), the mask scores a person's speech 0.0 until it has matched the
  lamp's sound, so the sound's pause from then on is the mask's silence.
  It is judged by its pause as it stood when the answer began to sound:
  ended or quiet for 300 ms, it is let go and the answer plays; still
  going, it is decided at once by the duplex rules for speech over the
  lamp, as without the switch, and stays decided (the first builder's
  re-check on #1130). So a cough still going when the answer begins can
  cut it, as before; one that had stopped cannot.

Nothing waits, and a new sound is decided as before, when its block comes
with no echo evidence (the mask keeps such a block only among its last 80
reads and 0.6 s, so a waited sound could not be held); when the turn is the
lamp's own (a hello, a look, a reminder, news, the head's "hm?": no input,
and a short "hi" or "not now" may answer it); when the answer has been
awaited more than 3 s since the person stopped (`WAIT_KEEPS_WITHIN_US`:
it may not come, and a "hello?" to a silent lamp must be heard; the backup
voice gives up a turn that has not begun in 3 s); when the lamp's name was
heard from the sound's start on (`name/rising` or `name/heard`: "Lamp, …
stop" is someone talking to it); and for a sound that begins within 1 s
after one was let go (`AFTER_LET_GO_US`: two in a row are more likely talk).

What it lets go, measured on the product's mask and hold
(`what_the_wait_lets_go_and_takes_by_length`): a sound of 0.75 s or less
(it has ended, or gone quiet for 300 ms, by its decision). From 0.8 s it
takes the turn. So of the sixth's 0.6 to 1.4 s sounds, the 0.8 s and longer
ones still cut the answer. What it costs:
- a real follow-up of 0.75 s or less ("No.", "Wait.") said while the
  answer is still to come is let go, and the lamp answers what was asked
  first; said again once the answer plays, it cuts in as before;
- a follow-up with a pause across its 1 s mark (0.7 s, 0.4 s of quiet,
  0.6 s; or 0.6 s, 0.5 s, 0.5 s) loses its first piece: the piece after the
  pause is taken at once, without it (`a_follow_up_with_a_pause_loses_only_its_first_piece`);
- a real new question in the gap is taken 1 s after its trigger, not at
  once.

Let go, the answer goes on; the record says `input_let_go_answer_waits`
with why (`quiet` or `ended`), and `answer_waits` once for each sound that
waited. Both voices are covered, as the backup voice takes its turns from
the same place (`coordinator/second.rs`). On the product's duplex hold a
candidate may wait 1.2 s for its decision, which is where the 1 s comes
from; on the standard one (200 ms) the wait ends 100 ms before the
deadline, so a sound is taken 100 ms later than before and never let go.
Tests, in `coordinator/tests/turn_taking.rs`, on the product's duplex mask
hearing each block with echo evidence, a full pre-roll, its hold, the real
admission and detector: the lengths above;
`while_an_answer_waits_a_short_sound_is_let_go_and_the_answer_goes_on`
(and taken at once with the switch off, on the same path);
`while_an_answer_waits_a_sound_that_goes_on_takes_the_turn_from_its_start`;
`a_waited_sound_needs_the_memory_the_wait_gives_the_mask`;
`once_the_answer_sounds_a_waited_sound_is_judged_by_its_pause_then` (the
answer really playing, its echo not yet matched);
`a_sound_heard_without_echo_evidence_does_not_wait`;
`a_sound_with_the_lamps_name_in_it_takes_the_turn`;
`the_wait_is_only_for_an_answer_to_someone_awaited_a_short_while`;
`the_wait_is_on_only_with_on`. The loop's own call is not driven by a
test (#1087).

**With `LAMP_ASKER_FIRST=on`, the person asked comes first while the
answer is awaited** (`crates/voice/src/asker_first.rs`, #829; off unless
set, and `record` to judge it without acting). On lamp-4ace in the busy
office at 14:21 on 2026-10-16, "Hey Lamp, I got the job today!" had no
answer yet 4.7 s after it ended. Then "¿Qué?" from across the room was
taken as a new request and replaced it (`user_interrupted`): the floor
hold had lapsed at 1.8 s. A person who has been asked something goes on
with whoever asked, and leaves the talk across the room alone.

- **The asker's level** is the middle of their own turn's speech blocks,
  heard while they said it, none within 0.3 s of the lamp's sound
  (`ECHO_TAIL_US`): not the lamp's echo, nor talk before the turn. The
  service's `local_endpoint` record gives it as `asker_level`, on the
  face's scale, after cancellation. With fewer than six such blocks there
  is no rule for that turn.
- **The rule:** from the end of the asker's speech, for at most 5 s
  (`KEEPS_FOR_US`) and until the answer begins to play, new speech is
  judged on its own blocks, from its own start. It takes the turn as soon
  as its upper tenth is no more than 6 dB under the asker's level
  (`MARGIN`, 45 on the face's scale): the asker going on, at their usual
  level, is taken at the six blocks that begin any turn, as without the
  rule, and a soft first word ("um, and…") before it does not keep them
  out. Speech whose upper tenth is under that waits as long as its
  candidate can, kept as it is: until its decision deadline or its oldest
  block's read history (600 ms, `turn_taking::HISTORY_US`) is 0.1 s off,
  or the utterance has ended. Nothing of it is thrown away, so if it then
  takes the turn, because the lamp's name came or louder speech did, it
  begins at its start. In duplex that is about 0.26 s after it began
  (0.21 s after its trigger, the six blocks that begin a turn): a
  name announced later than that (the name part announces it 78 to
  639 ms after it begins, `name.md`) comes too late. Still well under the
  asker at its last chance, it takes no turn and the lamp keeps the
  answer: the candidate is let go as for the floor hold. One still
  waiting when the answer begins to play goes on being judged on its own
  blocks until its last chance, the rule hearing over the answer only
  those the render does not explain, as the duplex rule hears a person
  over the lamp (`Hearing::heard(…).person()`). The lamp's own voice let
  through by the canceller is as loud as a person on a fifth of its blocks
  on lamp-4ace (`duplex-listening.md`), and judged on it a line from
  across the room took the turn from the answer (the reviewer on #1197).
  At its last chance, well under the asker then, it is let go and the
  answer plays; otherwise it is judged as anything said over the lamp's
  voice. Left to the duplex
  rule, a line from across the room was admitted a block into the answer
  and replaced it, as "¿Qué?" replaced the answer at 14:21. Judged at the
  answer's first block, the asker's own follow-up opening with a soft
  "um" was let go on that alone, and the asker was not heard at all (the
  reviewer on #1197). The service
  records `asker_first` once for the turn, with both levels and `acted`,
  and the journal says by how many decibels. In any room, busy or not.
- **Why it ends after 5 s:** an answer held back by the rule for whom it
  answers never plays, and someone who took their question back ("never
  mind") must be heard again; the floor hold stops at 1.8 s for the same
  reason. Ordinary answers begin 1.0 to 2.0 s after the asker stops
  (`connected-conversation-20261011.md`, 32 turns), so 5 s covers them.
  It rests on one sound, 14:21's at 4.7 s, and it can't tell a held
  answer from a slow one. A line the rule kept is kept until it pauses
  for 0.3 s (`LINE_PAUSE_US`), past the 5 s too, so it never takes the
  turn halfway through: a line across the room begun at 4.7 s was kept
  at its start and taken mid-line at 5 s, 0.29 s into it (the reviewer
  on #1092). It is kept for at most 2 s past the 5 s (`LINE_MOST_US`):
  steady talk across the room that never paused for 0.3 s kept the rule
  on with no end (the first builder on #1092), so it ends 7 s after the
  asker at the latest, though the line goes on. The rest of a line still
  going then takes the turn mid-line: a line at ±300 from 4.7 s to 8.7 s
  after the asker is kept to 7 s and taken 2.3 s into it, as a 2.6 s
  request (the reviewer on #1197). That is the cost of the bound.
- **What still takes the turn:** the asker, or anyone about as loud; and
  the lamp's name heard beginning after the asker's speech ended
  (`name/rising` on the bus, from the name part), however quiet.
- **Over the lamp's own voice nothing changes:** talk from across the room
  is mostly cancelled with the echo there, and the duplex rule judges
  what is left ([duplex-listening.md](duplex-listening.md)).
- **Its cost:** someone near the lamp who speaks much more softly than the
  asker is dropped, not held: they must say it again once the answer
  begins, or use its name. So is the asker's own softer follow-up after
  a loud first line (an excited "I got the job today!", then an ordinary
  "and…", is easily 6 dB down), and the asker leaning back from 30 to
  60 cm, about 6 dB. A soft "never mind" before an ordinary answer is
  dropped too, and the answer plays: the 5 s don't help there, and the
  asker can only stop it over the answer. The dropped speaker is given no
  sign. A person asked
  something while they answer someone else at least glances at them: a
  small glance toward them when the rule keeps the turn, once a turn, is
  a follow-up.
- **Latency:** speech at the asker's level is admitted when it would be
  without the rule. Only speech whose upper tenth is under the margin
  waits, until about 0.26 s after it begins (its last chance), before it is
  admitted or dropped.
- **The 6 dB is not yet measured** on the voice's microphone. It is a
  start from the room microphone's rises in [ears.md](ears.md): 19.5 to
  26.5 dB over the room's quiet for people at the desk, 9.8 to 15.1 dB
  for people farther off, and those overlap (desk talk to someone else,
  14.7 to 23.5 dB). `LAMP_ASKER_FIRST=record` is how it is set: the rule's
  record with `acted` false, the turn taken as without the rule, beside
  each turn's `asker_level` and each request's `speech_level`. Recorded
  only, it never waits: the candidate is taken at once and followed, and
  the record is written where `on` would have decided, on the same own
  blocks (`judged`: `at_its_last_chance`). Under `on`, one the answer
  began over says so (`judged`: `after_its_answer_began`).

## What is and is not shown

- `body_link::tests`: when a room counts as busy, and that an answer or
  half a minute of quiet ends it. `admission::tests`: that a pending
  candidate can be let go without authority and an accepted stream cannot.
  The hold and the kept answer themselves are a few lines each in the
  service's main loop and have no test of their own.
- `asker_first::tests`: the margin, judging on the new speech's own
  blocks, the wait and the last chance, the 5 s, a kept line and its 2 s
  bound, the asker's own turn's
  level, the name, a new request beginning afresh, and the switch's
  values. In the service's own loop with an answer awaited
  (`coordinator::tests::endpoint_session`):
  - `with_asker_first_quieter_talk_takes_no_turn_from_an_awaited_answer`:
    a second line 16.5 dB under the first takes no turn, and `asker_first`
    is recorded once with 178 and 54, over 25 blocks or more. One 4.4 dB
    under it takes the turn; one 8 dB under it does not. Recorded only, or
    without the switch, the quiet one takes the turn.
  - `with_asker_first_a_soft_start_to_the_askers_follow_up_is_kept_whole`:
    the asker's follow-up opening with 0.2 s at ±300 reaches the provider
    as 164 blocks, as without the rule (124 when it was judged at 60 ms,
    the reviewer's probe on #1092).
  - `with_asker_first_the_asker_just_after_talk_across_the_room_is_heard_whole`:
    half a second at ±300, then the asker 0.1 s later. The first of it
    takes no turn; the turn taken holds all of the asker's speech, the
    rest of the talk across the room before it when that ran on into
    theirs (0.22 s of the asker was cut when judged on the half second
    before).
  - `with_asker_first_a_short_quiet_line_ended_while_waiting_ends_nothing`:
    a 15-block quiet line, then the asker 0.3 s or 0.65 s after it. The
    session goes on, and the asker's turn is taken (the first builder's
    P1 on #1092: one kept waiting past its read history was refused and
    left the detector mid-utterance).
  - `with_asker_first_the_rule_ends_after_its_time_though_no_answer_played`.
  - `with_asker_first_a_late_short_word_is_decided_while_its_reads_last`:
    a 0.2 s word at ±300, 4.0, 4.2 or 4.5 s after the asker's end, takes
    no turn and is recorded, and the session goes on (the reviewer's P1 at
    00:00: waiting past 0.8 s of reads ended the voice or refused it).
  - `with_asker_first_a_line_kept_near_the_five_seconds_is_kept_whole`.
  - `with_asker_first_record_judges_where_on_does`.
  - `with_asker_first_a_loud_burst_just_before_does_not_let_soft_speech_in`:
    four blocks at the asker's level, then soft speech 60 ms later, takes
    no turn: speech is judged on its own blocks.
  - `with_asker_first_the_lamps_echo_is_not_the_askers_level`: the lamp
    answers and falls silent, and the next request opens on 0.2 s of its
    echo at ±6,000 before the asker's own ±800. The asker's level is 118,
    the ±800, and a line at ±700 while that answer is awaited takes the
    turn. With the echo counted it was kept out.
  - `with_asker_first_a_line_the_answer_begins_over_is_let_go_and_the_answer_plays`:
    a line at ±300 the answer begins 0.2 s into, with echo evidence as
    lamp-4ace's microphone gives it. It is let go at its last chance,
    recorded as `after_its_answer_began` with `acted` true, and the answer
    is not cut; without the rule it takes the turn before the answer
    comes. Recording only, as #1092 did, it was admitted a block into the
    answer.
  - `with_asker_first_the_askers_soft_start_over_the_answers_start_takes_the_turn`:
    the asker's follow-up opening with 0.2 s at ±300, then their own
    ±2,000, the answer beginning during the soft part: taken, with `on`,
    `record` and off alike, and nothing recorded (the reviewer's probe on
    #1197, where it was let go on its soft start).
  - `with_asker_first_the_lamps_own_voice_over_a_line_is_not_its_level`:
    the same line at ±300, the microphone at ±1,201 from the answer's
    first block with the render explaining it (the lamp's voice 4.4 dB
    under the asker): let go at its last chance, and the answer is not cut.
    Counting the leak, it took the turn.
  Each fails with its change taken out. The name's part is shown by the
  rule's own test only.
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
