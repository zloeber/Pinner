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
helm = false       # opt-in
k8s = false        # opt-in

ignore = ["**/node_modules/**", "**/.git/**", "**/vendor/**"]

[toolchain]
install = true

[pinning]
exact_ranges = true
```

Global flags: `--config`, `--offline`, `--dry-run`, `--ecosystem mise,terraform`, `--format text|json`.

`--ecosystem` only filters ecosystems that are already enabled in policy. Enable Helm/Kubernetes with `helm = true` / `k8s = true` in `pinner.toml` before filtering.
