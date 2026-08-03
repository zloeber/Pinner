# Pinner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Rust CLI (`pinner`) that rewrites floating versions to exact pins across mise/Node/Python/Docker/Actions, emits `pinner.lock.json`, and fails CI on drift—with optional toolchain bootstrap for resolver binaries.

**Architecture:** Cargo workspace with a shared `Ecosystem` trait (`pinner-ecosystem`), orchestration/policy/lock I/O in `pinner-core`, optional binary install/detect in `pinner-toolchain`, one crate per ecosystem, and a thin `pinner` CLI. Resolution order is always: existing `pinner.lock.json` → native lock evidence → policy + tool/registry.

**Tech Stack:** Rust 2024 edition (stable 1.96+), `clap` (CLI), `serde`/`serde_json`/`toml`/`toml_edit`, `serde_yaml`, `thiserror`/`anyhow`, `walkdir`/`globset`, `tempfile`/`assert_cmd`/`predicates` for tests, JSON Schema at `schemas/pinner.lock.schema.json`.

**Spec:** [`docs/superpowers/specs/2026-08-03-pinner-design.md`](../specs/2026-08-03-pinner-design.md)

## Global Constraints

- Binary crate name is `pinner`; workspace members match the design diagram exactly.
- Unified lock path is always repo-root `pinner.lock.json` (schema version `1`).
- Optional config is repo-root `pinner.toml` (or `--config`); defaults live in the binary.
- Exit codes: `0` success, `1` drift/findings, `2` tool/config/resolution error.
- Never invent a pin without lock evidence or a successful resolve; `--offline` fails closed.
- Structured parsers/patches only (no blind regex rewrites of whole files).
- Network-backed integration tests require `PINNER_NETWORK=1`; otherwise skip with a clear message.
- TDD: failing test → implement → pass → commit for every task.
- After Task 10 (mise E2E), Tasks 11–14 are independent and may run in parallel.

---

## File structure

```text
Cargo.toml                          # workspace
crates/
  pinner/                           # binary CLI
    src/main.rs
    src/cli.rs
    tests/cli_pin_check.rs
  pinner-core/
    src/lib.rs
    src/error.rs
    src/lock.rs
    src/policy.rs
    src/orchestrate.rs
    src/report.rs
  pinner-ecosystem/
    src/lib.rs                      # Ecosystem trait + shared types
  pinner-toolchain/
    src/lib.rs
    src/detect.rs
    src/ensure.rs
  pinner-mise/
    src/lib.rs
    src/discover.rs
    src/extract.rs
    src/resolve.rs
    src/rewrite.rs
  pinner-node/   src/...            # same internal layout
  pinner-python/ src/...
  pinner-docker/ src/...
  pinner-actions/ src/...
schemas/
  pinner.lock.schema.json
tests/fixtures/
  mise-floating/
  node-floating/
  python-floating/
  docker-floating/
  actions-floating/
.github/workflows/
  ci.yml
  consumer-example.yml              # documented consumer pattern
README.md
```

---

### Task 1: Cargo workspace scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `crates/pinner/Cargo.toml`
- Create: `crates/pinner/src/main.rs`
- Create: `crates/pinner-core/Cargo.toml`
- Create: `crates/pinner-core/src/lib.rs`
- Create: `crates/pinner-ecosystem/Cargo.toml`
- Create: `crates/pinner-ecosystem/src/lib.rs`
- Create: `crates/pinner-toolchain/Cargo.toml`
- Create: `crates/pinner-toolchain/src/lib.rs`
- Create: `crates/pinner-mise/Cargo.toml`
- Create: `crates/pinner-mise/src/lib.rs`
- Create: `crates/pinner-node/Cargo.toml`
- Create: `crates/pinner-node/src/lib.rs`
- Create: `crates/pinner-python/Cargo.toml`
- Create: `crates/pinner-python/src/lib.rs`
- Create: `crates/pinner-docker/Cargo.toml`
- Create: `crates/pinner-docker/src/lib.rs`
- Create: `crates/pinner-actions/Cargo.toml`
- Create: `crates/pinner-actions/src/lib.rs`
- Create: `.gitignore`
- Test: `cargo test --workspace`

**Interfaces:**
- Consumes: none
- Produces: compilable workspace; binary `pinner` prints `pinner 0.1.0` and exits 0

- [ ] **Step 1: Write root workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
  "crates/pinner",
  "crates/pinner-core",
  "crates/pinner-ecosystem",
  "crates/pinner-toolchain",
  "crates/pinner-mise",
  "crates/pinner-node",
  "crates/pinner-python",
  "crates/pinner-docker",
  "crates/pinner-actions",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/zloeber/Pinner"

