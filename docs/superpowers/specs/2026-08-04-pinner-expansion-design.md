# Pinner Expansion Design — New Targets, Walkthrough, Installer, Agent Skill

**Date:** 2026-08-04  
**Status:** Approved for implementation planning  
**Summary:** Expand Pinner with language and CI pin targets, close existing IaC/mise gaps, add a compact-list walkthrough TUI for humans and a strict agent JSON mode, ship a curl/shell user-local installer, and add `skills/pinner/SKILL.md` for AI agents.

## Problem

Pinner already pins mise, node, python, docker, GitHub Actions, terraform, helm, and k8s — but real repos still float versions in Cargo/Go/Ruby manifests, GitLab CI and Azure Pipelines, deeper GitHub workflow surfaces (images, reusable workflows), nested mise configs, Helm values images, and registry-backed Terraform/Helm resolves. Humans need guided review before rewrites; agents need a non-interactive contract. Distribution today is manual GitHub Release download or `cargo install`; there is no `curl | sh` path into a user bin. Agents lack a project skill describing how to audit and pin floating versions.

## Goals

1. **New pin targets:** Cargo, Go, Ruby (default on); GitLab CI and Azure DevOps (opt-in); deepen GitHub Actions (workflow/service images + reusable/composite `uses:`).
2. **Close gaps:** recursive mise discovery; Helm `values*.yaml` image pins; Terraform registry HTTP and Helm repo/OCI HTTP resolve (offline still requires lock or resolve map).
3. **Walkthrough mode:** per-finding approve / skip / edit with a compact-list TUI before rewrite.
4. **Dual interface:** beautiful human TUI; agentic JSON / `--agent` with no prompts.
5. **Installer:** `scripts/install.sh` installs the release binary to `${PINNER_INSTALL_DIR:-$HOME/.local/bin}`.
6. **Agent skill:** `skills/pinner/SKILL.md` playbook for finding and pinning floats.

### Non-goals

- Separate `pinner-ui` binary or second install artifact.
- Replacing Renovate/Dependabot.
- npm/Pixi/Conda/Homebrew ecosystems in this release.
- Guaranteeing bit-identical final build artifacts beyond freezing the declared graph.

## Decisions (from brainstorming)

| Topic | Choice |
|-------|--------|
| Scope / phasing | One big release (all of the above together) |
| Architecture | Extend-in-place: shared core + ecosystem crates + `pinner-ui` presentation crate |
| Walkthrough | Per-finding accept / skip / edit |
| Human vs agent | Two modes, one engine — TUI for humans; JSON/`--agent` for agents |
| Install dir | `~/.local/bin` default; `PINNER_INSTALL_DIR` override |
| New ecosystem defaults | Languages on; GitLab + Azure opt-in |
| Deeper GitHub | Images in workflows **and** reusable workflows / remote composites |
| Walkthrough layout | Compact list (spreadsheet rows; arrows + keybindings) |
| Walkthrough quit | Exit 0, nothing written |

## Architecture

```text
pinner (CLI)
  ├── mode select: TTY + --walkthrough → pinner-ui compact walkthrough
  │                 --format json / --agent / non-TTY → structured agent I/O
  ├── pinner-core          # policy, lock, pin/check/audit + walkthrough filter hooks
  ├── pinner-ui            # ratatui compact list; pretty TTY summaries
  ├── pinner-toolchain
  ├── pinner-ecosystem     # EcosystemKind + trait
  ├── existing ecosystems  # mise*, node, python, docker, actions*, terraform*, helm*, k8s
  └── new: cargo, go, ruby, gitlab, azure
```

`*` = extended in this effort (mise recursive; actions deepen; terraform/helm HTTP resolve; helm values images).

Outside the Rust graph:

- `scripts/install.sh` — OS/arch detect, download GitHub Release asset, install binary, PATH hint.
- `skills/pinner/SKILL.md` — agent instructions for audit → pin → check.

## Pin targets

### New ecosystems

| Kind | Default | Discover | Floating → pin |
|------|---------|----------|----------------|
| **cargo** | on | `Cargo.toml` (workspace + members) | `*` / bare / non-exact deps → exact semver in TOML; prefer `Cargo.lock` evidence |
| **go** | on | `go.mod` (+ `go.work` modules) | floating / `latest` / unbound → exact module versions; prefer `go.sum` / `go list` |
| **ruby** | on | `Gemfile` | unpinned / floating ranges per `pin_exact_ranges` → exact gems; prefer `Gemfile.lock` |
| **gitlab** | opt-in | `.gitlab-ci.yml`, nested CI includes | `image:` without digest; floating remote `include:` → digest / pinned ref |
| **azure** | opt-in | `azure-pipelines*.yml`, `.azure-pipelines/**` | floating `container` / task versions → digest or exact task version |

### GitHub Actions deepen (existing crate, default on)

- Pin `container:` / job containers / `services:` images to `@sha256:…` (reuse docker/k8s digest helpers).
- Pin reusable workflows (`owner/repo/.github/workflows/….yml@ref`) and remote composites to full commit SHA, same as ordinary `uses:`.

### Gap fixes

- **mise:** discover nested `.mise.toml` / `.tool-versions`, not only repo root.
- **helm:** pin images in `values.yaml` / `values*.yaml` in addition to chart versions.
- **terraform + helm:** implement registry HTTP (and OCI where needed). Offline: lock entry or `PINNER_*_RESOLVE_MAP` required.

