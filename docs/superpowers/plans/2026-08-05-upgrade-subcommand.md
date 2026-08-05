# Upgrade Subcommand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship opt-in `pinner upgrade` that re-resolves declared deps to newest versions per ecosystem, reuses pin’s rewrite/lock/walkthrough path with current→proposed TUI, and documents every provider’s preferred upgrade means (plus mise `cargo:` / `github:` install).

**Architecture:** Add `ResolveMode::{Pin, Upgrade}` on `EcosystemCtx`. Core `upgrade` / `upgrade_with_filter` select all upgradeable findings and call `resolve` in Upgrade mode (skip lock/native freeze; omit unchanged). CLI wires `Commands::Upgrade` like Pin. Docs: README matrix + 13 ecosystem guide pages.

**Tech Stack:** Existing Rust workspace (`pinner-core`, `pinner-ecosystem`, ecosystem crates, `pinner-ui`, clap CLI), resolve-map env seams, mdBook docs.

**Spec:** [`docs/superpowers/specs/2026-08-05-upgrade-subcommand-design.md`](../specs/2026-08-05-upgrade-subcommand-design.md)

## Global Constraints

- Command name is exactly `upgrade` (not `update`).
- Default policy is **latest available**, including major bumps.
- Candidates = all upgradeable `extract` findings (floating and exact); path/git/VCS/local remain skipped by extract.
- Upgrade resolve **bypasses** `pinner.lock.json` and native locks; `PINNER_*_RESOLVE_MAP` still wins when set.
- Unchanged versions (already latest) are **omitted** from proposed pins; empty set → no writes, success.
- Prefer **uv** for Python; prefer Rust HTTP clients over Node/Ruby CLIs when adding new online paths for cargo/go/ruby.
- Walkthrough quit → exit 0, nothing written; `--walkthrough` + `--agent`/json/non-TTY → exit 2.
- Exit codes: `0` ok/abort/no-op; `2` tool/config/resolve/invalid mode.
- mise install docs must show `github:zloeber/Pinner` and `cargo:pinner` only (no vague `mise install pinner`).
- TDD per task; before any `git push`, run `scripts/ci-local`.
- Do not put secrets in agent context.

---

## File structure

```text
crates/pinner-ecosystem/src/lib.rs     # ResolveMode + EcosystemCtx field
crates/pinner-core/src/orchestrate.rs  # upgrade / upgrade_with_filter
crates/pinner-core/src/lib.rs          # re-exports
crates/pinner-core/src/audit.rs        # Pin mode on ctx
crates/pinner/src/cli.rs               # Commands::Upgrade
crates/pinner/src/main.rs              # dispatch + run_upgrade
crates/pinner-ui/src/walkthrough.rs    # current → proposed column
crates/pinner-{mise,node,python,...}/  # Upgrade branches in resolve.rs
crates/pinner-toolchain/src/detect.rs  # tools for cargo/go when needed
docs/guide/ecosystems/*.md             # 13 provider pages
README.md, docs/guide/*, docs/SUMMARY.md, skills/pinner/SKILL.md
tests/fixtures/*-upgrade/              # exact-pin fixtures for upgrade
```

---

### Task 1: ResolveMode on EcosystemCtx

**Files:**
- Modify: `crates/pinner-ecosystem/src/lib.rs`
- Modify: every `EcosystemCtx { ... }` construction in `crates/pinner-core/src/orchestrate.rs`, `crates/pinner-core/src/audit.rs`, and ecosystem unit tests that build `EcosystemCtx`
- Test: `crates/pinner-ecosystem/tests/types_roundtrip.rs` (add mode unit test in lib if no serde on mode)

**Interfaces:**
- Consumes: existing `EcosystemCtx`
- Produces: `pub enum ResolveMode { Pin, Upgrade }` and `EcosystemCtx.resolve_mode: ResolveMode`

- [ ] **Step 1: Write the failing test**

Add to `crates/pinner-ecosystem/src/lib.rs` under `#[cfg(test)]`:

