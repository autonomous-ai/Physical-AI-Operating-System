# Sounds that are not speech, and answers that are not answers

Two things the runtime now stops by itself, because telling the model to
stop them did not work. Status on 2026-10-13: on lamp-4ace the first has
been seen to act on a played sound, without a fault; the second was built
after it was seen happening and has not happened since. Both end a turn
without a successor, a path the service already had for an answer the
provider interrupts.

## A sound

The lamp's ears are a speech detector (`crates/live/src/activity.rs`): six
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

So: when an utterance ends having had fewer than 25 blocks scored as speech
(`SCRAP_BLOCKS` in `crates/live/src/coordinator.rs`), the turn ends there
with outcome `not_speech`. Its end is not sent as a finished question, what
the provider may still answer is dropped, and the `local_endpoint` record
carries `not_speech` with the count. Two kinds of short utterance are never
treated so:

- one that began within four seconds of the lamp's own last sound: "Yes."
  and "No." answer what the lamp just asked. (This was ten seconds at
  first. On the lamp each sound it answered then excused the next one,
  eight seconds later.)
- one that took the turn from another: a quick "stop" over the lamp, or the
  short second half of a sentence after a pause.

What it does not catch: the two sounds of 31 and 33 blocks, a sound in the
four seconds after the lamp speaks, and anything long enough to be speech
that is not (a quiet voice across the room at a median of -33 dBFS was 60
blocks). The character file carries the rest: what seems to be in a
language other than English or Vietnamese is a noise, and is not answered
([character.md](character.md)). The lamp's light still shows that it heard
something for the length of the sound.

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

## Tests

Neither rule has a test of its own: both are a few lines in the service's
main loop, on a cancellation path that existing coordinator tests cover for
the provider's own interruptions. The whole `lamp-live` host suite passes
with them (`cargo test -p lamp-live -- --test-threads=2`; one test of a
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
