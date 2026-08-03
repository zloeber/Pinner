#!/usr/bin/env zsh
# Gate git push on lean local CI. Emits short actionable JSON for the agent.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

input="$(cat)"
cmd="$(
  python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("command") or "")' <<<"$input" 2>/dev/null \
    || true
)"
if [[ -z "$cmd" ]]; then
  print -r -- '{"permission":"allow"}'
  exit 0
fi

if [[ ! "$cmd" =~ (^|[[:space:];|&])git[[:space:]]+push([[:space:]]|$) ]]; then
  print -r -- '{"permission":"allow"}'
  exit 0
fi
if [[ "${PINNER_SKIP_LOCAL_CI:-}" == "1" ]] || [[ "$cmd" == *PINNER_SKIP_LOCAL_CI=1* ]]; then
  print -r -- '{"permission":"allow","agent_message":"local CI skipped (PINNER_SKIP_LOCAL_CI=1)"}'
  exit 0
fi

out="$(mktemp -t pinner-hook-ci)"
set +e
"$ROOT/scripts/ci-local" >"$out" 2>&1
status=$?
set -e

python3 - "$status" "$out" <<'PY'
import json, sys
status = int(sys.argv[1])
path = sys.argv[2]
with open(path, encoding="utf-8", errors="replace") as f:
    lines = f.read().splitlines()
# Keep the hook payload small.
summary = "\n".join(lines[-40:])
if status == 0:
    print(json.dumps({
        "permission": "allow",
        "agent_message": "local CI pass (fmt/clippy/test/schema)",
    }))
else:
    print(json.dumps({
        "permission": "deny",
        "user_message": "Local CI failed — fix before push (or PINNER_SKIP_LOCAL_CI=1).",
        "agent_message": (
            "Push blocked. Local CI summary:\n"
            + summary
            + "\nFix, then re-run scripts/ci-local before git push."
        ),
    }))
PY
rm -f "$out"
exit 0
