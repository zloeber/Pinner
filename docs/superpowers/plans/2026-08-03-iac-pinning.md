# IaC Version Pinning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Terraform (remote modules + providers), Helm (Chart.yaml + GitOps wrappers), and Kubernetes (core workload images) ecosystems to Pinner so floating IaC refs become strongest pins in sources and `pinner.lock.json`.

**Architecture:** Three new ecosystem crates (`pinner-terraform`, `pinner-helm`, `pinner-k8s`) plus `pinner-iac-common` for shared git-SHA and image-digest helpers. Extend `EcosystemKind`, lock schema, and policy defaults (terraform on; helm/k8s opt-in). Same discover → extract → resolve → rewrite orchestration as existing ecosystems.

**Tech Stack:** Rust 2024, existing workspace deps (`serde_yaml`, `walkdir`, `globset`, `pinner-toolchain`), plus `hcl-edit` for Terraform HCL parse/patch. HTTP via `std::process` tool seams and env resolve maps (same pattern as Docker/Actions); optional later `ureq` only if a task explicitly needs it.

**Spec:** [`docs/superpowers/specs/2026-08-03-iac-pinning-design.md`](../specs/2026-08-03-iac-pinning-design.md)

## Global Constraints

- Follow existing `Ecosystem` trait in `pinner-ecosystem`; no new CLI commands.
- Lock path remains repo-root `pinner.lock.json` (schema version `1`); extend ecosystem enum only.
- Terraform default **enabled**; Helm and K8s default **disabled** (opt-in via `pinner.toml`).
- `--ecosystem` filters already-enabled kinds; it does **not** override opt-in defaults.
- Pin style: registry exact version; git/HTTP → full commit SHA; images → `name@sha256:…`.
- Skip local Terraform module paths (`./`, `../`); do not pin CLI `required_version`.
- Helm must not rewrite values-file images; K8s owns workload images only.
- Structured parsers/patches only (HCL via `hcl-edit`, YAML via `serde_yaml`) — no blind whole-file regex.
- Never invent a pin without lock evidence or successful resolve; `--offline` fails closed.
- Network tests require `PINNER_NETWORK=1`; unit tests use `PINNER_*_RESOLVE_MAP` env seams.
- TDD: failing test → implement → pass → commit for every task.
- Exit codes unchanged: `0` success, `1` drift/findings, `2` tool/config/resolution error.

---

## File structure

```text
crates/
  pinner-iac-common/          # git SHA + image digest helpers + resolve-map parse
  pinner-terraform/           # discover/extract/resolve/rewrite
  pinner-helm/
  pinner-k8s/
  pinner-ecosystem/           # + Terraform, Helm, K8s kinds
  pinner-core/                # policy defaults + toml keys
  pinner-toolchain/           # soft tool requirements if any
  pinner/                     # register ecosystems + parse CLI names
schemas/pinner.lock.schema.json
tests/fixtures/
  terraform-floating/
  helm-floating/
  k8s-floating/
```

---

### Task 1: Ecosystem kinds, schema, and policy defaults

**Files:**
- Modify: `crates/pinner-ecosystem/src/lib.rs`
- Modify: `crates/pinner-ecosystem/tests/types_roundtrip.rs`
- Modify: `schemas/pinner.lock.schema.json`
- Modify: `crates/pinner-core/src/policy.rs`
- Modify: `crates/pinner-core/tests/policy_merge.rs`
- Modify: `crates/pinner/src/main.rs` (`parse_ecosystem` only — registration comes in Task 2)
- Modify: `crates/pinner-toolchain/src/detect.rs` (match arms for new kinds → empty tool lists)

**Interfaces:**
- Consumes: existing `EcosystemKind`, `Policy`
- Produces: `EcosystemKind::{Terraform, Helm, K8s}` with `as_str()` → `"terraform"|"helm"|"k8s"`; default policy enables Terraform only among the three; `pinner.toml` keys `terraform`/`helm`/`k8s`

- [ ] **Step 1: Write the failing policy test**

Add to `crates/pinner-core/tests/policy_merge.rs`:

