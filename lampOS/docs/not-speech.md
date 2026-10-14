# Sounds that are not speech, and answers that are not answers

Two things the runtime now stops by itself, because telling the model to
stop them did not work. Status on 2026-10-13: on lamp-4ace the first has
been seen to act on a played sound, without a fault; the second was built
after it was seen happening and has not happened since. Both end a turn
without a successor, a path the service already had for an answer the
provider interrupts.

## A sound

The lamp's ears are a speech detector (`crates/voice/src/activity.rs`): six
blocks of 10 ms in a row scored as speech start an utterance. A cough, a
click or a bump can do that. The utterance goes to the model like any
other, and the model hears words in it. On lamp-4ace on 2026-10-13, in the
service's own record:

- Nine times in seven minutes the lamp said "Sorry, I missed that. Can you
  say it again?" to such a sound, most of them transcribed as "¿Qué?". Its
  owner, sitting beside it and saying nothing, reported it.
- With the character file changed to forbid that answer, it answered them
  otherwise: "¿Cómo te llamas?" (17 blocks of speech) got "I'm Lamp. What's
  your name?"; "¿Qué?" (11 blocks) got "Ah, hi! I see your desk."

Fourteen such sounds that the lamp answered that day had 11 to 33 blocks
scored as speech in the whole utterance, twelve of them under 25. The four
shortest things really said to it in the same records ("How you doing?",
"Okay.", "lungs", "Ouais.") had 34 to 41.

So: when an utterance ends having had fewer than 30 blocks scored as speech
(`SCRAP_BLOCKS` in `crates/voice/src/not_speech.rs`; 25 at first), the turn ends there
with outcome `not_speech`. Its end is not sent as a finished question, what
the provider may still answer is dropped, and the `local_endpoint` record
carries `not_speech` with the count. Two kinds of short utterance are never
treated so:

- one that began within four seconds of the lamp's own last sound: "Yes."
  and "No." answer what the lamp just asked. (This was ten seconds at
  first. On the lamp each sound it answered then excused the next one,
  eight seconds later.)
- one that took the turn from the lamp's own answer (a quick "stop" over
  it), or from something that had itself been said (the short second half
  of a sentence after a pause). At first any turn that took over from
  another was let through, and so was a sound that followed a sound: two
  of 25 and 26 blocks, a second apart, got "I'm here if you need
  anything." Taking over from a sound now excuses nothing.

What it does not catch: sounds of 30 blocks and more (two of the fourteen
had 31 and 33; the real "How you doing?" had 34, so there is little room
left between them), a sound in the
four seconds after the lamp speaks, and anything long enough to be speech
that is not (a quiet voice across the room at a median of -33 dBFS was 60
blocks). The character file carries the rest: what seems to be in a
language other than English or Vietnamese is a noise, and is not answered
([character.md](character.md)). The lamp's light still shows that it heard
something for the length of the sound.

### A short phrase thrown out as a sound, and the floor that did it

"Good night, Lamp." was thrown out as a sound in 5 of 8 played runs on
2026-10-13. Blocks under the faint floor ([character.md](character.md),
`--faint-floor-db`, -28 dBFS on lamp-4ace) are not counted as speech
whatever the detector makes of them, and that was applied to each 10 ms
block alone. A `not_speech` record now carries how the last two seconds
of blocks were judged (`last_blocks`: twentieths of the detector's score,
the level, and how many blocks the detector scored as speech and the
floor turned away). The first one recorded: the detector scored 50 blocks
of the phrase as speech; it began at about -20 dBFS and trailed to about
-30; 28 of the 50 were turned away for being 1 to 4 dB under the floor,
and the 22 left were under the 30 an utterance needs.

So a block under the floor is now still counted for a while after a run of
blocks over it (`crates/voice/src/room_floor.rs`, `Near`): six blocks in a
row, as many as begin an utterance, and then 400 ms (`LAMP_FAINT_HOLD_MS`
in the voice unit; unset or 0, each block is judged alone as before). A
voice across the room has a block at the floor now and then and no such
run.

On lamp-4ace with the hold at 400 ms, none of 7 short phrases was thrown
out (six "Good night, Lamp." and a "Thank you, Lamp."), utterances with a
middle level of -28 to -31 dBFS and a loudest tenth of -23 to -25 among
them, and four lines played at a tenth of the volume were not admitted at
all.

A stricter run was then tried, each of the six blocks 3 dB clear of the
floor, to make it harder for a colleague's voice at the floor to earn the
hold. Replaying the one recorded phrase, it made no difference (49 blocks
counted either way), and on the lamp the next four short phrases passed.
The fifth did not: its loud start was four blocks 3 dB over, one just
over, two more and one just over, eight in a row over the floor and never
six in a row at the stricter level, and it was thrown out with 9 and 12
blocks counted. The stricter run was taken out again. One recording had
been too little to choose it by.

