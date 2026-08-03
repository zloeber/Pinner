# Pinner

Pin floating dependency versions across mise, Node, Python, Docker, GitHub Actions, Terraform, Helm, and Kubernetes. Rewrite manifests to exact pins, commit a unified `pinner.lock.json`, and fail CI when the graph drifts.

## Install

From this repository:

```bash
cargo install --locked --path crates/pinner
```

Or, once published / mirrored via mise:

```bash
mise install pinner   # when a mise backend/plugin is available
# or: cargo install pinner
```

## Quick start

```bash
# Resolve floating refs, rewrite sources, write pinner.lock.json
pinner pin

# Drift gate (no writes) — exit 1 on mismatch, 2 on tool/config errors
pinner check

# Report floating refs; optionally apply pins
pinner audit
pinner audit --fix

# Explain why a pin was chosen
pinner explain <name-or-path>
```

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

ignore = ["**/node_modules/**", "**/.git/**", "**/vendor/**"]

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
| **helm** | off (opt-in) | exact chart version strings | `Chart.yaml` dependencies; Flux `HelmRelease`; Argo CD `Application` |
| **k8s** | off (opt-in) | container images → `name@sha256:…` | YAML workloads: Deployment, StatefulSet, DaemonSet, Job, CronJob |

### What is skipped

- **Terraform:** local module sources (`./`, `../`, absolute paths); CLI `required_version`; `.terraform/` directory during discovery.
- **Helm:** `values.yaml` / `values.yml` (including images there — use the **k8s** ecosystem for workload images).
- **Kubernetes:** non-workload kinds (ConfigMap, HelmRelease, etc.).

### Resolution (lock, native evidence, env maps)

Resolve order matches other ecosystems: existing `pinner.lock.json` pins first, then ecosystem-specific evidence, then network/tool resolve when online.

- **Terraform providers:** may use `.terraform.lock.hcl` provider selections when present.
- **Terraform git modules:** `git ls-remote` (or equivalent) when online; rewritten to pin `?ref=<full-sha>`.
- **Registry HTTP clients** for Terraform module/provider registries and Helm chart repos/OCI are **not fully implemented** — offline/tests use env resolve maps; online Helm resolve still requires a map or lock today. K8s images use the shared Docker/buildx digest helper when online.

### Test / CI resolve-map env vars

Comma-separated `requested=pinned` pairs (see `pinner_iac_common::parse_resolve_map`). Keys may contain `=` (e.g. Terraform git `?ref=` URLs); use the **last** `=` as the separator. Empty keys are allowed (e.g. missing Helm chart version → `=1.2.3`).

```bash
export PINNER_TERRAFORM_RESOLVE_MAP='~> 5.0=5.1.0,git::https://example.com/org/mod.git?ref=main=11bd71901bbe5b1630ceea73d27597364c9af683'
export PINNER_HELM_RESOLVE_MAP='^1.0.0=1.2.3'
export PINNER_K8S_RESOLVE_MAP='nginx:latest=nginx@sha256:abc123…'
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
task run -- pin --dry-run
task pinner:audit
```

Fixture matrix under `tests/fixtures/*-floating` covers mise, node, python, docker, actions, terraform, helm, and k8s.
