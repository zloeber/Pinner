# Audit Progress, Parallel Audit, and Recursive `.gitignore` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship TTY stderr live progress for `pinner audit`, parallel ecosystem discover/extract on audit only, and recursive `.gitignore` filtering on all discovery.

**Architecture:** Add a repo-scoped `RepoIgnore` (via `ignore` crate) into `discover_manifests` for every command. Extend `audit` with an optional `AuditProgress` sink and `rayon` workers that emit phase events. `pinner-ui` renders status lines to stderr; CLI attaches the sink only when stderr is a TTY and format is interactive text.

**Tech Stack:** Rust workspace (`pinner-core`, `pinner-ui`, `pinner` CLI), `ignore` crate, `rayon`, existing `crossterm` colors, tempfile fixtures.

**Spec:** [`docs/superpowers/specs/2026-08-06-audit-progress-gitignore-design.md`](../specs/2026-08-06-audit-progress-gitignore-design.md)

## Global Constraints

- Progress only on **stderr**, and only when stderr is a TTY, format is text, and neither `--agent` nor `--format json` is set.
- Final findings / pretty audit panel stay on **stdout**; JSON/`--agent` stdout contract unchanged.
- Recursive `.gitignore` applies to **all** discovery (`pin` / `check` / `audit` / `upgrade` / `explain`), combined with policy `ignore_globs` (OR).
- Parallelism is **audit-only**; pin/check/upgrade/explain stay sequential.
- Final audit findings sorted by `(ecosystem, path, name)` for deterministic JSON.
- Missing `.gitignore` is not an error (empty rules).
- Progress sink I/O failures must not change audit exit codes.
- TDD per task; before any `git push`, run `scripts/ci-local`.
- Never put secrets in agent context.
- Before editing symbols, run GitNexus `impact` (repo `Pinner`); before commit, `detect_changes`.

---

## File structure

```text
Cargo.toml                              # workspace deps: ignore, rayon
crates/pinner-core/Cargo.toml           # depend on ignore, rayon
crates/pinner-core/src/gitignore.rs     # NEW: RepoIgnore
crates/pinner-core/src/progress.rs      # NEW: AuditProgress + AuditEvent + AuditPhase
crates/pinner-core/src/lib.rs           # mod + re-exports
crates/pinner-core/src/orchestrate.rs   # thread RepoIgnore through discover_*
crates/pinner-core/src/audit.rs         # progress + rayon parallel audit
crates/pinner-core/tests/gitignore_filter.rs  # NEW
crates/pinner-core/tests/audit_progress.rs    # NEW
crates/pinner-ui/src/progress.rs        # NEW: stderr TTY renderer
crates/pinner-ui/src/lib.rs             # re-export
crates/pinner/src/main.rs               # wire sink for interactive audit
docs/guide/quick-start.md               # live progress note
README.md                               # one-line audit progress mention if present
.mex/patterns/audit-progress-gitignore.md
.mex/patterns/INDEX.md
.mex/ROUTER.md                          # project state
```

---

### Task 1: `RepoIgnore` + unit tests

**Files:**
- Modify: `Cargo.toml` (workspace.dependencies)
- Modify: `crates/pinner-core/Cargo.toml`
- Create: `crates/pinner-core/src/gitignore.rs`
- Modify: `crates/pinner-core/src/lib.rs`
- Test: `crates/pinner-core/tests/gitignore_filter.rs`

**Interfaces:**
- Consumes: repo root `Path`
- Produces:
  - `pub struct RepoIgnore { /* private */ }`
  - `impl RepoIgnore { pub fn new(repo: &Path) -> Self; pub fn is_ignored(&self, path: &Path) -> bool; }`
  - `path` may be absolute or repo-relative; matcher normalizes to repo-relative with `/` separators
  - `is_ignored` is true when gitignore match is `Ignore` (not `Whitelist` / `None`)

- [ ] **Step 1: Add workspace dependencies**

In root `Cargo.toml` under `[workspace.dependencies]`:

```toml
ignore = "0.4"
rayon = "1"
```

In `crates/pinner-core/Cargo.toml` `[dependencies]`:

```toml
ignore = { workspace = true }
rayon = { workspace = true }
```

(`rayon` is unused until Task 4; adding now avoids a second Cargo.toml churn.)

- [ ] **Step 2: Write the failing tests**

Create `crates/pinner-core/tests/gitignore_filter.rs`:

```rust
use pinner_core::RepoIgnore;
use std::fs;
use tempfile::tempdir;

#[test]
fn nested_gitignore_skips_path() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join(".gitignore"), "skip-me/\n").unwrap();
    fs::write(dir.path().join("nested/.gitignore"), "secret.toml\n").unwrap();
    fs::create_dir_all(dir.path().join("skip-me")).unwrap();
    fs::write(dir.path().join("skip-me/Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
    fs::write(dir.path().join("nested/secret.toml"), "x=1\n").unwrap();
    fs::write(dir.path().join("nested/keep.toml"), "x=1\n").unwrap();

    let gi = RepoIgnore::new(dir.path());
    assert!(gi.is_ignored(std::path::Path::new("skip-me/Cargo.toml")));
    assert!(gi.is_ignored(std::path::Path::new("nested/secret.toml")));
    assert!(!gi.is_ignored(std::path::Path::new("nested/keep.toml")));
}

#[test]
fn gitignore_negation_reincludes() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("pkg")).unwrap();
    fs::write(
        dir.path().join(".gitignore"),
        "pkg/*\n!pkg/keep.toml\n",
    )
    .unwrap();
    fs::write(dir.path().join("pkg/drop.toml"), "x=1\n").unwrap();
    fs::write(dir.path().join("pkg/keep.toml"), "x=1\n").unwrap();

    let gi = RepoIgnore::new(dir.path());
    assert!(gi.is_ignored(std::path::Path::new("pkg/drop.toml")));
    assert!(!gi.is_ignored(std::path::Path::new("pkg/keep.toml")));
}

#[test]
fn missing_gitignore_ignores_nothing() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
    let gi = RepoIgnore::new(dir.path());
    assert!(!gi.is_ignored(std::path::Path::new("Cargo.toml")));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p pinner-core --test gitignore_filter`  
Expected: FAIL — `RepoIgnore` not found

- [ ] **Step 4: Implement `RepoIgnore`**

Create `crates/pinner-core/src/gitignore.rs`:

```rust
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// Cached gitignore matcher for a repository root (nested `.gitignore` supported).
pub struct RepoIgnore {
    root: PathBuf,
    matcher: Gitignore,
}

impl RepoIgnore {
    pub fn new(repo: &Path) -> Self {
        let root = repo.to_path_buf();
        let mut builder = GitignoreBuilder::new(&root);
        let _ = builder.add_line(None, ".git/");

        // Discover nested .gitignore files without descending into .git
        let walker = WalkBuilder::new(&root)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .filter_entry(|e| e.file_name() != ".git")
            .build();
        for entry in walker.flatten() {
            if entry.file_type().is_some_and(|t| t.is_file()) && entry.file_name() == ".gitignore"
            {
                let _ = builder.add(entry.path());
            }
        }

        let matcher = builder.build().unwrap_or_else(|_| Gitignore::empty());
        Self { root, matcher }
    }

    /// Returns true if `path` (absolute or repo-relative) is ignored by gitignore rules.
    pub fn is_ignored(&self, path: &Path) -> bool {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .unwrap_or(path)
                .to_path_buf()
        } else {
            path.to_path_buf()
        };
        let normalized = PathBuf::from(relative.to_string_lossy().replace('\\', "/"));
        self.matcher
            .matched(&normalized, false)
            .is_ignore()
    }
}
```

In `crates/pinner-core/src/lib.rs` add:

```rust
pub mod gitignore;
pub use gitignore::RepoIgnore;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p pinner-core --test gitignore_filter`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/pinner-core/Cargo.toml crates/pinner-core/src/gitignore.rs \
  crates/pinner-core/src/lib.rs crates/pinner-core/tests/gitignore_filter.rs
git commit -m "$(cat <<'EOF'
feat: add RepoIgnore for recursive .gitignore matching

EOF
)"
```

---

### Task 2: Filter manifests with gitignore on all discovery

**Files:**
- Modify: `crates/pinner-core/src/orchestrate.rs` (`discover_manifests`, `discover_and_extract`, and every caller that builds extract loops)
- Modify: `crates/pinner-core/src/audit.rs` (pass `RepoIgnore` into discover helpers)
- Test: extend `crates/pinner-core/tests/gitignore_filter.rs`

**Interfaces:**
- Consumes: `RepoIgnore` from Task 1
- Produces: updated signatures:

```rust
pub(crate) fn discover_manifests(
    ecosystem: &dyn Ecosystem,
    policy: &Policy,
    repo: &Path,
    gitignore: &RepoIgnore,
) -> Result<Vec<Manifest>, CoreError>

pub(crate) fn discover_and_extract(
    ecosystem: &dyn Ecosystem,
    policy: &Policy,
    repo: &Path,
    ctx: &EcosystemCtx<'_>,
    gitignore: &RepoIgnore,
) -> Result<(Vec<Manifest>, Vec<Finding>), CoreError>
```

Skip when `policy.is_ignored(&path) || gitignore.is_ignored(&path)`.

- [ ] **Step 1: Write the failing integration-style test**

Append to `crates/pinner-core/tests/gitignore_filter.rs`:

```rust
use pinner_core::{Policy, RunOptions, audit};
use pinner_ecosystem::Ecosystem;
use std::sync::Arc;

/// Minimal stub: discovers a single planted mise-like toml under an ignored dir.
struct StubEco;