[workspace.dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
toml = "0.8"
toml_edit = "0.22"
serde_yaml = "0.9"
walkdir = "2"
globset = "0.4"
tempfile = "3"
assert_cmd = "2"
predicates = "3"
pinner-core = { path = "crates/pinner-core" }
pinner-ecosystem = { path = "crates/pinner-ecosystem" }
pinner-toolchain = { path = "crates/pinner-toolchain" }
pinner-mise = { path = "crates/pinner-mise" }
pinner-node = { path = "crates/pinner-node" }
pinner-python = { path = "crates/pinner-python" }
pinner-docker = { path = "crates/pinner-docker" }
pinner-actions = { path = "crates/pinner-actions" }
```

- [ ] **Step 2: Create each crate with minimal `lib.rs` / `main.rs`**

`crates/pinner/src/main.rs`:

```rust
fn main() {
    println!("pinner {}", env!("CARGO_PKG_VERSION"));
}
```

Each library crate:

```rust
//! Placeholder until Task N fills this crate in.
```

Wire `crates/pinner/Cargo.toml` with `[[bin]] name = "pinner"` and path `src/main.rs`. Library crates depend on nothing yet except `pinner` depending on the libs as they land (for Task 1, binary may stand alone).

- [ ] **Step 3: Add `.gitignore`**

```gitignore
/target
**/*.rs.bk
.DS_Store
```

- [ ] **Step 4: Verify build**

Run: `cargo test --workspace`
Expected: PASS (0 tests or empty lib tests), `cargo run -p pinner` prints `pinner 0.1.0`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates .gitignore Cargo.lock
git commit -m "chore: scaffold Cargo workspace for pinner"
```

---

### Task 2: Shared ecosystem types and trait

**Files:**
- Create: `crates/pinner-ecosystem/src/lib.rs` (replace placeholder)
- Test: `crates/pinner-ecosystem/tests/types_roundtrip.rs`

**Interfaces:**
- Consumes: workspace deps `serde`, `serde_json`, `thiserror`
- Produces:
  - `EcosystemKind` enum (`Mise`, `Node`, `Python`, `Docker`, `Actions`) with `as_str()` → `"mise"` etc.
  - `Manifest { ecosystem, path: PathBuf }`
  - `Finding { ecosystem, name, requested, path, is_floating }`
  - `EvidenceKind` (`Lock`, `NativeLock`, `Registry`, `Tool`) serde rename `lock` / `native_lock` / `registry` / `tool`
  - `Pin { ecosystem, name, requested, pinned, path, evidence, metadata: Map<String, Value> }`
  - `Rewrite { path, new_contents }`
  - Shared run context (defined here so ecosystems do not depend on `pinner-core`):

```rust
#[derive(Debug, Clone)]
pub struct EcosystemCtx<'a> {
    pub lock_pins: &'a [Pin],
    pub offline: bool,
    pub pin_exact_ranges: bool,
}
```

  - Trait:

```rust
pub trait Ecosystem: Send + Sync {
    fn kind(&self) -> EcosystemKind;
    fn discover(&self, repo: &Path) -> Result<Vec<Manifest>, EcosystemError>;
    fn extract(
        &self,
        manifest: &Manifest,
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, EcosystemError>;
    fn resolve(
        &self,
        findings: &[Finding],
        ctx: &EcosystemCtx<'_>,
    ) -> Result<Vec<Pin>, EcosystemError>;
    fn rewrite(
        &self,
        manifest: &Manifest,
        pins: &[Pin],
    ) -> Result<Option<Rewrite>, EcosystemError>;
}
```

  - `EcosystemError` via `thiserror` with variants `Io`, `Parse { path, message }`, `Resolve { name, requested, hint }`, `Offline { name, requested }`

- [ ] **Step 1: Write failing round-trip test**

```rust
// crates/pinner-ecosystem/tests/types_roundtrip.rs
use pinner_ecosystem::{EvidenceKind, EcosystemKind, Pin};
use serde_json::json;
use std::path::PathBuf;

#[test]
fn pin_serializes_stable_field_names() {
    let pin = Pin {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "lts".into(),
        pinned: "22.11.0".into(),
        path: PathBuf::from(".mise.toml"),
        evidence: EvidenceKind::Lock,
        metadata: Default::default(),
    };
    let v = serde_json::to_value(&pin).unwrap();
    assert_eq!(v["ecosystem"], "mise");
    assert_eq!(v["evidence"], "lock");
    assert_eq!(v["pinned"], "22.11.0");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-ecosystem --test types_roundtrip`
Expected: FAIL (types missing)

- [ ] **Step 3: Implement types + trait in `lib.rs`**

Implement enums/structs with `Serialize`/`Deserialize`, `as_str` on `EcosystemKind`, and the `Ecosystem` trait + `EcosystemError` as specified in Interfaces.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pinner-ecosystem`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-ecosystem
git commit -m "feat: add shared ecosystem trait and pin types"
```

---

### Task 3: Lockfile read/write + JSON Schema

**Files:**
- Create: `crates/pinner-core/src/lock.rs`
- Create: `crates/pinner-core/src/error.rs`
- Modify: `crates/pinner-core/src/lib.rs`
- Modify: `crates/pinner-core/Cargo.toml` (deps: `serde`, `serde_json`, `thiserror`, `pinner-ecosystem`)
- Create: `schemas/pinner.lock.schema.json`
- Test: `crates/pinner-core/tests/lock_roundtrip.rs`

**Interfaces:**
- Consumes: `pinner_ecosystem::{Pin, EvidenceKind, EcosystemKind}`
- Produces:
  - `LockFile { version: u32, generated_at: String, pinner_version: String, entries: Vec<LockEntry> }`
  - `LockEntry` fields matching design: `ecosystem`, `name`, `requested`, `pinned`, `source` (string `"manifest"`), `path`, `evidence`, `metadata`
  - `LockFile::read(path) -> Result<LockFile, CoreError>`
  - `LockFile::write(path) -> Result<(), CoreError>` — stable key order via `serde_json::to_vec_pretty` after sorting `entries` by `(ecosystem, path, name)`
  - `LockFile::from_pins(pins: &[Pin], pinner_version: &str, generated_at: &str) -> LockFile`
  - Reject `version != 1` on read

- [ ] **Step 1: Write failing tests**

```rust
use pinner_core::lock::LockFile;
use pinner_ecosystem::{EvidenceKind, EcosystemKind, Pin};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn lock_roundtrip_preserves_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.lock.json");
    let pins = vec![Pin {
        ecosystem: EcosystemKind::Mise,
        name: "node".into(),
        requested: "lts".into(),
        pinned: "22.11.0".into(),
        path: PathBuf::from(".mise.toml"),
        evidence: EvidenceKind::Tool,
        metadata: Default::default(),
    }];
    let lock = LockFile::from_pins(&pins, "0.1.0", "2026-08-03T15:00:00Z");
    lock.write(&path).unwrap();
    let loaded = LockFile::read(&path).unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries[0].pinned, "22.11.0");
}

#[test]
fn lock_rejects_unknown_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.lock.json");
    std::fs::write(&path, r#"{"version":99,"generated_at":"","pinner_version":"","entries":[]}"#).unwrap();
    let err = LockFile::read(&path).unwrap_err();
    assert!(err.to_string().contains("version"));
}
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test -p pinner-core --test lock_roundtrip`
Expected: FAIL

- [ ] **Step 3: Implement `error.rs`, `lock.rs`, schema**

Schema must require `version`, `generated_at`, `pinner_version`, `entries` and entry properties listed in the design. Export `pub mod lock;` from `lib.rs`.

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test -p pinner-core --test lock_roundtrip`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core schemas/pinner.lock.schema.json
git commit -m "feat: add pinner.lock.json read/write and schema"
```

---

### Task 4: Policy defaults + optional `pinner.toml`

**Files:**
- Create: `crates/pinner-core/src/policy.rs`
- Modify: `crates/pinner-core/src/lib.rs`
- Test: `crates/pinner-core/tests/policy_merge.rs`

**Interfaces:**
- Consumes: `toml`, `EcosystemKind`
- Produces:

```rust
#[derive(Debug, Clone)]
pub struct Policy {
    pub enabled: Vec<EcosystemKind>, // default: all five
    pub ignore_globs: Vec<String>,   // default: ["**/node_modules/**", "**/.git/**", "**/vendor/**"]
    pub offline_default: bool,       // false
    pub toolchain_install: bool,     // true — allow `ensure` via mise
    pub pin_exact_ranges: bool,      // true — pin ^/~ in node when extracting as floating
    pub allow_floating: Vec<AllowFloating>, // empty
}

#[derive(Debug, Clone)]
pub struct AllowFloating {
    pub ecosystem: EcosystemKind,
    pub name: String,
    pub path_glob: Option<String>,
}

impl Policy {
    pub fn default_policy() -> Self { /* ... */ }
    pub fn load(path: Option<&Path>) -> Result<Self, CoreError>; // merge file over defaults
    pub fn is_enabled(&self, kind: EcosystemKind) -> bool;
    pub fn is_ignored(&self, path: &Path) -> bool; // globset match
}
```

Default pin styles are documented in comments and enforced by ecosystem crates (mise exact, node/python exact, docker digest, actions commit SHA)—policy exposes `pin_exact_ranges` and enable flags only in v1.

- [ ] **Step 1: Write failing tests**

```rust
use pinner_core::policy::Policy;
use pinner_ecosystem::EcosystemKind;
use std::fs;
use tempfile::tempdir;

