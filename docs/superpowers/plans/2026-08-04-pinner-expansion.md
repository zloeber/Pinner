# Pinner Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship new pin targets (Cargo, Go, Ruby, GitLab CI, Azure DevOps), deepen GitHub Actions, close mise/Helm/TF-Helm gaps, add compact-list walkthrough TUI + agent mode, curl installer, and `skills/pinner/SKILL.md` in one release.

**Architecture:** Extend-in-place. New ecosystem crates follow `discover`/`extract`/`resolve`/`rewrite`. `pinner-ui` owns ratatui walkthrough + pretty TTY summaries. `pinner-core` gains a pure walkthrough filter applied after resolve and before rewrite. Installer and skill live outside the Rust graph.

**Tech Stack:** Rust 2024 workspace, clap, serde/toml_edit/serde_yaml, ratatui + crossterm, existing `pinner-iac-common` image/git helpers, zsh/`curl` installer script.

**Spec:** [`docs/superpowers/specs/2026-08-04-pinner-expansion-design.md`](../specs/2026-08-04-pinner-expansion-design.md)

## Global Constraints

- One binary `pinner`; no second UI binary.
- Lock remains `pinner.lock.json` schema version `1`; extend ecosystem enum strings only.
- Languages (cargo/go/ruby) **default enabled**; gitlab/azure **opt-in**; actions deepen stays default-on with actions.
- `--ecosystem` filters already-enabled kinds; does **not** override opt-in defaults.
- Evidence order: lock → native lock → registry/tool → fail; `--offline` fails closed without lock/map.
- Walkthrough: compact list; accept/skip/edit/quit; quit → exit 0, no writes.
- `--agent` / `--format json` / non-TTY: never prompt; `--walkthrough` + agent/non-TTY → exit 2.
- User edits record `"user_override": true` in pin `metadata` (no new `EvidenceKind`).
- Helm **does** pin `values*.yaml` images in this release (overrides older IaC “no values images” note).
- Shell installer: linux/darwin × amd64/arm64 only; Windows uses Release zip.
- Network tests require `PINNER_NETWORK=1`; unit tests use `PINNER_*_RESOLVE_MAP` seams.
- TDD: failing test → implement → pass → commit per task.
- Exit codes: `0` success/abort-clean, `1` drift/findings, `2` tool/config/resolve/invalid mode.
- Before any `git push`, run `scripts/ci-local`.

---

## File structure

```text
crates/
  pinner-ui/                 # compact walkthrough + pretty TTY summaries
  pinner-cargo/
  pinner-go/
  pinner-ruby/
  pinner-gitlab/
  pinner-azure/
  pinner-ecosystem/          # + Cargo, Go, Ruby, Gitlab, Azure kinds
  pinner-core/               # policy + apply_walkthrough_decisions + pin hook
  pinner-actions/            # images + reusable/composite uses
  pinner-mise/               # recursive discover
  pinner-helm/               # values*.yaml images + HTTP/OCI chart resolve
  pinner-terraform/          # registry HTTP resolve
  pinner/                    # CLI flags, register, mode select
scripts/install.sh
skills/pinner/SKILL.md
tests/fixtures/{cargo,go,ruby,gitlab,azure}-floating/
schemas/pinner.lock.schema.json
```

---

### Task 1: Ecosystem kinds, schema, and policy defaults

**Files:**
- Modify: `crates/pinner-ecosystem/src/lib.rs`
- Modify: `crates/pinner-ecosystem/tests/types_roundtrip.rs`
- Modify: `schemas/pinner.lock.schema.json`
- Modify: `crates/pinner-core/src/policy.rs`
- Modify: `crates/pinner-core/tests/policy_merge.rs`
- Modify: `crates/pinner/src/main.rs` (`parse_ecosystem` only)
- Modify: `crates/pinner-toolchain/src/detect.rs` (empty tool arms for new kinds)

**Interfaces:**
- Consumes: existing `EcosystemKind`, `Policy`
- Produces: `EcosystemKind::{Cargo, Go, Ruby, Gitlab, Azure}` → `"cargo"|"go"|"ruby"|"gitlab"|"azure"`; defaults enable Cargo/Go/Ruby; gitlab/azure opt-in via `pinner.toml`

- [ ] **Step 1: Write the failing policy test**