impl Ecosystem for StubEco {
    fn kind(&self) -> pinner_ecosystem::EcosystemKind {
        pinner_ecosystem::EcosystemKind::Mise
    }
    fn discover(
        &self,
        repo: &std::path::Path,
    ) -> Result<Vec<pinner_ecosystem::Manifest>, pinner_ecosystem::EcosystemError> {
        let ignored = repo.join("ignored/.mise.toml");
        let kept = repo.join(".mise.toml");
        Ok(vec![
            pinner_ecosystem::Manifest {
                ecosystem: self.kind(),
                path: ignored,
            },
            pinner_ecosystem::Manifest {
                ecosystem: self.kind(),
                path: kept,
            },
        ])
    }
    fn extract(
        &self,
        manifest: &pinner_ecosystem::Manifest,
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Finding>, pinner_ecosystem::EcosystemError> {
        Ok(vec![pinner_ecosystem::Finding {
            ecosystem: self.kind(),
            name: "node".into(),
            requested: "latest".into(),
            path: manifest.path.clone(),
            is_floating: true,
        }])
    }
    fn resolve(
        &self,
        _findings: &[pinner_ecosystem::Finding],
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Pin>, pinner_ecosystem::EcosystemError> {
        Ok(vec![])
    }
    fn rewrite(
        &self,
        _manifest: &pinner_ecosystem::Manifest,
        _pins: &[pinner_ecosystem::Pin],
    ) -> Result<Option<pinner_ecosystem::Rewrite>, pinner_ecosystem::EcosystemError> {
        Ok(None)
    }
}

#[test]
fn audit_skips_gitignored_manifests() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("ignored")).unwrap();
    fs::write(dir.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::write(dir.path().join("ignored/.mise.toml"), "[tools]\nnode=\"latest\"\n").unwrap();
    fs::write(dir.path().join(".mise.toml"), "[tools]\nnode=\"latest\"\n").unwrap();

    let ecosystems: Vec<Arc<dyn Ecosystem>> = vec![Arc::new(StubEco)];
    let policy = Policy::default_policy();
    let opts = RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: true,
        offline: true,
        ecosystems_filter: None,
    };
    // After Task 3 signature includes progress: pass None.
    // For this task, if audit still has the old signature, call without progress.
    let report = audit(&ecosystems, &policy, &opts).unwrap();
    assert_eq!(report.findings.len(), 1);
    assert!(
        report.findings[0]
            .path
            .to_string_lossy()
            .contains(".mise.toml")
            && !report
                .findings[0]
                .path
                .to_string_lossy()
                .contains("ignored")
    );
}
```

Note: If `audit` already gained a progress param in a parallel branch, use `audit(..., None)`. Adjust when Task 3 lands — prefer implementing Task 2 with `audit(..., )` as it exists, then update the call in Task 3. For a linear plan: **implement Task 2 filter first with current `audit()` signature**; the StubEco test calls current `audit`. When Task 3 changes the signature, update this test to `audit(..., None)`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-core --test gitignore_filter audit_skips_gitignored_manifests`  
Expected: FAIL — both findings returned (gitignore not applied) or compile error until wired

- [ ] **Step 3: Wire `RepoIgnore` through discover**

In `orchestrate.rs`:

1. `use crate::gitignore::RepoIgnore;`
2. At the start of `pin_with_filter` / `upgrade_with_filter` / `check` (and any other entry that calls `discover_and_extract`):

```rust
let gitignore = RepoIgnore::new(&opts.repo);
```

3. Pass `&gitignore` into every `discover_and_extract(...)` call.

4. Update `discover_manifests`:

```rust
pub(crate) fn discover_manifests(
    ecosystem: &dyn Ecosystem,
    policy: &Policy,
    repo: &Path,
    gitignore: &RepoIgnore,
) -> Result<Vec<Manifest>, CoreError> {
    Ok(ecosystem
        .discover(repo)?
        .into_iter()
        .filter(|manifest| {
            let path = repo_relative(repo, &manifest.path);
            !policy.is_ignored(&path) && !gitignore.is_ignored(&path)
        })
        .collect())
}
```

5. Update `discover_and_extract` to accept and forward `gitignore`.

In `audit.rs` / `explain`: build `let gitignore = RepoIgnore::new(&opts.repo);` once and pass it into `discover_and_extract`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p pinner-core --test gitignore_filter`  
Expected: PASS  
Also: `cargo test -p pinner-core --lib` and `cargo test -p pinner --test audit_explain`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core/src/orchestrate.rs crates/pinner-core/src/audit.rs \
  crates/pinner-core/tests/gitignore_filter.rs
git commit -m "$(cat <<'EOF'
feat: honor recursive .gitignore when discovering manifests

EOF
)"
```

---

### Task 3: `AuditProgress` events + sequential audit with sink

**Files:**
- Create: `crates/pinner-core/src/progress.rs`
- Modify: `crates/pinner-core/src/lib.rs`
- Modify: `crates/pinner-core/src/audit.rs`
- Modify: `crates/pinner/src/main.rs` (pass `None` for now)
- Test: `crates/pinner-core/tests/audit_progress.rs`
- Modify: `crates/pinner-core/tests/gitignore_filter.rs` (`audit(..., None)`)

**Interfaces:**
- Produces:

