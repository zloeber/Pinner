# Pinner

Pin floating dependency versions across mise, Node, Python, Docker, GitHub Actions, Terraform, Helm, and Kubernetes. Rewrite manifests to exact pins, commit a unified `pinner.lock.json`, and fail CI when the graph drifts.

<div align="center">
<a href="https://zloeber.github.io/Pinner/">
<img src="docs/inc/pinner_logo_clean.png" width="520" alt="Pinner™ logo">
</a>
</div>

<p align="center">
    <a href="https://github.com/zloeber/Pinner/releases/latest">
        <img src="https://img.shields.io/github/v/release/zloeber/Pinner?color=blue&label=Latest%20Release" alt="Latest Release">
    </a>
    <a href="https://github.com/zloeber/Pinner/blob/main/LICENSE">
        <img src="https://img.shields.io/badge/License-MIT-ffffff?labelColor=d4eaf7&color=2e6cc4" alt="License: Apache 2.0">
    </a>
    <a href="https://deepwiki.com/zloeber/Pinner">
        <img alt="Ask DeepWiki" src="https://deepwiki.com/badge.svg">
    </a>
</p>

[![ci](https://github.com/zloeber/Pinner/actions/workflows/ci.yml/badge.svg)](https://github.com/zloeber/Pinner/actions/workflows/ci.yml)
[![docs](https://github.com/zloeber/Pinner/actions/workflows/docs.yml/badge.svg)](https://github.com/zloeber/Pinner/actions/workflows/docs.yml)
[![release](https://github.com/zloeber/Pinner/actions/workflows/release.yml/badge.svg)](https://github.com/zloeber/Pinner/actions/workflows/release.yml)

Docs (GitHub Pages): <https://zloeber.github.io/Pinner/>

## Install

**Linux / macOS (curl installer):**

```bash
curl -fsSL https://raw.githubusercontent.com/zloeber/Pinner/main/scripts/install.sh | bash
```

Installs to `~/.local/bin/pinner` (override with `PINNER_INSTALL_DIR`). Pin a version with `PINNER_VERSION=0.2.0`, or dry-run with `PINNER_INSTALL_DRY_RUN=1`.

Download a multi-platform binary manually from [GitHub Releases](https://github.com/zloeber/Pinner/releases) (Linux/macOS/Windows), or build from source:

```bash
cargo install --locked --path crates/pinner
```

Or via [mise](https://mise.jdx.dev) backends (availability depends on mise version and published package name):

```bash
# GitHub release backend (prebuilt)
mise use -g github:zloeber/Pinner

# Cargo backend (build from crates.io / git as configured)
mise use -g cargo:pinner
```

Curl installer and `cargo install` remain first-class.

## Quick start

```bash
# Resolve floating refs, rewrite sources, write pinner.lock.json
pinner pin

# Bump exact pins to latest (rewrite + lock)
pinner upgrade
pinner upgrade --walkthrough   # interactive accept/skip/edit (TTY)

# Drift gate (no writes) — exit 1 on mismatch, 2 on tool/config errors
pinner check

# Report floating refs; optionally apply pins
pinner audit
pinner audit --fix

# Explain why a pin was chosen
pinner explain <name-or-path>
```

Agents and CI should use `--agent` / `--format json` — never `--walkthrough` in automation.

## Provider support

| Provider | Default | Preferred upgrade means | Also supported | Upgrade pin style | Notes |
|----------|---------|-------------------------|----------------|-------------------|-------|
| mise | on | `mise latest` / `mise ls-remote` | `PINNER_MISE_RESOLVE_MAP` | exact tool version | Nested mise configs included |
| node | on | `npm view <pkg> version` | native lock ignored in upgrade; map | exact version in package.json | Workspaces supported |
| python | on | **`uv pip compile`** / uv resolve | poetry/pdm locks ignored in upgrade; map | exact `==` | Prefer uv over pip/poetry CLI |
| docker | on | `docker buildx imagetools inspect` | local inspect; map | `name@sha256:…` | Upgrades digest for image reference |
| actions | on | `gh api` (latest release → SHA) | docker digests for workflow images; map | `@<sha>` + image digests | Reusable/composite uses included |
| terraform | on | registry HTTP latest version | `git ls-remote`; `.terraform.lock.hcl` ignored in upgrade; map | exact version / full SHA | Local modules skipped |
| helm | opt-in | HTTP index / OCI tags latest | docker digests for values images; map | exact chart ver / image digest | Requires `helm = true` |
| k8s | opt-in | docker digests | map | image digest | Workload kinds only |
| cargo | on | crates.io HTTP API | `cargo` CLI if used; map | exact semver in Cargo.toml | Path/git deps skipped |
| go | on | `go list -m -u` | proxy.golang.org; map | exact module version | |
| ruby | on | RubyGems HTTP | `gem` optional; map | exact gem version | |
| gitlab | opt-in | docker digests + `git ls-remote` | map | digest / SHA | Opt-in in pinner.toml |
| azure | opt-in | docker digests; task versions via map/HTTP | map | digest / exact task ver | Opt-in; tasks map-only until marketplace HTTP |

Per-provider Pin / Upgrade / Check / Gaps: [docs/guide/ecosystems/](docs/guide/ecosystems/).

## Config (`pinner.toml`)

Optional repo-root overrides:

```toml
[ecosystems]
mise = true
node = true
python = true
docker = true
actions = true
terraform = true   # default on
helm = true        # opt-in (default off)
k8s = true         # opt-in (default off)

ignore = ["**/node_modules/**", "**/.git/**", "**/vendor/**", "**/tests/fixtures/**"]

[toolchain]
install = true

[pinning]
exact_ranges = true
```

Global flags: `--config`, `--offline`, `--dry-run`, `--ecosystem mise,node`, `--format text|json`.

`--ecosystem` filters **already-enabled** kinds from policy; it does not turn on opt-in ecosystems. To pin Helm or Kubernetes, set `helm = true` / `k8s = true` in `pinner.toml` (or rely on defaults for Terraform).

## IaC ecosystems (Terraform, Helm, Kubernetes)

| Ecosystem | Default | Pin style | Sources |
|-----------|---------|-----------|---------|
| **terraform** | on | exact semver for registry modules/providers; git/HTTP module sources → full commit SHA in `?ref=` | `*.tf` / `*.tofu` — remote `module` blocks and `required_providers` |
| **helm** | off (opt-in) | exact chart version strings; floating images in `values*.yaml` → `name@sha256:…` | `Chart.yaml` dependencies; Flux `HelmRelease`; Argo CD `Application`; `values.yaml` / `values*.yaml` images |
| **k8s** | off (opt-in) | container images → `name@sha256:…` | YAML workloads: Deployment, StatefulSet, DaemonSet, Job, CronJob |

### What is skipped

- **Terraform:** local module sources (`./`, `../`, absolute paths); CLI `required_version`; `.terraform/` directory during discovery.
- **Kubernetes:** non-workload kinds (ConfigMap, HelmRelease, etc.).

### Resolution (lock, native evidence, env maps)

Resolve order matches other ecosystems: existing `pinner.lock.json` pins first, then ecosystem-specific evidence, then network/tool resolve when online.

- **Terraform providers:** may use `.terraform.lock.hcl` provider selections when present.
- **Terraform git modules:** when online, `git ls-remote` can resolve a floating `ref` to a full SHA (rewritten as `?ref=<full-sha>`).
- **Terraform registry modules/providers:** when online (and map/lock miss), HTTP GET against `registry.terraform.io` version APIs selects the latest matching version. Offline still requires a lock entry or `PINNER_TERRAFORM_RESOLVE_MAP`.
- **Helm charts:** when online (and map miss), HTTP repo `index.yaml` (and OCI tags list) can resolve chart versions. Prefer `PINNER_HELM_RESOLVE_MAP` for offline/tests.
- **Helm values images / K8s images:** use the shared Docker/buildx digest helper when online; maps preferred in tests.

### Test / CI resolve-map env vars

Comma-separated `{name}@{requested}=pinned` pairs (see `pinner_iac_common::parse_resolve_map` / `resolve_map_lookup`). Prefer the `name@requested` form so shared constraints (e.g. `~> 5.0`) do not collide across artifacts. Lookup tries `name@requested` first, then a legacy bare `requested` key.

Keys may contain `=` (e.g. Terraform git `?ref=` URLs); use the **last** `=` as the separator. Empty `requested` (missing Helm chart version) uses `{name}@=` → key `{name}@`.

```bash
export PINNER_TERRAFORM_RESOLVE_MAP='vpc@~> 5.0=5.1.0,hashicorp/aws@~> 5.0=5.100.0,git_mod@git::https://example.com/org/mod.git?ref=main=11bd71901bbe5b1630ceea73d27597364c9af683'
export PINNER_HELM_RESOLVE_MAP='redis@^1.0.0=1.2.3,ingress-nginx@=4.10.0,nginx@nginx:latest=nginx@sha256:abc123…'
export PINNER_K8S_RESOLVE_MAP='nginx@nginx:latest=nginx@sha256:abc123…'
```

Network integration tests may require `PINNER_NETWORK=1`. With `--offline`, resolution fails closed unless the lock or resolve map supplies every pin.

Example opt-in Helm/K8s in `pinner.toml`:

```toml
[ecosystems]
helm = true
k8s = true
```

## Toolchain

Resolver binaries (mise, node/npm, uv, docker, gh) can be detected or installed when policy allows:

```bash
pinner toolchain status
pinner toolchain ensure
```

**Prefer a preinstalled [mise](https://mise.jdx.dev)** (checked on `PATH` and common locations such as `~/.local/bin/mise`). `toolchain.install = true` (or ensure with install allowed) may install tools *through* mise, but will **not** download mise via `curl|sh` unless you also set:

```bash
export PINNER_BOOTSTRAP_MISE=1
pinner toolchain ensure
```

That bootstrap prints a warning to stderr. Prefer installing mise yourself in CI images and developer machines.

Runtime resolution prefers `pinner.lock.json` and native locks (`package-lock.json` / `pnpm-lock.yaml` / `yarn.lock`, `uv.lock` / `poetry.lock` / `pdm.lock`), so tools are not required for every `check` once the lock is complete.

### Actions / Compose rewrite notes

GitHub Actions `uses:` and Compose `image:` rewrites are line-oriented and preserve indentation for common list forms (`- uses: …`, nested `steps:`). Full YAML comment round-trips are not guaranteed for exotic multi-line or flow-style nodes.

## CI

See [`.github/workflows/consumer-example.yml`](.github/workflows/consumer-example.yml) for a documentation-only sample. Typical gate:

```yaml
- uses: actions/checkout@v4
- run: cargo install --locked --path crates/pinner # or release binary
- run: pinner toolchain ensure
- run: pinner check
```

## Lock schema

`pinner.lock.json` is validated against [`schemas/pinner.lock.schema.json`](schemas/pinner.lock.schema.json).

## Development

Tooling for this repo is pinned in [`.mise.toml`](.mise.toml) and orchestrated via [Taskfile.yml](Taskfile.yml) ([taskfile.dev](https://taskfile.dev)).

### Bootstrap

```bash
# Install mise if needed: https://mise.jdx.dev/getting-started.html
mise trust
mise install          # rust (+rustfmt/clippy), node, uv, gh, task
task setup            # verifies binaries (docker is optional/host-provided)
```

Pinned tools:

| Tool | Purpose |
|------|---------|
| `rust` 1.96.0 (+ rustfmt, clippy) | Build and test Pinner |
| `node` / `npm` | Node ecosystem resolver evidence |
| `uv` | Python ecosystem resolver evidence |
| `gh` | GitHub Actions resolve |
| `task` | Task runner |
| `docker` | Host-installed only (not via mise) |

### Common tasks

```bash
task                  # list tasks
task setup            # mise install + version smoke checks
task build            # cargo build --workspace
task test             # cargo test --workspace
task fmt              # cargo fmt
task fmt:check        # formatting gate
task clippy           # clippy -D warnings
task ci               # fmt:check + clippy + test + schema
task ci:local         # lean CI summary (preferred before push)
task docs             # build mdBook into ./book (requires mdbook)
task run -- pin --dry-run
task pinner:audit
```

Fixture matrix under `tests/fixtures/*-floating` covers mise, node, python, docker, actions, terraform, helm, and k8s.

## CI, docs, and releases

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| [`ci.yml`](.github/workflows/ci.yml) | push / PR | **lint** (`fmt` + `clippy -D warnings`) and **test** (workspace + schema) |
| [`docs.yml`](.github/workflows/docs.yml) | push to `main` (docs paths) | Build mdBook and deploy [GitHub Pages](https://zloeber.github.io/Pinner/) |
| [`semantic-release.yml`](.github/workflows/semantic-release.yml) | push to `main` | Conventional commits → annotated tag `vX.Y.Z` (needs `PAT_TOKEN`) |
| [`release.yml`](.github/workflows/release.yml) | tag `v*.*.*` | Multi-platform release binaries + GitHub Release |

### Cutting a release

**Automated (preferred):** merge conventional commits to `main` (`feat:`, `fix:`, `feat!:` / `BREAKING CHANGE`). Semantic-release tags `vX.Y.Z`; the release workflow builds binaries. Seed `PAT_TOKEN` once via [`Secretfile.yml`](Secretfile.yml) / SecretZero (see [releasing guide](docs/guide/releasing.md)).

**Manual:**

```bash
git tag -a v0.2.0 -m "v0.2.0"
git push origin v0.2.0
```

No Cargo.toml pre-bump required—the release job aligns workspace version from the tag. `pinner --version` prefers the latest `v*` git tag. Details: [docs/guide/releasing.md](docs/guide/releasing.md).

### GitHub Pages setup (one-time)

In the repo **Settings → Pages**, set **Source** to **GitHub Actions**. The first successful `docs` workflow on `main` publishes the site.

### Local CI before push

Agents and humans should run the lean gate before pushing:

```bash
scripts/ci-local          # fmt + clippy + test + schema (short output)
```

Cursor enforces this via `.cursor/rules/pre-push-local-ci.mdc` and a `beforeShellExecution` hook on `git push`. Optional git hook:

```bash
git config core.hooksPath .githooks   # local only
```

Emergency skip: `PINNER_SKIP_LOCAL_CI=1 git push`.