#[test]
fn defaults_enable_all_ecosystems() {
    let p = Policy::default_policy();
    assert!(p.is_enabled(EcosystemKind::Mise));
    assert!(p.is_enabled(EcosystemKind::Actions));
}

#[test]
fn toml_can_disable_node() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pinner.toml");
    fs::write(&path, "[ecosystems]\nnode = false\n").unwrap();
    let p = Policy::load(Some(&path)).unwrap();
    assert!(!p.is_enabled(EcosystemKind::Node));
    assert!(p.is_enabled(EcosystemKind::Mise));
}

#[test]
fn ignore_globs_skip_node_modules() {
    let p = Policy::default_policy();
    assert!(p.is_ignored(std::path::Path::new("app/node_modules/pkg/package.json")));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-core --test policy_merge`
Expected: FAIL

- [ ] **Step 3: Implement `policy.rs`**

TOML shape:

```toml
[ecosystems]
mise = true
node = true
python = true
docker = true
actions = true

ignore = ["**/node_modules/**", "**/.git/**", "**/vendor/**"]

[toolchain]
install = true

[pinning]
exact_ranges = true
```

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-core --test policy_merge`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core
git commit -m "feat: add policy defaults and pinner.toml loading"
```

---

### Task 5: Toolchain detect + ensure

**Files:**
- Create: `crates/pinner-toolchain/src/detect.rs`
- Create: `crates/pinner-toolchain/src/ensure.rs`
- Modify: `crates/pinner-toolchain/src/lib.rs`
- Modify: `crates/pinner-toolchain/Cargo.toml` (`which`-style: use `std::process::Command` + `PATH` lookup; dep on `pinner-ecosystem`)
- Test: `crates/pinner-toolchain/tests/status_unit.rs`

**Interfaces:**
- Consumes: `EcosystemKind`, policy `toolchain_install`
- Produces:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: String,          // "mise", "node", "npm", "uv", "docker", "gh"
    pub required_by: Vec<EcosystemKind>,
    pub present: bool,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
}

pub fn required_tools(enabled: &[EcosystemKind]) -> Vec<&'static str>;
pub fn status(enabled: &[EcosystemKind]) -> Vec<ToolStatus>;
pub fn ensure(enabled: &[EcosystemKind], allow_install: bool) -> Result<Vec<ToolStatus>, ToolchainError>;
```

`ensure` behavior:
1. Compute missing tools from `status`.
2. If none missing, return status.
3. If `!allow_install`, return `ToolchainError::Missing { tools }`.
4. If `mise` present: run `mise install` for missing tools that mise can provide (`node`, `uv`, `gh` as applicable). If `mise` missing, install mise via the official curl script only when `allow_install` and not offline—**in unit tests, mock by injecting a `CommandRunner` trait**.

```rust
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError>;
}
pub struct RealCommandRunner;
```

Production `ensure` uses `RealCommandRunner`. Tests use a fake that records calls.

Mapping:
- mise ecosystem → `mise`
- node → `node`, `npm`
- python → `uv`
- docker → `docker`
- actions → `gh`

- [ ] **Step 1: Write failing tests with fake runner**

```rust
use pinner_ecosystem::EcosystemKind;
use pinner_toolchain::{
    ensure_with_runner, status, CommandOutput, CommandRunner, ToolchainError,
};
use std::collections::HashSet;
use std::sync::Mutex;

struct FakeRunner {
    present: Mutex<HashSet<String>>,
    installs: Mutex<Vec<String>>,
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError> {
        if program == "mise" && args.first() == Some(&"install") {
            self.installs.lock().unwrap().push(args.join(" "));
            for tool in args.iter().skip(1) {
                let name = tool.split('@').next().unwrap().to_string();
                self.present.lock().unwrap().insert(name);
            }
            return Ok(CommandOutput { status: 0, stdout: String::new(), stderr: String::new() });
        }
        if matches!(program, "mise" | "node" | "npm" | "uv" | "docker" | "gh")
            && args == ["--version"]
        {
            if self.present.lock().unwrap().contains(program) {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "1.0.0\n".into(),
                    stderr: String::new(),
                });
            }
            return Err(ToolchainError::Missing { tools: vec![program.into()] });
        }
        Ok(CommandOutput { status: 0, stdout: String::new(), stderr: String::new() })
    }
}

#[test]
fn status_reports_mise_entry() {
    let s = status(&[EcosystemKind::Mise]);
    assert!(s.iter().any(|t| t.name == "mise"));
}

#[test]
fn ensure_errors_when_install_disallowed_and_missing() {
    let fake = FakeRunner {
        present: Mutex::new(HashSet::new()),
        installs: Mutex::new(vec![]),
    };
    let err = ensure_with_runner(&fake, &[EcosystemKind::Mise], false).unwrap_err();
    assert!(matches!(err, ToolchainError::Missing { .. }));
}
```

`ensure` delegates to `RealCommandRunner`; tests call `ensure_with_runner`.

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-toolchain`
Expected: FAIL

- [ ] **Step 3: Implement detect/ensure**

Keep install logic conservative: prefer `mise install node@lts uv gh` style when mise exists; never auto-install Docker Desktop.

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-toolchain`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-toolchain
git commit -m "feat: add toolchain status and ensure with injectable runner"
```

---

### Task 6: Orchestration — pin and check (core)

**Files:**
- Create: `crates/pinner-core/src/orchestrate.rs`
- Create: `crates/pinner-core/src/report.rs`
- Modify: `crates/pinner-core/src/lib.rs`
- Test: `crates/pinner-core/tests/orchestrate_fake_ecosystem.rs`

**Interfaces:**
- Consumes: `Ecosystem` trait, `Policy`, `LockFile`, toolchain not required inside core
- Produces:

```rust
pub struct RunOptions {
    pub repo: PathBuf,
    pub dry_run: bool,
    pub offline: bool,
    pub ecosystems_filter: Option<Vec<EcosystemKind>>,
}

pub struct RunReport {
    pub pins: Vec<Pin>,
    pub rewrites: Vec<Rewrite>,
    pub findings: Vec<Finding>,
    pub drift: Vec<DriftItem>,
}

pub struct DriftItem {
    pub path: PathBuf,
    pub name: String,
    pub expected: String,
    pub actual: String,
}

pub fn pin(ecosystems: &[Arc<dyn Ecosystem>], policy: &Policy, opts: &RunOptions) -> Result<RunReport, CoreError>;
pub fn check(ecosystems: &[Arc<dyn Ecosystem>], policy: &Policy, opts: &RunOptions) -> Result<RunReport, CoreError>;
```

`pin` algorithm:
1. Filter ecosystems by policy + `--ecosystem`.
2. Discover manifests; skip ignored paths.
3. Load existing lock if present → `lock_pins`.
4. Build `EcosystemCtx { lock_pins, offline: opts.offline, pin_exact_ranges: policy.pin_exact_ranges }`.
5. Extract findings with ctx; keep floating and non-allowlisted.
6. For each ecosystem, `resolve(findings, &ctx)`.
7. For each manifest, `rewrite`; if `!dry_run`, write `new_contents`.
8. Build `LockFile::from_pins` and write `repo/pinner.lock.json` unless dry-run.
9. Return report.

`check` algorithm:
1. Require `pinner.lock.json` exists else `CoreError::MissingLock`.
2. Re-discover/extract/resolve with `offline: true` preferred when lock covers all; if resolve would need network, compare lock entries to current manifest requested/pinned text instead.
3. Concrete v1 check: after loading lock, for each lock entry verify the file at `path` contains the `pinned` value (ecosystem-specific containment via `extract` showing `requested == pinned` and not floating). Also flag any new floating findings not allowlisted.
4. Populate `drift`; Ok even with drift (CLI maps non-empty drift → exit 1). Use `Result` only for hard errors.

- [ ] **Step 1: Write fake ecosystem + failing orchestration test**

```rust
struct FakeEco;
impl Ecosystem for FakeEco {
    fn kind(&self) -> EcosystemKind { EcosystemKind::Mise }
    fn discover(&self, repo: &Path) -> Result<Vec<Manifest>, EcosystemError> {
        Ok(vec![Manifest { ecosystem: self.kind(), path: repo.join(".mise.toml") }])
    }
    fn extract(&self, manifest: &Manifest, _ctx: &EcosystemCtx<'_>) -> Result<Vec<Finding>, EcosystemError> {
        let text = std::fs::read_to_string(&manifest.path).unwrap();
        let floating = text.contains("latest");
        Ok(vec![Finding {
            ecosystem: EcosystemKind::Mise,
            name: "node".into(),
            requested: if floating { "latest".into() } else { "22.11.0".into() },
            path: manifest.path.clone(),
            is_floating: floating,
        }])
    }
    fn resolve(&self, findings: &[Finding], ctx: &EcosystemCtx<'_>) -> Result<Vec<Pin>, EcosystemError> {
        findings.iter().map(|f| {
            if let Some(p) = ctx.lock_pins.iter().find(|p| p.name == f.name) {
                return Ok(p.clone());
            }
            if ctx.offline && f.is_floating {
                return Err(EcosystemError::Offline { name: f.name.clone(), requested: f.requested.clone() });
            }
            Ok(Pin {
                ecosystem: f.ecosystem,
                name: f.name.clone(),
                requested: f.requested.clone(),
                pinned: "22.11.0".into(),
                path: f.path.clone(),
                evidence: EvidenceKind::Tool,
                metadata: Default::default(),
            })
        }).collect()
    }
    fn rewrite(&self, manifest: &Manifest, pins: &[Pin]) -> Result<Option<Rewrite>, EcosystemError> {
        let pin = &pins[0];
        Ok(Some(Rewrite {
            path: manifest.path.clone(),
            new_contents: format!("[tools]\nnode = \"{}\"\n", pin.pinned),
        }))
    }
}

#[test]
fn pin_rewrites_and_writes_lock() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\nnode = \"latest\"\n").unwrap();
    let eco: Arc<dyn Ecosystem> = Arc::new(FakeEco);
    let report = pin(&[eco], &Policy::default_policy(), &RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: false,
        offline: false,
        ecosystems_filter: None,
    }).unwrap();
    assert_eq!(report.pins[0].pinned, "22.11.0");
    assert!(dir.path().join("pinner.lock.json").exists());
    let body = std::fs::read_to_string(dir.path().join(".mise.toml")).unwrap();
    assert!(body.contains("22.11.0"));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-core --test orchestrate_fake_ecosystem`
Expected: FAIL

- [ ] **Step 3: Implement `orchestrate.rs` + minimal `report.rs`**

Also add `check` test: after pin, mutate file back to `latest`, `check` returns drift non-empty.

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core
git commit -m "feat: add pin/check orchestration over Ecosystem plugins"
```

