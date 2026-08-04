---
name: fix-task-install-version-lag
description: task install / cargo install reports an old pinner version while GitHub Releases are newer.
last_updated: 2026-08-04
---

# Fix task install showing stale Cargo.toml version

## When to use
`task install` prints `Installing pinner v0.1.0` (or other stale version) after a newer GitHub Release exists.

## Root cause
`cargo install --path` reports `[workspace.package].version` from `Cargo.toml`. Tag-first releases used to leave that field lagging; `pinner --version` can still show the git tag via `build.rs`.

## Steps
1. Confirm: `rg '^version' Cargo.toml` under `[workspace.package]` vs `gh release list` / `git describe --tags --match 'v*'`.
2. Sync now: set workspace version to the latest release, `cargo update -w`, commit as `chore:` (not `fix:`/`feat:`) so semantic-release does not bump again.
3. Ensure `semantic-release.yml` commits `chore(release): vX.Y.Z` (Cargo.toml + Cargo.lock) before pushing the annotated tag.
4. Optional: have `task install` print `"$HOME"/.cargo/bin/pinner --version` after install.

## Verify
- [ ] `task install` shows `Installing pinner vX.Y.Z` matching the latest release
- [ ] `"$HOME"/.cargo/bin/pinner --version` matches
- [ ] Next releasable merge creates a chore(release) commit then tag