```rust
#[test]
fn defaults_enable_terraform_but_not_helm_or_k8s() {
    let p = Policy::default_policy();
    assert!(p.is_enabled(EcosystemKind::Terraform));
    assert!(!p.is_enabled(EcosystemKind::Helm));
    assert!(!p.is_enabled(EcosystemKind::K8s));
}

#[test]
fn toml_can_enable_helm_and_k8s() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.toml");
    fs::write(
        &path,
        "[ecosystems]\nhelm = true\nk8s = true\n",
    )
    .unwrap();
    let p = Policy::load(Some(&path)).unwrap();
    assert!(p.is_enabled(EcosystemKind::Helm));
    assert!(p.is_enabled(EcosystemKind::K8s));
    assert!(p.is_enabled(EcosystemKind::Terraform));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-core --test policy_merge defaults_enable_terraform -- --nocapture`  
Expected: FAIL (unknown variant / missing kind)

- [ ] **Step 3: Extend `EcosystemKind` and schema**

In `crates/pinner-ecosystem/src/lib.rs`, add variants `Terraform`, `Helm`, `K8s` to the enum and `as_str()` arms (`"terraform"`, `"helm"`, `"k8s"`).

In `schemas/pinner.lock.schema.json`, extend `"enum"` under `$defs/ecosystem` to include those three strings.

Add a round-trip assertion in `types_roundtrip.rs` for `EcosystemKind::Terraform` serializing as `"terraform"`.

- [ ] **Step 4: Wire policy defaults and toml keys**

In `Policy::default_policy`, append `EcosystemKind::Terraform` to the enabled list (do **not** append Helm/K8s).

In `EcosystemsSection`, add `terraform: Option<bool>`, `helm: Option<bool>`, `k8s: Option<bool>` and apply them in `merge_file` via `apply_ecosystem`.

In `crates/pinner/src/main.rs` `parse_ecosystem`, add `"terraform" | "helm" | "k8s"` arms.

In `crates/pinner-toolchain/src/detect.rs` `required_tools` and `required_by`, add match arms for the three kinds returning empty slices / no tools (soft HTTP later).

- [ ] **Step 5: Run tests and commit**

Run: `cargo test -p pinner-core --test policy_merge && cargo test -p pinner-ecosystem && cargo test --workspace`  
Expected: PASS

```bash
git add crates/pinner-ecosystem crates/pinner-core schemas/pinner.lock.schema.json crates/pinner/src/main.rs crates/pinner-toolchain/src/detect.rs
git commit -m "$(cat <<'EOF'
feat: add terraform/helm/k8s ecosystem kinds and policy defaults

EOF
)"
```

---

### Task 2: Scaffold IaC crates and register ecosystems

**Files:**
- Modify: `Cargo.toml` (workspace members + deps)
- Create: `crates/pinner-iac-common/Cargo.toml`
- Create: `crates/pinner-iac-common/src/lib.rs`
- Create: `crates/pinner-terraform/Cargo.toml`
- Create: `crates/pinner-terraform/src/{lib,discover,extract,resolve,rewrite}.rs`
- Create: `crates/pinner-helm/Cargo.toml`
- Create: `crates/pinner-helm/src/{lib,discover,extract,resolve,rewrite}.rs`
- Create: `crates/pinner-k8s/Cargo.toml`
- Create: `crates/pinner-k8s/src/{lib,discover,extract,resolve,rewrite}.rs`
- Modify: `crates/pinner/Cargo.toml`
- Modify: `crates/pinner/src/main.rs` (`register_ecosystems`)

**Interfaces:**
- Consumes: `Ecosystem` trait, kinds from Task 1
- Produces: Compiling crates; `TerraformEcosystem` / `HelmEcosystem` / `K8sEcosystem` registered; discover returns `Ok(vec![])` until later tasks; `pinner-iac-common` exports `parse_resolve_map(raw: &str) -> HashMap<String, String>`

- [ ] **Step 1: Write failing compile/register smoke**

Add `crates/pinner/tests/cli_smoke.rs` assertion or extend existing smoke so `--ecosystem terraform` is accepted (if a test already covers unknown ecosystems, add terraform to the allow list). Minimal new test in `crates/pinner-terraform/tests/scaffold.rs`:

```rust
use pinner_ecosystem::{Ecosystem, EcosystemKind};
use pinner_terraform::TerraformEcosystem;
use std::path::Path;

#[test]
fn terraform_kind_and_empty_discover() {
    let eco = TerraformEcosystem;
    assert_eq!(eco.kind(), EcosystemKind::Terraform);
    let manifests = eco.discover(Path::new(".")).unwrap();
    assert!(manifests.is_empty() || manifests.iter().all(|m| m.ecosystem == EcosystemKind::Terraform));
}
```

(After scaffold, empty discover is fine; Task 4 will find real files.)

- [ ] **Step 2: Create workspace members**

