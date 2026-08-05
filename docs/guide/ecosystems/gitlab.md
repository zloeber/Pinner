# gitlab

**Default:** off (opt-in: `gitlab = true`). **Preferred upgrade tool:** docker digests + `git ls-remote`.

## Pin

GitLab CI includes and related image refs when enabled. Lock → map → digests / `git ls-remote`. Digest / SHA pin style.

## Upgrade

Docker digests for images; `git ls-remote` for include refs (HEAD/default). Map fallback. Pin style: digest / SHA.

## Check

Drift vs `pinner.lock.json` when gitlab enabled.

## Gaps

- Requires `gitlab = true` in `pinner.toml`.
- Private includes need git auth or maps offline.