---

### Task 7: CLI wiring (`pin`, `check`, `toolchain`)

**Files:**
- Create: `crates/pinner/src/cli.rs`
- Modify: `crates/pinner/src/main.rs`
- Modify: `crates/pinner/Cargo.toml` (clap, all crates)
- Test: `crates/pinner/tests/cli_smoke.rs`

**Interfaces:**
- Consumes: `pinner_core::{pin, check, Policy, RunOptions}`, `pinner_toolchain::{status, ensure}`, ecosystem constructors
- Produces: clap commands matching the design; maps reports to exit codes

```rust
#[derive(Parser)]
#[command(name = "pinner", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    offline: bool,
    #[arg(long, global = true)]
    dry_run: bool,
    #[arg(long, global = true, value_delimiter = ',')]
    ecosystem: Option<Vec<String>>,
    #[arg(long, global = true, default_value = "text")]
    format: Format, // text | json
}

enum Commands {
    Pin,
    Check,
    Audit { #[arg(long)] fix: bool }, // stub message until Task 15
    Explain { target: String },        // stub until Task 15
    Toolchain(ToolchainCmd),
}
enum ToolchainCmd { Status, Ensure }
```

Register ecosystems: `vec![Arc::new(pinner_mise::MiseEcosystem), ...]` — for Task 7, mise may still be stub returning empty discover; CLI must still run.

