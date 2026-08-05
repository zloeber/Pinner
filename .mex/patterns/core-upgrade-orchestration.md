---
name: core-upgrade-orchestration
description: Add or change pinner-core upgrade / upgrade_with_filter sharing pin's rewrite/lock pipeline.
triggers:
  - "upgrade_with_filter"
  - "ResolveMode::Upgrade"
  - "RunReport.upgraded"
edges:
  - target: context/architecture.md
    condition: when changing how pin and upgrade share orchestration
last_updated: 2026-08-05
---

# Core upgrade orchestration

## Context

Load `pin_with_filter` / `run_resolve_rewrite` in `crates/pinner-core/src/orchestrate.rs` and `RunReport` in `report.rs`. Upgrade reuses pin's walkthrough → rewrite → lock path with different candidate selection and resolve mode.

## Steps

1. Select **all** extract findings (not only floating); do **not** filter `allow_floating`.
2. Build `EcosystemCtx` with `resolve_mode: ResolveMode::Upgrade` and `lock_pins: &[]`.
3. After resolve, drop pins where `metadata.previous == pinned` (unchanged).
4. Empty proposed set → success, **no writes**, `upgraded: 0`.
5. Set `report.upgraded` to the count of applied upgrade pins.
6. Prefer resolved pins in `pins_for_full_graph` so exact bumps land in the lock.

## Gotchas

- Pin mode must keep floating + allowlist filtering; only Upgrade takes all findings.
- Prior lock is still needed for lock merge / unselected ecosystems even when resolve bypasses it.
- `RunReport.upgraded` serializes for all commands (0 outside upgrade).

## Verify

- [ ] `cargo test -p pinner-core upgrade_rewrites_exact_pins_pin_does_not`
- [ ] `cargo test -p pinner-core` (pin idempotency / walkthrough still green)
