#!/usr/bin/env bash
# One closed-loop iteration: `cargo syrp check`, then `cargo syrp test` (for one package if given).
# Prints exactly one JSON line and exits with the worse of the two CLI exit codes, ranked the way
# the CLI ranks them (internal > usage > environment > the domain codes 4-9 > failed > ok).
#
#   scripts/loop.sh                 # check, then test the whole workspace
#   scripts/loop.sh syrp-geom       # check, then test one package
#
# SYRP_CMD overrides the command (default "cargo syrp"); the skill's own tests use it.
set -uo pipefail
pkg="${1:-}"
read -r -a syrp <<<"${SYRP_CMD:-cargo syrp}"

# Run one CLI command; print its envelope, or a stand-in envelope if the output is not JSON.
run() {
  local out code
  out="$("${syrp[@]}" "$@" --json 2>/dev/null)"; code=$?
  if ! jq -e 'type == "object"' >/dev/null 2>&1 <<<"$out"; then
    out="$(jq -cn --arg c "$*" --argjson e "$code" '{ok: false, exit_code: $e, results: [],
      diagnostics: [{code: null, message: ("`cargo syrp " + $c + "` printed no JSON envelope")}]}')"
  fi
  printf '%s\n' "$out"
  return "$code"
}

rank() { case "$1" in 0) echo 0 ;; 1) echo 1 ;; 10) echo 10 ;; 2) echo 9 ;; 3) echo 8 ;; *) echo 5 ;; esac; }

check="$(run check)"; c=$?
if [ -n "$pkg" ]; then test_out="$(run test --crate "$pkg")"; else test_out="$(run test)"; fi
t=$?
exit_code=$c
if [ "$(rank "$t")" -gt "$(rank "$c")" ]; then exit_code=$t; fi

jq -cn --argjson check "$check" --argjson test "$test_out" --arg pkg "$pkg" --argjson exit "$exit_code" \
  '{schema: "syrp.loop/v1", ok: ($exit == 0), exit_code: $exit,
    package: (if $pkg == "" then null else $pkg end),
    check: {ok: $check.ok, exit_code: $check.exit_code,
            diagnostics: [$check.diagnostics[]? | {code, message}]},
    test: {ok: $test.ok, exit_code: $test.exit_code, summary: ($test.results[0] // null),
           diagnostics: [$test.diagnostics[]? | {code, message}]}}'
exit "$exit_code"
