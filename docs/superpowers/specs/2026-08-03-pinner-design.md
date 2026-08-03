# Pinner Design

**Date:** 2026-08-03  
**Status:** Approved for implementation planning  
**Summary:** Rust CLI that pins floating tool and dependency versions across a repository, emits a unified lockfile, and gates CI on drift—so builds rest on an idempotent dependency graph.

## Problem

Repositories accumulate floating version references: `latest` in `.mise.toml`, semver ranges in `package.json`, unpinned Python deps, `FROM python:latest`, and GitHub Actions on `@v4` or `@main`. Those references resolve differently over time and across machines, so artifacts are not reproducible.

Pinner makes the dependency graph explicit and checkable: rewrite sources to exact pins, commit a single lock that mirrors the graph, and fail CI when reality drifts.

## Goals

1. **Rewrite** source manifests so floating refs become exact pins.
2. **Lock** the full cross-ecosystem graph in one committed artifact (`pinner.lock.json`).
3. **Check** in CI that the working tree and resolved graph match the lock (drift detection).
4. **Prefer evidence** from existing native locks before hitting the network.
5. **Policy-driven** pin style per ecosystem (exact versions, digests, commit SHAs).
6. **Toolchain bootstrap** to detect, optionally install, and test against resolver binaries (mise, node/npm, uv, docker, gh).

### Non-goals (v1)

- Replacing Renovate/Dependabot (no auto-PR bot).
- Cryptographic signing or attestations beyond a stable lock document.
- Guaranteeing bit-identical final binaries by itself (that remains the build system’s job atop a frozen graph).
- Private registry auth UX beyond passing through env credentials to underlying tools.

## Success criteria

- Same committed lock + same inputs ⇒ `pinner check` is clean across machines/CI.
- `pinner pin` is idempotent: a second run produces no further source or lock diff.
- Floating refs in supported ecosystems are either pinned or reported as failures under check/audit.
- Bit-identical release artifacts are a downstream goal enabled by the freeze, not Pinner’s sole responsibility.

## Architecture

Approach: **shared core + ecosystem plugin crates**, plus a **toolchain** crate for optional install and integration testing.

```text
pinner (CLI)
    ├── pinner-core          # policy, lock I/O, orchestration
    ├── pinner-toolchain     # detect / ensure / status for resolver binaries
    ├── pinner-ecosystem     # shared trait + types
    ├── pinner-mise
    ├── pinner-node
    ├── pinner-python
    ├── pinner-docker
    └── pinner-actions
```

### Ecosystem trait

Each ecosystem implements:

| Method | Role |
|--------|------|
| `discover(repo)` | Find manifests |
| `extract(manifest)` | Floating / unpinned refs |
| `resolve(refs, evidence, policy, tools)` | Concrete pins (lock → native evidence → network/tool) |
| `rewrite(manifest, pins)` | Structured patch |

Shared types live in `pinner-ecosystem` (`Finding`, `Pin`, `Rewrite`, `ResolveSource`).

### Toolchain bootstrap

- `pinner toolchain status` — required tools vs available, per enabled ecosystem.
- `pinner toolchain ensure` — install missing tools (default: via mise) when allowed; respect offline/CI flags.
- Integration tests call `ensure` or skip with a clear message when install/network is disabled.
- Runtime resolution prefers committed lock / native lock evidence so tools are not hard-required for every `check` when the lock is already complete.

## CLI

| Command | Behavior |
|---------|----------|
| `pinner pin` | Discover → resolve → rewrite sources to exact pins → regenerate `pinner.lock.json` |
| `pinner check` | Non-zero exit on drift vs committed lock; no writes |
| `pinner audit` | Report floating refs (text/JSON); `--fix` applies pin for reported findings |
| `pinner toolchain status` | Show tool availability |
| `pinner toolchain ensure` | Optionally install missing tools |
| `pinner explain <path\|pkg>` | Why a pin was chosen (evidence vs registry, policy) |

Common flags: `--dry-run`, `--offline`, `--ecosystem <list>`, `--config <path>`, `--format text|json`.

Exit codes: `0` success; `1` drift/findings; `2` tool/config/resolution error.

## Lock format

Single repo-root file: **`pinner.lock.json`** (committed). Versioned schema, stable key ordering.

Conceptual entry shape:

