#!/usr/bin/env bash
# B2: scripts/loop.sh runs check then test (for an optional crate), prints exactly one JSON line,
# and exits with the worse CLI exit code. First against a stub, then once against the real CLI.
set -uo pipefail
skill="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
loop="$skill/scripts/loop.sh"
export SYRP_CMD="bash $skill/tests/fake-syrp.sh"
fail=0
# check_exit test_exit expected_exit
for case in "0 0 0" "1 0 1" "0 1 1" "1 3 3" "4 1 4" "2 10 10" "9 1 9"; do
  read -r c t want <<<"$case"
  out="$(FAKE_CHECK_EXIT=$c FAKE_TEST_EXIT=$t bash "$loop" syrp-demo)"; got=$?
  [ "$got" = "$want" ] || { echo "  check=$c test=$t: exit $got, want $want"; fail=1; }
  [ "$(wc -l <<<"$out")" = 1 ] || { echo "  check=$c test=$t: not one line"; fail=1; }
  jq -e --argjson w "$want" '.exit_code == $w and .ok == ($w == 0) and .package == "syrp-demo"
      and .test.summary.args == "--crate syrp-demo --json"' <<<"$out" >/dev/null \
    || { echo "  check=$c test=$t: bad summary $out"; fail=1; }
done
out="$(bash "$loop")"
jq -e '.package == null and .test.summary.args == "--json"' <<<"$out" >/dev/null \
  || { echo "  no package: bad summary $out"; fail=1; }
out="$(FAKE_CHECK_RAW=1 FAKE_CHECK_EXIT=10 bash "$loop")"; got=$?
[ "$got" = 10 ] && jq -e '.check.ok == false and (.check.diagnostics | length) == 1' <<<"$out" >/dev/null \
  || { echo "  non-JSON check output: exit $got, $out"; fail=1; }
unset SYRP_CMD
# The real CLI, on a small package.
out="$(cd "$skill" && bash "$loop" syrp-geom)"; got=$?
[ "$(wc -l <<<"$out")" = 1 ] && jq -e --argjson g "$got" '.exit_code == $g and .check.exit_code != null
    and (.test.summary.passed | type) == "number"' <<<"$out" >/dev/null \
  || { echo "  real run: exit $got, $out"; fail=1; }
exit "$fail"