Add to `crates/pinner-core/tests/policy_merge.rs`:

```rust
#[test]
fn defaults_enable_languages_but_not_gitlab_or_azure() {
    let p = Policy::default_policy();
    assert!(p.is_enabled(EcosystemKind::Cargo));
    assert!(p.is_enabled(EcosystemKind::Go));
    assert!(p.is_enabled(EcosystemKind::Ruby));
    assert!(!p.is_enabled(EcosystemKind::Gitlab));
    assert!(!p.is_enabled(EcosystemKind::Azure));
}

#[test]
fn toml_can_enable_gitlab_and_azure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinner.toml");
    std::fs::write(
        &path,
        "[ecosystems]\ngitlab = true\nazure = true\n",
    )
    .unwrap();
    let p = Policy::load(Some(&path)).unwrap();
    assert!(p.is_enabled(EcosystemKind::Gitlab));
    assert!(p.is_enabled(EcosystemKind::Azure));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-core --test policy_merge defaults_enable_languages -- --nocapture`  
Expected: FAIL (unknown variant)

- [ ] **Step 3: Extend `EcosystemKind` and schema**

Add variants `Cargo`, `Go`, `Ruby`, `Gitlab`, `Azure` and `as_str()` arms. Extend schema enum. Add round-trip asserts in `types_roundtrip.rs`.

- [ ] **Step 4: Wire policy defaults and toml keys**

Append `Cargo`, `Go`, `Ruby` to `default_policy` enabled list. Add `cargo`/`go`/`ruby`/`gitlab`/`azure` fields on `EcosystemsSection` and `apply_ecosystem` calls. Extend `parse_ecosystem` and toolchain `required_tools` match arms (empty for now; soft `cargo`/`go`/`bundle` later if resolve needs them).

- [ ] **Step 5: Run tests and commit**

Run: `cargo test -p pinner-core --test policy_merge && cargo test -p pinner-ecosystem`  
Expected: PASS

```bash
git add crates/pinner-ecosystem crates/pinner-core schemas/pinner.lock.schema.json crates/pinner/src/main.rs crates/pinner-toolchain/src/detect.rs
git commit -m "$(cat <<'EOF'
feat: add cargo/go/ruby/gitlab/azure ecosystem kinds

EOF
)"
```

---

### Task 2: Scaffold new ecosystem crates and register

**Files:**
- Modify: `Cargo.toml` (workspace members + deps)
- Create: `crates/pinner-{cargo,go,ruby,gitlab,azure}/Cargo.toml` and `src/{lib,discover,extract,resolve,rewrite}.rs`
- Modify: `crates/pinner/Cargo.toml`, `crates/pinner/src/main.rs` (`register_ecosystems`)

**Interfaces:**
- Consumes: `Ecosystem` trait + kinds from Task 1
- Produces: `CargoEcosystem`, `GoEcosystem`, `RubyEcosystem`, `GitlabEcosystem`, `AzureEcosystem` registered in CLI; stubs return empty discover/extract until later tasks

- [ ] **Step 1: Create stub crates**

Each `lib.rs` mirrors `NodeEcosystem` structure. Stub modules:

```rust
// discover.rs
pub(crate) fn discover(_repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
    Ok(Vec::new())
}
```

Same empty pattern for extract/resolve/rewrite (`rewrite` → `Ok(None)`).

Workspace `Cargo.toml` members + `[workspace.dependencies]` path entries. Each crate depends on `pinner-ecosystem`, `thiserror` as needed; gitlab/azure also `serde_yaml`, `walkdir`; cargo also `toml`/`toml_edit`; go/ruby similar.

- [ ] **Step 2: Register in CLI**

In `register_ecosystems()` append `Arc::new(CargoEcosystem)`, etc. Depend on new crates from `crates/pinner/Cargo.toml`.

- [ ] **Step 3: Compile and commit**

Run: `cargo build -p pinner`  
Expected: PASS

```bash
git add Cargo.toml crates/pinner-cargo crates/pinner-go crates/pinner-ruby crates/pinner-gitlab crates/pinner-azure crates/pinner
git commit -m "$(cat <<'EOF'
feat: scaffold cargo/go/ruby/gitlab/azure ecosystem crates

EOF
)"
```

---

### Task 3: Cargo ecosystem (pin `Cargo.toml`)