Exit mapping in `main`:
- `Err(CoreError::...)` → 2
- `check`/`audit` with findings/drift → 1
- else → 0

- [ ] **Step 1: Write CLI smoke test**

```rust
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn pin_help_lists_commands() {
    Command::cargo_bin("pinner")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("pin"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("toolchain"));
}
```

- [ ] **Step 2: Run — expect FAIL** (help text missing)

Run: `cargo test -p pinner --test cli_smoke`
Expected: FAIL or binary lacks subcommands

- [ ] **Step 3: Implement clap CLI + wire `pin`/`check`/`toolchain status|ensure`**

`Audit`/`Explain` print `not implemented` to stderr and exit 2 until Task 15.

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner --test cli_smoke`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner
git commit -m "feat: wire clap CLI for pin, check, and toolchain"
```

---

### Task 8: `pinner-mise` discover + extract

**Files:**
- Create: `crates/pinner-mise/src/discover.rs`
- Create: `crates/pinner-mise/src/extract.rs`
- Modify: `crates/pinner-mise/src/lib.rs`
- Create: `tests/fixtures/mise-floating/.mise.toml`
- Create: `tests/fixtures/mise-floating/.tool-versions`
- Test: `crates/pinner-mise/tests/extract_floating.rs`

**Interfaces:**
- Consumes: `toml` / line parser for `.tool-versions`
- Produces: `MiseEcosystem` implementing `discover` + `extract` (resolve/rewrite stub `unimplemented` or empty until Task 9)

Fixture `.mise.toml`:

```toml
[tools]
node = "latest"
python = "3.12"
```

Fixture `.tool-versions`:

```
ruby latest
```

Floating detection: `latest`, `lts`, empty, semver ranges (`^`, `~`, `>=`), and bare channel names that are not exact `MAJOR.MINOR.PATCH` (treat `3.12` as floating minor channel → will pin in resolve; extract marks `is_floating = !is_exact_semver`).

Exact semver: match `^\d+\.\d+\.\d+([.-].+)?$`.

- [ ] **Step 1: Write failing extract tests against fixtures**

```rust
#[test]
fn discovers_mise_toml_and_tool_versions() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-floating");
    let eco = MiseEcosystem;
    let manifests = eco.discover(&repo).unwrap();
    assert_eq!(manifests.len(), 2);
}

#[test]
fn extracts_latest_as_floating() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-floating");
    let eco = MiseEcosystem;
    let m = eco.discover(&repo).unwrap();
    let ctx = EcosystemCtx { lock_pins: &[], offline: false, pin_exact_ranges: true };
    let findings: Vec<_> = m.iter().flat_map(|x| eco.extract(x, &ctx).unwrap()).collect();
    assert!(findings.iter().any(|f| f.name == "node" && f.requested == "latest" && f.is_floating));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-mise`
Expected: FAIL

- [ ] **Step 3: Implement discover + extract**

Use `toml::Value` to read `[tools]` table. Parse `.tool-versions` as `name version` lines.

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-mise`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-mise tests/fixtures/mise-floating
git commit -m "feat(mise): discover manifests and extract floating tools"
```

---

### Task 9: `pinner-mise` resolve + rewrite

**Files:**
- Create: `crates/pinner-mise/src/resolve.rs`
- Create: `crates/pinner-mise/src/rewrite.rs`
- Modify: `crates/pinner-mise/src/lib.rs`
- Test: `crates/pinner-mise/tests/resolve_rewrite.rs`

**Interfaces:**
- Consumes: lock pins, optional `CommandRunner` for `mise latest <tool>` / `mise ls-remote`
- Produces: full `Ecosystem` impl

Resolve order for each finding:
1. Matching lock pin with same `name` + `requested` → reuse (`EvidenceKind::Lock`)
2. Else if offline → `EcosystemError::Offline`
3. Else run `mise latest <name>` (or `mise ls-remote <name> | tail`) → `EvidenceKind::Tool`
4. For tests, inject runner via `MiseEcosystem::with_runner(Arc<dyn CommandRunner>)`

