# Releasing

Pinner uses **tag-driven semantic versions**. A GitHub Release (with multi-platform binaries) is created when a version tag is pushed.

## Versioning

1. Bump `[workspace.package].version` in the root `Cargo.toml` (for example `0.2.0`).
2. Commit the bump (and any changelog notes).
3. Tag and push:

```bash
git tag -a v0.2.0 -m "v0.2.0"
git push origin v0.2.0
```

The tag **must** match `v` + the Cargo workspace version (`v0.2.0` ↔ `0.2.0`). The release workflow fails if they diverge.

Supported tag patterns:

- `vMAJOR.MINOR.PATCH` (for example `v1.0.0`)
- Pre-release suffix: `vMAJOR.MINOR.PATCH-SUFFIX` (for example `v1.0.0-rc.1`) — Cargo version must match including the suffix.

## What the release workflow builds

| Target | Runner |
|--------|--------|
| `x86_64-unknown-linux-gnu` | ubuntu-latest |
| `aarch64-unknown-linux-gnu` | ubuntu-24.04-arm |
| `x86_64-apple-darwin` | macos-15-intel |
| `aarch64-apple-darwin` | macos-latest |
| `x86_64-pc-windows-msvc` | windows-latest |

Artifacts are attached to the GitHub Release as `.tar.gz` (Unix) or `.zip` (Windows). Asset names follow `pinner-{version}-{target}.tar.gz` (for example `pinner-0.2.0-x86_64-unknown-linux-gnu.tar.gz`).

## End-user install

After a release is published, users on Linux or macOS can install with:

```bash
curl -fsSL https://raw.githubusercontent.com/zloeber/Pinner/main/scripts/install.sh | bash
```

Environment variables:

| Variable | Default | Purpose |
|----------|---------|---------|
| `PINNER_VERSION` | latest GitHub Release | Pin a semver (without `v`) |
| `PINNER_INSTALL_DIR` | `$HOME/.local/bin` | Install destination |
| `PINNER_INSTALL_DRY_RUN` | unset | Set to `1` to print URL and path only |
| `PINNER_REPO` | `zloeber/Pinner` | GitHub `owner/repo` for releases |

Windows users download the `.zip` asset from the GitHub Release page.

## Dry run

Use **Actions → release → Run workflow** with `dry_run=true` to build packages without publishing a release. Set `dry_run=false` only when you intend to publish from `workflow_dispatch` (normally you publish by pushing a tag).

## CI quality gates

Pull requests and pushes run:

- **lint** — `cargo fmt --check` and `clippy -D warnings`
- **test** — full workspace tests + lock schema fixtures

Documentation is built with mdBook and deployed to GitHub Pages from `main`.
