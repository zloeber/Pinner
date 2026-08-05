# node

**Default:** on. **Preferred upgrade tool:** `npm view <pkg> version`.

## Pin

Finds floating ranges in `package.json` (workspaces supported). Resolve order: lock → native lock (`package-lock.json` / `pnpm-lock.yaml` / `yarn.lock`) → `PINNER_NODE_RESOLVE_MAP` → `npm view` when online. Rewrites exact versions in `package.json`.

## Upgrade

Ignores native lockfiles for upgrade evidence. Uses `npm view <pkg> version` (or resolve map). Detects pnpm/yarn locks for rewrite context only. Pin style: exact version in `package.json`.

## Check

Drift gate against `pinner.lock.json`.

## Gaps

- Path / link / workspace protocol deps are not upgraded as registry packages.
- Private registries need credentials available to `npm` / maps for offline CI.
