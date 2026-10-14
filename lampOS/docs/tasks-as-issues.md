# Builders' tasks as GitHub issues

Proposed by the fifth builder on 2026-10-14 (`fifth-P3` in its log), at the
fifth builder's person's request. Nothing here holds, and no issue or label
is made, until the owner and the first builder say yes.

## The trouble with one file

- `work-split.md` is about 1,400 lines, and every builder reads all of it
  on every loop: most of it is tasks that are not its own, and it costs
  each of them that much context every half hour.
- A task's state is free text in a table cell, so no program can ask
  "what is open for me".
- Nothing tells a builder that something changed: each polls every half
  hour.
- Only the first builder writes it, so every question and answer goes
  through each builder's own log and the "Answers" section.

## Proposed: one issue for each task

- **One issue per task.** Its body keeps the headings the list uses today:
  Why, Wanted, Hand back, Most likely wrong. Long specifications and all
  evidence stay as Markdown in the repository and are linked from it.
- **Labels:** `builder:<name>` for whose task it is (see "Who takes a
  task"); `status:taken`, `status:on-lamp`; a closed issue is a task on
  `lamp-voice`.
- **Order:** labels `p1`, `p2`, `p3`, or one pinned "Queue" issue that the
  first builder keeps in order. One of the two, not both.
- **A builder's next task:** `gh issue list --label builder:<name> --state
  open`, highest first, claimed as "Who takes a task" says.
- **Questions and answers** are comments on the issue they are about, so the
  answer is beside the question and the builder is told of it.
- **Pull requests** say `Closes #N`.

No GitHub Project is needed.

## Who takes a task

A builder's name is an identifier, not a number or a place in a line.
`fifth` is a name the owner chose; the next may be `servo`, `kiwi` or
anything else. Nothing here assumes names that count, come in order, or
grow by one for each new builder.

1. **Identity is the name**, carried on an issue as the label
   `builder:<name>`. GitHub's assignee cannot tell agents apart: every agent
   works through the same account.
2. **One role file for each builder**, `lampOS/builders/<name>.md`: the
   branches it may push, its lamp or lamps, the crates it owns, its log
   file, and what it may and may not do. A person starts a session and says
   only "you are <name>". The agent reads its role file, runs
   `gh issue list --label builder:<name> --state open`, takes the issue
   highest in order and starts.
3. **Claiming:** the agent adds `status:taken` and comments one line,
   `taken by <name> · session <session id> · <lamp>`. Before claiming it
   skips any issue already `status:taken`. After claiming it reads the
   comments again: if the first "taken" comment is not its own, it releases
   the issue (removes its label, says so) and takes the next. That keeps two
   sessions given the same name from working one task twice.
4. **At any scale** (N builders, M people, K lamps): who did a piece of work
   is the builder's name, the person who started it and the lamp. A second
   person working in the same area is given a builder name of their own and
   a role file of their own, not a suffix on someone else's. A lamp belongs
   to one builder at a time, as its `LAMP.md` says
   ([builder-with-a-lamp.md](builder-with-a-lamp.md), "Which lamp is whose").

## Moving to it

The first builder opens an issue for each open task and a role file for
each builder. `work-split.md` then
keeps only the rules and the loops ("The loop, for the builders without a
lamp", "The loop, for the builder with a lamp", "Rules for all"), and the
per-builder logs keep only what was run and what came out.

## What it costs

- Every builder needs `gh` signed in to the repository.
- GitHub becomes something the builders' coordination depends on. Code,
  evidence and decisions stay in Git, so a GitHub outage stops the
  conversation, not the record.

## What is to be decided

By the first builder and the owner: adopt it or not, and order by labels or
by a Queue issue.
