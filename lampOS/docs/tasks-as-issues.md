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
- **Labels:** `slot:second` … `slot:fifth` (one more for each new builder);
  `status:taken`, `status:on-lamp`; a closed issue is a task on
  `lamp-voice`.
- **Order:** labels `p1`, `p2`, `p3`, or one pinned "Queue" issue that the
  first builder keeps in order. One of the two, not both.
- **A builder's next task:** `gh issue list --label slot:<name> --state
  open`, highest first. It claims one by adding `status:taken` and saying so
  in a comment.
- **Questions and answers** are comments on the issue they are about, so the
  answer is beside the question and the builder is told of it.
- **Pull requests** say `Closes #N`.

No GitHub Project is needed.

## Moving to it

The first builder opens an issue for each open task. `work-split.md` then
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
