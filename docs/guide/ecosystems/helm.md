# helm

**Default:** off (opt-in: `helm = true`). **Preferred upgrade tool:** HTTP repo `index.yaml` / OCI tags (latest chart).

## Pin

Chart dependencies (`Chart.yaml`), Flux/Argo sources, and floating images in `values*.yaml`. Lock → map → HTTP/OCI / docker digests. Exact chart version and `name@sha256:…` for images.

## Upgrade

Latest chart version from index/OCI; values images via docker digests. Map: `PINNER_HELM_RESOLVE_MAP`. Pin style: exact chart ver / image digest.

## Check

Drift vs `pinner.lock.json` (only when helm enabled).

## Gaps

- Requires `helm = true` in `pinner.toml`.
- Private/OCI repos need network credentials or maps for offline CI.
