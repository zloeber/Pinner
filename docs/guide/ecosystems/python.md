# python

**Default:** on. **Preferred upgrade tool:** **uv** (`uv pip compile` / uv resolve).

## Pin

Finds floating specs in `requirements*.txt`, `pyproject.toml`, etc. Evidence: lock → native locks (`uv.lock` / `poetry.lock` / `pdm.lock`) → `PINNER_PYTHON_RESOLVE_MAP` → uv when online. Rewrites exact `==` pins.

## Upgrade

Ignores poetry/pdm locks for upgrade. Prefers uv resolve / `uv pip compile` over pip or poetry CLI; map fallback. Pin style: exact `==`.

## Check

Drift vs `pinner.lock.json`.

## Gaps

- Prefer uv; other package managers are not the upgrade path.
- Extras / URL / VCS deps may be skipped or map-only depending on extract support.