```rust
pub enum AuditPhase {
    Discover,
    Extract,
}

pub enum AuditEvent {
    AuditStarted { ecosystems: Vec<EcosystemKind> },
    EcosystemStarted { kind: EcosystemKind },
    EcosystemPhase { kind: EcosystemKind, phase: AuditPhase },
    EcosystemFinished {
        kind: EcosystemKind,
        manifests: usize,
        floating: usize,
    },
    EcosystemFailed { kind: EcosystemKind, error: String },
    AuditFinished { findings: usize },
}

pub trait AuditProgress: Send + Sync {
    fn on_event(&self, event: AuditEvent);
}

pub fn audit(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    progress: Option<&dyn AuditProgress>,
) -> Result<RunReport, CoreError>;
```

- [ ] **Step 1: Write the failing recording-sink test**

Create `crates/pinner-core/tests/audit_progress.rs` using the same `StubEco` pattern (copy a trimmed stub that discovers only `repo/.mise.toml`):

```rust
use pinner_core::{AuditEvent, AuditPhase, AuditProgress, Policy, RunOptions, audit};
use pinner_ecosystem::{Ecosystem, EcosystemKind, Finding, Manifest};
use std::sync::{Arc, Mutex};

struct RecordingSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl AuditProgress for RecordingSink {
    fn on_event(&self, event: AuditEvent) {
        self.events.lock().unwrap().push(event);
    }
}

// StubEco: discover one .mise.toml, extract one floating finding — same as Task 2 stub but single path.

#[test]
fn audit_emits_phase_events() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\nnode=\"latest\"\n").unwrap();
    let sink = RecordingSink {
        events: Mutex::new(Vec::new()),
    };
    let ecosystems: Vec<Arc<dyn Ecosystem>> = vec![Arc::new(StubEco)];
    let policy = Policy::default_policy();
    let opts = RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: true,
        offline: true,
        ecosystems_filter: Some(vec![EcosystemKind::Mise]),
    };
    let report = audit(&ecosystems, &policy, &opts, Some(&sink)).unwrap();
    assert_eq!(report.findings.len(), 1);
    let events = sink.events.lock().unwrap().clone();
    assert!(matches!(events.first(), Some(AuditEvent::AuditStarted { .. })));
    assert!(events.iter().any(|e| matches!(
        e,
        AuditEvent::EcosystemPhase { phase: AuditPhase::Discover, .. }
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        AuditEvent::EcosystemPhase { phase: AuditPhase::Extract, .. }
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        AuditEvent::EcosystemFinished { floating: 1, .. }
    )));
    assert!(matches!(
        events.last(),
        Some(AuditEvent::AuditFinished { findings: 1 })
    ));
}
```

Make `AuditEvent` derive `Debug, Clone` so the test can clone the vec.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-core --test audit_progress`  
Expected: FAIL — types/signature missing

- [ ] **Step 3: Implement progress module + sequential audit with events**

`crates/pinner-core/src/progress.rs` — define `AuditPhase`, `AuditEvent`, `AuditProgress` as in Interfaces (derive `Debug, Clone` on enums).

Update `audit` (still **sequential** in this task):

```rust
pub fn audit(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    progress: Option<&dyn AuditProgress>,
) -> Result<RunReport, CoreError> {
    let emit = |event: AuditEvent| {
        if let Some(p) = progress {
            p.on_event(event);
        }
    };

    let lock_path = opts.repo.join(LOCK_NAME);
    let lock_pins = if lock_path.exists() {
        lock_to_pins(LockFile::read(&lock_path)?)
    } else {
        Vec::new()
    };
    let ctx = EcosystemCtx {
        repo: &opts.repo,
        lock_pins: &lock_pins,
        offline: opts.offline,
        pin_exact_ranges: policy.pin_exact_ranges,
        resolve_mode: ResolveMode::Pin,
    };
    let gitignore = RepoIgnore::new(&opts.repo);

    let selected: Vec<_> = selected_ecosystems(ecosystems, policy, opts).collect();
    emit(AuditEvent::AuditStarted {
        ecosystems: selected.iter().map(|e| e.kind()).collect(),
    });

    let mut report = RunReport::default();
    for ecosystem in selected {
        let kind = ecosystem.kind();
        emit(AuditEvent::EcosystemStarted { kind });
        emit(AuditEvent::EcosystemPhase {
            kind,
            phase: AuditPhase::Discover,
        });
        // Split discover/extract so phases are real: call discover_manifests then extract loop,
        // or emit Discover then Extract around discover_and_extract with two emit points
        // by inlining the two stages here.
        let manifests = match discover_manifests(ecosystem.as_ref(), policy, &opts.repo, &gitignore)
        {
            Ok(m) => m,
            Err(err) => {
                emit(AuditEvent::EcosystemFailed {
                    kind,
                    error: err.to_string(),
                });
                return Err(err);
            }
        };
        emit(AuditEvent::EcosystemPhase {
            kind,
            phase: AuditPhase::Extract,
        });
        let mut extracted = Vec::new();
        for manifest in &manifests {
            match ecosystem.extract(manifest, &ctx) {
                Ok(findings) => {
                    for mut finding in findings {
                        finding.path = repo_relative(&opts.repo, &finding.path);
                        extracted.push(finding);
                    }
                }
                Err(err) => {
                    let core_err = CoreError::from(err);
                    emit(AuditEvent::EcosystemFailed {
                        kind,
                        error: core_err.to_string(),
                    });
                    return Err(core_err);
                }
            }
        }
        let floating = extracted
            .iter()
            .filter(|f| f.is_floating && !is_allowlisted(f, policy, &opts.repo))
            .count();
        emit(AuditEvent::EcosystemFinished {
            kind,
            manifests: manifests.len(),
            floating,
        });
        report.findings.extend(extracted.into_iter().filter(|finding| {
            finding.is_floating && !is_allowlisted(finding, policy, &opts.repo)
        }));
    }

    emit(AuditEvent::AuditFinished {
        findings: report.findings.len(),
    });
    Ok(report)
}
```

Export from `lib.rs`. Update `main.rs`: `audit(&ecosystems, &policy, &opts, None)`. Update `gitignore_filter` test call.

Ensure `CoreError: From<EcosystemError>` already exists (it does via orchestrate paths); if audit previously used `?` on discover_and_extract, keep the same conversion.

- [ ] **Step 4: Run tests**

Run: `cargo test -p pinner-core --test audit_progress`  
Run: `cargo test -p pinner-core --test gitignore_filter`  
Run: `cargo test -p pinner --test audit_explain`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core/src/progress.rs crates/pinner-core/src/lib.rs \
  crates/pinner-core/src/audit.rs crates/pinner/src/main.rs \
  crates/pinner-core/tests/audit_progress.rs crates/pinner-core/tests/gitignore_filter.rs
git commit -m "$(cat <<'EOF'
feat: emit AuditProgress events during audit

EOF
)"
```

