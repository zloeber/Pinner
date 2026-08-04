# IaC Version Pinning Design

**Date:** 2026-08-03  
**Status:** Approved for implementation planning  
**Summary:** Extend Pinner with Terraform, Helm, and Kubernetes ecosystems that discover floating infrastructure-as-code version refs, resolve them to strongest pins, rewrite sources, and record them in `pinner.lock.json`.

## Problem

Infrastructure repos accumulate floating refs: Terraform module constraints (`~> 3.0`), git module `ref=main`, unpinned providers, Helm chart `*` / missing versions in Chart.yaml and GitOps CRDs, and Kubernetes workload images on `:latest` or mutable tags. Those resolve differently over time, so apply/deploy is not reproducible.

Pinner already freezes mise, Node, Python, Docker, and GitHub Actions. This feature brings the same discover → resolve → rewrite → lock → check loop to common IaC surfaces.

## Goals

1. Pin **remote Terraform modules** and **provider version constraints** to exact versions (registry) or full commit SHAs (git/HTTP).
2. Pin **Helm chart dependencies** and **GitOps chart refs** (`HelmRelease`, `Application`) to exact chart versions.
3. Pin **Kubernetes workload container images** (core kinds only) to digests.
4. Emit/update unified `pinner.lock.json` entries for the new ecosystems.
5. Keep `pinner check` offline-first against the committed lock.
6. Follow the existing ecosystem plugin trait and CLI (no new top-level commands).

### Non-goals

- Local Terraform module paths (`./`, `../`).
- Terraform/OpenTofu CLI `required_version` (mise already covers tool versions).
- ~~Image pins inside Helm `values.yaml` (owned by the Kubernetes ecosystem when those images appear in workload manifests).~~ **Superseded (2026-08-04 expansion):** Helm **must** pin floating images in `values.yaml` / `values*.yaml` (see expansion design/plan Task 10). Workload manifests remain owned by the **k8s** ecosystem.
- Scanning arbitrary CRDs for embedded images beyond the five core workload kinds.
- Replacing Renovate/Dependabot or opening upgrade PRs.
- Private registry auth UX beyond passing through existing env credentials (same as Docker/Actions today).

## Decisions (from design review)

| Topic | Choice |
|-------|--------|
| Scope | Terraform + Helm + K8s (phased delivery) |
| Terraform surface | Remote modules + `required_providers` |
| Pin style | Strongest freeze |
| Helm surface | `Chart.yaml` deps + `HelmRelease` / `Application` |
| K8s surface | Deployment, StatefulSet, DaemonSet, Job, CronJob |
| Defaults | Terraform **on**; Helm and K8s **opt-in** |
| Packaging | Three ecosystem crates + shared `pinner-iac-common` |

## Architecture

```text
pinner (CLI)
    ├── pinner-core              # policy, lock I/O, orchestration (unchanged flow)
    ├── pinner-ecosystem         # + EcosystemKind::{Terraform, Helm, K8s}
    ├── pinner-iac-common        # git SHA resolve, registry/OCI HTTP helpers, image digest seams
    ├── pinner-terraform         # default enabled
    ├── pinner-helm              # opt-in
    ├── pinner-k8s               # opt-in
    └── existing mise/node/python/docker/actions…
```

Each new crate implements the existing `Ecosystem` trait:

| Method | Role |
|--------|------|
| `discover(repo)` | Find manifests |
| `extract(manifest)` | Floating / unpinned refs |
| `resolve(findings, ctx)` | Concrete pins (lock → native evidence → network/tool) |
| `rewrite(manifest, pins)` | Structured patch |

**Shared helpers (`pinner-iac-common`):**

- Git ref → full commit SHA (same idea as Actions / `gh`)
- Terraform module/provider registry HTTP
- Helm repo index / OCI chart version lookup
- Image tag → digest (reuse Docker resolve patterns; K8s must not duplicate registry logic)

**Kind split:** three `EcosystemKind` values so `--ecosystem`, policy toggles, and lock entries stay independent. Helm does not pin container images; that is exclusively `pinner-k8s`.

**Delivery order:** Terraform → Helm → K8s. Each phase ships pin/check/audit + fixtures before the next starts.

## Per-ecosystem behavior

### Terraform (`pinner-terraform`) — default on

| Stage | Behavior |
|-------|----------|
| Discover | `**/*.{tf,tofu}` (honor ignore globs) |
| Extract | `module` blocks with remote `source` (Terraform Registry, `git::`, `github.com`, HTTP/HTTPS). Skip local `./` and `../`. Also floating `required_providers` version constraints. |
| Floating signals | Missing / `latest` module version; constraints that are not a single exact version (`~>`, `>=`, ranges); git `ref=main` / `master` / branch / non-SHA tag; unpinned or ranged provider versions |
| Resolve | `pinner.lock.json` → `.terraform.lock.hcl` (providers) → registry API / git → fail closed under `--offline` |
| Rewrite | Exact `version = "x.y.z"` for registry modules and providers; git/HTTP sources get full commit SHA in `ref=` (query or equivalent). Structured HCL parse/patch — not blind regex. |
| Pin style | Registry: exact version. Git/HTTP: full commit SHA. |

**Non-goals (terraform crate):** local path modules; CLI `required_version`; rewriting generated `.terraform/` trees.

### Helm (`pinner-helm`) — default off

