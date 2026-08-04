# Agent guide — Pinner

Guidance for AI coding agents and human contributors working **in this repository**. For using the shipped CLI against other projects, see [`skills/pinner/SKILL.md`](skills/pinner/SKILL.md).

Pinner is a Rust workspace CLI that discovers floating dependency versions across ecosystems, resolves them to exact pins, rewrites manifests, and gates CI with `pinner.lock.json`.

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
- Do **not** require a manual `Cargo.toml` bump for CI releases; local workspace version may lag.
- `pinner --version` comes from `crates/pinner/build.rs` (`git describe --match 'v*'`), falling back to `CARGO_PKG_VERSION`.
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
