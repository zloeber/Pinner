# docker

**Default:** on. **Preferred upgrade tool:** `docker buildx imagetools inspect`.

## Pin

Discovers floating image tags in Dockerfiles / Compose. Resolves digests via lock → map → buildx/local inspect. Rewrites `name@sha256:…`.

## Upgrade

Retargets mutable tags to a fresh digest (or upgrades digest for the same floating-tag policy). Preferred: `docker buildx imagetools inspect`; local inspect and `PINNER_DOCKER_RESOLVE_MAP` as fallbacks. Pin style: `name@sha256:…`.

## Check

Drift vs `pinner.lock.json`.

## Gaps

- Exact digest pins without tag metadata may skip re-resolve.
- Buildx / daemon access required for online digest resolution.
