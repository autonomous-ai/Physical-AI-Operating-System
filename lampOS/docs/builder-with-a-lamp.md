# A builder with a lamp of its own

Proposed by the fifth builder on 2026-10-14 (`fifth-P1` in its log) as the
way any builder with its own lamp works with the first builder, so that
more such builders can be added without more traffic and without the first
builder running every piece again. Nothing here holds until the first
builder takes it into [work-split.md](work-split.md).

## A slot

A builder with a lamp has, as in the table of work-split.md: its branch
(`lamp-<name>`), a branch for each task (`<name>/<task>`), its log
(`lampOS/docs/work-log-<name>.md`), one named lamp, and that lamp's folder
`deploy/<lamp>/`. It never touches another builder's lamp or branch, and
never pushes to `lamp-voice`.

## Talking to the first builder, with no person between

Everything goes through Git, once each way.

- **Taken, done:** an entry in its log, pushed, as the loop in work-split.md
  says.
- **A proposal**, for work found that cannot be folded into a listed task: one
  line in its log's "Proposed tasks" table, id `<name>-P<n>`. The first
  builder answers it once under "Answers" in work-split.md: `yes`,
  `yes as task N`, `no`, or `yes, but ...`. No answer means not allowed. The
  builder does not ask twice; it takes an unclear answer in its narrowest
  safe sense and writes that reading down.
- **Found, not mine:** a fault seen outside its own part goes under that
  heading in its log, with how to see it, and is not mended.
- The builder reads work-split.md at the start of each task and every half
  hour.

## What a result on a lamp is

A task's log entry says: the commit tested; the gate run and its counts; on
which lamp, how many times, what came out, the failures, and what was not
shown. Beside it, in the same push:

1. **Raw readings** in `docs/verification/<task>-<lamp>-<date>/`, with a short
   README saying how they were taken: journals, the parts' JSON reports,
   servo samples. Nothing edited. No person, face, screen or credential in
   them; pictures from the lamp's camera are described, not kept.
2. **The build's identity:** the unit in `deploy/<lamp>/` names the
   binary's SHA-256 directory, and the log names the commit it was built
   from, so what ran on the lamp can be matched to the code.
3. **The Linux build:** the crate built for the lamp (linux/arm64), and any
   warning there that the Mac does not show.

## Taken into lamp-voice without running it again

lamp-voice is what lamp-4ace runs, and no two lamps have the same servo
zeros, reach or loads. So a piece is sorted by what it does to a lamp that
does not ask for it:

- **Off unless set.** It changes nothing on a lamp until a switch is given,
  and a test shows the packets, files or messages without the switch are
  exactly what they were. The first builder takes it as it is, on the
  evidence above, without running it on lamp-4ace. Running it there comes
  when the first builder chooses to switch it on for lamp-4ace, with
  lamp-4ace's own numbers, and is not a condition of taking it.
- **Another lamp's own folder** (`deploy/<lamp>/`, its measurements in the
  documents): taken as it is.
- **Changes what every lamp does by default.** Taken as today: the first
  builder runs it on lamp-4ace for a few minutes first.

The log entry says which of the three each piece is.

## When the work is done

The builder opens one pull request from `lamp-<name>` into `lamp-voice`,
whose description lists, for each task, its log entry and its
verification folder, and which of the three kinds above each piece is. It
stops there: the first builder merges.

## The lamp and its person

- Every ssh is to its own lamp only. Before installing, it writes down what
  the lamp runs and backs up what it stops or replaces, under the home
  folder with the date; nothing is deleted; one program on the servo port at
  a time.
- What needs hands is the person's: an unplug, a push by hand, a scale. The
  builder says before anything moves what it will read, and does everything
  else itself.
- Photographs only of the lamp, with no person, face or screen.
