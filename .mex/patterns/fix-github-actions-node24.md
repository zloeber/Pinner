---
name: fix-github-actions-node24
description: Upgrade GitHub Actions to Node 24 runtimes and keep semantic-release PAT usage push-only.
last_updated: 2026-08-04
---

# Fix GitHub Actions Node 20 + semantic-release PAT auth

## When to use
Node 20 deprecation warnings on Actions, or semantic-release fails checkout with `could not read Username for 'https://github.com': terminal prompts disabled`.

## Steps
1. Confirm failure: `gh run view <id> --log-failed` — note whether auth fails at checkout (token passed to checkout) or push.
2. Bump first-party actions to Node 24 majors: `actions/checkout@v5+`, `upload-artifact@v6+`, `download-artifact@v7+`, `deploy-pages@v5`, `upload-pages-artifact@v5` (embeds upload-artifact@v7). Prefer `jdx/mise-action@v4`, `softprops/action-gh-release@v3`.
3. Avoid Node 20 third-party actions when a binary/curl install is easy (e.g. mdBook release tarball).
4. For workflow-chaining tag pushes: checkout with default `GITHUB_TOKEN` and `persist-credentials: false`; use `PAT_TOKEN` only on `git push` of the tag (clear `http.*.extraheader` first). Fail fast if `PAT_TOKEN` is empty. See [fix-semantic-release-chain.md](fix-semantic-release-chain.md) if the tag lands but `release` never runs.
5. If push still fails: human re-seeds a classic PAT via SecretZero (`Secretfile.yml`) — agents must not handle the token value.

## Verify
- [ ] No workflow under `.github/workflows/` references `actions/checkout@v4` or other Node 20 action majors listed above
- [ ] `semantic-release.yml` does not pass `token: ${{ secrets.PAT_TOKEN }}` into checkout
- [ ] `semantic-release.yml` sets `persist-credentials: false` and clears extraheader before PAT push
- [ ] Releasing docs mention classic PAT + push-only usage
