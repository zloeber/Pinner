# Quick start

Install the latest release (Linux / macOS, x86_64 or arm64):

```bash
curl -fsSL https://raw.githubusercontent.com/zloeber/Pinner/main/scripts/install.sh | bash
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

Modes:

```bash
pinner pin --agent              # JSON, no prompts (automation / agents)
pinner pin --walkthrough        # interactive compact-list gate (TTY only)
pinner audit --format json      # machine-readable findings
```

Opt-in ecosystems (`helm`, `k8s`, `gitlab`, `azure`) need `pinner.toml` — see [Configuration](configuration.md).

Exit codes: `0` success, `1` drift/findings, `2` tool/config/resolution/invalid mode error.

See the [repository README](../README.md) for ecosystem coverage, IaC notes, and resolve-map environment variables used in tests.
