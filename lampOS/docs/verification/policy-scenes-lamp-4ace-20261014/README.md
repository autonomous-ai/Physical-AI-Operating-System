# Labelled played scenes for the local judge, lamp-4ace, 2026-10-14

The first builder's first file of labelled lamp records for task 17
(`lamp-policy-fit`), and the fit it gives. Played voices only.

## How it was made

On lamp-4ace, 15:24 to 15:46 local, with the voice as it runs there
(`e93b96849`, `LAMP_ATTENTION=busy`, the fast end-of-turn model on), the
policy service was stopped and `lamp-policy-judges` (`e57fc439…`, built from
`001bf889f`) ran in its place twice, ten minutes each, as a transient unit
with the service's key and options:

    lamp-policy-judges --local-judge --key-file "$CREDENTIALS_DIRECTORY/jev.key" \
      --log-dir /var/lib/lampos-policy-judges --show-judges --seconds 600

Meanwhile the Mac's loudspeaker played, with macOS voices, seven rounds of
`twopeople.sh` (two people talk for nine lines; then one says "Hey Lamp,
what do you think we should have for lunch?" and, ten seconds later, "And
what would you drink with that?") and two of `near.sh` (ten lines said to
someone whose name sounds like the lamp's, "Hey Lan, did you see this?").
The policy service was started again after each run.

`label.py` joins each `choice` record with the scenes' own script:

    python3 label.py judges-2026-10-14.jsonl OFFSET_US scenes.jsonl SPLITS SCENE.txt...

A choice belongs to the played line, among those begun in the 20 s before
it, whose words are most like what the voice heard (at least 0.6, letters
only); the labels are what that line was written to be, never what a judge
or the lamp did. The talk between the two people: `others_business`,
`names_someone_else` unknown, not directed. The call: `calls_lamp`,
`lamp_could_answer`, directed. The follow-up: `follows_lamp`,
`lamp_could_answer`, directed. The near names: `names_someone_else`,
`others_business`, not directed. The other three questions are false
throughout. Cues and `baseline` are the choice record's own; `duration_ms`
stays null as the record has it. Splits are by scene: train the first
batch's rounds 1 and 3 and its near names and the second batch's rounds
1 and 2; calibration the first batch's round 2 and the second's round 3;
test the second batch's round 4 and its near names.

70 choices were logged; 58 are in `scenes.jsonl` (49 not said to the lamp,
9 said to it). The 12 left out are heard texts like no played line
("Sí.", "Mira, mira.", "Obrigado." where the call was played): the voice's
transcript was another language or nothing like the words, or, once, a
sentence that may have been someone in the room. The judges' own records
are not here for that reason: they may hold what someone else said.

## The fit

    cargo run --release -p lamp-policy --bin lamp-policy-fit --offline -- \
      scenes.jsonl fit-weights.json fit-report.json

On the Mac: train 25, calibration 16, test 17. On the 17 test rows the
fitted local judge stayed quiet for all: no false answer, both calls
missed. The busy-room rule's copy inside the policy, on the same
snapshots (`same_snapshot`), would have let 10 through: 9 false answers
and 1 missed call. That copy is not what the voice did: the voice spoke
into none of the 63 talk lines played, because the model itself gave no
answer to most of them. No `voice_first_output` snapshot was recorded.

## What it shows and what it does not

It shows the file is what `lamp-policy-fit` takes, from records made on
the lamp, end to end. It does not show a better judge: training holds one
call heard as a call and a handful of directed rows, the voices are played
from one loudspeaker, no face was ever seen (nobody sat at the lamp), and
in two of the seven rounds the voice received no transcript at all for
the nine talk lines (no choice was made for them). More scenes with calls,
quiet-room scraps and nameless follow-ups come from the first builder in
the evening.
