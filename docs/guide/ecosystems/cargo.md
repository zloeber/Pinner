# cargo

**Default:** on. **Preferred upgrade tool:** crates.io HTTP API.

## Pin

Floating deps in `Cargo.toml`. Lock → map → crates.io (or `cargo` CLI if used). Exact semver in manifests. Path/git deps skipped for registry resolve.

## Upgrade

crates.io HTTP for latest matching version; optional `cargo search` / `cargo info` if present; `PINNER_CARGO_RESOLVE_MAP`. Pin style: exact semver in `Cargo.toml`.

## Check

Drift vs `pinner.lock.json`.

## Gaps

- Path and git dependencies are not upgraded via crates.io.
- Workspace members follow crate extract rules (not every path dep).