```rust
#[test]
fn resolve_mode_defaults_are_distinct() {
    assert_ne!(ResolveMode::Pin, ResolveMode::Upgrade);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-ecosystem resolve_mode_defaults_are_distinct -- --nocapture`  
Expected: FAIL — `ResolveMode` not found

- [ ] **Step 3: Write minimal implementation**

In `crates/pinner-ecosystem/src/lib.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolveMode {
    Pin,
    Upgrade,
}
```

Add field to `EcosystemCtx`:

```rust
pub resolve_mode: ResolveMode,
```

Update all call sites to `resolve_mode: ResolveMode::Pin` (pin/check/audit paths).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pinner-ecosystem resolve_mode_defaults_are_distinct`  
Expected: PASS  
Also: `cargo test -p pinner-core --lib` and fix any missing field errors in tests.

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-ecosystem crates/pinner-core crates/pinner-*/src
git commit -m "$(cat <<'EOF'
feat: add ResolveMode to EcosystemCtx

EOF
)"
```

---

### Task 2: Core `upgrade` / `upgrade_with_filter`

**Files:**
- Modify: `crates/pinner-core/src/orchestrate.rs`
- Modify: `crates/pinner-core/src/lib.rs`
- Modify: `crates/pinner-core/src/report.rs` (add `upgraded: usize` if report lacks a suitable field; reuse `pinned` count with clear JSON key `upgraded` via existing counters — prefer documenting `pins_written` semantics; add `pub upgraded: usize` to `RunReport`)
- Test: `crates/pinner-core/tests/orchestrate_fake_ecosystem.rs`

**Interfaces:**
- Consumes: `pin_with_filter` staging/rewrite helpers; `ResolveMode`
- Produces: `pub fn upgrade(...)`, `pub fn upgrade_with_filter(..., walkthrough: Option<&mut WalkthroughFilter<'_>>)` → `Result<RunReport, CoreError>`

- [ ] **Step 1: Write the failing test**

In `crates/pinner-core/tests/orchestrate_fake_ecosystem.rs`, add a fake ecosystem that returns both floating and exact findings, and in `resolve` if `ctx.resolve_mode == ResolveMode::Upgrade` returns a newer pin for the exact finding. Assert `upgrade()` rewrites the exact pin and sets report upgraded ≥ 1. Assert `pin()` does not rewrite the exact pin when only exact findings exist.

```rust
#[test]
fn upgrade_rewrites_exact_pins_pin_does_not() {
    // fixture: tool = "1.0.0" exact
    // fake resolve Upgrade → "2.0.0" with metadata upgrade/previous
    // upgrade() → file contains 2.0.0
    // reset file to 1.0.0; pin() → file still 1.0.0
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-core upgrade_rewrites_exact_pins_pin_does_not -- --nocapture`  
Expected: FAIL — `upgrade` not found

- [ ] **Step 3: Write minimal implementation**

Refactor shared pipeline from `pin_with_filter` into an internal `run_resolve_rewrite(mode, finding_filter, ...)` or duplicate carefully:

```rust
pub fn upgrade(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
) -> Result<RunReport, CoreError> {
    upgrade_with_filter(ecosystems, policy, opts, None)
}

pub fn upgrade_with_filter(
    ecosystems: &[Arc<dyn Ecosystem>],
    policy: &Policy,
    opts: &RunOptions,
    walkthrough: Option<&mut WalkthroughFilter<'_>>,
) -> Result<RunReport, CoreError> {
    let ctx = EcosystemCtx {
        repo: &opts.repo,
        lock_pins: &[], // unused in Upgrade mode; keep empty
        offline: opts.offline,
        pin_exact_ranges: policy.pin_exact_ranges,
        resolve_mode: ResolveMode::Upgrade,
    };
    // finding filter: all findings (not only is_floating), still skip allowlisted if desired —
    // spec: allowlisted floating refs remain upgradeable; do not filter allow_floating out.
    // After resolve, drop pins where pinned == previous (ecosystems should omit; belt-and-suspenders here).
    // Then same walkthrough → rewrite → lock as pin_with_filter.
}
```

