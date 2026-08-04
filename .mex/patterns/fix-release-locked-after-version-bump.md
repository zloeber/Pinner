---
name: fix-release-locked-after-version-bump
description: release.yml fails cargo build --locked after rewriting workspace.package version from the git tag.
last_updated: 2026-08-04
---

# Fix release --locked failure after tag version rewrite

## When to use
`release` workflow fails on all matrix targets with:
`cannot update the lock file .../Cargo.lock because --locked was passed`

## Root cause
Tag-first releases rewrite `[workspace.package].version` in `Cargo.toml` before build. Workspace path packages in `Cargo.lock` still list the old version, so Cargo wants to rewrite the lockfile and `--locked` aborts.

## Steps
1. Confirm failure is after the “Align Cargo.toml workspace version” step and before/at “Build release binary”.
2. After installing the Rust toolchain, run `cargo update -w` (refreshes workspace member versions only; leave third-party pins alone).
3. Keep `cargo build --locked --release -p pinner --target …`.
4. Re-run the failed tag workflow (or re-push the tag) after merge.

## Verify
- [ ] Local: rewrite version to a newer semver → `cargo update -w` → `cargo build --locked -p pinner` succeeds
- [ ] `release.yml` includes the sync step between toolchain install and locked build
