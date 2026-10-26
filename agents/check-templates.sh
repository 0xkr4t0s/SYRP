#!/usr/bin/env bash
# Renders every role prompt with exactly the keys its workflow passes and fails if any
# {{PLACEHOLDER}} is left unfilled. Run in CI (ci.yml, check job).
set -euo pipefail
cd "$(dirname "$0")"
fail=0
check() {
  local template="$1"; shift
  local out
  out="$(./render.sh "$template" "$@")"
  if grep -o '{{[A-Z_]*}}' <<<"$out" | sort -u | grep .; then
    echo "::error::$template has unfilled placeholders (above)"; fail=1
  fi
}
# agent.yml
for role in implementer spec-room; do
  check "roles/$role.md" WP=x ISSUE=1 MODE=implement PR=1 MODEL=m
done
# reviewer.yml
for role in reviewer adversarial; do
  check "roles/$role.md" PR=1 WP=x KIND=primary MODEL=m RISKS='[]' REVIEW_PATH=/tmp/r.json
done
# pm-agent.yml
check roles/pr-manager.md PR=1 TASK=conflict MODEL=m
exit $fail