Export from `lib.rs`: `pub use orchestrate::{..., upgrade, upgrade_with_filter};`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pinner-core upgrade_rewrites_exact_pins_pin_does_not`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-core
git commit -m "$(cat <<'EOF'
feat: add upgrade orchestration in pinner-core

EOF
)"
```

---

### Task 3: CLI `upgrade` command

**Files:**
- Modify: `crates/pinner/src/cli.rs`
- Modify: `crates/pinner/src/main.rs`
- Test: `crates/pinner/tests/walkthrough_mode_cli.rs` (add upgrade cases)
- Create: `crates/pinner/tests/upgrade_cli_smoke.rs`

**Interfaces:**
- Consumes: `upgrade`, `upgrade_with_filter`
- Produces: `Commands::Upgrade` dispatch mirroring `Commands::Pin`

- [ ] **Step 1: Write the failing test**

```rust
// crates/pinner/tests/upgrade_cli_smoke.rs
#[test]
fn upgrade_help_lists_subcommand() {
    let output = Command::new(env!("CARGO_BIN_EXE_pinner"))
        .arg("upgrade")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn upgrade_walkthrough_with_agent_exits_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_pinner"))
        .args(["--walkthrough", "--agent", "upgrade"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner upgrade_help_lists_subcommand -- --nocapture`  
Expected: FAIL (unknown command or help missing)

- [ ] **Step 3: Write minimal implementation**

In `cli.rs`:

```rust
/// Re-resolve declared deps to newest versions and rewrite / lock
Upgrade,
```

In `main.rs`, mirror `Commands::Pin` with `run_upgrade` calling `upgrade` / `upgrade_with_filter`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pinner upgrade_help_lists_subcommand upgrade_walkthrough_with_agent_exits_2`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner
git commit -m "$(cat <<'EOF'
feat: add pinner upgrade CLI subcommand

EOF
)"
```

---

### Task 4: Walkthrough side-by-side for upgrades

**Files:**
- Modify: `crates/pinner-ui/src/walkthrough.rs`
- Test: unit tests in same file

**Interfaces:**
- Consumes: `Pin.metadata` keys `upgrade` (bool) and `previous` (string)
- Produces: column/cells `current → proposed` when upgrade metadata present

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn format_pin_transition_prefers_previous_for_upgrade() {
    let mut pin = sample_pin("1.0.0", "2.0.0");
    pin.metadata.insert("upgrade".into(), Value::Bool(true));
    pin.metadata.insert("previous".into(), Value::String("1.0.0".into()));
    assert_eq!(format_pin_transition(&pin), "1.0.0 → 2.0.0");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pinner-ui format_pin_transition_prefers_previous_for_upgrade`  
Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

```rust
fn format_pin_transition(pin: &Pin) -> String {
    let current = pin
        .metadata
        .get("upgrade")
        .and_then(|v| v.as_bool())
        .filter(|u| *u)
        .and_then(|_| pin.metadata.get("previous").and_then(|v| v.as_str()))
        .unwrap_or(pin.requested.as_str());
    format!("{current} → {}", pin.pinned)
}
```

Use in table rows; header `current → proposed` when any pin has upgrade metadata, else `requested → proposed`. Set title to `proposed upgrades` when any upgrade metadata present.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pinner-ui format_pin_transition_prefers_previous_for_upgrade`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pinner-ui
git commit -m "$(cat <<'EOF'
feat: show current→proposed in upgrade walkthrough

EOF
)"
```

---

### Task 5: Shared upgrade pin helper

**Files:**
- Create: `crates/pinner-ecosystem/src/upgrade.rs` (or functions in `lib.rs`)
- Modify: `crates/pinner-ecosystem/src/lib.rs` (`mod upgrade; pub use`)

**Interfaces:**
- Produces:

```rust
pub fn upgrade_pin(
    finding: &Finding,
    previous: &str,
    newest: &str,
    evidence: EvidenceKind,
    channel: &str,
) -> Option<Pin>
```

Returns `None` if `previous == newest` (omit unchanged). Sets metadata `upgrade`, `previous`, `upgrade_channel`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn upgrade_pin_omits_unchanged() {
    let f = Finding { /* ... requested 1.0.0 ... */ };
    assert!(upgrade_pin(&f, "1.0.0", "1.0.0", EvidenceKind::Registry, "map").is_none());
    let p = upgrade_pin(&f, "1.0.0", "2.0.0", EvidenceKind::Registry, "map").unwrap();
    assert_eq!(p.pinned, "2.0.0");
    assert_eq!(p.metadata["previous"], "1.0.0");
}
```

