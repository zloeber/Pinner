# k8s

**Default:** off (opt-in: `k8s = true`). **Preferred upgrade tool:** docker digests (`docker buildx imagetools inspect`).

## Pin

Container images in workload YAML (Deployment, StatefulSet, DaemonSet, Job, CronJob). Lock → map → digests. Rewrites `name@sha256:…`.

## Upgrade

Fresh digests for image references. Map: `PINNER_K8S_RESOLVE_MAP`. Pin style: image digest.

## Check

Drift vs `pinner.lock.json` when k8s enabled.

## Gaps

- Non-workload kinds (ConfigMap, HelmRelease, etc.) skipped.
- Requires `k8s = true` in `pinner.toml`.