**Files:**
- Create: `tests/fixtures/cargo-floating/Cargo.toml`, `Cargo.lock` (with exact versions for evidence)
- Modify: `crates/pinner-cargo/src/{discover,extract,resolve,rewrite}.rs`
- Create: `crates/pinner-cargo/tests/cargo_pin.rs`

**Interfaces:**
- Consumes: `EcosystemCtx`, workspace member discovery
- Produces: floating `*` / non-exact / (`^`/`~` when `pin_exact_ranges`) → exact semver rewrite; evidence from `Cargo.lock` package entries or `PINNER_CARGO_RESOLVE_MAP`

- [ ] **Step 1: Fixture + failing extract test**

Fixture `Cargo.toml`:

```toml
[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
tokio = { version = "^1", features = ["rt"] }
```

Test:

```rust
#[test]
fn extracts_floating_cargo_deps() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cargo-floating");
    let eco = CargoEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert!(!manifests.is_empty());
    let ctx = EcosystemCtx {
        repo: &repo,
        lock_pins: &[],
        offline: true,
        pin_exact_ranges: true,
    };
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert!(findings.iter().any(|f| f.name == "serde" && f.is_floating));
    assert!(findings.iter().any(|f| f.name == "tokio" && f.is_floating));
}
```

- [ ] **Step 2: Run test — expect FAIL**

Run: `cargo test -p pinner-cargo --test cargo_pin extracts_floating -- --nocapture`

- [ ] **Step 3: Implement discover/extract**

Discover: walk for `Cargo.toml`, skip `target/`. Parse with `toml::Value`. Extract deps from `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, and target-specific tables. Treat bare major (`"1"`), `*`, `latest`, and (if `pin_exact_ranges`) `^`/`~`/`>=` as floating. Skip path/git deps.

- [ ] **Step 4: Implement resolve/rewrite + tests**

Resolve: lock pin → parse `Cargo.lock` `[[package]]` name/version → `PINNER_CARGO_RESOLVE_MAP` (`name=requested:pinned` lines) → offline error. Rewrite with `toml_edit` setting version strings / table `version` fields to exact pinned.

Add resolve+rewrite test using fixture lock or env map; assert second pin is idempotent.

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-cargo tests/fixtures/cargo-floating
git commit -m "$(cat <<'EOF'
feat: pin floating Cargo.toml dependencies

EOF
)"
```

---

### Task 4: Go ecosystem (`go.mod`)

**Files:**
- Create: `tests/fixtures/go-floating/{go.mod,go.sum}`
- Modify: `crates/pinner-go/src/*`
- Create: `crates/pinner-go/tests/go_pin.rs`

**Interfaces:**
- Produces: floating `require` directives (pseudo `latest`, non-exact) → exact `vX.Y.Z` / module versions; evidence `go.sum` / `PINNER_GO_RESOLVE_MAP`

- [ ] **Step 1: Failing extract test + fixture**

`go.mod`:

```
module example.com/demo

go 1.22

require (
        golang.org/x/sync v0.0.0-20181221193216-37e7f081c4d4
        github.com/stretchr/testify v1.9.0
)
```

Mark a synthetic floating case by using a comment-free require with version `latest` in fixture (or a replace-free unbound form the extractor treats as floating). Prefer a clear floating token:

```
require github.com/example/lib latest
```

- [ ] **Step 2: Implement discover/extract/resolve/rewrite**

Discover `go.mod` (and modules listed in `go.work` if present). Extract `require` lines; floating if version is `latest` or empty. Resolve via `go.sum` first-line version for module path, else env map. Rewrite replaces the version token on the matching require line (line-oriented structured replace; keep go.mod formatting stable).

- [ ] **Step 3: Tests pass + commit**

Run: `cargo test -p pinner-go`  
Expected: PASS

```bash
git add crates/pinner-go tests/fixtures/go-floating
git commit -m "$(cat <<'EOF'
feat: pin floating go.mod module versions

EOF
)"
```

---

### Task 5: Ruby ecosystem (`Gemfile`)

**Files:**
- Create: `tests/fixtures/ruby-floating/{Gemfile,Gemfile.lock}`
- Modify: `crates/pinner-ruby/src/*`
- Create: `crates/pinner-ruby/tests/ruby_pin.rs`

