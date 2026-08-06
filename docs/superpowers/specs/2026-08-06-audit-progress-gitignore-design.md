# Audit live progress, parallel ecosystems, and recursive `.gitignore`

**Date:** 2026-08-06  
**Status:** Approved for planning  
**Scope:** Interactive `pinner audit` progress UX; parallel ecosystem audit; recursive `.gitignore` on all discovery

## Goal

Make `pinner audit` show beautiful, descriptive progress while engines run on a TTY, speed audit with parallel ecosystems, and ensure discovery always respects nested `.gitignore` files (in addition to existing policy ignore globs).

## Non-goals

- Parallel `pin` / `check` / `upgrade` / `explain`
- Full-screen ratatui live dashboard
- Changing the JSON / `--agent` stdout contract
- Rewriting every ecosystem walker to prune at walk time (central filter after discover is enough)
- Progress on non-TTY or when `--format json` / `--agent` is set

## Decisions (from brainstorming)

| Topic | Choice |
|-------|--------|
| Progress UX | Rich progress on **stderr** when stderr is a TTY (text mode); final findings / pretty panel on **stdout** |
| `.gitignore` | All discovery commands; nested rules; combine with existing `ignore_globs` |
| Concurrency | Parallel ecosystems for **audit only** |
| Approach | Progress callback / sink in core + UI renderer + shared gitignore filter |

## Architecture

```
CLI (audit, TTY text)
  → builds ProgressSink (stderr, colored status lines)
  → audit(..., Some(&sink))

pinner-core::audit
  → parallel ecosystem workers
  → each: discover → policy+gitignore filter → extract → emit events
  → merge findings (deterministic order) → RunReport

pinner-ui
  → render live progress (stderr)
  → existing emit_pretty_audit on stdout at end

discover_manifests (all commands)
  → ecosystem.discover()
  → drop paths matching policy ignore_globs OR recursive .gitignore
```

**Boundaries**

- `pinner-core` owns orchestration, ignore matching, progress events — no ANSI/TTY logic
- `pinner-ui` owns rendering
- CLI decides whether a sink is attached (`None` for JSON / agent / non-TTY)

## Progress UX

### Events

| Event | When |
|-------|------|
| `AuditStarted { ecosystems }` | Before workers launch |
| `EcosystemStarted { kind }` | Worker begins |
| `EcosystemPhase { kind, phase }` | `discover` / `extract` |
| `EcosystemFinished { kind, manifests, floating }` | Success |
| `EcosystemFailed { kind, error }` | Error surfaced before CLI exit |
| `AuditFinished { findings }` | All workers done |

### Rendering rules

- Progress only on **stderr**
- Final pretty panel / findings stay on **stdout** (unchanged text/JSON contracts)
- Attach a progress sink only when **stderr is a TTY**, format is text, and neither `--agent` nor `--format json` is set
- Prefer simple colored status lines (optional spinner via elapsed ticks); not a full-screen TUI
- Errors keep today’s fail-fast behavior (propagate; exit code 2); progress may show which ecosystem failed

### Example (stderr)

```
pinner audit · 9 ecosystems · parallel
  … mise        discover
  … node        extract
  ✓  cargo      3 manifests · 2 floating
  ✓  docker     1 manifest · 0 floating
```

## Recursive `.gitignore`

- Use the `ignore` crate (git/ripgrep semantics): root + nested `.gitignore`, parent-dir rules, negation (`!`), `.git` exclusion
- Apply in `discover_manifests` for **all** commands: skip if `policy.is_ignored(path)` **or** gitignore matches
- Keep policy `ignore_globs` (defaults: `node_modules`, `.git`, `vendor`, `tests/fixtures`, plus `pinner.toml` overrides)
- Build one matcher per repo root for the run (cache on entry to orchestrate/audit)
- Ecosystems keep existing `walkdir` discovers; filtering stays centralized after discover
- No new CLI flag — always on

## Concurrency

- Parallelize ecosystem workers inside `audit` only via `rayon` (`par_iter` over selected ecosystems)
- Each worker: discover → ignore filter → extract → progress events
- Progress delivery: `Mutex` around the sink, or a channel drained on the main thread before return
- Deterministic final finding order: sort by ecosystem, then path, then name (stable JSON / tests)
- Cap parallelism at selected ecosystem count
- Audit does not call `resolve`; process-global resolve-map env locks are out of this path
- Pin / check / upgrade / explain remain sequential

## API sketch

```rust
pub trait AuditProgress: Send + Sync {
    fn on_event(&self, event: AuditEvent);
}

pub enum AuditEvent {
    AuditStarted { ecosystems: Vec<EcosystemKind> },
    EcosystemStarted { kind: EcosystemKind },
    EcosystemPhase { kind: EcosystemKind, phase: AuditPhase },
    EcosystemFinished {
        kind: EcosystemKind,
        manifests: usize,
        floating: usize,
    },
    EcosystemFailed { kind: EcosystemKind, error: String },
    AuditFinished { findings: usize },
}

pub enum AuditPhase {
    Discover,
    Extract,
}

pub fn audit(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    progress: Option<&dyn AuditProgress>,
) -> Result<RunReport, CoreError>;
```

Existing call sites pass `None` unless the CLI attaches a UI sink for interactive text audit.

`discover_manifests` gains access to a repo-scoped gitignore matcher (via `RunOptions` or a small helper built once per run).

## Testing

- Unit: nested `.gitignore` skips a planted manifest; `!` negation re-includes; policy glob still skips fixtures
- Unit: recording progress sink captures phase order for fixture audit
- Unit: parallel audit finding set matches sequential baseline (order normalized)
- CLI: existing `audit --format json` / `--agent` tests unchanged
- UI: render progress lines to a buffer (no real TTY)
- Docs: quick-start / README note that interactive audit shows live progress; agents keep JSON

## Error handling

- Ecosystem discover/extract errors: same as today (return `CoreError`, exit 2)
- Progress sink I/O errors: ignore or best-effort write to stderr — must not change audit exit semantics
- Missing `.gitignore`: treat as empty rules (not an error)

## Rollout

1. Add `ignore` dependency + gitignore filter in `discover_manifests` + tests
2. Add `AuditProgress` / events; wire sequential audit with optional sink
3. Parallelize audit workers; stabilize finding order
4. Implement TTY stderr renderer in `pinner-ui`; wire CLI
5. Docs + pattern update under `.mex/`

## Success criteria

- Interactive `pinner audit` shows per-ecosystem discover/extract status on stderr
- `--agent` / `--format json` / non-TTY remain quiet and contract-compatible
- Files ignored by nested `.gitignore` are not discovered by any command
- Audit completes with parallel ecosystems and deterministic finding order
- `scripts/ci-local` passes