---

### Task 4: Parallelize audit with `rayon` + stable ordering

**Files:**
- Modify: `crates/pinner-core/src/audit.rs`
- Test: `crates/pinner-core/tests/audit_progress.rs`

**Interfaces:**
- Consumes: Task 3 `audit` + `rayon`
- Produces: same public API; internal parallel `par_iter`; findings sorted before return

- [ ] **Step 1: Write the failing order/stability test**

Append to `audit_progress.rs`:

```rust
#[test]
fn audit_findings_are_sorted_deterministically() {
    // Stub that returns two ecosystems via two Arc stubs with kinds Mise and Node,
    // each emitting a floating finding with paths "b.toml" and "a.toml" respectively
    // such that unsorted ecosystem iteration order would differ from sorted order.
    // Assert after audit: findings sorted by (ecosystem as_str, path, name).
}
```

Concrete stub implementation for the plan:

```rust
struct MultiStub {
    kind: EcosystemKind,
    file: &'static str,
}

impl Ecosystem for MultiStub {
    fn kind(&self) -> EcosystemKind {
        self.kind
    }
    fn discover(
        &self,
        repo: &std::path::Path,
    ) -> Result<Vec<Manifest>, pinner_ecosystem::EcosystemError> {
        Ok(vec![Manifest {
            ecosystem: self.kind,
            path: repo.join(self.file),
        }])
    }
    fn extract(
        &self,
        manifest: &Manifest,
        _ctx: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<Finding>, pinner_ecosystem::EcosystemError> {
        Ok(vec![Finding {
            ecosystem: self.kind,
            name: "dep".into(),
            requested: "latest".into(),
            path: manifest.path.clone(),
            is_floating: true,
        }])
    }
    fn resolve(
        &self,
        _: &[Finding],
        _: &pinner_ecosystem::EcosystemCtx<'_>,
    ) -> Result<Vec<pinner_ecosystem::Pin>, pinner_ecosystem::EcosystemError> {
        Ok(vec![])
    }
    fn rewrite(
        &self,
        _: &Manifest,
        _: &[pinner_ecosystem::Pin],
    ) -> Result<Option<pinner_ecosystem::Rewrite>, pinner_ecosystem::EcosystemError> {
        Ok(None)
    }
}

#[test]
fn audit_findings_are_sorted_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.toml"), "").unwrap();
    std::fs::write(dir.path().join("a.toml"), "").unwrap();
    let ecosystems: Vec<Arc<dyn Ecosystem>> = vec![
        Arc::new(MultiStub {
            kind: EcosystemKind::Node,
            file: "b.toml",
        }),
        Arc::new(MultiStub {
            kind: EcosystemKind::Mise,
            file: "a.toml",
        }),
    ];
    let policy = Policy::default_policy();
    let opts = RunOptions {
        repo: dir.path().to_path_buf(),
        dry_run: true,
        offline: true,
        ecosystems_filter: Some(vec![EcosystemKind::Mise, EcosystemKind::Node]),
    };
    let report = audit(&ecosystems, &policy, &opts, None).unwrap();
    let keys: Vec<_> = report
        .findings
        .iter()
        .map(|f| {
            (
                f.ecosystem.as_str().to_string(),
                f.path.to_string_lossy().into_owned(),
                f.name.clone(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
}
```

- [ ] **Step 2: Run test (may pass sequentially; keep as regression)**

Run: `cargo test -p pinner-core --test audit_progress audit_findings_are_sorted_deterministically`  
Expected: PASS after Step 3 even under parallel execution

- [ ] **Step 3: Parallelize**