Not shown: the thing the floor is for, colleagues' voices from across a
working office, which nothing played here stands in for. With the hold, a
voice whose loud moments sit just over the floor is let in where before
only its loud blocks were counted; how often that happens in the real
office is to be read from the `not_speech` records and the count of turns
admitted on a working day.

### A word thrown out as a sound: "Hello?"

The gate was built for sounds, and it costs a word. On 2026-10-14, with
nobody in the office, the check of eight lines said to the lamp without
its name ([character.md](character.md)) gave seven answers: the first
line, a played "Hello?", ended as `not_speech`. Played six more times,
nine seconds apart, it was thrown out six times in six. The detector
scored 16 blocks of it as speech in the check and 23, 18, 20, 19, 20 and
22 in the six; 30 are needed. Three blocks of the first were under the
faint floor and none of the others.

That is a synthetic voice at a quick rate (`say` at 185 words a minute),
and the word lasts about a fifth of a second by the detector. The same
clip was sent on as speech in each of the ten earlier runs of that check
in which it was played into quiet, all of them before the gate existed,
and the lamp answered it in nine. The service wrote it as "Hello." in
five of the ten and as "albero", "al loro", "Claro.", "alarm" and "Hola."
in the other five: for a word this short the text is no better a judge
than the count.

So the count of blocks cannot tell this word from the sounds the gate is
for: fourteen sounds had 11 to 33 blocks, and this word has 16 to 23.
Lowering the number lets the sounds back in. What might tell them apart
is how the blocks were judged, not how many there were, and until now
only an utterance that was thrown out recorded that. An utterance taken
as said with fewer than 80 blocks of speech (`SHORT_BLOCKS`) now carries
the same `last_blocks` in its `local_endpoint` record, so a working day
leaves both kinds side by side: the short things people said, which were
answered, and the sounds, which were not. They are numbers about the
sound, two seconds of scores and levels, and nothing of the sound itself.

Not shown: a person's "Hello?". The four shortest things really said to
the lamp on 2026-10-13 had 34 to 41 blocks, so a word said at a person's
pace may well pass where this clip does not. Nothing here has measured
it. The new field is not covered by a test.

**Measured on lamp-4ace on the evening of 2026-10-14** (nobody in the office;
played from the Mac's loudspeaker a metre away; `noises.sh` and
`shortwords.sh` with the played-test tools):

- *Noises at an idle lamp:* the fourteen macOS system sounds, a knock and a
  bang, three times each, 8 to 14 s apart. The lamp said nothing after any
  of the 48. The detector took six tonal ones (Glass, Hero, Submarine, Blow,
  Bottle, Funk) for a voice 16 times and the lamp began to listen; the gate
  threw out 15 of those records here as sounds, with 12 to 26 blocks of
  speech and an unbroken run of 2 to 18 blocks scored 0.9 or more.
- *Short words into quiet, not after the lamp's own:* "Hello?", "Hi.", "Hey.",
  "Yes.", "No.", "Thanks.", "Okay.", "Lamp?", "Sorry?", "What?", "Hey Lamp.",
  "Good morning." in three voices, 14 s apart: 6 of 36 answered, 4 more
  heard and not answered, 26 not heard at all. Of the 26, 13 reached the
  gate and were thrown out as sounds (12 to 29 blocks of speech, runs of 3 to
  19), and 13 never started an utterance: the detector did not take them
  for speech.

So neither the count nor the longest sure run tells these words from those
sounds: every threshold that keeps "Hi." keeps "Glass" too. The next thing to
try is to let a short utterance in a quiet room go to the model with a note
that it may not be words said to the lamp, and that if it is not it says
nothing (`not_for_me`), since the model hears the sound itself. The same two
played sets measure it: words answered against noises answered.

**Built that evening** (`LAMP_SHORT_SOUNDS=quiet-room`, off unless set;
`not_speech::sent_anyway`): an utterance the gate would throw out goes to the
model after all while the room is not busy. Its first try on lamp-4ace
(22:27, voice `c2ac775f5`) showed a side effect: the short words the model
left alone counted as unanswered talk, the room was taken for busy after
three, and from then on even "Good morning." was held back by the busy-room
rule ([attention.md](attention.md)). So a turn let through only to be judged
no longer counts toward a busy room, answered or not (`body_link::Attention::
not_counted`); an answer still clears the count. Measured again after that.