Root `Cargo.toml` — add members and path deps:

```toml
# members += 
"crates/pinner-iac-common",
"crates/pinner-terraform",
"crates/pinner-helm",
"crates/pinner-k8s",

# workspace.dependencies +=
hcl-edit = "0.9"
pinner-iac-common = { path = "crates/pinner-iac-common" }
pinner-terraform = { path = "crates/pinner-terraform" }
pinner-helm = { path = "crates/pinner-helm" }
pinner-k8s = { path = "crates/pinner-k8s" }
```

Each ecosystem crate mirrors `pinner-docker` layout: `lib.rs` implementing `Ecosystem`, modules returning empty/`Ok(None)` stubs except `kind()`.

`pinner-iac-common/src/lib.rs`:

```rust
use std::collections::HashMap;

pub fn parse_resolve_map(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::parse_resolve_map;

    #[test]
    fn parse_entries() {
        let m = parse_resolve_map("a=b,c=d");
        assert_eq!(m.get("a").map(String::as_str), Some("b"));
        assert_eq!(m.get("c").map(String::as_str), Some("d"));
    }
}
```

Wire `register_ecosystems` in `main.rs`:

```rust
Arc::new(pinner_terraform::TerraformEcosystem),
Arc::new(pinner_helm::HelmEcosystem),
Arc::new(pinner_k8s::K8sEcosystem),
```

Add deps on the three crates (+ iac-common as needed) in `crates/pinner/Cargo.toml`.

- [ ] **Step 3: Run tests and commit**

Run: `cargo test --workspace`  
Expected: PASS

```bash
git add Cargo.toml Cargo.lock crates/pinner-iac-common crates/pinner-terraform crates/pinner-helm crates/pinner-k8s crates/pinner
git commit -m "$(cat <<'EOF'
feat: scaffold terraform, helm, and k8s ecosystem crates

EOF
)"
```

---

### Task 3: `pinner-iac-common` git SHA + image digest helpers

**Files:**
- Modify: `crates/pinner-iac-common/Cargo.toml` (add `pinner-toolchain`)
- Modify: `crates/pinner-iac-common/src/lib.rs`
- Create: `crates/pinner-iac-common/src/git.rs`
- Create: `crates/pinner-iac-common/src/image.rs`
- Create: `crates/pinner-iac-common/tests/helpers.rs`

**Interfaces:**
- Consumes: `pinner_toolchain::CommandRunner`
- Produces:
  - `pub fn resolve_git_sha(runner: &dyn CommandRunner, repo_url: &str, ref_name: &str) -> Result<String, String>`
  - `pub fn normalize_digest_ref(requested: &str, digest_or_ref: &str) -> Option<String>`
  - `pub fn image_name(requested: &str) -> String` (strip tag/digest)
  - `pub fn resolve_image_digest(runner: &dyn CommandRunner, image: &str) -> Result<String, String>` (docker inspect then buildx, same order as docker crate)

- [ ] **Step 1: Write failing tests**

```rust
// crates/pinner-iac-common/tests/helpers.rs
use pinner_iac_common::{image_name, normalize_digest_ref, parse_resolve_map};

#[test]
fn image_name_strips_tag_and_digest() {
    assert_eq!(image_name("ghcr.io/org/app:1.2.3"), "ghcr.io/org/app");
    assert_eq!(
        image_name("ghcr.io/org/app@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "ghcr.io/org/app"
    );
}

#[test]
fn normalize_digest_builds_name_at_sha() {
    assert_eq!(
        normalize_digest_ref(
            "alpine:latest",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        )
        .as_deref(),
        Some("alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p pinner-iac-common --test helpers`  
Expected: FAIL (missing items)

- [ ] **Step 3: Implement helpers**

Port `normalize_digest_ref` / image name stripping / docker digest resolve from `pinner-docker` into `image.rs`. Implement `resolve_git_sha` via `git ls-remote <repo_url> <ref_name>` parsing the first 40-char SHA (full SHA). Keep `parse_resolve_map` public.

- [ ] **Step 4: Tests + commit**

Run: `cargo test -p pinner-iac-common`  
Expected: PASS

```bash
git add crates/pinner-iac-common
git commit -m "$(cat <<'EOF'
feat(iac-common): add git SHA and image digest helpers

EOF
)"
```

---

### Task 4: Terraform discover + extract