**Interfaces:**
- Produces: `gem "x"` without version or with floating constraint → `gem "x", "X.Y.Z"`; evidence from `Gemfile.lock` SPECS

- [ ] **Step 1: Failing extract test**

Gemfile:

```ruby
source "https://rubygems.org"
gem "rake"
gem "rspec", ">= 3.0"
```

With `pin_exact_ranges: true`, both floating.

- [ ] **Step 2: Implement discover/extract/resolve/rewrite**

Discover `Gemfile`. Extract `gem` calls (simple parser for `gem "name"` / `gem 'name', 'constraint'`). Resolve from `Gemfile.lock` specs section or `PINNER_RUBY_RESOLVE_MAP`. Rewrite inserts/replaces version argument with exact string.

- [ ] **Step 3: Tests + commit**

```bash
git add crates/pinner-ruby tests/fixtures/ruby-floating
git commit -m "$(cat <<'EOF'
feat: pin floating Gemfile dependencies

EOF
)"
```

---

### Task 6: GitLab CI ecosystem (opt-in)

**Files:**
- Create: `tests/fixtures/gitlab-floating/.gitlab-ci.yml`
- Modify: `crates/pinner-gitlab/src/*`
- Create: `crates/pinner-gitlab/tests/gitlab_pin.rs`
- Reuse: `pinner-iac-common` image digest helpers

**Interfaces:**
- Produces: `image:` without `@sha256:` → digest pin; floating remote `include:` project/ref → pinned ref when resolvable via map/git

- [ ] **Step 1: Failing extract test**

```yaml
image: node:latest
include:
  - project: 'group/ci-templates'
    file: '/template.yml'
    ref: main
```

- [ ] **Step 2: Implement**

Discover `.gitlab-ci.yml` and nested includes on disk (`local:` only for discover walk; remote includes are findings). Extract images via YAML walk; extract remote includes with non-SHA `ref`. Resolve images via existing digest helpers + `PINNER_DOCKER_RESOLVE_MAP`/`PINNER_GITLAB_RESOLVE_MAP`. Rewrite YAML preserving structure with `serde_yaml` round-trip only on touched keys when possible; prefer line-aware image replace matching docker crate style if safer.

- [ ] **Step 3: Tests + commit**

```bash
git add crates/pinner-gitlab tests/fixtures/gitlab-floating
git commit -m "$(cat <<'EOF'
feat: opt-in GitLab CI image and include pinning

EOF
)"
```

---

### Task 7: Azure DevOps ecosystem (opt-in)

**Files:**
- Create: `tests/fixtures/azure-floating/azure-pipelines.yml`
- Modify: `crates/pinner-azure/src/*`
- Create: `crates/pinner-azure/tests/azure_pin.rs`

**Interfaces:**
- Produces: floating `container: image:` and `task: Name@N` major-only floats → digest / exact task version via map

- [ ] **Step 1: Failing extract test**

```yaml
pool:
  vmImage: ubuntu-latest
resources:
  containers:
    - container: build
      image: node:latest
steps:
  - task: UseNode@1
```

Treat `image: node:latest` as floating. Task `@1` is floating when policy wants exact (always for azure tasks in v1).

- [ ] **Step 2: Implement discover/extract/resolve/rewrite + tests + commit**

```bash
git add crates/pinner-azure tests/fixtures/azure-floating
git commit -m "$(cat <<'EOF'
feat: opt-in Azure Pipelines container and task pinning

EOF
)"
```

---

### Task 8: Deepen GitHub Actions (images + reusable workflows)

**Files:**
- Modify: `crates/pinner-actions/src/{extract,resolve,rewrite}.rs`
- Modify: `crates/pinner-actions/tests/actions_pin.rs`
- Create/extend: `tests/fixtures/actions-floating/.github/workflows/ci.yml`

**Interfaces:**
- Produces: floating `container:` / `services.*.image` → digest pins; reusable `owner/repo/.github/workflows/x.yml@ref` already partially covered by nested path `uses:` — ensure extract/resolve/rewrite treat them like actions; add fixture proving it

- [ ] **Step 1: Failing tests**

Extend fixture:

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    container: node:20
    services:
      redis:
        image: redis:latest
    steps:
      - uses: org/repo/.github/workflows/reuse.yml@v1