Rewrite:
- `.mise.toml`: use `toml_edit::DocumentMut` to set `tools.<name> = "pinned"`
- `.tool-versions`: replace line for tool name with `name pinned`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn prefers_lock_pin_over_tool() {
    // finding node/latest, lock has pinned 22.11.0 → evidence Lock, no command calls
}

#[test]
fn rewrite_mise_toml_sets_exact_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".mise.toml");
    std::fs::write(&path, "[tools]\nnode = \"latest\"\n").unwrap();
    let manifest = Manifest { ecosystem: EcosystemKind::Mise, path: path.clone() };
    let pins = vec![/* node → 22.11.0 */];
    let rw = MiseEcosystem::default().rewrite(&manifest, &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("22.11.0"));
    assert!(!rw.new_contents.contains("latest"));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-mise --test resolve_rewrite`
Expected: FAIL

- [ ] **Step 3: Implement resolve + rewrite**

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-mise`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-mise
git commit -m "feat(mise): resolve via lock/mise and rewrite manifests"
```

---

### Task 10: Mise end-to-end CLI (pin idempotent + check)

**Files:**
- Create: `crates/pinner/tests/mise_e2e.rs`
- Modify: wire `MiseEcosystem` in CLI registry (if not already)
- Create: `.github/workflows/ci.yml` (build + unit tests; network job optional)

**Interfaces:**
- Consumes: Tasks 1–9
- Produces: green e2e proving design success criteria for mise

- [ ] **Step 1: Write e2e test with fake mise runner OR `PINNER_NETWORK`**

Default CI path uses injected/fake resolution by setting env `PINNER_MISE_RESOLVE_MAP=node=22.11.0,python=3.12.7` read by `MiseEcosystem` when present (test seam). Network path optional:

```rust
#[test]
fn pin_then_check_is_clean_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    // copy fixture files into dir
    std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0,python=3.12.7,ruby=3.3.5");
    Command::cargo_bin("pinner").unwrap()
        .current_dir(dir.path())
        .args(["pin"])
        .assert()
        .success();
    Command::cargo_bin("pinner").unwrap()
        .current_dir(dir.path())
        .args(["check"])
        .assert()
        .success();
    // second pin: capture lock + toml hashes before/after — equal
    Command::cargo_bin("pinner").unwrap()
        .current_dir(dir.path())
        .args(["pin"])
        .assert()
        .success();
}
```

- [ ] **Step 2: Run — expect FAIL until CLI uses real mise crate**

Run: `cargo test -p pinner --test mise_e2e`
Expected: FAIL then fix wiring

- [ ] **Step 3: Implement resolve-map seam + CI workflow**

`.github/workflows/ci.yml`:

```yaml
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --workspace
```

- [ ] **Step 4: Run full workspace tests**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner crates/pinner-mise .github/workflows/ci.yml
git commit -m "test: mise end-to-end pin/check idempotency"
```

---

### Task 11: `pinner-node`

**Files:**
- Create: `crates/pinner-node/src/discover.rs`
- Create: `crates/pinner-node/src/extract.rs`
- Create: `crates/pinner-node/src/resolve.rs`
- Create: `crates/pinner-node/src/rewrite.rs`
- Modify: `crates/pinner-node/src/lib.rs`
- Create: `tests/fixtures/node-floating/package.json`
- Create: `tests/fixtures/node-floating/package-lock.json`
- Test: `crates/pinner-node/tests/node_pin.rs`
- Modify: `crates/pinner/src/cli.rs` (register `NodeEcosystem`)

**Interfaces:**
- `discover`: `package.json` files; follow `workspaces` globs one level
- `extract`: `dependencies`/`devDependencies`/`peerDependencies` values that are `latest`, `*`, or (when `ctx.pin_exact_ranges`) `^`/`~`/`>=` → `is_floating`
- `resolve`: `ctx.lock_pins` → sibling `package-lock.json` packages[name].version → if `ctx.offline` error → else `npm view <pkg> version`
- `rewrite`: mutate dependency strings to exact versions via `serde_json::Value`

Fixture `package.json`:

```json
{
  "name": "demo",
  "dependencies": {
    "left-pad": "^1.3.0",
    "ms": "latest"
  }
}
```

Minimal lock (`packages` node for lockfileVersion 3) must resolve `ms` → `2.1.3` and `left-pad` → `1.3.0`.

- [ ] **Step 1: Write failing tests**

```rust
use pinner_ecosystem::{Ecosystem, EcosystemCtx, EcosystemKind};
use pinner_node::NodeEcosystem;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/node-floating")
}

#[test]
fn extracts_latest_and_caret_as_floating() {
    let eco = NodeEcosystem;
    let ctx = EcosystemCtx { lock_pins: &[], offline: true, pin_exact_ranges: true };
    let manifests = eco.discover(&fixture()).unwrap();
    let findings: Vec<_> = manifests.iter().flat_map(|m| eco.extract(m, &ctx).unwrap()).collect();
    assert!(findings.iter().any(|f| f.name == "ms" && f.is_floating));
    assert!(findings.iter().any(|f| f.name == "left-pad" && f.requested.starts_with('^')));
}

#[test]
fn resolves_from_package_lock_when_offline() {
    let eco = NodeEcosystem;
    let ctx = EcosystemCtx { lock_pins: &[], offline: true, pin_exact_ranges: true };
    let manifests = eco.discover(&fixture()).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert_eq!(pins.iter().find(|p| p.name == "ms").unwrap().pinned, "2.1.3");
}

#[test]
fn rewrite_sets_exact_versions() {
    let eco = NodeEcosystem;
    let manifests = eco.discover(&fixture()).unwrap();
    let ctx = EcosystemCtx { lock_pins: &[], offline: true, pin_exact_ranges: true };
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    let rw = eco.rewrite(&manifests[0], &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("\"ms\": \"2.1.3\""));
    assert!(!rw.new_contents.contains("latest"));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-node`
Expected: FAIL

- [ ] **Step 3: Implement crate + register in CLI**

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-node && cargo test -p pinner`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-node tests/fixtures/node-floating crates/pinner
git commit -m "feat(node): pin package.json using lockfile evidence"
```

