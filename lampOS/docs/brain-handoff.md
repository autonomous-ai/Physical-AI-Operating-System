# Brain workstream handoff

Handed over by Agent 2 on 12 October 2026, at the owner's request. Everything
below is in this repository; nothing depends on the previous session.

## Read in this order

1. [The brain design](brain.md). Start with §0, the organization, and §11, the
   decisions still open with the owner.
2. [V1 brain baseline](v1-brain-baseline.md): how V1 decided what Lamp says
   and does, with evidence. Detailed notes are in [v1-baseline/](v1-baseline/README.md).
3. [Brain research](brain-research/README.md):
   - where the brain plugs into lampOS;
   - V1's wire protocols for the economy route;
   - Google's facts on Gemini Live tools, speech synthesis and prices.

## State

- **Design only. No brain code exists.**
  - The owner said on 12 October: "don't build anything yet; let's just
    discuss the brain design first".
  - A first draft of the `lamp-brain` crate was started and then discarded
    at that request. It is not in the repository.
- **Branch.** `lamp-a2-brain`, cut from `lamp-v2-chat` `10607a439`, with its
  upstream unset. It holds:
  - the V1 baseline and notes;
  - this design, the research and this handoff;
  - the scope change recorded in `AGENTS.md` and `docs/architecture.md`.
- **Related, separate work.** `lamp-a2-endpoint-fast` (`5a92b37c4`) is the
  delivered opt-in early end-of-turn package. It is not part of the brain;
  Main composes it.

## Owner decisions so far (12 October 2026)

- The brain is in scope for V2, and its owner is this workstream, not Main.
- Initiative is **rare**.
- **Gemini's own voice everywhere; no ElevenLabs.**
- The owner questioned whether Hermes still belongs ("this is part of the
  redesign"). The design recommends no agent runtime on the Lamp (`brain.md`
  §1.1); the owner has not confirmed.
- The owner asked for V1 to be studied closely before any improvements:
  "we don't know how to improve if we don't know the baseline".

## What the owner was weighing last

The discussion ended on the nervous-system model (`brain.md` §0):

- **Senses** in, through reflexes, perception and attention.
- **Gemini** as the cortex.
- **Executive** and **motor planning** out to the actions.

The owner asked two things along the way:

- "How is this better than V1?" The answer is `brain.md` §8.1: the same body
  and senses, with decisions moved from prompts to code, one decision path,
  and proof by metrics. It is not yet better in practice: today V2 does less
  than V1.
- "So Hermes is the brain in V1 and Gemini is the brain in V2?" Not quite.
  Gemini talks in both. In V2 the brain is the code loop around Gemini.

**Raise these first** (`brain.md` §11):

1. How Lamp knows it is addressed.
2. Which senses are needed at launch.
3. Whether motion is needed at launch.

Then re-cut the phases (`brain.md` §10).

## Facts the next steps depend on

- **At the Gemini usage limit, lampOS stops.** The `serve` service exits with
  an error, and the pilot unit does not restart
  ([integration §4](brain-research/lampos-integration.md)). This is the most
  urgent product gap, whatever the rest of the design becomes.
- **A Gemini tool call is a fatal session error today** (`crates/gemini`).
  Tools need a wire change owned by Agent 1.
- **Every output needs an admitted turn owner.** Unprompted speech, light or
  a look needs a new owner kind in `lamp-interaction` (Main).
- **Missing today:** volume control, presence perception, a wall clock in the
  core, and a memory store.
- **Inference cost on the device.** Main measured the endpoint worker on a
  real 4ace A733 at about 370 ms of warm inference per verdict (cold
  433–561 ms, peak RSS 102,348 KiB). Budget any on-device perception
  accordingly.

## Coordination state

Teammates are reached with `harness send <name> "…"`, in short messages. Use
`--fyi` for news that needs no action.

**Main, `servos` (Codex, office).** Told the following:

- this workstream owns the brain;
- the docs and the scope change were pushed;
- coordinator seams will come as separate small proposals:
  - a route decision instead of the quota exit;
  - the tool path;
  - volume.

**Agent 1, `voice` (Claude Code, office).**

- **Resumption.** Told that V1's proxy did not forward Gemini session
  resumption (V1 `hal/config.py:754-757`), while lampOS asks for it. Agent 1
  is adding a session-wide count and will check it on the Lamp.
- **Tools.** Asked for the tool-call proposal (`toolCall`, `toolResponse`,
  `toolCallCancellation`, `clientContent`, `usageMetadata`) as a branch
  against `origin/lamp-v2-chat`.
- **Removing V1 from 4ace.** Agent 1 is doing this at the owner's instruction
  ("remove v1 from lamp entirely… deploy v2… clean").
  - The backup is verified, on the office Mac at
    `~/lamp-4ace-v1-backup-20261012`: 6 GB, private.
  - It includes `~/.hermes` (memories), `/root/local` (users, flow event
    logs) and `config.json`. It holds face data and keys: never copy it into
    git.
  - 4ace's V1 config had `wakeword=false`, `agent_runtime=hermes` and
    `realtime.gemini.model=gemini-3.8-live-extended-thinking`, running Hermes
    Agent v0.21.5.

## Working rules carried over

**Language and code:**

- lampOS is Rust only, with docs and comments in English only. No Python
  services on the Lamp.

**Git:**

- Commit or push only on an explicit request, separately each time.
- Commit messages are one line, with no attribution trailer.
- `git add` exact paths, after reading `git status --short`.
- Never force-push. Never use a bare `git stash`.
- On new branches cut from `lamp-v2-chat`, unset the upstream.

**Devices:**

- Ask the owner before any `ssh` or `sshpass` to a Lamp.
- Device work is scheduled by Main.

**How the owner works:**

- Wants short, direct answers.
- Asks "why" and "how is this better" to understand a design before
  agreeing.
- Wants discussion before building.

**Tooling on M2:**

- Cargo needs `export PATH="$HOME/.cargo/bin:$PATH"`.
- The shell's `grep` is ugrep; use `rg` or python for complex searches.

**Reading V1:** `git worktree add --detach <dir> 170a1b85e`, or
`git show 170a1b85e:<path>`.