```

Assert extract finds three floating findings (container, service image, reusable uses).

- [ ] **Step 2: Implement extract for images**

Parse YAML documents for `container` (string or `image:` map) and `services.*.image`. Name findings `container:<job>` / `service:<job>/<svc>`. Reuse `pinner_iac_common` / docker digest resolve. Rewrite updates those YAML scalars to `name@sha256:…`.

Confirm `split_owner_action_ref` already accepts nested workflow paths; add resolve/rewrite coverage if missing.

- [ ] **Step 3: Tests + commit**

```bash
git add crates/pinner-actions tests/fixtures/actions-floating
git commit -m "$(cat <<'EOF'
feat: pin Actions container images and reusable workflow refs

EOF
)"
```

---

### Task 9: Recursive mise discovery

**Files:**
- Modify: `crates/pinner-mise/src/discover.rs`
- Modify: `crates/pinner-mise/tests/*` or add fixture `tests/fixtures/mise-nested/`
- Modify: existing e2e if needed

**Interfaces:**
- Produces: all `.mise.toml` / `.tool-versions` under repo (skip `.git`/`node_modules`/`target`)

- [ ] **Step 1: Failing test**

Nested fixture `apps/web/.mise.toml` with `node = "latest"`. Assert discover returns ≥2 manifests including nested path.

- [ ] **Step 2: Implement WalkDir discover (skip common dirs)**

```rust
for entry in WalkDir::new(repo).into_iter().filter_entry(|e| !should_skip(e.path())) {
    // if file name in MANIFEST_NAMES → push Manifest
}
```

- [ ] **Step 3: Tests + commit**

```bash
git add crates/pinner-mise tests/fixtures/mise-nested
git commit -m "$(cat <<'EOF'
feat: discover nested mise and tool-versions manifests

EOF
)"
```

---

### Task 10: Helm values images + Terraform/Helm HTTP resolve

**Files:**
- Modify: `crates/pinner-helm/src/{discover,extract,resolve,rewrite}.rs`
- Modify: `crates/pinner-terraform/src/resolve.rs`
- Modify/create tests + fixtures under `tests/fixtures/helm-floating/`, terraform fixtures
- Possibly extend `pinner-iac-common` with HTTP GET helper using `std::process` (`curl`) or small `ureq` dependency if added to workspace

**Interfaces:**
- Helm: discover `values.yaml` / `values*.yaml`; extract image-like string values without digest; pin digests
- Terraform: after map/native lock miss, online GET `https://registry.terraform.io/v1/modules/.../versions` (and providers API) to pick latest matching version; still fail offline without evidence
- Helm charts: online repo index / OCI tag resolve when map missing (document env `PINNER_HELM_RESOLVE_MAP` fallback)

- [ ] **Step 1: Failing helm values extract test**

`values.yaml`:

```yaml
image:
  repository: ghcr.io/example/app
  tag: latest
```

Or single string `image: ghcr.io/example/app:latest` — support both common shapes used in fixture.

- [ ] **Step 2: Implement values discover/extract/rewrite**

Include values files in discover (remove the “skip values.yaml” discover exclusion). Extract floating tags; resolve digests; rewrite tag or image string.

- [ ] **Step 3: Terraform registry HTTP unit test with map seam + optional NETWORK test**

Implement `resolve_terraform_registry_module(name, requested) -> version` behind a function injectable/mocked in unit tests; real HTTP only under `PINNER_NETWORK=1`.

- [ ] **Step 4: Helm chart HTTP/OCI resolve similarly**

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-helm crates/pinner-terraform crates/pinner-iac-common tests/fixtures
git commit -m "$(cat <<'EOF'
feat: helm values image pins and registry HTTP resolve

EOF
)"
```

---

### Task 11: Walkthrough decision filter in core

**Files:**
- Modify: `crates/pinner-core/src/lib.rs` (exports)
- Create: `crates/pinner-core/src/walkthrough.rs`
- Modify: `crates/pinner-core/src/orchestrate.rs` (`pin` accepts optional filter)
- Create: `crates/pinner-core/tests/walkthrough_filter.rs`

**Interfaces:**
- Consumes: resolved `Vec<Pin>`
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinDecision {
    Accept,
    Skip,
    Edit { pinned: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkthroughOutcome {
    Continue { pins: Vec<Pin> },
    Aborted,
}

/// Apply per-pin decisions in order. `decisions.len()` must equal `pins.len()`.
pub fn apply_walkthrough_decisions(
    pins: &[Pin],
    decisions: &[PinDecision],
) -> Result<WalkthroughOutcome, CoreError> {
    // Skip → omit; Edit → clone pin with new pinned + metadata user_override=true; Accept → keep
}
```

Extend `RunOptions` or add `pin_with_walkthrough(..., decisions)` / callback:

```rust
pub fn pin(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    walkthrough: Option<&mut dyn FnMut(&[Pin]) -> Result<WalkthroughOutcome, CoreError>>,
) -> Result<RunReport, CoreError>
```

After resolve collects `graph_pins` / before rewrite apply: if `Some(cb)`, call cb; on `Aborted` return empty report / dedicated flag without writing; on `Continue` use filtered pins for rewrite+lock.

Alternatively keep `pin` signature and add `pub fn pin_with_filter(...)` to avoid breaking all call sites — update all call sites in the same task.

- [ ] **Step 1: Unit tests for `apply_walkthrough_decisions`**

```rust
#[test]
fn skip_removes_pin_edit_sets_metadata() { /* ... */ }