**Files:**
- Modify: `crates/pinner-terraform/Cargo.toml` (`hcl-edit`, `walkdir`)
- Modify: `crates/pinner-terraform/src/discover.rs`
- Modify: `crates/pinner-terraform/src/extract.rs`
- Create: `crates/pinner-terraform/tests/extract_floating.rs`
- Create: `tests/fixtures/terraform-floating/modules.tf`
- Create: `tests/fixtures/terraform-floating/providers.tf`

**Interfaces:**
- Consumes: `EcosystemCtx`, HCL files
- Produces: Findings for remote modules + floating providers; `is_floating` true when not exact; local modules omitted

- [ ] **Step 1: Add fixtures**

`tests/fixtures/terraform-floating/modules.tf`:

```hcl
module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.0"
}

module "local_mod" {
  source = "./modules/local"
}

module "git_mod" {
  source = "git::https://example.com/org/mod.git?ref=main"
}
```

`tests/fixtures/terraform-floating/providers.tf`:

```hcl
terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}
```

- [ ] **Step 2: Failing extract test**

```rust
#[test]
fn extracts_remote_modules_and_providers_skips_local() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/terraform-floating");
    let eco = TerraformEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(manifests.len() >= 2);
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let mut findings = Vec::new();
    for m in &manifests {
        findings.extend(eco.extract(m, &ctx).unwrap());
    }
    assert!(findings.iter().any(|f| f.name.contains("vpc") && f.is_floating));
    assert!(findings.iter().any(|f| f.name.contains("git_mod") && f.is_floating));
    assert!(findings.iter().any(|f| f.name == "hashicorp/aws" || f.name.contains("aws")));
    assert!(!findings.iter().any(|f| f.name.contains("local_mod")));
}
```

- [ ] **Step 3: Implement discover + extract**

- Discover: walk `*.tf` / `*.tofu`, skip `.git` / `.terraform`
- Extract modules: parse with `hcl-edit`; remote if source does not start with `.` or `/` (relative local). Floating if missing version, not exact semver, or git ref not full 40-char SHA.
- Extract `required_providers`: floating if constraint is not a single exact version (`= "x.y.z"` or `"x.y.z"`).
- Finding `name`: module label or provider source; `requested`: version constraint or full source+ref string.

- [ ] **Step 4: Tests + commit**

Run: `cargo test -p pinner-terraform`  
Expected: PASS

```bash
git add crates/pinner-terraform tests/fixtures/terraform-floating
git commit -m "$(cat <<'EOF'
feat(terraform): discover manifests and extract floating modules/providers

EOF
)"
```

---

### Task 5: Terraform resolve + rewrite

**Files:**
- Modify: `crates/pinner-terraform/src/resolve.rs`
- Modify: `crates/pinner-terraform/src/rewrite.rs`
- Modify: `crates/pinner-terraform/tests/extract_floating.rs` (or new `resolve_rewrite.rs`)
- Depend on: `pinner-iac-common`

**Interfaces:**
- Consumes: `PINNER_TERRAFORM_RESOLVE_MAP` (`requested=pinned` pairs), lock pins, optional `.terraform.lock.hcl` for providers
- Produces: Pins + HCL rewrites with exact `version` / `ref=<sha>`

- [ ] **Step 1: Failing resolve/rewrite test**

Use env map:

```rust
#[test]
fn resolve_and_rewrite_via_env_map() {
    // set PINNER_TERRAFORM_RESOLVE_MAP for vpc constraint and git ref
    // pin + rewrite modules.tf
    // assert version = "5.1.0" (example) and ref=<40 hex>
}
```

Exact pinned strings in the test must match the map values you set.

- [ ] **Step 2: Implement resolve**

Order: lock match → env map → (providers) parse `.terraform.lock.hcl` if present → offline error → (optional network later gated). For this task, env map + lock + native lock are enough; real registry/git can call `pinner_iac_common::resolve_git_sha` when `PINNER_NETWORK=1` OR when map misses and not offline — if network path is incomplete, fail with clear hint naming the map for tests.

- [ ] **Step 3: Implement rewrite**

Use `hcl-edit` to set module `version` attribute, update git source query `ref`, set provider `version` to exact `"x.y.z"`. Preserve unrelated blocks.

- [ ] **Step 4: Tests + commit**

Run: `cargo test -p pinner-terraform`  
Expected: PASS

```bash
git add crates/pinner-terraform
git commit -m "$(cat <<'EOF'
feat(terraform): resolve and rewrite remote modules and providers

EOF
)"
```

---

### Task 6: Terraform fixture idempotency (pin/check)

