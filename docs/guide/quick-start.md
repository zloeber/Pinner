# Quick start

Install the latest release (Linux / macOS, x86_64 or arm64):

```bash
curl -fsSL https://raw.githubusercontent.com/zloeber/Pinner/main/scripts/install.sh | zsh
```

Or build from this repository:

```bash
cargo install --locked --path crates/pinner
```

Common commands:

```bash
pinner pin      # resolve + rewrite + write pinner.lock.json
pinner check    # drift gate (no writes)
pinner audit    # report floating refs
pinner audit --fix
pinner explain <name-or-path>
```

Exit codes: `0` success, `1` drift/findings, `2` tool/config/resolution error.

See the [repository README](../README.md) for ecosystem coverage, IaC notes, and resolve-map environment variables used in tests.