- `ecosystem`, `name`, `requested`, `pinned`
- `source` / `path` (manifest location)
- `evidence`: `lock` | `native_lock` | `registry` | `tool`
- `metadata` (e.g. digest vs tag, action commit vs moving tag)

Resolution order per finding:

1. Existing valid `pinner.lock.json` entry for that request
2. Native lock evidence (`package-lock.json`, `uv.lock`, etc.)
3. Policy + network/tool resolve
4. Fail closed if unresolved under `--offline` or strict policy

Rewrite mode **pins sources**; the unified lock **mirrors** the result (sources are mutated; lock is regenerated from the pinned graph).

## Policy and config

- **Defaults in the binary** so zero-config works for common cases.
- Optional **`pinner.toml`** for overrides: enable/disable ecosystems, ignore globs, toolchain install preferences, per-ecosystem pin style, rare allowlisted floating refs.

Default pin styles:

| Ecosystem | Default pin |
|-----------|-------------|
| mise | Exact tool version |
| node / python | Exact version (no ranges) |
| docker | Image digest |
| actions | Commit SHA (human tag retained in metadata/comments when useful) |

## Ecosystems (v1)

| Ecosystem | Discover | Floating signals | Rewrite | Prefer evidence from |
|-----------|----------|------------------|---------|----------------------|
| mise | `.mise.toml`, `.tool-versions` | `latest`, `lts`, missing version, ranges | Exact versions in those files | mise lock / installed list; else `mise` |
| Node | `package.json` (+ workspaces) | `latest`, `*`, `^`/`~` when policy says pin-exact | Exact versions in `package.json` | `package-lock.json` / `pnpm-lock.yaml` / `yarn.lock` |
| Python | `pyproject.toml`, `requirements*.txt` | unpinned, `>=`, `*` | Exact pins (`==` / uv-compatible) | `uv.lock` / `poetry.lock` / `pdm.lock` |
| Docker | `Dockerfile*`, compose image lines | `:latest`, untagged, floating tags | Digest-pinned refs | Local inspect; else registry |
| Actions | `.github/workflows/*`, `action.yml` | `@main` / `@master` / floating `@vN` | `@<commitsha>` | Prior lock; else GitHub API / `gh` |

Shared rules:

- Honor ignore globs; skip vendored paths.
- Never invent a pin without evidence or successful resolve.
- Use structured parsers/patches (TOML/JSON/YAML-aware), not blind regex rewrites.
- Fixture-based unit tests per ecosystem; integration tests under `tests/fixtures/`.

## Errors

- Resolution failure → exit `2` with ecosystem, path, requested ref, and actionable hint.
- No silent partial success for targeted ecosystems unless `--continue-on-error` (audit-only).
- Malformed/untrusted lock → reject; do not half-apply rewrites.

## Testing

- **Unit:** parsers, policy merge, lock round-trip, patch apply (no network).
- **Fixtures:** sample repos per ecosystem under `tests/fixtures/`.
- **Integration:** `toolchain ensure` then `pin` / `check`; network tests gated (`PINNER_NETWORK=1`).
- **Idempotency:** `pin` twice ⇒ empty second diff; `check` clean after `pin`.
- **Toolchain:** missing-binary scenarios → ensure installs, or skip when install disabled.

## CI (consumer repos)

```yaml
- run: pinner toolchain ensure
- run: pinner check
```

Optional: `pinner audit --format json` for annotations. A clean `check` is the freeze gate; artifact bit-stability remains the build system’s responsibility.

## Implementation phasing (for planning)

Phasing does not shrink v1 scope; it orders delivery so the core is testable early:

1. Workspace scaffold, `pinner-ecosystem` trait, lock schema, policy defaults, toolchain status/ensure.
2. `pinner-mise` end-to-end (pin / check / lock) as the reference ecosystem.
3. Node, Python, Docker, Actions crates behind the same trait.
4. `audit` / `explain`, CI examples, fixture matrix, idempotency tests.

## Concrete packaging defaults

- Cargo workspace crate names match the architecture diagram above; binary crate is `pinner`.
- Lock JSON Schema lives at `schemas/pinner.lock.schema.json` and is validated in this repo’s CI on lock fixtures.
- Evidence parsers are best-effort across lockfile format versions; CI integration tests pin and exercise current stable mise, node/npm, and uv via `pinner toolchain ensure`.
