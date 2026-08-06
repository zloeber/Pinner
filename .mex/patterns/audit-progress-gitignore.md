---
name: audit-progress-gitignore
description: Live audit progress sink, parallel audit ecosystems, recursive .gitignore on discover.
---

# Audit progress + gitignore

## Steps
1. Discovery filtering belongs in `discover_manifests` (policy globs OR `RepoIgnore`), never only in one ecosystem crate.
2. Progress events are core types; ANSI rendering stays in `pinner-ui`.
3. Attach `StderrAuditProgress` only when stderr is a TTY and format is interactive text.
4. Parallelism is audit-only; always sort findings before return.
5. Progress goes to stderr; findings/pretty panel to stdout.

## Gotchas
- `AuditProgress` callbacks from rayon must be serialized (mutex) to avoid interleaved ANSI.
- Nested `.gitignore` requires adding every `.gitignore` file to `GitignoreBuilder`, not only the root file.
- JSON/`--agent` tests must assert progress banners never appear on stdout.

## See also

- [core-audit-progress.md](core-audit-progress.md) — detailed `AuditProgress` / rayon / CLI wiring
- [core-gitignore-discovery.md](core-gitignore-discovery.md) — detailed `RepoIgnore` in `discover_manifests`