#[test]
fn abort_outcome_when_signaled_by_caller() {
    // caller returns Aborted → orchestrate writes nothing (test with fake ecosystem)
}
```

- [ ] **Step 2: Implement filter + wire orchestrate**

- [ ] **Step 3: Tests + commit**

```bash
git add crates/pinner-core
git commit -m "$(cat <<'EOF'
feat: core walkthrough accept/skip/edit filter before rewrite

EOF
)"
```

---

### Task 12: `pinner-ui` compact walkthrough + CLI modes

**Files:**
- Create: `crates/pinner-ui/` (ratatui + crossterm)
- Modify: `crates/pinner/src/cli.rs` (`--walkthrough`, `--agent`)
- Modify: `crates/pinner/src/main.rs` (mode select, emit pretty vs json)
- Modify: `Cargo.toml` workspace
- Create: `crates/pinner/tests/walkthrough_mode_cli.rs` (assert `--walkthrough --agent` fails)

**Interfaces:**
- Produces:

```rust
// pinner-ui
pub fn run_compact_walkthrough(pins: &[Pin]) -> std::io::Result<WalkthroughOutcome>;
pub fn emit_pretty_report(report: &RunReport, writer: &mut impl Write) -> std::io::Result<()>;
```

Compact list UI: table rows; highlight selection; keys Enter/a accept, s skip, e edit (input popup/line), q → `Aborted`. Store decisions then `apply_walkthrough_decisions` inside UI or return decisions to main.

CLI:

```rust
#[arg(long, global = true)]
pub walkthrough: bool,
#[arg(long, global = true)]
pub agent: bool,
```

Mode rules in `run`:

```rust
if cli.walkthrough && (cli.agent || cli.format == Format::Json || !stdout_is_tty()) {
    return Err("walkthrough requires an interactive TTY (not --agent/--format json)".into());
}
if cli.agent {
    // force JSON emit path
}
```

- [ ] **Step 1: CLI failure test**

```rust
#[test]
fn walkthrough_with_agent_exits_2() {
    Command::cargo_bin("pinner")
        .unwrap()
        .args(["--walkthrough", "--agent", "pin"])
        .assert()
        .failure()
        .code(2);
}
```

- [ ] **Step 2: Implement `pinner-ui` + wire `pin --walkthrough`**

- [ ] **Step 3: Pretty TTY text summaries when format=text and TTY and not walkthrough**

Keep JSON path byte-stable for agents.

- [ ] **Step 4: Commit**

```bash
git add crates/pinner-ui crates/pinner Cargo.toml
git commit -m "$(cat <<'EOF'
feat: compact walkthrough TUI and --agent mode

EOF
)"
```

---

### Task 13: `scripts/install.sh` + docs

**Files:**
- Create: `scripts/install.sh` (zsh-compatible `#!/usr/bin/env bash` with `set -euo pipefail` is fine if project scripts are bash; user rule prefers zsh for scripting — use `#!/usr/bin/env zsh` with equivalent options)
- Modify: `README.md`, `docs/guide/quick-start.md`, `docs/guide/releasing.md`
- Modify: `.github/workflows/ci.yml` (optional dry-run job step)

