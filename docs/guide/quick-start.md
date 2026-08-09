# Quick start

Install the latest release (Linux / macOS, x86_64 or arm64):

```bash
curl -fsSL https://raw.githubusercontent.com/zloeber/Pinner/main/scripts/install.sh | bash
```

Or via mise backends:

```bash
mise use -g github:zloeber/Pinner
# or: mise use -g cargo:pinner
```

Or build from this repository:

```bash
cargo install --locked --path crates/pinner
```

Common commands:

```bash
pinner pin       # resolve floating → exact + rewrite + pinner.lock.json
pinner upgrade   # bump exact pins to latest + rewrite + lock
pinner check     # drift gate (no writes)
pinner audit     # report floating refs (live progress on TTY stderr; pretty panel on stdout)
pinner audit --fix
pinner explain <name-or-path>
pinner pin --recursive            # include subdirectories under current path
pinner pin --path ./services/api  # scan a specific directory
```

Modes:

```bash
pinner pin --agent                 # JSON, no prompts (automation / agents)
pinner upgrade --agent             # same for upgrades
pinner pin --walkthrough           # interactive compact-list gate (TTY only)
pinner upgrade --walkthrough       # interactive accept/skip/edit per row (TTY only)
pinner audit --format json         # machine-readable findings
pinner audit --agent               # same JSON contract for agents
```

Interactive text `audit` prints per-ecosystem discover/extract progress on stderr; `--agent` / `--format json` stay quiet on stderr for progress.

**Agents:** prefer `--agent` / `--format json`. Never pass `--walkthrough` in non-interactive or agent loops.

Opt-in ecosystems (`helm`, `k8s`, `gitlab`, `azure`) need `pinner.toml` — see [Configuration](configuration.md).

Exit codes: `0` success, `1` drift/findings, `2` tool/config/resolution/invalid mode error.

See the [repository README](../README.md) for the provider matrix and [ecosystems](ecosystems/README.md) for Pin / Upgrade / Check / Gaps per provider.
