---
name: core-audit-progress
description: Emit AuditProgress events from parallel audit (rayon); optional sink at call sites; stable finding order
triggers:
  - "AuditProgress"
  - "AuditEvent"
  - "audit progress"
  - "audit sink"
  - "parallel audit"
  - "rayon audit"
edges:
  - target: "context/architecture.md"
    condition: "when wiring CLI or UI sinks to audit"
  - target: "patterns/core-gitignore-discovery.md"
    condition: "when changing discover_manifests / gitignore during audit"
last_updated: 2026-08-06
---

# Core audit progress

## Context

`pinner-core::audit` reports floating findings without writes. Progress is optional via `Option<&dyn AuditProgress>`. Ecosystems run via `rayon::par_iter`; progress emits are serialized with a local `Mutex<()>`; findings are sorted by `(ecosystem.as_str(), path, name)` before return.

## Steps

1. Define / extend events in `crates/pinner-core/src/progress.rs` (`AuditPhase`, `AuditEvent`, `AuditProgress`); derive `Debug, Clone` on enums. Trait must be `Send + Sync`.
2. Export from `lib.rs`.
3. In `audit`, emit: `AuditStarted` → per ecosystem `Started` / `Phase(Discover)` / `Phase(Extract)` / `Finished` (or `Failed`) → `AuditFinished`.
4. Split discover vs extract (call `discover_manifests` then extract loop) so phase events are real.
5. Parallelize selected ecosystems with `par_iter`; wrap `on_event` with `Mutex<()>` so sinks see non-interleaved events.
6. Collect `Vec<Result<...>>`, fold first `Err` after join (preserve fail-fast: `EcosystemFailed` then `Err`, no `AuditFinished`).
7. Sort `report.findings` before `AuditFinished`.
8. Call sites without a sink: `audit(..., None)`.
9. UI sink: `pinner-ui::StderrAuditProgress` implements `AuditProgress`; `format_audit_event` for pure formatting/tests; writes to stderr with optional crossterm color.
10. CLI (`crates/pinner`): attach sink only when `Format::Text && !cli.agent && stderr_is_tty()`; otherwise `None`. Progress must never appear on stdout (JSON/`--agent` stay clean).
11. Integration tests under `crates/pinner-core/tests/audit_progress.rs` (phases, failure contract, deterministic sort); CLI contract in `crates/pinner/tests/audit_explain.rs` (`audit_json_stdout_is_findings_only`).

## Gotchas

- Signature change breaks every `audit(` call — update CLI + integration tests together.
- Emit `EcosystemFailed` before returning `Err` so sinks see failures.
- Finding paths must stay repo-relative (`repo_relative`) for lock/check parity with `discover_and_extract`.
- Parallel join may finish other ecosystems after one fails; still omit `AuditFinished` on any `Err`.
- Do not rely on ecosystem iteration order for report stability — always sort findings.

## Verify

- [ ] `cargo test -p pinner-core --test audit_progress`
- [ ] `cargo test -p pinner-core --test gitignore_filter`
- [ ] `cargo test -p pinner --test audit_explain`

## Debug

- Missing Discover/Extract events → still using `discover_and_extract` without intermediate emits.
- Zero findings but Finished with floating > 0 → allowlist filter applied inconsistently between count and report.
- Non-deterministic findings order → missing post-join sort by `(ecosystem, path, name)`.
- Interleaved / racy sink panics → progress emit not guarded by `Mutex<()>`.

## Update Scaffold

- [x] Update `.mex/ROUTER.md` "Current Project State" when parallel audit lands
- [x] Keep this pattern in sync with rayon + stable ordering
- [x] CLI attaches `StderrAuditProgress` for interactive text stderr TTY
