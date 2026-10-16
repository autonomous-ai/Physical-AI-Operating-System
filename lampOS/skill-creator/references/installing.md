# Installing a skill on a lamp

The lamp's skills live in `/var/lib/lampOS/skills/<name>/`. The runtime
(`lamp-skills`) reloads a folder when it changes: a skill is live at once,
with no restart, and gone at once when removed.

## Before touching the lamp

- Ask the owner. Say which skill, and what it will do (what will move, light
  up or speak) and when.
- Check the skill's tests passed ([testing.md](testing.md)), or say plainly
  that it is untested.
- Tell the owner what the runtime does not do yet: until the Policy's gate
  (#939), **a skill acts even while the lamp is paused**, and keeps out of a
  conversation only if it has a `[[while]]` on `voice/state`. Install only if
  they accept that.

## Install

From the computer with the skill folder, to the lamp by its name (`lamp-xxxx.local`),
by key, never by password:

```sh
scp -r stretch-break orangepi@lamp-xxxx.local:/tmp/
# Whole, beside the live folder, then swapped in: the runtime never sees half a skill.
ssh orangepi@lamp-xxxx.local '
  sudo cp -r /tmp/stretch-break /var/lib/lampOS/skills/.stretch-break.new &&
  sudo rm -rf /var/lib/lampOS/skills/stretch-break &&
  sudo mv /var/lib/lampOS/skills/.stretch-break.new /var/lib/lampOS/skills/stretch-break &&
  rm -r /tmp/stretch-break'
```

(A folder whose name starts with a dot is not a skill: the runtime skips it.)

From a Mac, never let it add its `._NAME` files: `scp -r` as above adds
none, and a copy by `tar` needs `COPYFILE_DISABLE=1`:

```sh
COPYFILE_DISABLE=1 tar -C skills -cf - stretch-break | ssh orangepi@lamp-xxxx.local 'tar -C /tmp -xf -'
```

(The runtime skips any file whose name begins with a dot, `._NAME` among
them, but a clean folder is the one to install.)

The runtime looks at the folder every two seconds and loads every skill again
when a file other than a test changed; cooldowns are kept across a reload.
Until the Policy's gate is built (#939), a lamp runs `lamp-skills` with
`LAMP_SKILLS_ASK=skip` or nothing a skill starts is done.

Then check it loaded: the runtime writes what it loaded, and why it refused
a skill (a key it does not know, a topic that does not exist), to its journal.

```sh
ssh orangepi@lamp-xxxx.local 'journalctl -u lamp-skills --since "1 min ago" --no-pager'
```

Fix a refusal there, not by guessing. `skills/events` says what a skill did
(triggered, asked, answered, done, with why), not whether it loaded.

## Turn it off, or remove it

- Turning a skill off by voice ("Lamp, stop doing ...") is not built yet: the
  runtime has no off switch for one skill, so tell the owner to ask for it
  to be removed.
- To remove it: `sudo rm -r /var/lib/lampOS/skills/<name>`.

## Never

- Never install without the owner's yes.
- Never change another skill's folder, or the lamp's own parts.
- Never leave a half-copied folder: copy to `/tmp` first, then move it in whole.
