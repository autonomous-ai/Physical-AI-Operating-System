# Labelled played scenes for the local judge, lamp-4ace, 2026-10-14

The first builder's first file of labelled lamp records for task 17
(`lamp-policy-fit`), and the fit it gives. Played voices only.

## How it was made

On lamp-4ace, 15:24 to 16:02 local, with the voice as it runs there
(`e93b96849`, `LAMP_ATTENTION=busy`, the fast end-of-turn model on), the
policy service was stopped and `lamp-policy-judges` (`e57fc439…`, built from
`001bf889f`) ran in its place three times, ten minutes each, as a transient
unit with the service's key and options:

    lamp-policy-judges --local-judge --key-file "$CREDENTIALS_DIRECTORY/jev.key" \
      --log-dir /var/lib/lampos-policy-judges --show-judges --seconds 600

Meanwhile the Mac's loudspeaker played, with macOS voices:

- runs 1 and 2: seven rounds of `twopeople.sh` (two people talk for nine
  lines; then one says "Hey Lamp, what do you think we should have for
  lunch?" and, ten seconds later, "And what would you drink with that?")
  and two of `near.sh` (ten lines said to someone whose name sounds like
  the lamp's, "Hey Lan, did you see this?");
- run 3: seven scenes of `calls.sh` (here): two lines of talk, a call with
  "Hey Lamp", a follow-up without the name, two more lines of talk while
  or just after the lamp answers, a call starting with a bare "Lamp," and
  its follow-up. Half the lines are said to the lamp.

The policy service was started again after each run.

`label.py` joins each `choice` record with the scenes' own script:

    python3 label.py judges-2026-10-14.jsonl OFFSET_US scenes.jsonl SPLITS SCENE.txt...

A choice belongs to the played line, among those begun in the 20 s before
it, whose words are most like what the voice heard (at least 0.6, letters
only); the labels are what that line was written to be, never what a judge
or the lamp did. Talk between two people: `others_business`,
`names_someone_else` unknown, not directed. A call: `calls_lamp`,
`lamp_could_answer`, directed. A follow-up: `follows_lamp`,
`lamp_could_answer`, directed. The near names: `names_someone_else`,
`others_business`, not directed. The other questions are false throughout.
Cues and `baseline` are the choice record's own; `duration_ms` stays null
as the record has it. Splits are by scene, given on the command line (the
`split` of each row says which).

111 choices were logged; 89 are in `scenes.jsonl` (62 not said to the
lamp, 27 said to it). The 22 left out are heard texts like no played line:
the voice's transcript came back in another language ("Obrigado." where the
call was played, "29 de noviembre", "¿Qué?"), as a fragment, or, once, as
a sentence that may have been someone in the room. The judges' own records
are not here for that reason: they may hold what someone else said.

## The fit

    cargo run --release -p lamp-policy --bin lamp-policy-fit --offline -- \
      scenes.jsonl fit-weights.json fit-report.json

On the Mac: train 35, calibration 26, test 28. `listening_noise` and
`asks_to_stop` have no positive label and stay prior-only; `calls_lamp`
has 3 positives in train.

On the 28 test rows (8 said to the lamp) the fitted local judge **never
chose to answer**: it looked up for 12 and stayed quiet for 16, so no false
answer and all 8 calls missed. The busy-room rule's copy inside the policy,
on the same snapshots (`same_snapshot`), let 21 through: 14 false answers
and 1 missed call. That copy is not what the voice did: in the
two-people rounds the voice spoke into none of the 63 talk lines, mostly
because the model itself gave no answer. No `voice_first_output` snapshot
was recorded. An earlier fit of these runs' first 58 rows gave the same
picture (2 of 2 test calls missed, no false answer).

## What it shows and what it does not

It shows the file is what `lamp-policy-fit` takes, from records made on
the lamp, end to end, and that with this much data the fitted choice is
far too cautious to use: it would hold every call. It does not show what
more data would give: the voices are played from one loudspeaker, no face
belonged to a speaker (the voices came from a loudspeaker; nobody sat at the lamp). The eyes did see faces in the office during the runs: colleagues at work, not the played speakers, which is where the rows' 6 true and 8 false `facing` cues come from (75 unknown); they are not ground truth for whether the caller faced the lamp, a call heard as a call is rare
(the transcript often drops "Hey Lamp" or comes back in another
language), and in two of the seven two-people rounds the voice received no
transcript at all for the nine talk lines.
