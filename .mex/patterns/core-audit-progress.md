---
name: core-audit-progress
description: Emit AuditProgress events from sequential audit; optional sink at call sites
triggers:
  - "AuditProgress"
  - "AuditEvent"
  - "audit progress"
  - "audit sink"
edges:
  - target: "context/architecture.md"
    condition: "when wiring CLI or UI sinks to audit"
  - target: "patterns/core-gitignore-discovery.md"
    condition: "when changing discover_manifests / gitignore during audit"
last_updated: 2026-08-06
---

# Core audit progress

## Context

`pinner-core::audit` reports floating findings without writes. Progress is optional via `Option<&dyn AuditProgress>`. Keep audit **sequential** until a parallel-audit task lands.

## Steps

1. Define / extend events in `crates/pinner-core/src/progress.rs` (`AuditPhase`, `AuditEvent`, `AuditProgress`); derive `Debug, Clone` on enums.
2. Export from `lib.rs`.
3. In `audit`, emit: `AuditStarted` → per ecosystem `Started` / `Phase(Discover)` / `Phase(Extract)` / `Finished` (or `Failed`) → `AuditFinished`.
4. Split discover vs extract (call `discover_manifests` then extract loop) so phase events are real.
5. Call sites without a sink: `audit(..., None)`.
6. Integration test with a `RecordingSink` under `crates/pinner-core/tests/audit_progress.rs`.

## Gotchas

- Signature change breaks every `audit(` call — update CLI + integration tests together.
- Emit `EcosystemFailed` before returning `Err` so sinks see failures.
- Finding paths must stay repo-relative (`repo_relative`) for lock/check parity with `discover_and_extract`.
- Do not introduce rayon here unless the task explicitly parallelizes audit.

## Verify

- [ ] `cargo test -p pinner-core --test audit_progress`
- [ ] `cargo test -p pinner-core --test gitignore_filter`
- [ ] `cargo test -p pinner --test audit_explain`

## Debug

- Missing Discover/Extract events → still using `discover_and_extract` without intermediate emits.
- Zero findings but Finished with floating > 0 → allowlist filter applied inconsistently between count and report.

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` "Current Project State" if audit progress status changed
- [ ] Keep this pattern in sync when Task 4 adds parallel audit + rayon
