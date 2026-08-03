# Pinner

Pin floating dependency versions across mise, Node, Python, Docker, and GitHub Actions. Rewrite manifests to exact pins, commit a unified `pinner.lock.json`, and fail CI when the graph drifts.

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

ignore = ["**/node_modules/**", "**/.git/**", "**/vendor/**"]

[toolchain]
install = true

[pinning]
exact_ranges = true
```

Global flags: `--config`, `--offline`, `--dry-run`, `--ecosystem mise,node`, `--format text|json`.

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

Fixture matrix under `tests/fixtures/*-floating` covers mise, node, python, docker, and actions.