### Policy

- Extend `EcosystemKind` and `pinner.toml` enable/disable lists.
- Languages honor `pin_exact_ranges` like node/python where applicable.
- Existing `allow_floating` glob semantics unchanged.

## Walkthrough, TUI, and agent I/O

### Compact-list walkthrough

Triggered by `--walkthrough` on `pin` and `audit --fix`:

1. After successful resolve, before rewrite/lock write.
2. Compact table: ecosystem · name · requested → proposed · path.
3. Keys: arrows move; **Enter/a** accept; **s** skip; **e** edit (inline alternate pin); **q** quit.
4. Header shows `n/N` and accepted/skipped counts.
5. Skipped pins omitted from rewrite/lock; edits replace `pin.pinned` and record user override in pin metadata.
6. Quit: exit **0**, write nothing, message that the run aborted cleanly.

### Pretty human output (non-walkthrough TTY)

- Summary panels for counts and per-ecosystem breakdown on `pin` / `check` / `audit` when stdout is a TTY and format is text.
- Not a full-screen app unless walkthrough is requested.

### Agent mode

- `--agent` or `--format json` (and non-TTY without `--walkthrough`): no prompts; stable JSON reports; exit codes unchanged (`0` ok, `1` drift/findings, `2` tool/config/resolve).
- `--walkthrough` combined with `--agent` or non-TTY: exit **2** with a clear error.

## CLI surface

| Flag / command | Behavior |
|----------------|----------|
| `--walkthrough` | Interactive compact-list gate before writes |
| `--agent` | Force agent mode (JSON, no TUI prompts) |
| Existing `pin` / `check` / `audit` / `explain` / `toolchain` | Unchanged semantics aside from new ecosystems and presentation |

## Installer

`scripts/install.sh`:

1. Detect OS/arch for the shell installer: linux/darwin × amd64/arm64 only. Windows users continue to use GitHub Release `.zip` assets (no `install.sh` path in this release).
2. Version: `PINNER_VERSION` or latest GitHub Release tag.
3. Download matching release asset.
4. Install to `${PINNER_INSTALL_DIR:-$HOME/.local/bin}/pinner`, `chmod +x`.
5. If install dir not on `PATH`, print a one-line hint.
6. Fail loud on unsupported arch, HTTP failure, or checksum mismatch when checksums are published with the release.
7. `PINNER_INSTALL_DRY_RUN=1` prints planned URL/path without downloading (CI-friendly).

Document `curl -fsSL …/install.sh | sh` (or equivalent raw GitHub URL) in README and quick-start.

## Agent skill

`skills/pinner/SKILL.md` must instruct agents to:

1. Detect floating versions via `pinner audit --format json` (or `--agent`).
2. Prefer `pinner pin` then `pinner check` over manual edits.
3. Never use `--walkthrough` in automated loops.
4. Respect ecosystem enablement in `pinner.toml`; enable gitlab/azure when those files are present.
5. Treat exit `1` as actionable drift/findings and exit `2` as hard failure.
6. After pinning, verify idempotency with a second `check` / dry-run pin.

## Data flow

```text
discover → extract → resolve
    → [optional walkthrough filter: accept/skip/edit]
    → rewrite manifests + write pinner.lock.json
    → emit TUI summary or JSON RunReport
```

Resolve failure still writes nothing (all-or-nothing). Dry-run stages the same plan without filesystem writes.

## Error handling

| Situation | Exit | Writes |
|-----------|------|--------|
| Success | 0 | As commanded |
| Drift / audit findings | 1 | None for check/audit-without-fix |
| Config / resolve / toolchain / invalid walkthrough+agent | 2 | None |
| Walkthrough quit | 0 | None |

## Testing

- New ecosystem crates: fixtures under `tests/fixtures/{cargo,go,ruby,gitlab,azure}-floating/` plus discover/extract/resolve/rewrite tests.
- Actions deepen, Helm values, recursive mise, TF/Helm HTTP: fixtures + `crates/pinner/tests` e2e where practical; HTTP resolve tests may use recorded fixtures / maps offline.
- Walkthrough decision filtering unit-tested in core without a real TTY.
- Installer: shellcheck + dry-run assertions in CI.
- Skill path presence checked via docs/CI sanity if lightweight.

## Success criteria

- `pinner audit` / `pin` / `check` cover Cargo, Go, Ruby by default and GitLab/Azure when enabled.
- Nested mise, Helm values images, and Actions images/reusable refs are pinned or reported.
- Terraform/Helm registry resolve works online without requiring only a pre-seeded map (map/lock still required offline).
- Walkthrough compact list supports accept/skip/edit/quit with no partial writes on quit.
- `--agent` / JSON never blocks on prompts.
- `curl | sh` installer places a working binary under `~/.local/bin` (or `PINNER_INSTALL_DIR`).
- `skills/pinner/SKILL.md` gives agents a clear pin workflow.

## Implementation notes

- Follow existing ecosystem crate layout: `discover` / `extract` / `resolve` / `rewrite` + `lib.rs` trait impl.
- Register new kinds in `EcosystemKind`, policy defaults, CLI filter parsing, and `register_ecosystems()`.
- Prefer evidence order: lock → native lock → registry/tool → fail with hint.
- Keep lock schema backward compatible; new ecosystem string values only.
- Add `.superpowers/` to `.gitignore` (brainstorm companion artifacts).
