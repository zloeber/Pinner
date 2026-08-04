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
