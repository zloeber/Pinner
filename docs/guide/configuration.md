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

ignore = ["**/node_modules/**", "**/.git/**", "**/vendor/**"]

[toolchain]
install = true

[pinning]
exact_ranges = true
```

Global flags: `--config`, `--offline`, `--dry-run`, `--ecosystem mise,terraform`, `--format text|json`, `--agent`, `--walkthrough`.

`--ecosystem` only filters ecosystems that are already enabled in policy. Enable Helm/Kubernetes/GitLab/Azure with `helm = true` / `k8s = true` / `gitlab = true` / `azure = true` in `pinner.toml` before filtering.

## Agent vs walkthrough

- `--agent` (or `--format json` / non-TTY): structured JSON, no prompts. Prefer this in automation and AI agent loops.
- `--walkthrough`: interactive compact-list gate before writes. Requires a TTY; combining with `--agent` / `--format json` / non-TTY exits `2`.
- Agents should never pass `--walkthrough`.