In `audit.rs`:

```rust
use rayon::prelude::*;
use std::sync::Mutex;
```

Replace the sequential `for ecosystem in selected` loop with:

1. `let progress_mu = progress.map(|p| Mutex::new(p));` — actually trait object reference can't move into mutex easily. Prefer:

```rust
struct ProgressBridge<'a>(&'a dyn AuditProgress);
impl ProgressBridge<'_> {
    fn emit(&self, event: AuditEvent) {
        self.0.on_event(event);
    }
}
let bridge = progress.map(ProgressBridge);
let bridge_mu = bridge.as_ref().map(|_| Mutex::new(()));
let emit = |event: AuditEvent| {
    if let (Some(b), Some(mu)) = (bridge.as_ref(), bridge_mu.as_ref()) {
        let _guard = mu.lock().unwrap();
        b.emit(event);
    }
};
```

Simpler approach matching the spec: wrap the sink in `Mutex` at the CLI later; for core, take `Option<&dyn AuditProgress>` and synchronize with:

```rust
let sink: Option<&dyn AuditProgress> = progress;
let emit = |event: AuditEvent| {
    if let Some(p) = sink {
        // AuditProgress is Sync; serialize event delivery
        static ORDER: ... // NO — use a local Mutex<()> for emit ordering
    }
};
```

Use this exact pattern:

```rust
let emit_lock = Mutex::new(());
let emit = |event: AuditEvent| {
    if let Some(p) = progress {
        let _g = emit_lock.lock().unwrap();
        p.on_event(event);
    }
};
```

Then:

```rust
let results: Result<Vec<(EcosystemKind, Vec<Finding>, usize, usize)>, CoreError> = selected
    .par_iter()
    .map(|ecosystem| {
        let kind = ecosystem.kind();
        emit(AuditEvent::EcosystemStarted { kind });
        emit(AuditEvent::EcosystemPhase {
            kind,
            phase: AuditPhase::Discover,
        });
        let manifests = discover_manifests(ecosystem.as_ref(), policy, &opts.repo, &gitignore)
            .map_err(|err| {
                emit(AuditEvent::EcosystemFailed {
                    kind,
                    error: err.to_string(),
                });
                err
            })?;
        emit(AuditEvent::EcosystemPhase {
            kind,
            phase: AuditPhase::Extract,
        });
        let mut extracted = Vec::new();
        for manifest in &manifests {
            for mut finding in ecosystem.extract(manifest, &ctx).map_err(|e| {
                let err = CoreError::from(e);
                emit(AuditEvent::EcosystemFailed {
                    kind,
                    error: err.to_string(),
                });
                err
            })? {
                finding.path = repo_relative(&opts.repo, &finding.path);
                extracted.push(finding);
            }
        }
        let floating_findings: Vec<_> = extracted
            .into_iter()
            .filter(|f| f.is_floating && !is_allowlisted(f, policy, &opts.repo))
            .collect();
        let floating = floating_findings.len();
        emit(AuditEvent::EcosystemFinished {
            kind,
            manifests: manifests.len(),
            floating,
        });
        Ok((kind, floating_findings, manifests.len(), floating))
    })
    .collect();

let mut report = RunReport::default();
for (_kind, findings, _, _) in results? {
    report.findings.extend(findings);
}
report.findings.sort_by(|a, b| {
    (
        a.ecosystem.as_str(),
        a.path.as_os_str(),
        a.name.as_str(),
    )
        .cmp(&(
            b.ecosystem.as_str(),
            b.path.as_os_str(),
            b.name.as_str(),
        ))
});
emit(AuditEvent::AuditFinished {
    findings: report.findings.len(),
});
Ok(report)
```

**Lifetime:** `EcosystemCtx` holds `&Path` / `&[Pin]` — those are `Sync`, so sharing `&ctx` across rayon is fine. `Policy` and `RepoIgnore` must be `Sync` (they are if they only contain owned data + `Gitignore`).

If `par_iter().map(...).collect::<Result<...>>()` short-circuits poorly, collect `Vec<Result<...>>` then fold the first error — either is acceptable; prefer failing fast on first `Err` after join.

- [ ] **Step 4: Run tests**

Run: `cargo test -p pinner-core --test audit_progress`  
Run: `cargo test -p pinner --test audit_explain`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core/src/audit.rs crates/pinner-core/tests/audit_progress.rs
git commit -m "$(cat <<'EOF'
feat: parallelize audit ecosystems with stable finding order

EOF
)"
```

---

### Task 5: TTY stderr progress renderer in `pinner-ui`

**Files:**
- Create: `crates/pinner-ui/src/progress.rs`
- Modify: `crates/pinner-ui/src/lib.rs`
- Test: unit tests inside `progress.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `AuditProgress`, `AuditEvent` from `pinner-core`
- Produces:

```rust
pub struct StderrAuditProgress {
    color: bool,
}

impl StderrAuditProgress {
    pub fn new(color: bool) -> Self;
}

impl AuditProgress for StderrAuditProgress {
    fn on_event(&self, event: AuditEvent);
}
```

