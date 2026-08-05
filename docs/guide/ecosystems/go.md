# go

**Default:** on. **Preferred upgrade tool:** `go list -m -u` (when `go` is present).

## Pin

Module requirements in `go.mod`. Lock → map → `go list` / proxy.golang.org. Exact module versions.

## Upgrade

Prefers `go list -m -u`; falls back to proxy.golang.org HTTP and `PINNER_GO_RESOLVE_MAP`. Pin style: exact module version.

## Check

Drift vs `pinner.lock.json`.

## Gaps

- Replace / exclude directives may limit what can be upgraded.
- Offline without lock/map fails closed.