**What the played noises did with it** (lamp-4ace, 2026-10-14, the noise set
of 23:23:56 to 23:34:20, voice `9cdd727f5`, empty office at night): the
tonal alert sounds the detector takes for a voice were now sent on. Twelve
requests by 23:31:09, none answered, each 13 to 38 blocks of speech, ten of
them under the gate's 30, all closed after the four seconds the provider
waits. The model said nothing to any of them, as it should. Three other parts
of the lamp still took each one for talk:

- The provider's own room judgement ([room_told](../crates/voice/src/room_told.rs))
  counted each as something said and left alone. Three of them inside 37 s
  (38, 17 and 13 blocks) made the room busy at 23:24:39, and the session was
  told "the office around you is busy, with people talking to each other". It
  stayed busy until 23:55:14, through the next two played tests.
- The run of unanswered requests that renews the session after eight
  counted them too: the session was renewed at 23:52:48.
- The body went to its thinking pose for each one, rising to full height for
  four seconds.

So the coordinator now marks such a request when it ends (`ProviderInput::End`
`sound`, recorded as `let_through_as_sound` on its `local_endpoint`). Left
unanswered, the provider counts it as it counts a sound it never saw: not in
the room's run, not in the five quiet minutes, not in the renewal run, and its
journal line reads "a sound (request N) got no answer", which the played-test
quiet gate does not count. The body keeps the pose it had while the sound was
heard instead of thinking, and an answer brings it back as usual. The ring
still shows its waiting light for those four seconds.

The played sets of that night ran before the change was installed:

- *Noises* (23:23:56 to 23:34:20, the run above): 0 of 48 answered; 15 taken
  for a voice and listened to, the same six tonal sounds as before (Glass,
  Hero, Submarine, Blow, Bottle, Funk).
- *Short words* (23:38 to 23:47): **0 of 36 answered**, 14 heard. The journal
  says why: "its answer is held back: a busy room, and nothing shows it was
  called", and for "Hey Lamp." "the lamp's name was heard in a busy room and
  nobody is turned to the lamp; the model is not told". At 23:52:48 the
  session was renewed after eight unanswered requests.

So that run measured the side effect, not the switch. It is to be run again with
the change installed.

## An answer with no word in it

Told to stay quiet, the model sometimes says so aloud: an answer whose
transcript is "..." and whose sound is a second or two of nothing. To the
runtime that is an answer. The lamp shows its speaking face, counts the
turn as answered, and whoever speaks next is taken to be interrupting it.
A wording of the thread note produced sixteen of these in two scenes
([conversation-thread.md](conversation-thread.md)); with that gone they
came back on a later character file, three in one scene, to lines between
two other people.

The provider's transcript of an answer arrives before the answer is played
(the first sound is held 350 ms; on the lamp the "..." came about 180 ms
after it). When the first words of an answer to arrive have no letter or
digit in them and none of it has been played, the turn ends there with
outcome `nothing_said`. It is not played, and for the purpose of knowing
whether the room's talk has been for the lamp
([group-talk.md](group-talk.md)) a turn counts as answered only if the
lamp was heard in it. An answer that begins "..." and goes on to words is
lost with it. None was seen.

## A function's name said aloud

With seven functions declared (2026-10-13, the memory's three added), the
model sometimes writes a call as words and does not make it: in one test
three answers were "not_for_me", played through the speaker to two people
talking to each other. The same guard now ends an answer that opens with
a function's name (a word of lower-case letters with an underscore in
it, or the service's own way of writing a call, such as
`print(default_api.not_for_me())`) before any of it is played, with
outcome `nothing_said` and a `function_named` record. Its words are not
kept as something the lamp said (`Thread::lamp`), so they are neither
told to the next session nor taken for a conversation going on. On
lamp-4ace, in the next run in which it happened, "not_for_me()" came
twice as an answer's text and neither was played.

## Tests

Neither rule has a test of its own: both are a few lines in the service's
main loop, on a cancellation path that existing coordinator tests cover for
the provider's own interruptions. The whole `lamp-voice` host suite passes
with them (`cargo test -p lamp-voice -- --test-threads=2`; one test of a
motor worker's two-second start timed out in one such run and passed alone
three times in three).

On lamp-4ace, `sounds2.sh`: a question, then eight short sounds eleven
seconds apart, each more than four seconds after the lamp's last word (a
played "Ha.", "Mm." and "Uh.", a synthetic throat-clearing, four of the
Mac's alert sounds), then a second question. The two questions were
answered and none of the eight sounds was. One ("Ha.") was taken for the
start of speech and ended as `not_speech` 0.64 s later; two ("Mm.", heard
as "M&M", and an alert sound) were long enough to be sent on and the model
left them unanswered; five were never taken for speech at all. No restart,
and the turns after the dropped one went as usual. These are played and
synthetic sounds. A person's cough beside the lamp, which is what was
reported, has not been tried on purpose.