Also: `pub fn format_audit_event(event: &AuditEvent) -> String` for tests (pure formatting).

- [ ] **Step 1: Write failing format tests**

In `crates/pinner-ui/src/progress.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use pinner_core::{AuditEvent, AuditPhase};
    use pinner_ecosystem::EcosystemKind;

    #[test]
    fn formats_started_banner() {
        let s = format_audit_event(&AuditEvent::AuditStarted {
            ecosystems: vec![EcosystemKind::Mise, EcosystemKind::Cargo],
        });
        assert!(s.contains("audit"));
        assert!(s.contains("2"));
        assert!(s.contains("parallel"));
    }

    #[test]
    fn formats_finished_ecosystem() {
        let s = format_audit_event(&AuditEvent::EcosystemFinished {
            kind: EcosystemKind::Cargo,
            manifests: 3,
            floating: 2,
        });
        assert!(s.contains("cargo"));
        assert!(s.contains("3"));
        assert!(s.contains("2"));
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p pinner-ui formats_started_banner`  
Expected: FAIL — module missing

- [ ] **Step 3: Implement renderer**

```rust
use std::io::{self, Write};

use crossterm::style::{Color, ResetColor, SetForegroundColor};
use pinner_core::{AuditEvent, AuditPhase, AuditProgress};
use pinner_ecosystem::EcosystemKind;

pub struct StderrAuditProgress {
    color: bool,
}

impl StderrAuditProgress {
    pub fn new(color: bool) -> Self {
        Self { color }
    }
}

pub fn format_audit_event(event: &AuditEvent) -> String {
    match event {
        AuditEvent::AuditStarted { ecosystems } => format!(
            "pinner audit · {} ecosystem{} · parallel",
            ecosystems.len(),
            if ecosystems.len() == 1 { "" } else { "s" }
        ),
        AuditEvent::EcosystemStarted { kind } => {
            format!("  … {:<12} starting", kind.as_str())
        }
        AuditEvent::EcosystemPhase { kind, phase } => {
            let phase = match phase {
                AuditPhase::Discover => "discover",
                AuditPhase::Extract => "extract",
            };
            format!("  … {:<12} {phase}", kind.as_str())
        }
        AuditEvent::EcosystemFinished {
            kind,
            manifests,
            floating,
        } => format!(
            "  ✓  {:<12} {manifests} manifest{} · {floating} floating",
            kind.as_str(),
            if *manifests == 1 { "" } else { "s" }
        ),
        AuditEvent::EcosystemFailed { kind, error } => {
            format!("  ✗  {:<12} {error}", kind.as_str())
        }
        AuditEvent::AuditFinished { findings } => format!(
            "pinner audit · done · {findings} finding{}",
            if *findings == 1 { "" } else { "s" }
        ),
    }
}

impl AuditProgress for StderrAuditProgress {
    fn on_event(&self, event: AuditEvent) {
        let line = format_audit_event(&event);
        let mut err = io::stderr().lock();
        let _ = match (&event, self.color) {
            (AuditEvent::EcosystemFinished { .. }, true) => {
                write!(err, "{}", SetForegroundColor(Color::Green)).and_then(|_| {
                    writeln!(err, "{line}")?;
                    write!(err, "{}", ResetColor)
                })
            }
            (AuditEvent::EcosystemFailed { .. }, true) => {
                write!(err, "{}", SetForegroundColor(Color::Red)).and_then(|_| {
                    writeln!(err, "{line}")?;
                    write!(err, "{}", ResetColor)
                })
            }
            (AuditEvent::AuditStarted { .. } | AuditEvent::AuditFinished { .. }, true) => {
                write!(err, "{}", SetForegroundColor(Color::DarkCyan)).and_then(|_| {
                    writeln!(err, "{line}")?;
                    write!(err, "{}", ResetColor)
                })
            }
            _ => writeln!(err, "{line}"),
        };
        let _ = err.flush();
    }
}
```

Export from `lib.rs`:

```rust
mod progress;
pub use progress::{StderrAuditProgress, format_audit_event};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p pinner-ui`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-ui/src/progress.rs crates/pinner-ui/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: render audit progress lines on stderr

