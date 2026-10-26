#!/usr/bin/env bash
# One closed-loop iteration: check, then test. Prints one JSON line; exits with the worst code.
set -uo pipefail
pkg="${1:-}"
check="$(cargo syrp check --json)"; c=$?
if [ -n "$pkg" ]; then test_out="$(cargo syrp test --crate "$pkg" --json)"; else test_out="$(cargo syrp test --json)"; fi
t=$?
jq -cn --argjson check "$check" --argjson test "$test_out" \
  '{check: {ok: $check.ok, diagnostics: [$check.diagnostics[] | {code, message}]},
    test: {ok: $test.ok, summary: $test.results[0], diagnostics: [$test.diagnostics[] | {code, message}]}}'
if [ $c -ne 0 ]; then exit $c; fi
exit $t
