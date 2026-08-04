# Agent guide — Pinner

Guidance for AI coding agents and human contributors working **in this repository**. For using the shipped CLI against other projects, see [`skills/pinner/SKILL.md`](skills/pinner/SKILL.md).

Pinner is a Rust workspace CLI that discovers floating dependency versions across ecosystems, resolves them to exact pins, rewrites manifests, and gates CI with `pinner.lock.json`.

# Navigation

1. Review .mex/ROUTER.md for project structure and navigation.
2. Use 'gitnexus' (skills or mcp) for structural analysis and impact assessments.

## Quick orientation

| Area | Location |
|------|----------|
| CLI binary | `crates/pinner` |
| Orchestration / policy / lock | `crates/pinner-core` |
| Ecosystem trait | `crates/pinner-ecosystem` |
| Ecosystems | `crates/pinner-{mise,node,python,docker,actions,terraform,helm,k8s,cargo,go,ruby,gitlab,azure}` |
| Walkthrough TUI | `crates/pinner-ui` |
| Fixtures | `tests/fixtures/*-floating/` |
| Installer | `scripts/install.sh` |
| Lean local CI | `scripts/ci-local` |
| Release docs | `docs/guide/releasing.md` |
| Secrets manifest | `Secretfile.yml` (SecretZero; never commit values) |

Default-on ecosystems include mise, node, python, docker, actions, terraform, cargo, go, ruby. Opt-in: helm, k8s, gitlab, azure (`pinner.toml`).

## Absolute rules

1. **Never put secrets in agent context.** Do not read `.env`, paste PATs, or run SecretZero reveal/render into chat. Seed `PAT_TOKEN` via SecretZero human/web flows only (`Secretfile.yml`).
2. **Do not push without local CI.** Run `scripts/ci-local` and fix failures first (see below).
3. **Prefer agentic CLI flags in automation.** Use `--format json` or `--agent`. Never use `--walkthrough` in non-interactive / agent loops.
4. **Conventional commits on `main` drive releases.** See Semantic release below.

## Pre-push CI (required)

Before **any** `git push` (including `-u` / force-with-lease):

```bash
scripts/ci-local
```

Gates (same as GitHub Actions `ci.yml`):

1. `fmt` — `cargo fmt --all -- --check`
2. `clippy` — workspace clippy `-D warnings`
3. `test` — workspace tests
4. `schema` — lock schema fixtures

Rules:

- Do **not** push if `scripts/ci-local` exits non-zero.
- Use the script’s short summary in chat — do not dump full cargo logs.
- Fix with hinted commands (`cargo fmt --all`, `cargo test -p …`, etc.), re-run `scripts/ci-local`, then push.
- Emergency skip (rare): `PINNER_SKIP_LOCAL_CI=1 git push` — explain why in the commit/PR note.
- Cursor enforces this via `.cursor/rules/pre-push-local-ci.mdc` and `.cursor/hooks/pre-push-ci.sh`.
- Prefer `scripts/ci-local` over `task ci` for lean agent-friendly output.

### Useful local commands

```bash
task setup                 # mise tools
cargo build -p pinner
cargo test -p pinner-cargo # targeted crate
cargo run -p pinner -- --version
PINNER_INSTALL_DRY_RUN=1 bash scripts/install.sh
```

## Testing expectations

- Follow existing ecosystem patterns: `discover` / `extract` / `resolve` / `rewrite` + fixtures under `tests/fixtures/`.
- Prefer resolve-map env seams (`PINNER_*_RESOLVE_MAP`) for offline unit tests; gate live network with `PINNER_NETWORK=1`.
- Serialize tests that mutate process-global env (mutex), especially resolve-map cases.
- After CLI flag or orchestration changes, extend `crates/pinner/tests/` (smoke, walkthrough mode, idempotency sweep).
- Exit codes for the product CLI: `0` ok, `1` drift/findings, `2` tool/config/resolve/invalid mode.

## Semantic release (semrel)

Automated path after merge to `main`:

1. Conventional commit subjects on `main`:
   - `feat:` → minor (`v0.2.0` → `v0.3.0`)
   - `fix:` / `refactor:` / `perf:` → patch
   - `feat!:` or body `BREAKING CHANGE` → major
   - Docs/chore-only → **no** tag
2. [`.github/workflows/semantic-release.yml`](.github/workflows/semantic-release.yml) pushes annotated tag `vMAJOR.MINOR.PATCH` using `secrets.PAT_TOKEN`.
3. [`.github/workflows/release.yml`](.github/workflows/release.yml) builds multi-platform binaries and publishes the GitHub Release (tag-first: rewrites workspace `Cargo.toml` version for the build).

Notes for agents:

- Prefer PR titles/commits that match conventional commit style when the change should release.
- Do **not** require a manual `Cargo.toml` bump for CI releases; semantic-release commits `chore(release): vX.Y.Z` with the workspace version + lock sync before tagging.
- `pinner --version` comes from `crates/pinner/build.rs` (`git describe --match 'v*'`), falling back to `CARGO_PKG_VERSION`. `task install` shows Cargo’s package version from `Cargo.toml`.
- Manual tags still work: `git tag -a vX.Y.Z && git push origin vX.Y.Z`.
- Details: [`docs/guide/releasing.md`](docs/guide/releasing.md).

### PAT / SecretZero (humans)

Semantic-release needs a classic PAT (`contents:write`) seeded as Actions secret `PAT_TOKEN` via [`Secretfile.yml`](Secretfile.yml). Agents must not handle the token value; instruct humans to use `secretzero agent sync --web` or `secretzero web`.

## Developing ecosystems

New ecosystems: crate `crates/pinner-<name>/` with trait impl, register in `crates/pinner/src/main.rs`, extend `EcosystemKind` + schema + policy defaults, add fixtures and tests. Evidence order: lock → native lock → resolve map → online/tool → fail; `--offline` fails closed.

## Dogfooding this repo

```bash
cargo run -p pinner -- audit --format json
cargo run -p pinner -- pin --agent
cargo run -p pinner -- check --agent
```

Agent skill for consumers: [`skills/pinner/SKILL.md`](skills/pinner/SKILL.md).

## Docs

- User guide (mdBook): `docs/guide/` — `task docs` / `task docs:serve`
- Design/plans: `docs/superpowers/`

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **Pinner** (1905 symbols, 5511 relationships, 158 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/Pinner/context` | Codebase overview, check index freshness |
| `gitnexus://repo/Pinner/clusters` | All functional areas |
| `gitnexus://repo/Pinner/processes` | All execution flows |
| `gitnexus://repo/Pinner/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
