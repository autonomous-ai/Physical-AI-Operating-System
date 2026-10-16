# The lamp skill creator

A Claude Code skill (`SKILL.md` and `references/`) that makes physical
skills for an Autonomous lamp: "Lamp, learn to ..." hands the request to
Claude Code with this skill (#941, `lampOS/docs/skills.md`).

Install it into Claude Code, on the computer that takes the lamp's tasks:

```sh
mkdir -p ~/.claude/skills
cp -r lampOS/skill-creator ~/.claude/skills/lamp-skill-creator
```

It adapts V1's `skill-creator` (`skills/skill-creator` in autonomous-os):
the same process (capture what is wanted, draft, test, iterate), taught the
lamp's topics and services, `skill.toml`, tests on the lamp's own bus
recordings and installing to a lamp. V1's Python evaluation scripts are left
out: lampOS is Rust only, and a lamp skill is tested by replaying the lamp's
recordings through the runtime (`lamp-skills`, #938).
