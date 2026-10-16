# The V1 moves judged by a person on lamp-06c1, 2026-10-16

#995 part 3 and #1058, by el. lamp-06c1 (blue-lamp), set up the way a home is
(`deploy/lamp-06c1/`). The person sat in front of the lamp; every clip was
named to them in the session before it played, and their words are kept
below as they gave them.

## How the clips were played

Each clip was sent as `{"gesture":"NAME"}`, one datagram on
`/run/lampos-body/mood.sock`, by a scratch script on the lamp (outside the
repository). It waited until the body's journal showed it standing still at
full height (`stand 1.0`, mood `Resting` or `Listening`), since a clip asked of a
folded or rising lamp is forgotten (body.md, "Gestures"). Then it read the
journal for `gesture NAME` and `gesture over`. When the lamp had folded, a
line spoken by the Mac in front of it (`say -v Samantha "Lamp, what is two
plus two?"`) woke it first. Every clip below logged `gesture NAME` …
`gesture over`: none was dropped, stopped or given way. A clip that was sent
but left no `gesture NAME` line (once, `dance_chill` at 15:54:27, with the
body's own idle sway starting) was sent again.

## Round 1: today's clips (build 1009, `lamp-voice` `6d2bea2e1`), 13:29–13:49

| Clip | The person |
|---|---|
| acknowledge | seen |
| curious | kinda short, not like V1's curious |
| happy | barely moves |
| caring | looks good |
| surprised | seen |
| confused | the head just tilts a little |
| excited | kinda good, but a little lack of energy; excited should show more energy |
| sleepy | good, looks sleepy enough |
| dance_chill | looks good (after a second play) |
| dance_groove | looks okay |
| dance_hype | good |
| dance_waltz | (played twice) "all these dances look the same" |
| dance_jazz | same |

Verdict: keep all 13; make the dances more distinct and energetic, and
curious, happy, confused and excited bigger.

## Head roll, 14:11

`roll-body_joint_states.jsonl`: `lamp-bus record body/joint_states` (500
messages, none missed) while today's build played `confused` (asks 189
counts of roll) at 14:11:23 and 14:11:42 and `headshake` (asks ±135) at
14:11:33, standing. Head roll commanded stayed at 9–10 counts and position
at 10 throughout: the body drops a gesture's roll on a head that does not
tilt standing (`body.rs`, `stand_roll <= roll`), as the head's roll turns the
round head about its own axis. Hence no roll in the reworked clips.

## Round 2: #1058 at `ffa3d3445` (body `14ad86bb`, ARM64 run 38867161740), 14:56–15:05

| Clip | The person |
|---|---|
| curious | I think it is enough |
| happy | okay |
| confused (twice) | okay |
| excited | okay, but it would look more excited if it bounced twice |
| dance_chill | can it sweep further? It just sweeps a little; more sweep makes it noticeable and more like a dance |
| dance_groove | now too much: I thought it was going to fall to the back |
| dance_hype | this one is great |
| dance_waltz | looks good to me |
| dance_jazz | fine too |

## Round 3: #1058 at `e154e0847` (body `e998bb99`, ARM64 run 38874113322), 15:52–15:56

`dance_chill` and `dance_groove` swing about the standing pose, `excited`
bounces twice, and `surprised`, `excited` and `dance_groove` are within V1's
22 mm tip line with a margin (#1072):

| Clip | The person |
|---|---|
| surprised | okay |
| excited | okay |
| dance_chill | looks great |
| dance_groove | great |

## Centre of mass

Through `lamp-lean` with `deploy/lamp-4ace/lean-first-software.json` on
lamp-4ace's stand pose (`--stand 1772,2043,2015,2657,2602`), every frame of
every clip, by a scratch example (not committed): `surprised` 18.0 mm (25.7
before), `excited` 17.4 (23.3 at shoulder 1.0), `sleepy` 17.4 (unchanged),
`dance_groove` 8.8 (23.5 in round 2). Every clip is at most 18.0 mm.

## Not shown

- Once each, on one lamp, with one person, in an office. The stills taken
  through the Mac's camera were looked at and deleted.
- The centre of mass is V1's estimated model (1.49 kg against 7), not weighed.
- No clip was asked by the voice or a skill here; all came on `mood.sock`.
