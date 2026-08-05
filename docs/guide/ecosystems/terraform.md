# terraform

**Default:** on. **Preferred upgrade tool:** Terraform Registry HTTP (latest unconstrained version).

## Pin

Remote `module` blocks and `required_providers` in `*.tf` / `*.tofu`. Lock → `.terraform.lock.hcl` (providers) → map → registry HTTP / `git ls-remote`. Exact semver or full git SHA in `?ref=`.

## Upgrade

Ignores `.terraform.lock.hcl` for upgrade. Registry HTTP selects latest version (not `~>` floor). Git modules: `git ls-remote`. Map: `PINNER_TERRAFORM_RESOLVE_MAP`. Pin style: exact version / full SHA.

## Check

Drift vs `pinner.lock.json`.

## Gaps

- Local module sources (`./`, `../`, absolute paths) skipped.
- CLI `required_version` not pinned.
- Discovery skips `.terraform/`.