- [ ] **Step 2: Run to see fail; Step 3 implement; Step 4 pass; Step 5 commit**

```bash
git commit -m "$(cat <<'EOF'
feat: add shared upgrade_pin helper

EOF
)"
```

---

### Task 6: Python + Node upgrade resolve

**Files:**
- Modify: `crates/pinner-python/src/resolve.rs`
- Modify: `crates/pinner-node/src/resolve.rs`
- Test: crate-level resolve tests + fixtures under `tests/fixtures/python-upgrade/`, `tests/fixtures/node-upgrade/`

**Interfaces:**
- Consumes: `ResolveMode::Upgrade`, `upgrade_pin`
- Produces: Upgrade path that skips lock/native; uses `uv` / `npm view`; maps first

- [ ] **Step 1: Write failing tests** with resolve maps forcing newer versions while lock/native would keep old.

- [ ] **Step 2: Run fail; Step 3: branch at top of `resolve_one`:**

```rust
if ctx.resolve_mode == ResolveMode::Upgrade {
    return resolve_upgrade(...);
}
```

`resolve_upgrade`: map → online tool → fail; never read pinner.lock/native lock. `previous` = finding.requested if exact else native/lock peek **only for display** (optional read for metadata, not as chosen pin).

- [ ] **Step 4: Pass tests; Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat: upgrade resolve for python and node

EOF
)"
```

---

### Task 7: Mise + Docker + Actions upgrade resolve

**Files:**
- Modify: `crates/pinner-mise/src/resolve.rs`
- Modify: `crates/pinner-docker/src/resolve.rs`
- Modify: `crates/pinner-actions/src/resolve.rs`
- Tests + maps as above

**Behavior:**
- mise: `mise latest` / `ls-remote` (already exists) under Upgrade for exact pins too
- docker: re-resolve image to fresh digest (tag from reference; if already digest-only without tag metadata, skip / None)
- actions: latest release commit via `gh api`; images as docker

- [ ] Steps: failing tests → implement Upgrade branches → pass → commit

```bash
git commit -m "$(cat <<'EOF'
feat: upgrade resolve for mise, docker, and actions

EOF
)"
```

---

### Task 8: Terraform + Helm + K8s upgrade resolve

**Files:**
- Modify: `crates/pinner-terraform/src/resolve.rs`
- Modify: `crates/pinner-helm/src/resolve.rs`
- Modify: `crates/pinner-k8s/src/resolve.rs`

**Behavior:**
- terraform: registry **latest** version (not constrained match); git modules → current default-branch SHA via `ls-remote`
- helm: latest chart from index/OCI; values images → new digests
- k8s: image digests

- [ ] Steps: TDD → commit

```bash
git commit -m "$(cat <<'EOF'
feat: upgrade resolve for terraform, helm, and k8s