**Interfaces:**
- Env: `PINNER_VERSION`, `PINNER_INSTALL_DIR` (default `$HOME/.local/bin`), `PINNER_INSTALL_DRY_RUN=1`, `PINNER_REPO` default `zloeber/Pinner`
- Asset: `pinner-${version}-${target}.tar.gz` matching release workflow (`x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`)

- [ ] **Step 1: Write installer**

```zsh
#!/usr/bin/env zsh
set -euo pipefail
# detect OS/arch → rustc target triple
# resolve version via GitHub API releases/latest or PINNER_VERSION
# URL=https://github.com/${REPO}/releases/download/v${VERSION}/pinner-${VERSION}-${TARGET}.tar.gz
# if DRY_RUN: print URL and INSTALL_DIR and exit 0
# download to temp, extract pinner binary, install to INSTALL_DIR, chmod +x
# PATH hint if needed
```

- [ ] **Step 2: Dry-run smoke in CI or local**

Run: `PINNER_INSTALL_DRY_RUN=1 PINNER_VERSION=0.1.0 zsh scripts/install.sh`  
Expected: prints URL containing `pinner-0.1.0-` and install path; exit 0

- [ ] **Step 3: Document curl install in README/quick-start**

```bash
curl -fsSL https://raw.githubusercontent.com/zloeber/Pinner/main/scripts/install.sh | zsh
```

- [ ] **Step 4: Commit**

```bash
git add scripts/install.sh README.md docs/guide
git commit -m "$(cat <<'EOF'
feat: add curl/zsh installer for user-local pinner binary

EOF
)"
```

---

### Task 14: Agent skill + guide docs + e2e sweep

**Files:**
- Create: `skills/pinner/SKILL.md`
- Modify: `docs/guide/configuration.md` (new ecosystems + walkthrough/agent flags)
- Modify: `docs/SUMMARY.md` if new pages added
- Modify: `crates/pinner/tests/idempotency_all_fixtures.rs` to include new fixtures
- Run: `scripts/ci-local`

**Interfaces:**
- Skill frontmatter + body covering audit JSON → pin → check; never walkthrough; enable gitlab/azure when present

- [ ] **Step 1: Write `skills/pinner/SKILL.md`**

```markdown
---
name: pinner
description: Find and pin floating dependency versions across a repository with the pinner CLI (audit/pin/check). Use when versions use latest, ranges, unpinned images, or floating CI refs.
---

# Pinner

## When to use
- Repo has floating versions (`latest`, `*`, `^`, unpinned images, `@v4` actions, etc.)
- Need reproducible pins + `pinner.lock.json`

## Agent workflow
1. `pinner audit --format json` (or `--agent`)
2. Enable opt-in ecosystems in `pinner.toml` if GitLab/Azure files exist:
   ```toml
   [ecosystems]
   gitlab = true
   azure = true
   ```
3. `pinner pin --agent` (or `--format json`)
4. `pinner check --agent` — expect exit 0
5. Never use `--walkthrough` in automation

## Exit codes
- 0 ok
- 1 drift / audit findings
- 2 config/resolve/toolchain/invalid mode
```

- [ ] **Step 2: Update configuration + quick-start docs for new flags/ecosystems**

- [ ] **Step 3: Extend idempotency fixture sweep; run full CI local**

Run: `scripts/ci-local`  
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add skills/pinner docs/guide docs/SUMMARY.md crates/pinner/tests
git commit -m "$(cat <<'EOF'
docs: add pinner agent skill and expansion guide updates

EOF
)"
```

---

## Plan self-review

1. **Spec coverage:** New ecosystems (Tasks 3–7), Actions deepen (8), mise/helm/tf gaps (9–10), walkthrough+agent UX (11–12), installer (13), skill (14), kinds/policy/scaffold (1–2). Quit exit 0 and compact list covered in 11–12. `PINNER_INSTALL_DIR` in 13.
2. **Placeholders:** None intentional; HTTP resolve details lean on existing map seams + NETWORK gate as in prior IaC plan.
3. **Type consistency:** `PinDecision` / `WalkthroughOutcome` / `apply_walkthrough_decisions` named consistently across Tasks 11–12; ecosystem kind strings match schema.

---
