# Releasing

Pinner uses **tag-driven semantic versions**. A GitHub Release (with multi-platform binaries) is created when a version tag is pushed—usually by the semantic-release workflow after conventional commits land on `main`.

## Automated path (preferred)

1. Merge PRs to `main` using conventional commit subjects:
   - `feat:` → minor bump
   - `fix:` / `refactor:` / `perf:` → patch bump
   - `feat!:` or commit body containing `BREAKING CHANGE` → major bump
   - Docs/chore-only commits do **not** create a release
2. [`.github/workflows/semantic-release.yml`](../../.github/workflows/semantic-release.yml) runs on `main`, computes the next version, and pushes an annotated tag `vMAJOR.MINOR.PATCH`.
3. [`.github/workflows/release.yml`](../../.github/workflows/release.yml) runs on that tag, rewrites `[workspace.package].version` from the tag for the build, compiles binaries, and publishes the GitHub Release.

`pinner --version` reports the latest `v*` git tag (via `build.rs` / `git describe`), falling back to `Cargo.toml` when git history is unavailable.

### One-time: seed `PAT_TOKEN` with SecretZero

Semantic-release needs a classic PAT with `contents:write` so the tag push can trigger `release.yml` (the default `GITHUB_TOKEN` often cannot chain workflows).

1. Create a classic PAT with repo contents write access for `zloeber/Pinner`.
2. From the repo root, sync via SecretZero using [`Secretfile.yml`](../../Secretfile.yml) (maps `release_token` → Actions secret `PAT_TOKEN`):

```bash
# Human / local — do not paste the token into agent chat
export GITHUB_TOKEN=...   # token that can write GitHub Actions secrets
export PAT_TOKEN=...      # the release PAT to seed
secretzero agent sync --web
# or: secretzero web
```

3. Bind the `production` GitHub Actions environment (used by semantic-release) so it can read `PAT_TOKEN`, or store `PAT_TOKEN` as a repository secret.

Never commit PAT values.

## Manual tag (still supported)

```bash
git tag -a v0.2.0 -m "v0.2.0"
git push origin v0.2.0
```

You do **not** need to bump `Cargo.toml` first. The release workflow sets the workspace version from the tag before building. Local `Cargo.toml` may lag; the CLI still prefers git tags when present.

Supported tag patterns:

- `vMAJOR.MINOR.PATCH` (for example `v1.0.0`)
- Pre-release suffix: `vMAJOR.MINOR.PATCH-SUFFIX` (for example `v1.0.0-rc.1`)

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

Use **Actions → release → Run workflow** with `dry_run=true` to build packages without publishing a release. Set `dry_run=false` only when you intend to publish from `workflow_dispatch` (normally you publish by pushing a tag or via semantic-release).

## CI quality gates

Pull requests and pushes run:

- **lint** — `cargo fmt --check` and `clippy -D warnings`
- **test** — full workspace tests + lock schema fixtures

Documentation is built with mdBook and deployed to GitHub Pages from `main`.