**Files:**
- Create: `crates/pinner/tests/terraform_e2e.rs` (or extend `idempotency_all_fixtures.rs`)
- Modify: `tests/fixtures/terraform-floating/` as needed for a complete pin under env map

**Interfaces:**
- Consumes: CLI `pin`/`check` with `--ecosystem terraform`
- Produces: Idempotent pin; second pin empty diff; check clean

- [ ] **Step 1: Write e2e test** mirroring `mise_e2e.rs` / idempotency pattern: copy fixture to tempdir, set `PINNER_TERRAFORM_RESOLVE_MAP`, run `pinner pin --ecosystem terraform`, run again, assert no further rewrite; run `pinner check --ecosystem terraform` exit 0.

- [ ] **Step 2: Implement any glue fixes; run test; commit**

```bash
git add crates/pinner/tests tests/fixtures/terraform-floating
git commit -m "$(cat <<'EOF'
test(terraform): pin/check idempotency for floating fixture

EOF
)"
```

---

### Task 7: Helm discover/extract/resolve/rewrite + fixtures

**Files:**
- Modify: `crates/pinner-helm/src/*`
- Create: `crates/pinner-helm/tests/helm_pin.rs`
- Create: `tests/fixtures/helm-floating/Chart.yaml`
- Create: `tests/fixtures/helm-floating/helmrelease.yaml`
- Create: `tests/fixtures/helm-floating/application.yaml`

**Interfaces:**
- Consumes: YAML Chart.yaml deps; Flux HelmRelease; Argo Application
- Produces: Exact chart versions rewritten; `PINNER_HELM_RESOLVE_MAP` seam

- [ ] **Step 1: Fixtures with floating chart versions** (`version: "*"` / missing / range) plus HelmRelease/Application chart specs.

- [ ] **Step 2: TDD extract → resolve → rewrite** for Chart.yaml and one CRD each. Discover by filename `Chart.yaml` / `Chart.yml` and by `kind: HelmRelease` / `kind: Application`.

- [ ] **Step 3: Do not touch `values.yaml` images.**

- [ ] **Step 4: `cargo test -p pinner-helm` + optional CLI pin with `--ecosystem helm` after enabling in a temp `pinner.toml`. Commit.

```bash
git commit -m "$(cat <<'EOF'
feat(helm): pin Chart.yaml deps and GitOps chart refs

EOF
)"
```

---

### Task 8: Kubernetes workload image pinning

**Files:**
- Modify: `crates/pinner-k8s/src/*`
- Create: `crates/pinner-k8s/tests/k8s_pin.rs`
- Create: `tests/fixtures/k8s-floating/deployment.yaml` (and one CronJob)

**Interfaces:**
- Consumes: `pinner_iac_common` image helpers; `PINNER_K8S_RESOLVE_MAP`
- Produces: Digest-pinned images on Deployment/StatefulSet/DaemonSet/Job/CronJob only

- [ ] **Step 1: Fixture Deployment with `image: nginx:latest` and CronJob with floating image.**

- [ ] **Step 2: TDD discover by kind; extract containers + initContainers; resolve via map/common helper; rewrite to `@sha256:…` with tag in metadata.

- [ ] **Step 3: Ignore non-target kinds (ConfigMap, HelmRelease). Commit.

```bash
git commit -m "$(cat <<'EOF'
feat(k8s): pin core workload container images to digests

EOF
)"
```

---

### Task 9: Docs, README, and consumer examples

**Files:**
- Modify: `README.md`
- Modify: `.github/workflows/consumer-example.yml` (optional helm/k8s note)
- Ensure: design/plan already present

**Interfaces:**
- Consumes: shipped behavior from Tasks 1–8
- Produces: User-facing docs for terraform default-on and helm/k8s opt-in

- [ ] **Step 1: Document new ecosystems, pin styles, `pinner.toml` keys, resolve-map env vars for tests/CI.**

- [ ] **Step 2: Run `cargo test --workspace`; commit.**

```bash
git commit -m "$(cat <<'EOF'
docs: document terraform, helm, and k8s pinning

EOF
)"
```

---

## Self-review checklist (plan author)

1. **Spec coverage:** Terraform modules+providers, Helm Chart+GitOps, K8s five kinds, defaults, lock schema, strongest pins, no local modules, no values images — each mapped to a task.
2. **Placeholders:** None intentional; Tasks 7–8 specify behavior and seams even where full HTTP registry clients are deferred to env maps + shared helpers.
3. **Types:** `EcosystemKind` strings and crate names consistent across tasks.
