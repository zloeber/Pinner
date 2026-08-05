---
name: pinner
description: Find, pin, and upgrade dependency versions across a repository with the pinner CLI (audit/pin/upgrade/check). Use when versions use latest, ranges, unpinned images, floating CI refs, or when exact pins should bump to latest.
---

# Pinner

## When to use
- Repo has floating versions (`latest`, `*`, `^`, unpinned images, `@v4` actions, etc.) → **pin**
- Exact pins already present but should move to newer releases → **upgrade**
- Need reproducible pins + `pinner.lock.json` + CI drift gate → **check**

## pin vs upgrade
| Goal | Command |
|------|---------|
| Freeze floating → exact (first-time / after audit) | `pinner pin --agent` |
| Bump existing exact pins to latest | `pinner upgrade --agent` |
| Fail if lock/manifests drifted | `pinner check --agent` |

Do not use `upgrade` to discover floaters — use `audit` / `pin`. Do not use `pin` when the intent is “move to latest” on already-exact pins — use `upgrade`.

## Agent workflow
1. `pinner audit --format json` (or `--agent`)
2. Enable opt-in ecosystems in `pinner.toml` if needed:
   ```toml
   [ecosystems]
   helm = true
   k8s = true
   gitlab = true
   azure = true
   ```
3. `pinner pin --agent` for floating → exact, **or** `pinner upgrade --agent` to bump exact pins
4. `pinner check --agent` — expect exit 0
5. **Never** use `--walkthrough` in automation or agent loops (TTY-only; with `--agent` exits `2`)

## Exit codes
- 0 ok
- 1 drift / audit findings
- 2 config/resolve/toolchain/invalid mode