EOF
)"
```

---

### Task 9: Cargo + Go + Ruby online upgrade resolve

**Files:**
- Modify: `crates/pinner-cargo/src/resolve.rs` (+ small HTTP helper or reuse `pinner-iac-common` http)
- Modify: `crates/pinner-go/src/resolve.rs`
- Modify: `crates/pinner-ruby/src/resolve.rs`
- Modify: `crates/pinner-toolchain/src/detect.rs` (optional `go` tool listing for status when go enabled)

**Behavior:**
- cargo: crates.io API `/crates/{name}` → max_version (HTTP); map; skip path/git
- go: prefer `go list -m -u -json`; else proxy.golang.org; map
- ruby: RubyGems `https://rubygems.org/api/v1/versions/{name}/latest.json`; map

- [ ] Steps: TDD with maps offline; network tests behind `PINNER_NETWORK=1` → commit

```bash
git commit -m "$(cat <<'EOF'
feat: online upgrade resolve for cargo, go, and ruby

EOF
)"
```

---

### Task 10: Gitlab + Azure upgrade resolve

**Files:**
- Modify: `crates/pinner-gitlab/src/resolve.rs`
- Modify: `crates/pinner-azure/src/resolve.rs`

**Behavior:**
- gitlab: images → digests; includes → `git ls-remote`
- azure: images → digests; tasks → map or HTTP if available (map-only acceptable if no API yet — document as gap)

- [ ] Steps: TDD → commit

```bash
git commit -m "$(cat <<'EOF'
feat: upgrade resolve for gitlab and azure

EOF
)"
```

---

### Task 11: README + guide docs + mise install backends

**Files:**
- Modify: `README.md`
- Modify: `docs/guide/quick-start.md`
- Modify: `docs/guide/configuration.md`
- Modify: `docs/SUMMARY.md`
- Create: `docs/guide/ecosystems/README.md` (index)
- Create: `docs/guide/ecosystems/{mise,node,python,docker,actions,terraform,helm,k8s,cargo,go,ruby,gitlab,azure}.md`
- Modify: `skills/pinner/SKILL.md`

**Content requirements:**
- README Quick start includes `pinner upgrade` and `pinner upgrade --walkthrough`
- README Install mise section:

```bash
mise use -g github:zloeber/Pinner
mise use -g cargo:pinner
```

- README provider matrix matching the design table (Preferred upgrade means column)
- Each ecosystem page: Pin / Upgrade / Check / Gaps + preferred tool
- Skill: when to `pin` vs `upgrade`; never `--walkthrough` in agents

- [ ] **Step 1:** Add SUMMARY links and one ecosystem page; build docs if mdbook available: `task docs`
- [ ] **Step 2–4:** Complete all 13 pages + README + skill
- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
docs: document upgrade command and per-provider support

EOF
)"
```

---

### Task 12: End-to-end verification + `.mex` growth

**Files:**
- Modify: `.mex/ROUTER.md` (project state: upgrade planned/shipped)
- Create: `.mex/patterns/add-upgrade-resolve-mode.md` if no matching pattern
- Modify: `.mex/patterns/INDEX.md`

- [ ] **Step 1: Run local CI**

```bash
scripts/ci-local
```

Expected: all gates pass

- [ ] **Step 2: Manual smoke**

```bash
cargo run -p pinner -- upgrade --help
cargo run -p pinner -- upgrade --agent --dry-run
```

- [ ] **Step 3: Update `.mex` state + pattern; commit**

```bash
git commit -m "$(cat <<'EOF'
chore: record upgrade subcommand pattern and router state

EOF
)"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| `pinner upgrade` CLI | 3 |
| ResolveMode bypass lock/native | 1, 2, 5–10 |
| Latest including majors | 6–10 |
| Walkthrough current→proposed | 4 |
| Same agent/dry-run/offline | 2, 3 |
| Prefer uv | 6 |
| Cargo/Go/Ruby online | 9 |
| README matrix + 13 pages | 11 |
| mise cargo:/github: backends | 11 |
| Omit unchanged / no-op | 2, 5 |

## Placeholder scan

No TBD/TODO placeholders. Azure task HTTP may remain map-only if no API helper exists — explicitly allowed in Task 10 and must be stated on `docs/guide/ecosystems/azure.md` Gaps section.