---

### Task 12: `pinner-python`

**Files:**
- Create: `crates/pinner-python/src/discover.rs`
- Create: `crates/pinner-python/src/extract.rs`
- Create: `crates/pinner-python/src/resolve.rs`
- Create: `crates/pinner-python/src/rewrite.rs`
- Modify: `crates/pinner-python/src/lib.rs`
- Create: `tests/fixtures/python-floating/pyproject.toml`
- Create: `tests/fixtures/python-floating/requirements.txt`
- Create: `tests/fixtures/python-floating/uv.lock`
- Test: `crates/pinner-python/tests/python_pin.rs`
- Modify: `crates/pinner/src/cli.rs`

**Interfaces:**
- Discover `pyproject.toml` + `requirements*.txt`
- Extract unpinned / `*` / `>=` / bare names from PEP 508 and requirements lines
- Resolve: `ctx.lock_pins` → `uv.lock` package versions → offline error → else `uv pip compile` only when available
- Rewrite: `==version` in requirements; `toml_edit` for pyproject dependency lists

Fixture `requirements.txt`:

```
requests>=2.0
```

Fixture `uv.lock` must include package `requests` version `2.32.3`.

- [ ] **Step 1: Write failing tests**

```rust
use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_python::PythonEcosystem;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python-floating")
}

#[test]
fn extracts_unpinned_requirement() {
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx { lock_pins: &[], offline: true, pin_exact_ranges: true };
    let manifests = eco.discover(&fixture()).unwrap();
    let findings: Vec<_> = manifests.iter().flat_map(|m| eco.extract(m, &ctx).unwrap()).collect();
    assert!(findings.iter().any(|f| f.name == "requests" && f.is_floating));
}

#[test]
fn resolves_from_uv_lock_offline() {
    let eco = PythonEcosystem;
    let ctx = EcosystemCtx { lock_pins: &[], offline: true, pin_exact_ranges: true };
    let manifests = eco.discover(&fixture()).unwrap();
    let req = manifests.iter().find(|m| m.path.ends_with("requirements.txt")).unwrap();
    let findings = eco.extract(req, &ctx).unwrap();
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert_eq!(pins[0].pinned, "2.32.3");
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-python`
Expected: FAIL

- [ ] **Step 3: Implement + register in CLI**

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-python`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-python tests/fixtures/python-floating crates/pinner
git commit -m "feat(python): pin pyproject/requirements from uv.lock evidence"
```

---

### Task 13: `pinner-docker`

**Files:**
- Create: `crates/pinner-docker/src/discover.rs`
- Create: `crates/pinner-docker/src/extract.rs`
- Create: `crates/pinner-docker/src/resolve.rs`
- Create: `crates/pinner-docker/src/rewrite.rs`
- Modify: `crates/pinner-docker/src/lib.rs`
- Create: `tests/fixtures/docker-floating/Dockerfile`
- Create: `tests/fixtures/docker-floating/compose.yaml`
- Test: `crates/pinner-docker/tests/docker_pin.rs`
- Modify: `crates/pinner/src/cli.rs`

**Interfaces:**
- Discover `Dockerfile*` and `compose.yaml` / `docker-compose.yml`
- Extract `FROM` / `image:` refs lacking `@sha256:` or using `latest`/untagged
- Resolve: lock → docker inspect → registry; tests set `PINNER_DOCKER_RESOLVE_MAP=python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`
- Rewrite: digest form; preserve `AS stage` aliases

Fixture `Dockerfile`:

```dockerfile
FROM python:3.12 AS build
RUN echo ok
```

Fixture `compose.yaml`:

```yaml
services:
  app:
    image: alpine:latest
```

- [ ] **Step 1: Write failing tests**

```rust
use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_docker::DockerEcosystem;
use std::path::PathBuf;

#[test]
fn extracts_floating_from_and_compose_image() {
    std::env::set_var(
        "PINNER_DOCKER_RESOLVE_MAP",
        "python:3.12=python@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,alpine:latest=alpine@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/docker-floating");
    let eco = DockerEcosystem;
    let ctx = EcosystemCtx { lock_pins: &[], offline: false, pin_exact_ranges: true };
    let manifests = eco.discover(&repo).unwrap();
    let findings: Vec<_> = manifests.iter().flat_map(|m| eco.extract(m, &ctx).unwrap()).collect();
    assert!(findings.iter().any(|f| f.requested.contains("python:3.12")));
    assert!(findings.iter().any(|f| f.requested.contains("alpine:latest")));
    let pins = eco.resolve(&findings, &ctx).unwrap();
    assert!(pins.iter().all(|p| p.pinned.contains("@sha256:")));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-docker`
Expected: FAIL

- [ ] **Step 3: Implement + register in CLI**

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-docker`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-docker tests/fixtures/docker-floating crates/pinner
git commit -m "feat(docker): pin Dockerfile and compose images to digests"
```

---

### Task 14: `pinner-actions`

**Files:**
- Create: `crates/pinner-actions/src/discover.rs`
- Create: `crates/pinner-actions/src/extract.rs`
- Create: `crates/pinner-actions/src/resolve.rs`
- Create: `crates/pinner-actions/src/rewrite.rs`
- Modify: `crates/pinner-actions/src/lib.rs`
- Create: `tests/fixtures/actions-floating/.github/workflows/ci.yml`
- Test: `crates/pinner-actions/tests/actions_pin.rs`
- Modify: `crates/pinner/src/cli.rs`

**Interfaces:**
- Discover `.github/workflows/*.{yml,yaml}` and `**/action.yml`
- Extract `uses: owner/action@ref` where ref is not a full git SHA (40 or 64 hex)
- Resolve: lock → `gh api`; tests use `PINNER_ACTIONS_RESOLVE_MAP=actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683`
- Rewrite: `uses: owner/action@<sha> # v4`

Fixture workflow:

```yaml
name: ci
on: push
jobs:
  t:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
```

- [ ] **Step 1: Write failing tests**

