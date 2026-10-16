# Installing a skill on a lamp

The lamp's skills live in `/var/lib/lampOS/skills/<name>/`. The runtime
(`lamp-skills`) reloads a folder when it changes: a skill is live at once,
with no restart, and gone at once when removed.

## Before touching the lamp

- Ask the owner. Say which skill, and what it will do (what will move, light
  up or speak) and when.
- Check the skill's tests passed ([testing.md](testing.md)), or say plainly
  that it is untested.

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

(A folder whose name starts with a dot is not a skill: the runtime skips it.
*Proposed* to #938.)

Then watch it load: the runtime says so on `skills/events`.

```sh
ssh orangepi@lamp-xxxx.local 'lamp-bus tap skills/events --seconds 5'
```

A skill the runtime refuses (a field it does not know, a topic that does not
exist) is said on `skills/events` with why; fix it there, not by guessing.

## Turn it off, or remove it

- The owner can say "Lamp, stop doing ..."; the runtime turns it off.
- To remove it: `sudo rm -r /var/lib/lampOS/skills/<name>`.

## Never

- Never install without the owner's yes.
- Never change another skill's folder, or the lamp's own parts.
- Never leave a half-copied folder: copy to `/tmp` first, then move it in whole.