| Stage | Behavior |
|-------|----------|
| Discover | `**/Chart.yaml`; Flux `HelmRelease`; Argo CD `Application` (YAML matched by `apiVersion` / `kind`) |
| Extract | Chart dependencies (`dependencies[].name` / `version` / `repository`); HelmRelease / Application chart name + version + repo or OCI URL |
| Floating signals | Missing version, ranges, `*`, `latest` |
| Resolve | Lock → Helm repo index / OCI tags → exact chart version |
| Rewrite | Exact semver in Chart.yaml and CRD chart version fields. No values-file image rewriting. |

### Kubernetes (`pinner-k8s`) — default off

| Stage | Behavior |
|-------|----------|
| Discover | YAML/YML documents whose `kind` is Deployment, StatefulSet, DaemonSet, Job, or CronJob |
| Extract | `spec.template.spec.containers[].image` and `initContainers[].image` (CronJob via `jobTemplate.spec.template.spec`) |
| Floating signals | `:latest`, untagged images, non-digest tags |
| Resolve | Lock → shared image digest helper (same path as Docker) |
| Rewrite | `image: name@sha256:…`; retain original tag in pin `metadata` when useful |

## Policy and config

Optional `pinner.toml` overrides:

```toml
[ecosystems]
terraform = true   # default on
helm = false       # default off
k8s = false        # default off
```

Existing ignore globs and allowlisted floating refs apply; allowlist entries use the new ecosystem names (`terraform`, `helm`, `k8s`).

CLI filter works as today: `--ecosystem terraform,helm,k8s`.

## Lock format

Extend `schemas/pinner.lock.schema.json` ecosystem enum with `terraform`, `helm`, `k8s`. Entry shape unchanged (`ecosystem`, `name`, `requested`, `pinned`, `source`, `path`, `evidence`, `metadata`).

Example metadata:

| Ecosystem | Useful metadata |
|-----------|-----------------|
| terraform module | `source_type`: `registry` \| `git` \| `http`; `module_source` |
| terraform provider | `source_type`: `provider`; `registry_source` (e.g. `hashicorp/aws`) |
| helm | `repository`, `chart` |
| k8s | `tag`, `kind` |

Resolution order per finding (unchanged globally):

1. Valid `pinner.lock.json` entry
2. Native lock evidence (`.terraform.lock.hcl` for providers)
3. Registry / network / tool resolve
4. Fail closed if unresolved under `--offline` or strict policy

## Toolchain

| Ecosystem | Requirements |
|-----------|--------------|
| terraform | No `terraform` binary required for pin. Registry via HTTP. Git sources need `git` (or equivalent resolve). |
| helm | Prefer in-crate HTTP/OCI client; optional `helm` CLI fallback later if needed. |
| k8s | Prefer registry HTTP digest resolve; optional `docker` fallback shared with Docker ecosystem. |

`pinner toolchain status` / `ensure` gain entries only for tools that become hard requirements. Soft/optional tools stay documented but are not forced installs for `check` when the lock is complete.

Runtime: `pinner check` is offline-first. Network resolve runs on `pin` / `audit --fix` when lock/native evidence is missing.

## Errors

- Same exit codes: `0` success; `1` drift/findings; `2` tool/config/resolution error.
- Resolution failure includes ecosystem, path, requested ref, and actionable hint.
- No silent partial success for selected ecosystems: resolve all, then write rewrites + lock (existing orchestration).
- Malformed/untrusted lock → reject; do not half-apply rewrites.

## Testing

- **Unit:** HCL/YAML extract and rewrite fixtures (no network).
- **Resolve seams:** env maps analogous to `PINNER_DOCKER_RESOLVE_MAP` / `PINNER_ACTIONS_RESOLVE_MAP` for terraform, helm, and k8s.
- **Fixtures:** `tests/fixtures/terraform-floating/`, `helm-floating/`, `k8s-floating/`.
- **Idempotency:** `pin` twice ⇒ empty second source/lock diff; `check` clean after `pin`.
- **Network:** gated with `PINNER_NETWORK=1`.
- **Policy:** default-enabled terraform vs opt-in helm/k8s covered in policy merge tests.

## CLI / UX impact

No new commands. Existing:

```bash
pinner pin
pinner check
pinner audit
pinner audit --fix
pinner explain <name-or-path>
pinner toolchain status
```

Users enable Helm/K8s via config (required — `--ecosystem` only filters already-enabled kinds and does not override opt-in defaults):

```toml
[ecosystems]
helm = true
k8s = true
```

After opt-in, narrowing is allowed: `pinner pin --ecosystem helm`.

## Implementation phasing

1. **Foundation:** `EcosystemKind` + schema + policy defaults/opt-in; scaffold `pinner-iac-common` and three crates wired into the CLI (helm/k8s no-op until implemented).
2. **Terraform:** discover/extract/resolve/rewrite for remote modules + providers; fixtures; idempotency.
3. **Helm:** Chart.yaml + HelmRelease/Application; fixtures; idempotency.
4. **Kubernetes:** core workload image digests via shared helper; fixtures; idempotency.
5. **Docs / README / consumer CI examples** for the new ecosystems.

## Success criteria

- Remote Terraform modules and floating providers are pinned to exact versions or commit SHAs; local modules are ignored.
- With helm/k8s enabled, floating chart versions and core-workload images are pinned or reported.
- `pinner pin` is idempotent for the new ecosystems.
- `pinner check` fails on drift without network when the lock is present.
- Helm/K8s remain off unless opted in; Terraform runs by default and no-ops when no `.tf`/`.tofu` files exist.
