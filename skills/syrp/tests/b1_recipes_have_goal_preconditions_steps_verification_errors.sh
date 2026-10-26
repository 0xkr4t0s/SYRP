#!/usr/bin/env bash
# B1: the five recipes exist, each has the five sections, quotes real commands in its steps,
# and SKILL.md lists it.
set -uo pipefail
skill="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0
for r in start-a-work-package add-error-code create-a-crate fix-failing-check respond-to-review; do
  f="$skill/references/recipes/$r.md"
  if [ ! -f "$f" ]; then echo "  missing recipe $r.md"; fail=1; continue; fi
  for s in '**Goal:**' '**Preconditions**' '**Steps**' '**Verification**' '**Common errors**'; do
    grep -qF -- "$s" "$f" || { echo "  $r.md lacks $s"; fail=1; }
  done
  steps="$(sed -n '/^\*\*Steps\*\*/,/^\*\*Verification\*\*/p' "$f")"
  grep -q 'cargo syrp [a-z]' <<<"$steps" || { echo "  $r.md steps quote no cargo syrp command"; fail=1; }
  grep -q 'SYRP-[A-Z]*-[0-9]\{4\}' <(sed -n '/^\*\*Common errors\*\*/,$p' "$f") \
    || { echo "  $r.md common errors name no code"; fail=1; }
  grep -qF "$r.md" "$skill/SKILL.md" || { echo "  SKILL.md does not list $r.md"; fail=1; }
done
exit "$fail"
