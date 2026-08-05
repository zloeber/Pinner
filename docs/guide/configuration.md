# Configuration

Optional repo-root `pinner.toml`:

```toml
[ecosystems]
mise = true
node = true
python = true
docker = true
actions = true
terraform = true   # default on
cargo = true       # default on
go = true          # default on
ruby = true        # default on
helm = false       # opt-in
k8s = false        # opt-in
gitlab = false     # opt-in
azure = false      # opt-in

ignore = ["**/node_modules/**", "**/.git/**", "**/vendor/**", "**/tests/fixtures/**"]

[toolchain]
install = true

[pinning]
exact_ranges = true
```

Global flags: `--config`, `--offline`, `--dry-run`, `--ecosystem mise,terraform`, `--format text|json`, `--agent`, `--walkthrough`.

`--ecosystem` only filters ecosystems that are already enabled in policy. Enable Helm/Kubernetes/GitLab/Azure with `helm = true` / `k8s = true` / `gitlab = true` / `azure = true` in `pinner.toml` before filtering.

## Upgrade (v1)

`pinner upgrade` uses the same globals and `[ecosystems]` enable flags as `pin` / `check`. There is **no** `[upgrade]` table and no new required `pinner.toml` keys in v1. `--ecosystem` still filters enabled kinds only.

Upgrade skips native lock evidence (and Terraform `.terraform.lock.hcl`) so bumps target latest from preferred tools / registries / resolve maps — see [ecosystems](ecosystems/README.md). Prior `pinner.lock.json` pins are still passed into resolve context for display-only `previous` metadata.

Default ignore globs include `**/tests/fixtures/**` so dogfooding pin/upgrade at a repo root does not walk fixture trees.

## Agent vs walkthrough

- `--agent` (or `--format json` / non-TTY): structured JSON, no prompts. Prefer this in automation and AI agent loops.
- `--walkthrough`: interactive compact-list gate before writes. Requires a TTY; combining with `--agent` / `--format json` / non-TTY exits `2`.
- Agents should never pass `--walkthrough`.
