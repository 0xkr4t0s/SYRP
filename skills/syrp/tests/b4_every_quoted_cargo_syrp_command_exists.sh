#!/usr/bin/env bash
# B4: every `cargo syrp <command>` quoted in the skill (SKILL.md, recipes, scripts) exists:
# `cargo syrp <command> --help` exits 0. Arguments such as <id> or --json end the command.
set -uo pipefail
skill="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cmds="$(grep -rhoE 'cargo syrp( [a-z][a-z-]*)+' "$skill/SKILL.md" "$skill/references/recipes" "$skill/scripts" \
  | sed 's/^cargo syrp //' | sort -u)"
n="$(grep -c . <<<"$cmds")"
[ "$n" -ge 10 ] || { echo "  only $n commands found; the scan is broken"; exit 1; }
fail=0
while read -r c; do
  # shellcheck disable=SC2086 # word splitting is the point: "wp show" is two arguments
  (cd "$skill" && cargo syrp $c --help >/dev/null 2>&1) || { echo "  cargo syrp $c --help failed"; fail=1; }
done <<<"$cmds"
exit "$fail"
