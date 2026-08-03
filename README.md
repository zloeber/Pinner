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

Runtime resolution prefers `pinner.lock.json` and native locks, so tools are not required for every `check` once the lock is complete.

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

```bash
cargo test --workspace
```

Fixture matrix under `tests/fixtures/*-floating` covers mise, node, python, docker, and actions.