```rust
use pinner_ecosystem::{Ecosystem, EcosystemCtx};
use pinner_actions::ActionsEcosystem;
use std::path::PathBuf;

#[test]
fn pins_action_tag_to_sha_with_comment() {
    std::env::set_var(
        "PINNER_ACTIONS_RESOLVE_MAP",
        "actions/checkout@v4=11bd71901bbe5b1630ceea73d27597364c9af683",
    );
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/actions-floating");
    let eco = ActionsEcosystem;
    let ctx = EcosystemCtx { lock_pins: &[], offline: false, pin_exact_ranges: true };
    let manifests = eco.discover(&repo).unwrap();
    let findings = eco.extract(&manifests[0], &ctx).unwrap();
    assert!(findings[0].is_floating);
    let pins = eco.resolve(&findings, &ctx).unwrap();
    let rw = eco.rewrite(&manifests[0], &pins).unwrap().unwrap();
    assert!(rw.new_contents.contains("actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683"));
    assert!(rw.new_contents.contains("# v4"));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner-actions`
Expected: FAIL

- [ ] **Step 3: Implement + register in CLI**

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner-actions`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-actions tests/fixtures/actions-floating crates/pinner
git commit -m "feat(actions): pin GitHub Action uses to commit SHAs"
```

---

### Task 15: `audit` + `explain`

**Files:**
- Create: `crates/pinner-core/src/audit.rs`
- Modify: `crates/pinner-core/src/orchestrate.rs`
- Modify: `crates/pinner-core/src/lib.rs`
- Modify: `crates/pinner/src/cli.rs`
- Test: `crates/pinner/tests/audit_explain.rs`

**Interfaces:**
- `pub fn audit(...) -> Result<RunReport, CoreError>` — no writes; findings = floating not allowlisted
- `audit` + CLI `--fix` → call `pin` for those findings only
- `pub struct ExplainReport { pub name: String, pub path: PathBuf, pub requested: String, pub pinned: String, pub evidence: EvidenceKind, pub detail: String }`
- `pub fn explain(..., target: &str) -> Result<ExplainReport, CoreError>` — match lock entry by name or path substring

Audit JSON:

```json
{
  "findings": [
    {"ecosystem":"mise","name":"node","requested":"latest","path":".mise.toml","is_floating":true}
  ]
}
```

- [ ] **Step 1: Write failing CLI tests**

```rust
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn audit_json_reports_floating_mise_tool() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\nnode = \"latest\"\n").unwrap();
    std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0");
    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["audit", "--format", "json"])
        .assert()
        .failure() // exit 1 when findings exist
        .code(1)
        .stdout(predicate::str::contains("\"name\":\"node\""));
}

#[test]
fn explain_after_pin_shows_evidence() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\nnode = \"latest\"\n").unwrap();
    std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0");
    Command::cargo_bin("pinner").unwrap().current_dir(dir.path()).args(["pin"]).assert().success();
    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["explain", "node"])
        .assert()
        .success()
        .stdout(predicate::str::contains("22.11.0"));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p pinner --test audit_explain`
Expected: FAIL

- [ ] **Step 3: Implement audit/explain + remove CLI stubs**

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p pinner --test audit_explain`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core crates/pinner
git commit -m "feat: add audit and explain commands"
```

---

### Task 16: Fixture matrix, consumer CI example, README

**Files:**
- Create: `crates/pinner/tests/idempotency_all_fixtures.rs`
- Create: `.github/workflows/consumer-example.yml`
- Create: `README.md`
- Modify: `.github/workflows/ci.yml` — validate lock fixtures against `schemas/pinner.lock.schema.json` (install `check-jsonschema` or use a tiny Rust test that embeds the schema with `jsonschema` crate)

**Interfaces:**
- Consumes: all ecosystems
- Produces: documented install + usage; consumer workflow example

- [ ] **Step 1: Write idempotency test looping fixtures**

For each `tests/fixtures/*-floating`, copy to tempdir, set resolve-map env vars for that ecosystem, run `pinner pin` twice, assert second run does not change files (`dir_diff` via walking mtimes/hashes), then `pinner check` succeeds.

- [ ] **Step 2: Run — expect FAIL if any ecosystem gaps**

Run: `cargo test -p pinner --test idempotency_all_fixtures`
Expected: PASS when Tasks 10–14 complete

- [ ] **Step 3: Add README + consumer example workflow**

README sections: Install (cargo install / mise), Quick start (`pin`, `check`), Config (`pinner.toml`), Toolchain, CI snippet:

```yaml
- uses: actions/checkout@v4
- run: cargo install --locked --path crates/pinner # or release binary
- run: pinner toolchain ensure
- run: pinner check
```

`consumer-example.yml` is `workflow_dispatch` documentation-only sample.

Add schema validation test reading `schemas/pinner.lock.schema.json` + a golden lock from mise e2e.

- [ ] **Step 4: Full verification**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add README.md .github/workflows crates/pinner
git commit -m "docs: add README, consumer CI example, and fixture idempotency tests"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Rewrite sources | 6, 9–14 |
| Unified `pinner.lock.json` | 3, 6 |
| `pinner check` drift gate | 6, 7, 10 |
| Prefer native lock evidence | 9, 11, 12 |
| Policy defaults + `pinner.toml` | 4 |
| Toolchain status/ensure + test install seam | 5, 10 |
| mise / node / python / docker / actions | 8–14 |
| `audit` / `explain` | 15 |
| Exit codes 0/1/2 | 7 |
| Idempotent pin | 10, 16 |
| Schema at `schemas/pinner.lock.schema.json` | 3, 16 |
| Offline fail-closed | 6, 9 |
| Structured parsers | 8–14 |
| CI consumer pattern | 16 |
| Non-goals (no Renovate bot, no signing) | omitted by design |

## Self-review notes

- Types (`Pin`, `Finding`, `Ecosystem`, evidence rename) are defined in Task 2 and reused consistently.
- Test seams (`CommandRunner`, `PINNER_*_RESOLVE_MAP`) keep CI deterministic without shrinking v1 runtime behavior.
- Tasks 11–14 intentionally parallelizable after Task 10.
