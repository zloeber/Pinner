---
name: fix-semantic-release-chain
description: Tag is pushed by semantic-release but release.yml never starts — GITHUB_TOKEN auth hijacks the PAT push.
last_updated: 2026-08-04
---

# Fix semantic-release → release workflow chaining

## When to use
`Semantic Release` succeeds and creates `vX.Y.Z`, but the `release` workflow has no run for that tag.

## Root cause
`actions/checkout` persists `GITHUB_TOKEN` as `http.https://github.com/.extraheader`. That header overrides credentials embedded in a `git push` URL, so the tag push authenticates as the job token. GitHub does not start other workflows from `GITHUB_TOKEN` pushes.

## Steps
1. Confirm: `gh run list --workflow=release.yml` has no entry for the new tag; semantic-release log shows `Created and pushed tag: v…`.
2. In `.github/workflows/semantic-release.yml`:
   - Checkout with `persist-credentials: false` (still do **not** pass `PAT_TOKEN` into checkout).
   - Before the tag push: unset `http.https://github.com/.extraheader` and/or `git -c "http.https://github.com/.extraheader=" push …` with the PAT URL.
3. Keep `PAT_TOKEN` as a classic PAT (`contents:write`) seeded via `Secretfile.yml`.
4. Recover stuck tags: `git push origin :refs/tags/vX.Y.Z`, then re-run Semantic Release or re-push the annotated tag with a human/PAT identity.

## Verify
- [ ] `semantic-release.yml` has `persist-credentials: false` on checkout
- [ ] Tag push clears/overrides `http.*.extraheader` and uses `PAT_TOKEN` in the remote URL
- [ ] After the next releasable merge to `main`, `gh run list --workflow=release.yml` shows a run for the new tag
