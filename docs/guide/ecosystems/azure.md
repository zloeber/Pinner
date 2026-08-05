# azure

**Default:** off (opt-in: `azure = true`). **Preferred upgrade tool:** docker digests for images; Azure Pipelines **task versions via resolve map** (marketplace HTTP not implemented yet).

## Pin

Azure Pipelines YAML: floating task refs and container images when enabled. Lock → `PINNER_AZURE_RESOLVE_MAP` / docker map → digests for images. Exact task version / image digest.

## Upgrade

- **Images:** docker digests (`docker buildx` / map), same pattern as docker/k8s.
- **Tasks:** map-only via `PINNER_AZURE_RESOLVE_MAP` (`Name@Name@Major=Name@x.y.z` or `Name@Major=x.y.z`). Marketplace/version HTTP upgrade is **not** implemented yet.

Pin style: digest / exact task version.

## Check

Drift vs `pinner.lock.json` when azure enabled.

## Gaps

- **Tasks are map-only** — no Azure Marketplace HTTP resolve for pin or upgrade until that API path lands; set `PINNER_AZURE_RESOLVE_MAP` for CI/offline.
- Requires `azure = true` in `pinner.toml`.
- Image upgrade still needs docker/buildx or docker/azure maps when online evidence is required.
