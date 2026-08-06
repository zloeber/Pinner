---
name: core-gitignore-discovery
description: Wire RepoIgnore through discover_manifests for all pin/upgrade/check/audit/explain entry points
triggers:
  - gitignore
  - RepoIgnore
  - discover_manifests
  - discovery filter
edges:
  - target: context/architecture.md
    condition: when changing orchestration discovery flow
last_updated: 2026-08-06
---

# Core gitignore discovery

## Context

`RepoIgnore` (Task 1) lives in `pinner-core` and uses `matched_path_or_any_parents`. Filtering belongs in `discover_manifests`, not per-ecosystem crates.

## Steps

1. Build `let gitignore = RepoIgnore::new(&opts.repo);` once per entry point (`run_resolve_rewrite`, `check`, `audit`, `explain_via_resolve`).
2. Pass `&gitignore` into `discover_and_extract` → `discover_manifests`.
3. Skip when `policy.is_ignored(&path) || gitignore.is_ignored(&path)` on repo-relative paths.
4. Cover with StubEco + tempdir `.gitignore` via `audit` in `tests/gitignore_filter.rs`.

## Gotchas

- Do not change `RepoIgnore` parent-matching semantics.
- `audit` may gain a progress param later — update the StubEco test call site when that lands.
- Ecosystem `discover` still returns ignored paths; core filters after.

## Verify

- [ ] `cargo test -p pinner-core --test gitignore_filter`
- [ ] `cargo test -p pinner --test audit_explain`

## Update Scaffold

- [ ] Update `.mex/ROUTER.md` Current Project State when discovery filtering ships or changes
