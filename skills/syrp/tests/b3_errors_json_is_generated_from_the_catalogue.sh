#!/usr/bin/env bash
# B3: references/errors.json equals a fresh `scripts/refresh.sh` run, and every embedded-domain
# code the skill names is in it.
set -uo pipefail
skill="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp)"; trap 'rm -f "$tmp"' EXIT
fail=0
bash "$skill/scripts/refresh.sh" "$tmp" >/dev/null || { echo "  refresh.sh failed"; exit 1; }
diff -q "$tmp" "$skill/references/errors.json" >/dev/null \
  || { echo "  references/errors.json is stale: run scripts/refresh.sh"; fail=1; }
jq -e 'length > 0 and all(.[]; has("code") and has("title") and has("severity") and has("cause") and has("fix"))' \
  "$skill/references/errors.json" >/dev/null || { echo "  entries lack keys"; fail=1; }
domains="$(jq -r '[.[].code | split("-")[1]] | unique | join("|")' "$skill/references/errors.json")"
for code in $(grep -rhoE "SYRP-($domains)-[0-9]{4}" "$skill/SKILL.md" "$skill/references/recipes" | sort -u); do
  jq -e --arg c "$code" 'any(.[]; .code == $c)' "$skill/references/errors.json" >/dev/null \
    || { echo "  skill names $code, which the catalogue lacks"; fail=1; }
done
exit "$fail"
