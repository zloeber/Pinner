---
name: router
description: Session bootstrap and navigation hub. Read at the start of every session before any task. Contains project state, routing table, and behavioural contract.
edges:
  - target: context/architecture.md
    condition: when working on system design, integrations, or understanding how components connect
  - target: context/stack.md
    condition: when working with specific technologies, libraries, or making tech decisions
  - target: context/conventions.md
    condition: when writing new code, reviewing code, or unsure about project patterns
  - target: context/decisions.md
    condition: when making architectural choices or understanding why something is built a certain way
  - target: context/setup.md
    condition: when setting up the dev environment or running the project for the first time
  - target: patterns/INDEX.md
    condition: when starting a task — check the pattern index for a matching pattern file
last_updated: 2026-08-05
---

# Session Bootstrap

If you haven't already read `AGENTS.md`, read it now — it contains the project identity, non-negotiables, and commands.

Then read this file fully before doing anything else in this session.

## Current Project State

**Working:**
- Multi-ecosystem pin/check/audit CLI with fixtures and local CI (`scripts/ci-local`)
- Tag-driven releases (`semantic-release.yml` → `release.yml`) and mdBook docs Pages deploy
- Workflows use Node 24 action majors (`checkout@v5`, `mise-action@v4`, artifact/pages v5–v7)
- `pinner upgrade` shipped: core orchestration + CLI/walkthrough/`upgrade_pin`, all ecosystem `ResolveMode::Upgrade` paths (map → tool/registry), README matrix + mise `github:`/`cargo:` backends + 13 ecosystem guide pages + skill
- Upgrade patterns: `ecosystem-upgrade-resolve.md`, `core-upgrade-orchestration.md`, `docs-upgrade-and-ecosystems.md`

**Not yet built:**
- Broader `.mex/` pattern library beyond upgrade/release patterns (index still sparse outside those areas)

**Known issues:**
- Semantic-release tag push requires a valid classic `PAT_TOKEN` (repo secret); expired/wrong-scope PATs fail at push, not checkout
- Fixed 2026-08-04: checkout’s persisted `GITHUB_TOKEN` `http.extraheader` was overriding the PAT URL, so tags pushed without triggering `release.yml` (e.g. stuck `v0.2.0`) — see `.mex/patterns/fix-semantic-release-chain.md`
- Fixed 2026-08-04: tag-first `release.yml` rewrote Cargo.toml version then `cargo build --locked` failed until `cargo update -w` syncs path-package versions in Cargo.lock
- Fixed 2026-08-04: `task install` showed `pinner v0.1.0` because Cargo.toml lagged tags; semantic-release now commits the workspace version bump before tagging


## Routing Table

Load the relevant file based on the current task. Always load `context/architecture.md` first if not already in context this session.

| Task type | Load |
|-----------|------|
| Understanding how the system works | `context/architecture.md` |
| Working with a specific technology | `context/stack.md` |
| Writing or reviewing code | `context/conventions.md` |
| Making a design decision | `context/decisions.md` |
| Setting up or running the project | `context/setup.md` |
| Any specific task | Check `patterns/INDEX.md` for a matching pattern |

## Behavioural Contract

For every task, follow this loop:

1. **CONTEXT** — Load the relevant context file(s) from the routing table above. Check `patterns/INDEX.md` for a matching pattern. If one exists, follow it. Narrate what you load: "Loading architecture context..."
2. **BUILD** — Do the work. If a pattern exists, follow its Steps. If you are about to deviate from an established pattern, say so before writing any code — state the deviation and why.
3. **VERIFY** — Load `context/conventions.md` and run the Verify Checklist item by item. State each item and whether the output passes. Do not summarise — enumerate explicitly.
4. **DEBUG** — If verification fails or something breaks, check `patterns/INDEX.md` for a debug pattern. Follow it. Fix the issue and re-run VERIFY.
5. **GROW** — After completing the task:
   - If no pattern exists for this task type, create one in `patterns/` using the format in `patterns/README.md`. Add it to `patterns/INDEX.md`. Flag it: "Created `patterns/<name>.md` from this session."
   - If a pattern exists but you deviated from it or discovered a new gotcha, update it with what you learned.
   - If any `context/` file is now out of date because of this work, update it surgically — do not rewrite entire files.
   - Update the "Current Project State" section above if the work was significant.
