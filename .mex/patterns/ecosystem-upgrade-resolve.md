---
name: ecosystem-upgrade-resolve
description: Add ResolveMode::Upgrade branches in ecosystem resolve.rs (map → tool → fail; upgrade_pin).
triggers:
  - "resolve_upgrade"
  - "ResolveMode::Upgrade"
  - "PINNER_*_RESOLVE_MAP"
  - "upgrade_pin"
edges:
  - target: patterns/core-upgrade-orchestration.md
    condition: when wiring core upgrade orchestration to ecosystem resolve
  - target: crates/pinner-ecosystem/src/upgrade.rs
    condition: when building upgrade Pin metadata
last_updated: 2026-08-05
---

# Ecosystem upgrade resolve

## Context

Core passes `ResolveMode::Upgrade` with empty `lock_pins`. Each ecosystem must skip pinner.lock/native freeze for the chosen pin, prefer `PINNER_*_RESOLVE_MAP`, then online tool, then fail. Use `upgrade_pin` and omit `None`.

## Steps

1. Branch at top of `resolve_one`: `if ctx.resolve_mode == ResolveMode::Upgrade { return resolve_upgrade(...); }`.
2. `resolve_upgrade`: map → online tool → Offline/Resolve error. Never return Lock/NativeLock as the chosen pin.
3. `previous` for display: exact `requested` if exact-looking; else optional native-lock peek (metadata only).
4. Call `upgrade_pin(finding, previous, newest, evidence, channel)`; filter `None` in `resolve_findings`.
5. Offline unit tests: set `PINNER_<ECO>_RESOLVE_MAP` (`name=requested:pinned`) behind a mutex; fixtures under `tests/fixtures/<eco>-upgrade/`.

## Gotchas

- Pin mode must still prefer lock → native → map → tool.
- Upgrade online paths should resolve **newest** (e.g. unconstrained uv req), not re-pin the current exact version.
- Env-mutating tests must serialize with `env_lock` (poison clears after a panic mid-suite).

## Verify

- [ ] Map forces newer version while lock/native would keep old
- [ ] Unchanged map → empty pins
- [ ] Offline without map errors (does not freeze on native lock)
- [ ] `cargo test -p pinner-<eco>` and clippy clean
