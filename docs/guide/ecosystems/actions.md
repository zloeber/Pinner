# actions

**Default:** on. **Preferred upgrade tool:** `gh api` (latest release → commit SHA).

## Pin

Finds floating `uses:` refs and workflow images. Lock → map → `gh api` / docker digests. Rewrites `@<sha>` and image digests. Reusable and composite actions included.

## Upgrade

Prefers `gh api`: latest release tag’s commit, else default branch HEAD. Workflow images use docker digests. Map fallback (`PINNER_ACTIONS_RESOLVE_MAP`). Pin style: `@<sha>` + image digests.

## Check

Drift vs `pinner.lock.json`.

## Gaps

- Local / path actions are not remote-resolved.
- Private repos need `gh` auth; offline needs lock or map.
