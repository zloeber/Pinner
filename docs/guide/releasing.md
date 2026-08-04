# Releasing

Pinner uses **tag-driven semantic versions**. A GitHub Release (with multi-platform binaries) is created when a version tag is pushed—usually by the semantic-release workflow after conventional commits land on `main`.

## Automated path (preferred)

1. Merge PRs to `main` using conventional commit subjects:
   - `feat:` → minor bump
   - `fix:` / `refactor:` / `perf:` → patch bump
   - `feat!:` or commit body containing `BREAKING CHANGE` → major bump
   - Docs/chore-only commits do **not** create a release
2. [`.github/workflows/semantic-release.yml`](../../.github/workflows/semantic-release.yml) runs on `main`, computes the next version, commits a `chore(release): vX.Y.Z` bump of `[workspace.package].version` + `Cargo.lock`, and pushes an annotated tag on that commit.
3. [`.github/workflows/release.yml`](../../.github/workflows/release.yml) runs on that tag, aligns workspace version from the tag for the build (idempotent when already bumped), runs `cargo update -w`, compiles binaries with `--locked`, and publishes the GitHub Release.

`pinner --version` reports the latest `v*` git tag (via `build.rs` / `git describe`), falling back to `Cargo.toml` when git history is unavailable. `task install` / `cargo install --path` report the `Cargo.toml` package version—kept in sync by the release commit above.

### One-time: seed `PAT_TOKEN` with SecretZero

Semantic-release checks out with `GITHUB_TOKEN` (`persist-credentials: false`), then uses a classic PAT only for the tag push so `release.yml` can chain (default `GITHUB_TOKEN` pushes do not trigger other workflows). The push step also clears checkout’s `http.*.extraheader` so URL-embedded PAT credentials are not overridden by a persisted job token.

1. Create a **classic** PAT (`ghp_…`) with `repo` / contents write for `zloeber/Pinner`. Fine-grained tokens work only if they have Contents: Read and Write on this repo.
2. From the repo root, sync via SecretZero using [`Secretfile.yml`](../../Secretfile.yml) (maps `release_token` → Actions secret `PAT_TOKEN`):

```bash
# Human / local — do not paste the token into agent chat
export GITHUB_TOKEN=...   # token that can write GitHub Actions secrets
export PAT_TOKEN=...      # the release PAT to seed
secretzero agent sync --web
# or: secretzero web
```

3. Store `PAT_TOKEN` as a **repository** secret (SecretZero does this). The semantic-release job uses the `production` environment; repository secrets remain available there. Optionally add the same secret as an environment secret if you prefer environment-scoped credentials.

Never commit PAT values.

If semantic-release fails on “Failed to push tag with PAT_TOKEN”, the secret exists but the token value is expired, revoked, or missing write scope — create a new classic PAT and re-run `secretzero agent sync --web`.

If a tag is created but **release** never starts, the tag was almost certainly pushed with `GITHUB_TOKEN` (checkout’s persisted `http.extraheader` beating the PAT URL). Confirm with Actions: no `release` run for that tag. Fix is already in `semantic-release.yml` (`persist-credentials: false` + clear extraheader before push). To recover a stuck tag (for example `v0.2.0`): delete it and re-push with a human PAT, or delete it and re-run **Semantic Release** after the fix is on `main`:

```bash
git push origin :refs/tags/v0.2.0
# then: Actions → Semantic Release → Run workflow
# or re-push the annotated tag from a machine authenticated as a user/PAT
```

## Manual tag (still supported)

```bash
git tag -a v0.2.0 -m "v0.2.0"
git push origin v0.2.0
```

You do **not** need to bump `Cargo.toml` first for a manual tag: the release workflow sets the workspace version from the tag before building. Prefer letting semantic-release bump `Cargo.toml` on `main` so local `task install` matches the published version.

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
