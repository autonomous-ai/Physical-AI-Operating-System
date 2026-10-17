# Sonora dependency snapshot

Sonora 0.2.0 from crates.io (`sonora-0.2.0.crate`, SHA-256
`2e1ce442ab5cb392c4067b8f2cced20be4cf12e479f6dfbf44acbe9b14ac9250`, the
checksum the workspace lock pinned before this snapshot; upstream
<https://github.com/dignifiedquire/sonora>, commit
`a024d6ef8351add55be5e8b1d6cc35f555787660`, `crates/sonora`), under its
BSD-3-Clause license (`LICENSE`).

The files are the crate archive's, unchanged, except:

- `Cargo.lock` (upstream's own lockfile) is removed; a path dependency does
  not use it.
- `PATCH.md` (this file) is added.

The workspace uses it through `[patch.crates-io]` in `lampOS/Cargo.toml`
and excludes it from workspace membership, as `vendor/tough`. The other
Sonora crates (`sonora-aec3`, `sonora-agc2`, `sonora-ns`, ...) still come
from crates.io at the versions `Cargo.lock` pins.

Not run here: upstream's own test suite, examples and benches (their
dev-dependencies are not in the workspace lock).
