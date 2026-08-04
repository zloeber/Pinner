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

Artifacts are attached to the GitHub Release as `.tar.gz` (Unix) or `.zip` (Windows).

## Dry run

Use **Actions → release → Run workflow** with `dry_run=true` to build packages without publishing a release. Set `dry_run=false` only when you intend to publish from `workflow_dispatch` (normally you publish by pushing a tag).

## CI quality gates

Pull requests and pushes run:

- **lint** — `cargo fmt --check` and `clippy -D warnings`
- **test** — full workspace tests + lock schema fixtures

Documentation is built with mdBook and deployed to GitHub Pages from `main`.