EOF
)"
```

---

### Task 6: Wire CLI interactive audit progress

**Files:**
- Modify: `crates/pinner/src/main.rs`
- Test: `crates/pinner/tests/audit_explain.rs` (ensure JSON path still clean)

**Interfaces:**
- Consumes: `StderrAuditProgress`, `audit(..., progress)`
- Produces: interactive text audit attaches sink when `stderr_is_tty() && format == Text && !cli.agent`

- [ ] **Step 1: Add helper + failing documentation test comment**

In `main.rs`, add:

```rust
fn stderr_is_tty() -> bool {
    io::stderr().is_terminal()
}
```

(There is already `stdout_is_tty` — mirror it.)

- [ ] **Step 2: Wire Audit command**

Replace the non-fix audit branch:

```rust
Commands::Audit { fix } => {
    let (policy, opts, ecosystems) = prepare(&cli)?;
    if *fix {
        // unchanged
    } else {
        let use_progress = matches!(format, Format::Text)
            && !cli.agent
            && stderr_is_tty();
        let report = if use_progress {
            let sink = pinner_ui::StderrAuditProgress::new(true);
            audit(&ecosystems, &policy, &opts, Some(&sink))?
        } else {
            audit(&ecosystems, &policy, &opts, None)?
        };
        emit_audit(&report, format)?;
        if report.findings.is_empty() {
            Ok(ExitCode::SUCCESS)
        } else {
            Ok(ExitCode::from(1))
        }
    }
}
```

- [ ] **Step 3: Strengthen JSON test (stdout-only contract)**

In `audit_explain.rs`, extend `audit_json_reports_floating_mise_tool` (or add sibling):

```rust
#[test]
fn audit_json_stdout_is_findings_only() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mise.toml"),
        "[tools]\nnode = \"latest\"\n",
    )
    .unwrap();
    unsafe {
        std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0");
    }
    let assert = Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["audit", "--format", "json"])
        .assert()
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(assert.get_output().stdout);
    assert!(stdout.trim_start().starts_with('{'));
    assert!(!stdout.contains("pinner audit ·"));
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }
}
```

Note: `assert_cmd` output access — use `.get_output()` on the `Assert` if available, or capture via `Output` API:

```rust
let output = Command::cargo_bin("pinner")
    .unwrap()
    .current_dir(dir.path())
    .args(["audit", "--format", "json"])
    .output()
    .unwrap();
assert_eq!(output.status.code(), Some(1));
let stdout = String::from_utf8_lossy(&output.stdout);
assert!(stdout.trim_start().starts_with('{'));
assert!(!stdout.contains("pinner audit ·"));
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p pinner --test audit_explain`  
Run: `scripts/ci-local`  
Expected: all gates green

- [ ] **Step 5: Commit**

```bash
git add crates/pinner/src/main.rs crates/pinner/tests/audit_explain.rs
git commit -m "$(cat <<'EOF'
feat: show live audit progress on interactive TTY stderr

EOF
)"
```

---

### Task 7: Docs + `.mex` pattern

**Files:**
- Modify: `docs/guide/quick-start.md`
- Modify: `README.md` (audit bullet if it mentions TTY panel)
- Create: `.mex/patterns/audit-progress-gitignore.md`
- Modify: `.mex/patterns/INDEX.md`
- Modify: `.mex/ROUTER.md` (Current Project State)

**Interfaces:**
- None (docs only)

- [ ] **Step 1: Update quick-start**

Change the audit line to:

```markdown
pinner audit     # report floating refs (live progress on TTY stderr; pretty panel on stdout)
```

Add under Modes:

```markdown
Interactive text `audit` prints per-ecosystem discover/extract progress on stderr; `--agent` / `--format json` stay quiet on stderr for progress.
```

- [ ] **Step 2: Create pattern**

`.mex/patterns/audit-progress-gitignore.md`:

```markdown
---
name: audit-progress-gitignore
description: Live audit progress sink, parallel audit ecosystems, recursive .gitignore on discover.
---

# Audit progress + gitignore

## Steps
1. Discovery filtering belongs in `discover_manifests` (policy globs OR `RepoIgnore`), never only in one ecosystem crate.
2. Progress events are core types; ANSI rendering stays in `pinner-ui`.
3. Attach `StderrAuditProgress` only when stderr is a TTY and format is interactive text.
4. Parallelism is audit-only; always sort findings before return.
5. Progress goes to stderr; findings/pretty panel to stdout.

## Gotchas
- `AuditProgress` callbacks from rayon must be serialized (mutex) to avoid interleaved ANSI.
- Nested `.gitignore` requires adding every `.gitignore` file to `GitignoreBuilder`, not only the root file.
- JSON/`--agent` tests must assert progress banners never appear on stdout.
```

Add INDEX row alphabetically.

Update ROUTER "Working" bullet to mention live audit progress + recursive gitignore.

- [ ] **Step 3: Commit**

```bash
git add docs/guide/quick-start.md README.md \
  .mex/patterns/audit-progress-gitignore.md .mex/patterns/INDEX.md .mex/ROUTER.md
git commit -m "$(cat <<'EOF'
docs: document audit progress and gitignore discovery

EOF
)"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| TTY stderr progress | 5, 6 |
| Quiet agent/JSON/non-TTY | 6 |
| Pretty panel still stdout | 6 (unchanged `emit_audit`) |
| Recursive `.gitignore` all commands | 1, 2 |
| Policy globs still apply | 2 |
| Parallel audit only | 4 |
| Deterministic finding order | 4 |
| `AuditProgress` / events API | 3 |
| Tests for gitignore / progress / order / JSON | 1–6 |
| Docs + `.mex` | 7 |
| `scripts/ci-local` | 6 |

## Self-review notes

- No TBD placeholders; `rayon` chosen (not "or std::thread").
- `audit` signature change is explicit; all call sites listed.
- Stub ecosystems in tests avoid network and resolve-map env.
- Task 2 note about `audit(..., None)` after Task 3 is resolved by ordering: Task 2 uses pre-progress signature; Task 3 updates call sites including the gitignore test.
