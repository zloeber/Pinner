# mise

**Default:** on. **Preferred upgrade tool:** `mise latest` / `mise ls-remote`.

## Pin

Discovers floating tool versions in mise configs (including nested), resolves via lock → `PINNER_MISE_RESOLVE_MAP` → mise CLI when online, rewrites to exact tool versions.

## Upgrade

Bypasses lock evidence. Prefers `mise latest` / `mise ls-remote` for the newest matching tool version; falls back to `PINNER_MISE_RESOLVE_MAP`. Pin style: exact tool version string.

## Check

Compares discovered mise pins against `pinner.lock.json` (no writes).

## Gaps

- Offline without lock/map fails closed.
- Tools not known to mise cannot be resolved via the mise CLI path.
